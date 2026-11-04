# Run the tests of one part of Aurelius. The server and the desktop app are
# released separately; each release runs only its own part plus the shared
# crates (the contract between them).
#   scripts/test.ps1 -Part server     the shared crates + everything of the background server
#   scripts/test.ps1 -Part desktop    the shared crates + the desktop app
#   scripts/test.ps1 -Part shared     only the shared crates (protocol, client, domain, platform)
#   scripts/test.ps1                  everything (-Part all)
# -List prints the packages without running anything.
param(
    [ValidateSet("server", "desktop", "shared", "all")][string]$Part = "all",
    [switch]$List
)
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

# What both sides compile against: a change here can break either one.
$shared = @("aurelius-domain", "aurelius-protocol", "aurelius-host", "aurelius-client")
$server = @(
    "aurelius-storage", "aurelius-application", "aurelius-runtime", "aurelius-tools", "aurelius-daemon", "aurelius-remote-gateway", "aurelius-agent-bridge", "xtask"
)
# clients\desktop still builds until it is removed; the app's own tests run at the end of this script.
$desktop = @("aurelius-desktop")

$packages = switch ($Part) {
    "server" { $shared + $server }
    "desktop" { $shared + $desktop }
    "shared" { $shared }
    default { $shared + $server + $desktop }
}
if ($List) { $packages; return }

$cargoArgs = @($packages | ForEach-Object { "-p"; $_ })
Write-Host "Testing $Part ($($packages.Count) packages)..."
# Tests run in their own (dev) environment and build folder, so a running
# production or dev app never locks the files.
cargo test --target-dir target/work @cargoArgs
exit $LASTEXITCODE

# The app and its shared TypeScript.
if ($Part -in @("desktop", "all")) {
    npx -y pnpm@12.16.1 install --frozen-lockfile
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    npx -y pnpm@12.16.1 lint
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    npx -y pnpm@12.16.1 test
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
