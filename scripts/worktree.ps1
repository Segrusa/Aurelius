# Create and remove git worktrees, each with its own cargo build folder.
# Worktrees live in <main checkout>/target/wt-<name> and always build into
# <worktree>/target. Sharing one build folder between branches makes cargo
# treat crates of another branch as up to date (false compile errors, the
# wrong xtask.exe), so never point CARGO_TARGET_DIR anywhere else.
#   scripts/worktree.ps1 new <name> [-Base main] [-Branch <branch>]
#       git worktree target/wt-<name> on <branch> (default feat/<name>; created
#       from -Base when it doesn't exist yet) and CARGO_TARGET_DIR for it
#   scripts/worktree.ps1 env <name>
#       print the CARGO_TARGET_DIR lines for the worktree (PowerShell and bash)
#   scripts/worktree.ps1 remove <name> [-Force]
#       delete the worktree's build folder, then the worktree, then
#       git worktree prune; the branch stays. -Force also drops uncommitted changes.
param(
    [Parameter(Mandatory, Position = 0)][ValidateSet("new", "env", "remove")][string]$Command,
    [Parameter(Mandatory, Position = 1)][ValidatePattern('^[A-Za-z0-9][A-Za-z0-9._-]*$')][string]$Name,
    [string]$Base = "main",
    [string]$Branch,
    [switch]$Force
)
$ErrorActionPreference = "Stop"

# The main checkout, also when this script runs from inside a worktree.
$common = git -C $PSScriptRoot rev-parse --path-format=absolute --git-common-dir
if ($LASTEXITCODE -ne 0) { throw "Not inside the Aurelius git repository." }
$root = [IO.Path]::GetFullPath((Split-Path -Parent $common))
$worktree = Join-Path $root "target\wt-$Name"
$targetDir = Join-Path $worktree "target"

function Same-Path([string]$a, [string]$b) {
    if (-not $a -or -not $b) { return $false }
    [IO.Path]::GetFullPath($a).TrimEnd('\', '/') -ieq [IO.Path]::GetFullPath($b).TrimEnd('\', '/')
}

function Test-Worktree {
    $paths = git -C $root worktree list --porcelain | Where-Object { $_ -like "worktree *" } |
        ForEach-Object { $_.Substring(9) }
    foreach ($p in $paths) { if (Same-Path $p $worktree) { return $true } }
    $false
}

function Show-Env {
    $bashPath = $targetDir -replace '\\', '/'
    Write-Host "Build folder for wt-${Name}: $targetDir"
    Write-Host "  PowerShell: `$env:CARGO_TARGET_DIR = '$targetDir'"
    Write-Host "  bash:       export CARGO_TARGET_DIR='$bashPath'"
    Write-Host "  xtask:      cargo run -p xtask -- <args>   (run from $worktree)"
}

# Warn when CARGO_TARGET_DIR points somewhere other than this worktree's build
# folder, above all at the shared target of the main checkout.
function Warn-TargetDir {
    $current = $env:CARGO_TARGET_DIR
    if (-not $current -or (Same-Path $current $targetDir)) { return }
    $what = if (Same-Path $current (Join-Path $root "target")) { "the SHARED build folder of the main checkout" } else { "another folder" }
    Write-Warning "CARGO_TARGET_DIR points to $what ($current), not to $targetDir. Builds there mix branches; set it to this worktree's target."
}

switch ($Command) {
    "new" {
        if (Test-Path -LiteralPath $worktree) { throw "$worktree already exists." }
        if (-not $Branch) { $Branch = "feat/$Name" }
        git -C $root show-ref --verify --quiet "refs/heads/$Branch"
        if ($LASTEXITCODE -eq 0) {
            Write-Host "Checking out existing branch $Branch in $worktree"
            git -C $root worktree add $worktree $Branch
        } else {
            Write-Host "Creating branch $Branch from $Base in $worktree"
            git -C $root worktree add -b $Branch $worktree $Base
        }
        if ($LASTEXITCODE -ne 0) { throw "git worktree add failed." }
        Warn-TargetDir
        # Takes effect for the rest of this PowerShell session (a script shares
        # the caller's process). Shells that started this script as a separate
        # process (bash, pwsh -File) must use the printed line.
        $env:CARGO_TARGET_DIR = $targetDir
        Show-Env
    }
    "env" {
        if (-not (Test-Worktree)) { Write-Warning "$worktree is not a worktree (yet)." }
        Warn-TargetDir
        Show-Env
    }
    "remove" {
        if (-not (Test-Worktree)) { throw "$worktree is not a registered worktree; nothing removed." }
        # Leave the folder first: Windows can't delete the current directory.
        $here = (Get-Location).ProviderPath
        if ($here.StartsWith($worktree, [StringComparison]::OrdinalIgnoreCase)) { Set-Location $root }
        # Only this worktree's own build folder; never cargo clean, which would
        # wipe the build folder cargo resolves to (possibly the shared one).
        if (Test-Path -LiteralPath $targetDir) {
            Write-Host "Deleting $targetDir"
            Remove-Item -LiteralPath $targetDir -Recurse -Force
        }
        $removeArgs = @("worktree", "remove")
        if ($Force) { $removeArgs += "--force" }
        git -C $root @removeArgs $worktree
        if ($LASTEXITCODE -ne 0) { throw "git worktree remove failed (uncommitted changes? use -Force to drop them). Build folder already deleted." }
        git -C $root worktree prune
        if (Same-Path $env:CARGO_TARGET_DIR $targetDir) { Remove-Item Env:CARGO_TARGET_DIR }
        Write-Host "Removed wt-$Name (branch kept)."
    }
}
