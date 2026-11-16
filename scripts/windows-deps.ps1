# Check (and with -Install, install) what building Aurelius on Windows needs:
#   Git, Rust (rustup + the toolchain from rust-toolchain.toml), the Visual
#   Studio 2022 Build Tools with the C++ workload (the MSVC linker and the
#   Windows SDK) and Node.js (the major version in .node-version).
# Claude Code is not needed for the build; the script only says when it is missing.
# CMake, NASM and LLVM are not needed on Windows: aws-lc-sys builds with cc and
# ships prebuilt assembly, and nothing runs bindgen for this target.
#
#   scripts/windows-deps.ps1             check only; exit 1 and show what to run when something is missing
#   scripts/windows-deps.ps1 -Install    install what is missing with winget (the Build Tools ask for admin rights)
#
# install-production.ps1 and update-desktop.ps1 run this first (-InstallDeps passes -Install).
param([switch]$Install)
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

$nodeMajor = [int]((Get-Content .node-version -Raw).Trim() -replace '^v', '' -replace '\..*$', '')
$toolchain = [regex]::Match((Get-Content rust-toolchain.toml -Raw), '(?m)^channel = "([^"]+)"').Groups[1].Value
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"

# winget and the installers change PATH in the registry only; read it again for this session.
function Update-SessionPath {
    $machine = [Environment]::GetEnvironmentVariable("Path", "Machine")
    $user = [Environment]::GetEnvironmentVariable("Path", "User")
    $env:Path = "$machine;$user"
    if ((Test-Path $cargoBin) -and ($env:Path -notlike "*$cargoBin*")) { $env:Path = "$cargoBin;$env:Path" }
}

function Test-Command([string]$Name) { [bool](Get-Command $Name -ErrorAction SilentlyContinue) }

function Test-Msvc {
    if (-not (Test-Path $vswhere)) { return $false }
    $found = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    return [bool]$found
}

function Get-NodeMajor {
    if (-not (Test-Command node)) { return 0 }
    $version = (& node --version) -replace '^v', ''
    return [int]($version -split '\.')[0]
}

function Test-Toolchain {
    if (-not (Test-Command rustup)) { return $false }
    return [bool](& rustup toolchain list | Where-Object { $_ -like "$toolchain-*" })
}

# Each missing piece with its winget package and the extra winget arguments.
function Get-Missing {
    $missing = @()
    if (-not (Test-Command git)) {
        $missing += [pscustomobject]@{ Name = "Git"; Id = "Git.Git"; Args = @() }
    }
    if (-not (Test-Command rustup)) {
        $missing += [pscustomobject]@{ Name = "Rust (rustup)"; Id = "Rustlang.Rustup"; Args = @() }
    }
    if (-not (Test-Msvc)) {
        $missing += [pscustomobject]@{
            Name = "Visual Studio 2022 Build Tools (C++)"; Id = "Microsoft.VisualStudio.2022.BuildTools"
            Args = @("--override", "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended")
        }
    }
    $node = Get-NodeMajor
    if ($node -lt $nodeMajor) {
        $have = if ($node -eq 0) { "missing" } else { "have $node" }
        $missing += [pscustomobject]@{ Name = "Node.js $nodeMajor ($have)"; Id = "OpenJS.NodeJS.LTS"; Args = @() }
    }
    return $missing
}

Update-SessionPath
$missing = @(Get-Missing)

if ($missing.Count -gt 0 -and -not $Install) {
    Write-Host "Missing for the build:"
    foreach ($m in $missing) { Write-Host "  $($m.Name)" }
    Write-Host "Install them with:"
    foreach ($m in $missing) {
        $extra = if ($m.Args.Count -gt 0) { " $($m.Args[0]) `"$($m.Args[1])`"" } else { "" }
        Write-Host "  winget install --id $($m.Id) -e$extra"
    }
    Write-Host "or let the script install them: scripts\install-production.ps1 -InstallDeps (or scripts\windows-deps.ps1 -Install)."
    exit 1
}

if ($missing.Count -gt 0) {
    if (-not (Test-Command winget)) {
        Write-Error "winget is not available. Install 'App Installer' from the Microsoft Store, or install the missing pieces yourself."
        exit 1
    }
    foreach ($m in $missing) {
        Write-Host "Installing $($m.Name) ..."
        winget install --id $m.Id -e --source winget --accept-package-agreements --accept-source-agreements @($m.Args)
        # -1978335189: already installed, no newer version. Anything else that is not 0 failed.
        if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne -1978335189) {
            Write-Error "Installing $($m.Name) failed (winget exit $LASTEXITCODE)."
            exit 1
        }
    }
    Update-SessionPath
    $still = @(Get-Missing)
    if ($still.Count -gt 0) {
        Write-Host "Still missing after the install: $(($still | ForEach-Object { $_.Name }) -join ', ')."
        Write-Host "Open a new PowerShell window (so PATH is read again) and run the script again."
        exit 1
    }
}

# rustup does not always install the pinned toolchain by itself; do it once here.
if (-not (Test-Toolchain)) {
    Write-Host "Installing the Rust toolchain $toolchain ..."
    & rustup toolchain install $toolchain --profile minimal --component rustfmt --component clippy
    if ($LASTEXITCODE -ne 0) { Write-Error "Installing Rust $toolchain failed."; exit 1 }
}

if (-not (Test-Command claude)) {
    Write-Host "Note: Claude Code is not installed. Aurelius needs it, installed and logged in: https://claude.com/claude-code"
}
Write-Host "Build dependencies OK (Rust $toolchain, MSVC, Node $(Get-NodeMajor), Git)."
exit 0
