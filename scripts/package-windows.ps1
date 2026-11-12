# Builds the release exe and packs it into a zip with a checksum.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$json = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
$version = ($json.packages | Where-Object { $_.name -eq 'live-translator' }).version

cargo build --release -p live-translator
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

$out = Join-Path $root 'target/release/distribute'
New-Item -ItemType Directory -Force $out | Out-Null
$stage = Join-Path $out ".package.$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Force $stage | Out-Null
try {
    Copy-Item (Join-Path $root 'target/release/live-translator.exe') $stage
    Copy-Item (Join-Path $root 'INSTALL-Windows.txt') $stage
    $archive = "Live-Translator_${version}_Windows_x64.zip"
    $zip = Join-Path $out $archive
    if (Test-Path $zip) { Remove-Item $zip -Force }
    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
    $hash = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
    Set-Content -Path "$zip.sha256" -Value "$hash  $archive" -Encoding ascii
} finally {
    Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
}
Write-Host ""
Write-Host $zip
Write-Host "$zip.sha256"
