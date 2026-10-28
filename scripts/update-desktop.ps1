# Update the installed Windows desktop app (aurelius.exe) to the latest main.
# Only the app is rebuilt and replaced: the server/service is not touched, and
# no tests run (use install-production.ps1 for a full, tested install).
#
#   scripts/update-desktop.ps1          pull main, rebuild and replace the app if there is something new
#   scripts/update-desktop.ps1 -Force   rebuild and replace even when it's up to date
#
# The app is started again if it was running. Your data and the pairing stay.
param([switch]$Force)
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

$dest = Join-Path $env:LOCALAPPDATA "Programs\Aurelius"
if (-not (Test-Path (Join-Path $dest "aurelius.exe"))) {
    Write-Error "Aurelius isn't installed yet: run scripts\install-production.ps1 -Role client"
    exit 1
}
$commitFile = Join-Path $dest "aurelius.commit"
$oldCommit = if (Test-Path $commitFile) { (Get-Content $commitFile -Raw).Trim() } else { "" }
$oldVersion = if (Test-Path "$dest\aurelius.version") { (Get-Content "$dest\aurelius.version" -Raw).Trim() } else { "?" }

$branch = (git rev-parse --abbrev-ref HEAD).Trim()
if ($branch -ne "main") { Write-Error "This checkout is on '$branch', not main. Switch first: git switch main"; exit 1 }
if (git status --porcelain --untracked-files=no) {
    Write-Error "This checkout has local changes. Commit or stash them first (git status)."; exit 1
}

Write-Host "Fetching the latest main ..."
git pull --ff-only
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$newCommit = (git rev-parse HEAD).Trim()
$newVersion = [regex]::Match((Get-Content clients/desktop/Cargo.toml -Raw), '(?m)^version = "([^"]+)"').Groups[1].Value

if ($oldCommit -eq $newCommit -and -not $Force) {
    Write-Host "Already up to date: Aurelius $oldVersion ($($newCommit.Substring(0,7)))."
    exit 0
}
$was = if ($oldCommit) { $oldCommit.Substring(0, [Math]::Min(7, $oldCommit.Length)) } else { "unknown" }
Write-Host "Updating Aurelius $oldVersion ($was) -> $newVersion ($($newCommit.Substring(0,7))) ..."

$env:AURELIUS_ENV = "production"
cargo build --release -p aurelius-desktop -p aurelius-client
if ($LASTEXITCODE -ne 0) { Write-Error "Build failed; the installed app is unchanged."; exit 1 }

$running = @(Get-Process aurelius -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$dest*" })
if ($running.Count -gt 0) {
    $running | Stop-Process
    $running | ForEach-Object { $_.WaitForExit(5000) | Out-Null }
}
Copy-Item ".\target\release\aurelius.exe" (Join-Path $dest "aurelius.exe") -Force
Copy-Item ".\THIRD-PARTY-LICENSES.md" (Join-Path $dest "THIRD-PARTY-LICENSES.md") -Force
Set-Content -Path "$dest\aurelius.version" -Value $newVersion -NoNewline
Set-Content -Path $commitFile -Value $newCommit -NoNewline

if ($running.Count -gt 0) { Start-Process -FilePath (Join-Path $dest "aurelius.exe") -WorkingDirectory $dest }
Write-Host "Aurelius app updated to $newVersion ($($newCommit.Substring(0,7)))."
