# Start Aurelius in the DEV environment (debug build).
# Data: %LOCALAPPDATA%\Aurelius-Dev · own background service · own credentials.
# Dev and production never share anything; a DEV badge shows in the title bar.
#   scripts/dev.ps1                 rebuild and restart both
#   scripts/dev.ps1 -Part server    only the background service; the app window
#                                   stays open and reconnects by itself
#   scripts/dev.ps1 -Part desktop   only the app; the service keeps running
param([ValidateSet("all", "server", "desktop")][string]$Part = "all")
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
$env:AURELIUS_ENV = "dev"

$debug = Join-Path (Get-Location) "target\debug"
$ctl = "$debug\aurelius-ctl.exe"
function Stop-Daemon {
    if (Test-Path $ctl) { & $ctl shutdown 2>$null | Out-Null }
    Start-Sleep -Milliseconds 500
    Get-Process aurelius-daemon -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$debug*" } | Stop-Process -Force
}

if ($Part -eq "server") {
    # The open app starts a service as soon as it loses one, which would lock
    # the program being built. So the running one keeps going while the build
    # runs: Windows lets a running program be renamed, not replaced.
    Remove-Item "$debug\aurelius-daemon.exe.old*", "$debug\deps\aurelius_daemon-*.exe.old*" -Force -ErrorAction SilentlyContinue
    $stamp = Get-Date -Format "HHmmss"
    foreach ($exe in @(Get-Item "$debug\aurelius-daemon.exe", "$debug\deps\aurelius_daemon-*.exe" -ErrorAction SilentlyContinue)) {
        Rename-Item $exe.FullName "$($exe.Name).old$stamp" -ErrorAction SilentlyContinue
    }
    cargo build -p aurelius-daemon -p aurelius-client
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Stop-Daemon
    # An open window reconnects and starts the new service; otherwise start the app.
    $open = Get-Process electron -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$PWD*" }
    if (-not $open) { Start-Process -FilePath "npx" -ArgumentList "-y", "pnpm@12.16.1", "--filter", "@aurelius/electron", "dev" -NoNewWindow }
    return
}

# Stop what gets rebuilt first: Windows locks running programs, so the build
# could not replace them. Work resumes after the restart.
Get-Process electron -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$PWD*" } | Stop-Process -Force
if ($Part -eq "all") { Stop-Daemon }
$packages = if ($Part -eq "desktop") { @() } else { @("--workspace", "--exclude", "aurelius-desktop") }
if ($packages.Count -gt 0) { cargo build @packages }
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$env:AURELIUS_DAEMON_PATH = "$debug\aurelius-daemon.exe"
Start-Process -FilePath "npx" -ArgumentList "-y", "pnpm@12.16.1", "--filter", "@aurelius/electron", "dev" -NoNewWindow
