# Update the installed Windows server (the background service) to the latest main.
# Only the server programs are rebuilt and replaced: aurelius-daemon.exe, the
# approval bridge (aurelius-bridge.exe) and aurelius-ctl.exe. The app, the
# role, autostart, remote access and the Start menu are not touched, and no
# tests run (use install-production.ps1 for a full, tested install, and
# update-desktop.ps1 for the app).
#
#   scripts/update-server.ps1          pull main, rebuild and replace the server if there is something new
#   scripts/update-server.ps1 -Force   rebuild and replace even when it's up to date
#   scripts/update-server.ps1 -InstallDeps   first install missing build tools with winget
#
# A running server is restarted on the new version, which stops the answers it
# is working on; a server that wasn't running stays off. Your data and the
# pairing stay.
param([switch]$Force, [switch]$InstallDeps)
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
& ./scripts/windows-deps.ps1 -Install:$InstallDeps
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$dest = Join-Path $env:LOCALAPPDATA "Programs\Aurelius"
$daemonExe = Join-Path $dest "aurelius-daemon.exe"
$ctl = Join-Path $dest "aurelius-ctl.exe"
$roleFile = Join-Path $dest "role.txt"
$role = if (Test-Path $roleFile) { (Get-Content $roleFile -Raw).Trim() } else { "both" }
if ($role -eq "client" -or -not (Test-Path $daemonExe)) {
    Write-Error "The Aurelius server isn't installed here: run scripts\install-production.ps1 -Role server (or -Role both)"
    exit 1
}
$commitFile = Join-Path $dest "aurelius-server.commit"
$oldCommit = if (Test-Path $commitFile) { (Get-Content $commitFile -Raw).Trim() } else { "" }

$branch = (git rev-parse --abbrev-ref HEAD).Trim()
if ($branch -ne "main") { Write-Error "This checkout is on '$branch', not main. Switch first: git switch main"; exit 1 }
if (git status --porcelain --untracked-files=no) {
    Write-Error "This checkout has local changes. Commit or stash them first (git status)."; exit 1
}

Write-Host "Fetching the latest main ..."
git pull --ff-only
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$newCommit = (git rev-parse HEAD).Trim()
$newVersion = [regex]::Match((Get-Content Cargo.toml -Raw), '(?m)^version = "([^"]+)"').Groups[1].Value

if ($oldCommit -eq $newCommit -and -not $Force) {
    Write-Host "Already up to date: the server is at $($newCommit.Substring(0,7))."
    exit 0
}
$was = if ($oldCommit) { $oldCommit.Substring(0, [Math]::Min(7, $oldCommit.Length)) } else { "unknown" }
Write-Host "Updating the server $was -> $newVersion ($($newCommit.Substring(0,7))) ..."

$env:AURELIUS_ENV = "production"
cargo build --release -p aurelius-daemon -p aurelius-agent-bridge -p aurelius-client
if ($LASTEXITCODE -ne 0) { Write-Error "Build failed; the installed server is unchanged."; exit 1 }

# The installed programs that are running now.
function Get-Installed([string[]]$Names) {
    @(Get-Process $Names -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$dest*" })
}
$wasRunning = (Get-Installed @("aurelius-daemon")).Count -gt 0

# Windows locks a running program but lets it be renamed: the old one moves
# aside as .old and the new one takes its name, so nothing has to stop first
# (the in-app updater does the same; the server restarts by name).
$programs = @("aurelius-daemon.exe", "aurelius-bridge.exe", "aurelius-ctl.exe")
foreach ($exe in $programs) {
    $path = Join-Path $dest $exe
    $old = "$path.old"
    Remove-Item $old -Force -ErrorAction SilentlyContinue
    if (Test-Path $path) { Move-Item $path $old -Force }
    Copy-Item ".\target\release\$exe" $path -Force
}
Set-Content -Path $commitFile -Value $newCommit -NoNewline

if ($wasRunning) {
    Write-Host "Restarting the running server on the new version ..."
    # `aurelius-ctl restart` needs a signed-in board member; `shutdown` goes over
    # the internal channel and needs none. Whatever is still running after 10 s
    # is stopped, then the new program is started (as autostart does at sign-in).
    $ErrorActionPreference = "Continue"
    & $ctl shutdown 2>$null | Out-Null
    $ErrorActionPreference = "Stop"
    $deadline = (Get-Date).AddSeconds(10)
    while ((Get-Installed @("aurelius-daemon")).Count -gt 0 -and (Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 200
    }
    $left = Get-Installed @("aurelius-daemon")
    $left | Stop-Process -Force -ErrorAction SilentlyContinue
    $left | ForEach-Object { $_.WaitForExit(5000) | Out-Null }
    Start-Process -FilePath $daemonExe -WorkingDirectory $dest
    Write-Host "The new server is started. If an app is open and does not reconnect, restart it."
} else {
    Write-Host "The server wasn't running; it starts on the new version next time."
}
foreach ($exe in $programs) { Remove-Item (Join-Path $dest "$exe.old") -Force -ErrorAction SilentlyContinue }
Write-Host "Aurelius server updated to $newVersion ($($newCommit.Substring(0,7)))."
