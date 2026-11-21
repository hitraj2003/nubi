param([string]$OutputDirectory)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$build = Join-Path $root 'target/native-size'

Push-Location $root
try {
    $metadata = cargo metadata --no-deps --format-version 1 --locked --offline | ConvertFrom-Json
    if ($LASTEXITCODE) { throw 'Could not read the native package version.' }
    $version = ($metadata.packages | Where-Object name -eq 'nubi-app').version
    $hookVersion = ($metadata.packages | Where-Object name -eq 'nubi-hook').version
    $coreVersion = ($metadata.packages | Where-Object name -eq 'nubi-core').version
    if ($version -notmatch '^\d+\.\d+\.\d+$' -or $version -ne $hookVersion -or $version -ne $coreVersion) {
        throw 'Expected matching three-part app, hook and core versions.'
    }
    $name = "nubi-$version-setup.exe"
    $source = Get-Item -LiteralPath (Join-Path $build $name)
    $destination = if ($OutputDirectory) { $OutputDirectory } else { Join-Path $build 'signed-candidate' }
    New-Item -ItemType Directory -Path $destination -Force | Out-Null
    $candidate = Join-Path $destination $name
    Copy-Item -LiteralPath $source.FullName -Destination $candidate -Force
    cargo tauri signer sign --app-version $version $candidate
    if ($LASTEXITCODE) { throw 'Native updater signing failed.' }
    $manifest = [ordered]@{
        version = $version
        notes = ''
        pub_date = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
        platforms = @{
            'windows-x86_64' = @{
                signature = (Get-Content -LiteralPath "$candidate.sig" -Raw).Trim()
                url = "https://github.com/hitraj2003/nubi/releases/download/v$version/$name"
            }
        }
    }
    [IO.File]::WriteAllText((Join-Path $destination 'latest.json'), ($manifest | ConvertTo-Json -Depth 5))
} finally {
    Pop-Location
}
