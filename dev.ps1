# dev.ps1 - build/run helper for Claude Code Usage Monitor
# Imports the VS Build Tools (MSVC) environment, then runs cargo.
# This is a Cargo workspace (ccum-core lib + ccum-windows bin); `build`/`run`/`release` default
# to the ccum-windows binary (the workspace's `default-members`), same as before the workspace
# split. `test` explicitly runs across the whole workspace so ccum-core's tests aren't skipped.
# Usage:
#   .\dev.ps1            # cargo build (debug) + run
#   .\dev.ps1 build      # cargo build (debug)
#   .\dev.ps1 run        # cargo run (debug)
#   .\dev.ps1 release    # cargo build --release
#   .\dev.ps1 test       # cargo test --workspace
#   .\dev.ps1 <anything> # passed straight to cargo

param([Parameter(ValueFromRemainingArguments = $true)] [string[]] $Args)

$ErrorActionPreference = 'Stop'

# Import the MSVC environment. We do NOT call Launch-VsDevShell.ps1 directly: that wrapper locates
# the VS install by invoking `vswhere` *by bare name*, which fails when the VS Installer directory
# isn't on PATH (the common case), leaving link.exe unavailable and cargo hung. Instead we resolve
# the install path via vswhere's full path, then hand it to Enter-VsDevShell explicitly.
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $vswhere)) { throw "vswhere.exe nao encontrado em '$vswhere' - VS Build Tools instalado?" }
$vsPath = & $vswhere -latest -products * `
    -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
    -property installationPath
if (-not $vsPath) { throw "Nenhuma instalacao do VS com as VC Tools (MSVC) encontrada." }
$devShell = Join-Path $vsPath "Common7\Tools\Microsoft.VisualStudio.DevShell.dll"
Import-Module $devShell
# Enter-VsDevShell may still print a harmless "'vswhere.exe' is not recognized" line to stderr from
# an internal probe; because we pass -VsInstallPath the environment is set correctly regardless.
Enter-VsDevShell -VsInstallPath $vsPath -SkipAutomaticLocation `
    -DevCmdArguments '-arch=amd64 -host_arch=amd64' | Out-Null
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
Set-Location $PSScriptRoot

if (-not $Args -or $Args.Count -eq 0) { $Args = @('build') }

switch ($Args[0]) {
    'run'     { cargo run }
    'build'   { cargo build }
    'release' { cargo build --release }
    'test'    { cargo test --workspace }
    default   { cargo @Args }
}
