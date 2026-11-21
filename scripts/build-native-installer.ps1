[CmdletBinding()]
param(
    [string]$MakeNsis,
    [switch]$TestInstaller,
    [switch]$StandardProfile
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$buildFolder = if ($StandardProfile) { 'target/native-package' } else { 'target/native-size' }
$target = Join-Path $root $buildFolder
$payload = Join-Path $target 'release'
if (!$MakeNsis) {
    $command = Get-Command makensis.exe -ErrorAction SilentlyContinue
    $MakeNsis = if ($command) { $command.Source } else { Join-Path $env:LOCALAPPDATA 'tauri/NSIS/makensis.exe' }
}
if (!(Test-Path -LiteralPath $MakeNsis -PathType Leaf)) {
    throw 'Set -MakeNsis to the full path of makensis.exe.'
}
$MakeNsis = (Resolve-Path -LiteralPath $MakeNsis).Path

$previousOptimization = $env:CARGO_PROFILE_RELEASE_OPT_LEVEL
$previousPanic = $env:CARGO_PROFILE_RELEASE_PANIC
Push-Location $root
try {
    $metadata = cargo metadata --no-deps --format-version 1 --locked --offline | ConvertFrom-Json
    if ($LASTEXITCODE) { throw 'Could not read Cargo package versions.' }
    $version = ($metadata.packages | Where-Object name -eq 'nubi-app').version
    $hookVersion = ($metadata.packages | Where-Object name -eq 'nubi-hook').version
    $coreVersion = ($metadata.packages | Where-Object name -eq 'nubi-core').version
    if ($version -notmatch '^\d+\.\d+\.\d+$' -or $version -ne $hookVersion -or $version -ne $coreVersion) {
        throw 'Expected matching three-part app, hook and core versions.'
    }
    if (!$StandardProfile) {
        $env:CARGO_PROFILE_RELEASE_OPT_LEVEL = 'z'
        $env:CARGO_PROFILE_RELEASE_PANIC = 'abort'
    }
    cargo build --locked --offline --release --target-dir $target -p nubi-app -p nubi-hook
    if ($LASTEXITCODE) { throw 'Native release build failed.' }
    $artifactName = if ($TestInstaller) { "nubi-$version-installer-test.exe" } else { "nubi-$version-setup.exe" }
    $artifact = Join-Path $target $artifactName
    $defines = @("/DVERSION=$version", "/DPAYLOAD_DIR=$payload", "/DINSTALLER_OUT=$artifact")
    if ($TestInstaller) { $defines += '/DTEST_INSTALLER' }
    & $MakeNsis /WX @defines (Join-Path $PSScriptRoot 'native-installer.nsi')
    if ($LASTEXITCODE) { throw 'NSIS compilation failed.' }
    Get-Item -LiteralPath $artifact
} finally {
    if (!$StandardProfile) {
        $env:CARGO_PROFILE_RELEASE_OPT_LEVEL = $previousOptimization
        $env:CARGO_PROFILE_RELEASE_PANIC = $previousPanic
    }
    Pop-Location
}
