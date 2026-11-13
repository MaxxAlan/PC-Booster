<#
.SYNOPSIS
    End to end test for PC-Booster (Rust core + Tauri desktop app).
.DESCRIPTION
    1. runs the Rust unit tests
    2. runs the CLI self test and every CLI command
    3. runs the packaged desktop binary in self test mode
    4. checks the bundle artifacts
    5. runs the PowerShell vs Rust benchmark and prints the comparison
.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-e2e.ps1
#>
[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [int]$BenchmarkIterations = 5
)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$results = New-Object System.Collections.Generic.List[object]

function Add-Result {
    param([string]$Name, [bool]$Passed, [string]$Detail)
    $results.Add([pscustomobject]@{ name = $Name; passed = $Passed; detail = $Detail })
    $mark = if ($Passed) { 'PASS' } else { 'FAIL' }
    $color = if ($Passed) { 'Green' } else { 'Red' }
    Write-Host ("[{0}] {1} - {2}" -f $mark, $Name, $Detail) -ForegroundColor $color
}

function Invoke-Check {
    param([string]$Name, [scriptblock]$Body)
    try {
        $detail = & $Body
        Add-Result -Name $Name -Passed $true -Detail $detail
    } catch {
        Add-Result -Name $Name -Passed $false -Detail $_.Exception.Message
    }
}

Push-Location $root
try {
    if (-not $SkipBuild) {
        Write-Host '== build ==' -ForegroundColor Cyan
        $buildLog = Join-Path $env:TEMP 'pcbooster-e2e-build.log'
        cargo build --release >$buildLog 2>&1
        if ($LASTEXITCODE -ne 0) { Get-Content -LiteralPath $buildLog | Write-Host; throw 'cargo build failed' }
    }

    $cli = Join-Path $root 'target\release\pcbooster.exe'
    $gui = Join-Path $root 'target\release\pcbooster-app.exe'

    Write-Host '== unit tests ==' -ForegroundColor Cyan
    $testLog = Join-Path $env:TEMP 'pcbooster-e2e-cargo-test.log'
    cargo test -p pcbooster-core --release >$testLog 2>&1
    $testExit = $LASTEXITCODE
    $testOutput = Get-Content -LiteralPath $testLog -Raw
    $testPassed = ($testExit -eq 0) -and ($testOutput -match 'test result: ok')
    $testCount = ([regex]::Matches($testOutput, 'test result: ok\. (\d+) passed')).Groups | Where-Object { $_.Name -eq '1' } | ForEach-Object { $_.Value }
    Add-Result -Name 'rust unit tests' -Passed $testPassed -Detail "$testCount tests passed"

    Write-Host '== cli ==' -ForegroundColor Cyan
    Invoke-Check 'cli selftest' {
        $out = & $cli selftest 2>&1 | Out-String
        if ($LASTEXITCODE -ne 0) { throw "selftest exit code $LASTEXITCODE" }
        $json = $out | ConvertFrom-Json
        if (-not $json.ok) {
            $failed = ($json.checks | Where-Object { -not $_.passed } | ForEach-Object { $_.name }) -join ', '
            throw "checks failed: $failed"
        }
        "$($json.checks.Count) checks passed"
    }

    foreach ($command in @('overview', 'apps', 'ai', 'startup', 'services', 'temp', 'processes', 'disk', 'power', 'defender')) {
        Invoke-Check "cli $command --json" {
            $out = & $cli $command --json 2>&1 | Out-String
            if ($LASTEXITCODE -ne 0) { throw "exit code $LASTEXITCODE" }
            $null = $out | ConvertFrom-Json
            "$((($out | ConvertFrom-Json) | Measure-Object).Count) items"
        }
    }

    Write-Host '== desktop app ==' -ForegroundColor Cyan
    Invoke-Check 'desktop selftest (packaged binary)' {
        if (-not (Test-Path -LiteralPath $gui)) { throw "missing $gui" }
        $outFile = Join-Path $env:TEMP 'pcbooster-e2e-selftest.json'
        if (Test-Path -LiteralPath $outFile) { Remove-Item -LiteralPath $outFile -Force }
        $process = Start-Process -FilePath $gui -ArgumentList '--selftest', '--selftest-out', "`"$outFile`"" -Wait -PassThru
        if ($process.ExitCode -ne 0) { throw "exit code $($process.ExitCode)" }
        if (-not (Test-Path -LiteralPath $outFile)) { throw 'selftest output file missing' }
        $json = Get-Content -LiteralPath $outFile -Raw | ConvertFrom-Json
        if (-not $json.ok) { throw 'self test reported failures' }
        "$($json.checks.Count) checks passed from the GUI binary"
    }

    Invoke-Check 'desktop window starts' {
        $process = Start-Process -FilePath $gui -PassThru
        Start-Sleep -Seconds 6
        $alive = -not $process.HasExited
        if ($alive) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
        if (-not $alive) { throw 'the app exited immediately' }
        'window process stayed alive'
    }

    Write-Host '== bundle ==' -ForegroundColor Cyan
    Invoke-Check 'nsis installer built' {
        $bundle = Join-Path $root 'target\release\bundle\nsis'
        if (-not (Test-Path -LiteralPath $bundle)) { throw "missing $bundle (run: cargo tauri build)" }
        $installer = Get-ChildItem -LiteralPath $bundle -Filter '*.exe' | Select-Object -First 1
        if (-not $installer) { throw 'no installer exe found' }
        "$($installer.Name) ($([math]::Round($installer.Length/1KB)) KB)"
    }

    Write-Host '== benchmark ==' -ForegroundColor Cyan
    $rustJson = & (Join-Path $root 'target\release\pcbench.exe') $BenchmarkIterations | Out-String
    $psJson = & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $root 'bench\bench.ps1') -Iterations $BenchmarkIterations | Out-String
    $rust = $rustJson | ConvertFrom-Json
    $ps = $psJson | ConvertFrom-Json

    $rows = foreach ($rustCase in $rust.cases) {
        $psCase = $ps.cases | Where-Object { $_.name -eq $rustCase.name } | Select-Object -First 1
        [pscustomobject]@{
            operation   = $rustCase.name
            powershell_ms = if ($psCase) { $psCase.median_ms } else { $null }
            rust_ms     = $rustCase.median_ms
            speedup     = if ($psCase -and $rustCase.median_ms -gt 0) { [math]::Round($psCase.median_ms / $rustCase.median_ms, 2) } else { $null }
        }
    }
    $rows | Format-Table -AutoSize | Out-String | Write-Host
    $rustTotal = ($rust.cases | Measure-Object -Property median_ms -Sum).Sum
    $psTotal = ($ps.cases | Measure-Object -Property median_ms -Sum).Sum
    Write-Host ("Total: PowerShell {0:N1} ms vs Rust {1:N1} ms -> {2:N1}x faster" -f $psTotal, $rustTotal, ($psTotal / [math]::Max($rustTotal, 0.001)))
    $results | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $root 'target\e2e-results.json') -Encoding UTF8

    $failed = $results | Where-Object { -not $_.passed }
    Write-Host ''
    Write-Host ("E2E RESULT: {0}/{1} checks passed" -f ($results.Count - $failed.Count), $results.Count) -ForegroundColor $(if ($failed) { 'Red' } else { 'Green' })
    if ($failed) { exit 1 }
    exit 0
} finally {
    Pop-Location
}
