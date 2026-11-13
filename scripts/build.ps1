<#
.SYNOPSIS
    Build PC-Booster: Rust core, CLI and the Tauri desktop bundle.
.DESCRIPTION
    Creates the release binaries and the NSIS installer under target\release\bundle.
    Requires: Rust (stable-msvc), the tauri CLI (cargo install tauri-cli) and the
    WebView2 runtime.
.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build.ps1
#>
[CmdletBinding()]
param(
    [switch]$SkipBundle,
    [switch]$SkipTests
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    if (-not $SkipTests) {
        Write-Host '== tests ==' -ForegroundColor Cyan
        cargo test --release
        if ($LASTEXITCODE -ne 0) { throw 'tests failed' }
    }

    Write-Host '== release build ==' -ForegroundColor Cyan
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

    if (-not $SkipBundle) {
        Write-Host '== tauri bundle ==' -ForegroundColor Cyan
        cargo tauri build
        if ($LASTEXITCODE -ne 0) { throw 'cargo tauri build failed' }
        $bundle = Join-Path $root 'target\release\bundle\nsis'
        if (Test-Path -LiteralPath $bundle) {
            Get-ChildItem -LiteralPath $bundle -Filter '*.exe' | ForEach-Object {
                Write-Host ("installer: {0} ({1:N0} KB)" -f $_.Name, ($_.Length / 1KB)) -ForegroundColor Green
            }
        }
    }

    Write-Host '== artefacts ==' -ForegroundColor Green
    Get-ChildItem -LiteralPath (Join-Path $root 'target\release') -Filter 'pcbooster*.exe' |
        ForEach-Object { Write-Host ("{0}  {1:N0} KB" -f $_.Name, ($_.Length / 1KB)) }
} finally {
    Pop-Location
}
