# Runs Nubi in development mode with live UI reload.
# Edit any file in crates/nubi-app/ui and save: the open windows update without a rebuild.
# Rust changes still need this script to be run again.
$previousPreview = $env:SLINT_LIVE_PREVIEW
Push-Location (Split-Path -Parent $PSScriptRoot)
try {
    $env:SLINT_LIVE_PREVIEW = "1"
    cargo run -p nubi-app --features live-preview -- --dev @args
    $previewExit = $LASTEXITCODE
} finally {
    $env:SLINT_LIVE_PREVIEW = $previousPreview
    Pop-Location
}
exit $previewExit
