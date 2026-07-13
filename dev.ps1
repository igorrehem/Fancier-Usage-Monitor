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
$dev = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\Launch-VsDevShell.ps1"
& $dev -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
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
