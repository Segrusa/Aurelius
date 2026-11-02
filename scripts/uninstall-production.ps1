# Uninstall the PRODUCTION app: programs, Start menu shortcut and start at sign-in.
# Your data is kept unless you ask for it explicitly:
#   -RemoveData   also delete %LOCALAPPDATA%\Aurelius (database, files, backups, logs)
#                 and the API keys Aurelius stored in Windows Credential Manager.
# Project folders (Documents\Aurelius\...) are yours and are never deleted.
# The release signing key (developer machines) is never deleted.
param([switch]$RemoveData, [switch]$Yes)
$ErrorActionPreference = "Stop"

$dest = Join-Path $env:LOCALAPPDATA "Programs\Aurelius"
$data = Join-Path $env:LOCALAPPDATA "Aurelius"
$menu = Join-Path ([Environment]::GetFolderPath("Programs")) "Aurelius.lnk"

Write-Host "This removes:"
Write-Host "  programs        $dest"
Write-Host "  shortcut        $menu"
Write-Host "  aurelius:// link registration (current user)"
Write-Host "  start at sign-in (if on)"
if ($RemoveData) {
    Write-Host "  YOUR DATA       $data  (chats, tasks, memory, backups)"
    Write-Host "  API keys        Credential Manager entries 'Aurelius/...'"
} else {
    Write-Host "Kept: your data in $data and your API keys (use -RemoveData to delete them too)."
}
Write-Host "Never touched: your project folders in Documents\Aurelius."
if (-not $Yes) {
    $answer = Read-Host "Continue? (yes/no)"
    if ($answer -ne "yes") { Write-Host "Nothing changed."; exit 0 }
}

# Stop the app and its background service.
Get-Process aurelius, aurelius-daemon -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$dest*" } | Stop-Process -Force
if (Test-Path "$dest\aurelius-ctl.exe") { & "$dest\aurelius-ctl.exe" shutdown 2>$null | Out-Null }
Start-Sleep -Milliseconds 800

Remove-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" -Name "Aurelius" -ErrorAction SilentlyContinue
if (Test-Path $menu) { Remove-Item $menu -Force }
Remove-Item "HKCU:\Software\Classes\aurelius" -Recurse -Force -ErrorAction SilentlyContinue
if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
Write-Host "Programs removed."

if ($RemoveData) {
    if (Test-Path $data) { Remove-Item $data -Recurse -Force; Write-Host "Data removed." }
    $targets = cmdkey /list | Select-String "target=(Aurelius/[^\s]+)" | ForEach-Object { $_.Matches[0].Groups[1].Value }
    foreach ($t in $targets) {
        if ($t -eq "Aurelius/release-signing-key") { continue }
        cmdkey /delete:$t | Out-Null
    }
    Write-Host "API keys removed ($(@($targets | Where-Object { $_ -ne 'Aurelius/release-signing-key' }).Count))."
}
Write-Host "Aurelius is uninstalled."
