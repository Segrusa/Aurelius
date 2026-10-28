# Publish a new version of the server or the desktop app: bump its version,
# test its part, commit, tag and push. The two are released separately, each
# with its own version, tag, changelog and zip; they work together as long as
# their protocol major matches.
#   scripts/release.ps1 -Product server  -Version 0.6.1   tag server-v0.6.1  (aurelius-daemon + aurelius-ctl + aurelius-bridge + aurelius-mcp, and the aurelius-mcp Docker image)
#   scripts/release.ps1 -Product desktop -Version 0.6.1   tag desktop-v0.6.1 (aurelius.exe)
# GitHub Actions (.github/workflows/release-<product>.yml) then builds, signs
# and publishes the release; installed apps offer it as an update.
# -Breaking: required when a server release raises the protocol major (older
# apps then become read-only until they are updated too).
# -SkipTests: release without running the tests here (the workflow still builds).
param(
    [Parameter(Mandatory = $true)][ValidateSet("server", "desktop")][string]$Product,
    [Parameter(Mandatory = $true)][string]$Version,
    [switch]$Breaking,
    [switch]$SkipTests
)
$ErrorActionPreference = "Stop"
# Windows PowerShell 5.1 turns a native command's redirected stderr into a
# terminating error under "Stop" (cargo prints progress there); judge such
# commands by their exit code instead.
function Invoke-Native([scriptblock]$command) {
    $ErrorActionPreference = "Continue"
    & $command 2>$null
}
Set-Location (Split-Path -Parent $PSScriptRoot)
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Use a version like 0.6.1" }
if (git status --porcelain) { throw "Commit or stash your changes first." }
$gh = if (Test-Path "$env:ProgramFiles\GitHub CLI\gh.exe") { "$env:ProgramFiles\GitHub CLI\gh.exe" } else { "gh" }
& $gh auth status *> $null
if ($LASTEXITCODE -ne 0) { throw "Sign in to GitHub first: gh auth login" }

# @(...): a single path must stay a list, or splatting it below passes its characters.
$crates = @(switch ($Product) {
    "server" { @("crates/daemon/Cargo.toml") }
    # The desktop crate takes the workspace version (root Cargo.toml), which the workflow checks.
    "desktop" { @("Cargo.toml") }
})
$changelogFile = switch ($Product) {
    "server" { "docs/changelog/server.md" }
    "desktop" { "docs/changelog/desktop.md" }
}
$prefix = switch ($Product) { "server" { "server-v" } "desktop" { "desktop-v" } }
$tag = "$prefix$Version"
$name = switch ($Product) { "server" { "Aurelius server" } "desktop" { "Aurelius app" } }

# Release notes: the "## v<version>" section of the product's changelog (required).
$changelog = Get-Content $changelogFile -Raw -Encoding UTF8 -ErrorAction SilentlyContinue
$section = [regex]::Match("$changelog", "(?ms)^## v$([regex]::Escape($Version))\s*\r?\n(.*?)(?=^## v|\z)")
if (-not $section.Success -or -not $section.Groups[1].Value.Trim()) { throw "Add a '## v$Version' section with the release notes to $changelogFile first." }
$notes = $section.Groups[1].Value.Trim()

# The protocol is the contract between server and app. Since this product's
# last release: did the shared crates change, and did the major go up?
$last = Invoke-Native { git describe --tags --abbrev=0 --match "$prefix*" }
if (-not $last) { $last = Invoke-Native { git describe --tags --abbrev=0 --match "v*" } }
$sharedPaths = @("crates/protocol", "crates/client", "crates/domain", "crates/host")
$sharedChanged = $true
if ($last) {
    git diff --quiet $last HEAD -- @sharedPaths
    $sharedChanged = $LASTEXITCODE -ne 0
}
# Since v0.7 the major lives in rpc.rs (`pub const PROTOCOL_MAJOR: u16 = N;`);
# tags before that kept it in lib.rs, so fall back to the old pattern there.
function Get-Major([string]$text) {
    $m = [regex]::Match("$text", 'pub const PROTOCOL_MAJOR: u16 = (\d+);')
    if (-not $m.Success) { $m = [regex]::Match("$text", 'PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion \{ major: (\d+)') }
    if (-not $m.Success) { throw "No protocol major found" }
    [int]$m.Groups[1].Value
}
function Get-MajorAt([string]$rev) {
    $rpc = (git show "${rev}:crates/protocol/src/rpc.rs" 2>$null) -join "`n"
    if ($rpc -match 'PROTOCOL_MAJOR: u16 = \d+') { return Get-Major $rpc }
    Get-Major ((git show "${rev}:crates/protocol/src/lib.rs") -join "`n")
}
$majorNow = Get-Major (Get-Content crates/protocol/src/rpc.rs -Raw)
$majorThen = if ($last) { Get-MajorAt $last } else { $majorNow }
if ($majorNow -ne $majorThen -and -not $Breaking) {
    throw "The protocol major went from $majorThen to $majorNow since ${last}: apps and servers of the old major become read-only together. Release both sides and pass -Breaking."
}

foreach ($file in $crates) {
    $toml = Get-Content $file -Raw
    $current = [regex]::Match($toml, '(?m)^version = "([^"]+)"').Groups[1].Value
    if (-not $current) { throw "$file has no own version (version.workspace?)" }
    if ([version]$Version -le [version]$current) { throw "New version $Version must be higher than $current ($file)" }
    $toml = [regex]::Replace($toml, '(?m)^version = "[^"]+"', "version = `"$Version`"", 1)
    [IO.File]::WriteAllText((Resolve-Path $file), $toml)
}

Invoke-Native { cargo update --workspace --offline }
if ($LASTEXITCODE -ne 0) { Invoke-Native { cargo update --workspace } }
if ($LASTEXITCODE -ne 0) { git checkout -- @crates Cargo.lock; throw "cargo update failed; nothing was released." }
# Only this product's tests, unless the shared crates changed: then the other
# side compiles against something new too, so everything runs.
$part = if ($sharedChanged) { "all" } else { $Product }
if ($sharedChanged) { Write-Host "The shared crates (protocol, client, domain, host) changed since ${last}; testing the other side too." }
if ($SkipTests) { Write-Host "Skipping the tests (-SkipTests)." } else {
    ./scripts/test.ps1 -Part $part
    if ($LASTEXITCODE -ne 0) { git checkout -- @crates Cargo.lock; throw "Tests failed; nothing was released." }
}

git add @crates Cargo.lock
# The commit message carries the notes: the workflow publishes them on GitHub
# and the app shows them under "What's new".
# Through a file: Windows PowerShell 5.1 splits a multi-line argument with quotes.
$messageFile = Join-Path ([IO.Path]::GetTempPath()) "aurelius-release-message.txt"
[IO.File]::WriteAllText($messageFile, "Release $tag`n`n$notes`n", (New-Object Text.UTF8Encoding $false))
git commit -F $messageFile
$committed = $LASTEXITCODE
Remove-Item $messageFile
if ($committed -ne 0) { throw "git commit failed; nothing was tagged." }
git tag -a $tag -m "$name $Version"
git push origin HEAD
git push origin $tag
$workflow = "release-$Product"
Write-Host "Tagged $tag. GitHub Actions is building the signed release:"
& $gh run list --repo Segrusa/Aurelius --workflow $workflow --limit 1
