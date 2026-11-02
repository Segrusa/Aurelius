# Build a release and install it as the PRODUCTION app.
# Installed to %LOCALAPPDATA%\Programs\Aurelius with a Start menu shortcut "Aurelius".
# Data: %LOCALAPPDATA%\Aurelius (your real data) · own background service · own credentials.
#
# -Role:
#   both    the app and the server on this computer (default, as before)
#   server  only the background service: starts at sign-in, remote access on;
#           apps on other computers and phones connect to it over Tailscale
#   client  only the app; it connects to a server elsewhere (Settings → Servers)
#
# -SkipTests: install without running the tests first.
param(
    [switch]$NoLaunch,
    [switch]$SkipTests,
    [ValidateSet("both", "server", "client")][string]$Role = "both"
)
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
# Tests run in their own (dev) environment; release builds are production.
# Only the part this role installs (the server and the app are released apart).
$part = switch ($Role) { "server" { "server" } "client" { "desktop" } default { "all" } }
if ($SkipTests) { Write-Host "Skipping the tests (-SkipTests)." } else {
    ./scripts/test.ps1 -Part $part
    if ($LASTEXITCODE -ne 0) { Write-Error "Tests failed; production is not updated."; exit 1 }
}
$env:AURELIUS_ENV = "production"
# @(...) keeps a one-item list a list; splatting a bare string passes its characters.
$packages = @(switch ($Role) {
    "server" { @("-p", "aurelius-daemon", "-p", "aurelius-client", "-p", "aurelius-agent-bridge") }
    "client" { @("-p", "aurelius-desktop", "-p", "aurelius-client") }
    default { @("--workspace") }
})
cargo build --release @packages
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$dest = Join-Path $env:LOCALAPPDATA "Programs\Aurelius"
New-Item -ItemType Directory -Force $dest | Out-Null

# Stop the running production app and its background service (work resumes after restart).
Get-Process aurelius -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$dest*" } | Stop-Process
if (Test-Path "$dest\aurelius-daemon.exe") {
    & "$dest\aurelius-ctl.exe" shutdown 2>$null | Out-Null
    Start-Sleep -Milliseconds 800
}

$programs = switch ($Role) {
    # The daemon starts aurelius-bridge (Claude Code's approval MCP server) from its own folder.
    "server" { @("aurelius-daemon.exe", "aurelius-ctl.exe", "aurelius-bridge.exe") }
    "client" { @("aurelius.exe", "aurelius-ctl.exe") }
    default { @("aurelius.exe", "aurelius-daemon.exe", "aurelius-ctl.exe", "aurelius-bridge.exe") }
}
foreach ($exe in $programs) {
    Copy-Item ".\target\release\$exe" (Join-Path $dest $exe) -Force
}
# A different role than before: the programs it no longer has go.
$unused = @("aurelius.exe", "aurelius-daemon.exe", "aurelius-bridge.exe") | Where-Object { $programs -notcontains $_ }
foreach ($exe in $unused) { Remove-Item (Join-Path $dest $exe) -Force -ErrorAction SilentlyContinue }
Copy-Item ".\THIRD-PARTY-LICENSES.md" (Join-Path $dest "THIRD-PARTY-LICENSES.md") -Force
# The app reads this: a client install never starts a background service of its own.
Set-Content -Path (Join-Path $dest "role.txt") -Value $Role -NoNewline
# The app's own version, for the service that installs app updates (the app
# also writes it when it starts).
$appVersion = Join-Path $dest "aurelius.version"
if ($programs -contains "aurelius.exe") {
    $version = [regex]::Match((Get-Content clients/desktop/Cargo.toml -Raw), '(?m)^version = "([^"]+)"').Groups[1].Value
    Set-Content -Path $appVersion -Value $version -NoNewline
} else {
    Remove-Item $appVersion -Force -ErrorAction SilentlyContinue
}

$menu = Join-Path ([Environment]::GetFolderPath("Programs")) "Aurelius.lnk"
if ($Role -eq "server") {
    if (Test-Path $menu) { Remove-Item $menu -Force }
    Remove-Item "HKCU:\Software\Classes\aurelius" -Recurse -Force -ErrorAction SilentlyContinue
} else {
    $shell = New-Object -ComObject WScript.Shell
    $link = $shell.CreateShortcut($menu)
    $link.TargetPath = Join-Path $dest "aurelius.exe"
    $link.WorkingDirectory = $dest
    $link.Description = "Aurelius"
    $link.Save()

    # aurelius:// links open this app (current user only, no admin rights). The
    # link arrives as the first argument: aurelius.exe "aurelius://<host>/ticket/42".
    $scheme = "HKCU:\Software\Classes\aurelius"
    New-Item -Path "$scheme\shell\open\command" -Force | Out-Null
    Set-ItemProperty -Path $scheme -Name "(default)" -Value "URL:Aurelius link"
    Set-ItemProperty -Path $scheme -Name "URL Protocol" -Value ""
    Set-ItemProperty -Path "$scheme\shell\open\command" -Name "(default)" -Value ('"{0}" "%1"' -f (Join-Path $dest "aurelius.exe"))
}

$ctl = Join-Path $dest "aurelius-ctl.exe"
switch ($Role) {
    "server" {
        & $ctl autostart on
        # Starts the service (ctl starts it when it isn't running) and turns on remote access.
        & $ctl remote on
        Write-Host ""
        Write-Host "Server installed in $dest. It starts at sign-in; keep this computer signed in and awake, with Tailscale on."
        Write-Host "Pair a computer:  $ctl remote pair   (paste the 'computer' text into Aurelius on the other computer)"
        Write-Host "Then allow it:    $ctl remote allow --owner"
    }
    "client" {
        Remove-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" -Name "Aurelius" -ErrorAction SilentlyContinue
        Write-Host "App installed in $dest (Start menu: Aurelius). Connect it to your server in Settings → Servers."
        if (-not $NoLaunch) { Start-Process -FilePath (Join-Path $dest "aurelius.exe") -WorkingDirectory $dest }
    }
    default {
        Write-Host "Production installed in $dest (Start menu: Aurelius). Remove it with scripts\uninstall-production.ps1."
        if (-not $NoLaunch) { Start-Process -FilePath (Join-Path $dest "aurelius.exe") -WorkingDirectory $dest }
    }
}
