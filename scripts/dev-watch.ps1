# Rebuild and restart the DEV app whenever you save a source file (Ctrl+C stops).
# Real hot reload of Rust code isn't possible; this is the next best thing.
# Only desktop code changed: just the window restarts (the background service
# and its running work stay). Anything else changed: service and app restart.
param([int]$QuietMs = 700)
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
$root = Get-Location
$env:AURELIUS_ENV = "dev"
$debug = Join-Path $root "target\debug"

function Restart-App([bool]$WithService) {
    if ($WithService) {
        & (Join-Path $PSScriptRoot "dev.ps1")
        return
    }
    # electron-vite reloads the page by itself; this only opens a closed app again.
    & (Join-Path $PSScriptRoot "dev.ps1") -Part desktop
}

$watcher = New-Object IO.FileSystemWatcher "$root", "*.*"
$watcher.IncludeSubdirectories = $true
$watcher.NotifyFilter = [IO.NotifyFilters]::LastWrite -bor [IO.NotifyFilters]::FileName
# Server and shared crates live in crates\, the app in clients\ts\ and clients\electron\.
$relevant = {
    param($p)
    $p -match '\.(rs|toml|ico|png|ts|tsx|css)$' -and $p -notmatch '\\target\\' -and
        ($p -match '^(crates|clients\\ts|clients\\electron)\\' -or $p -eq 'Cargo.toml')
}

Write-Host "Starting the dev app; watching crates\ and clients\ for changes (Ctrl+C stops)."
Restart-App $true
while ($true) {
    $first = $watcher.WaitForChanged([IO.WatcherChangeTypes]::All, 60000)
    if ($first.TimedOut -or -not (& $relevant $first.Name)) { continue }
    $changed = @($first.Name)
    # Wait until saving has settled (editors often write several files).
    while ($true) {
        $next = $watcher.WaitForChanged([IO.WatcherChangeTypes]::All, $QuietMs)
        if ($next.TimedOut) { break }
        if (& $relevant $next.Name) { $changed += $next.Name }
    }
    $desktopOnly = -not ($changed | Where-Object { $_ -notlike "clients\*" })
    $what = if ($desktopOnly) { "app" } else { "service and app" }
    Write-Host ("{0:HH:mm:ss} {1} changed - restarting the {2}" -f (Get-Date), ($changed | Select-Object -Unique | Select-Object -First 3) -join ", ", $what)
    Restart-App (-not $desktopOnly)
}
