<#
.SYNOPSIS
    PowerShell baseline benchmark for PC-Booster.
.DESCRIPTION
    Measures the same operations as the Rust core (crates/pcbooster-core/src/bench.rs)
    so the two implementations can be compared on the same machine.
.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File bench/bench.ps1 -Iterations 5
#>
[CmdletBinding()]
param(
    [int]$Iterations = 5,
    [string]$OutputPath = ''
)

$ErrorActionPreference = 'Continue'

function Measure-Case {
    param(
        [string]$Name,
        [scriptblock]$Action,
        [int]$Repeat = $Iterations
    )
    # warm up
    $null = & $Action
    $samples = @()
    for ($i = 0; $i -lt $Repeat; $i++) {
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        $null = & $Action
        $sw.Stop()
        $samples += $sw.Elapsed.TotalMilliseconds
    }
    $sorted = $samples | Sort-Object
    [pscustomobject]@{
        name      = $Name
        median_ms = [math]::Round($sorted[[int]($sorted.Count / 2)], 3)
        min_ms    = [math]::Round($sorted[0], 3)
        max_ms    = [math]::Round($sorted[-1], 3)
        iterations = $Repeat
    }
}

function Get-BenchApps {
    $roots = @(
        'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKCU:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'
    )
    $apps = foreach ($root in $roots) {
        if (-not (Test-Path -LiteralPath $root)) { continue }
        foreach ($key in (Get-ChildItem -LiteralPath $root -ErrorAction SilentlyContinue)) {
            try {
                $reg = Get-Item -LiteralPath $key.PSPath -ErrorAction Stop
                $name = $reg.GetValue('DisplayName')
                if (-not $name) { continue }
                $uninstall = $reg.GetValue('UninstallString')
                $quiet = $reg.GetValue('QuietUninstallString')
                if (-not $uninstall -and -not $quiet) { continue }
                [pscustomobject]@{
                    Name    = [string]$name
                    Version = [string]$reg.GetValue('DisplayVersion')
                    SizeMB  = $reg.GetValue('EstimatedSize')
                }
            } catch { }
        }
    }
    $apps
}

function Get-BenchStorePackages {
    Get-AppxPackage -ErrorAction SilentlyContinue |
        Where-Object { -not $_.IsFramework -and -not $_.NonRemovable } |
        Select-Object Name, Version, PackageFullName, PackageFamilyName
}

function Get-BenchStartup {
    $items = @()
    foreach ($path in @(
            'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run',
            'HKCU:\Software\Microsoft\Windows\CurrentVersion\RunOnce',
            'HKLM:\Software\Microsoft\Windows\CurrentVersion\Run',
            'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run')) {
        if (-not (Test-Path -LiteralPath $path)) { continue }
        $key = Get-Item -LiteralPath $path
        foreach ($value in $key.GetValueNames()) {
            $items += [pscustomobject]@{ Name = $value; Command = [string]$key.GetValue($value) }
        }
    }
    $folder = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Startup'
    if (Test-Path -LiteralPath $folder) {
        foreach ($file in (Get-ChildItem -LiteralPath $folder -Force -ErrorAction SilentlyContinue)) {
            $items += [pscustomobject]@{ Name = $file.Name; Command = $file.FullName }
        }
    }
    $items
}

function Get-BenchProcesses {
    Get-Process -ErrorAction SilentlyContinue |
        Where-Object { $_.MainWindowTitle } |
        Select-Object Id, ProcessName, MainWindowTitle, @{N = 'MemoryMB'; E = { [math]::Round($_.WorkingSet64 / 1MB, 1) } }
}

function Get-BenchTemp {
    $cutoff = (Get-Date).AddDays(-7)
    Get-ChildItem -LiteralPath $env:TEMP -Force -ErrorAction SilentlyContinue |
        Where-Object { $_.LastWriteTime -lt $cutoff } |
        ForEach-Object {
            if ($_.PSIsContainer) {
                $bytes = (Get-ChildItem -LiteralPath $_.FullName -Recurse -Force -ErrorAction SilentlyContinue |
                        Measure-Object -Property Length -Sum).Sum
            } else { $bytes = $_.Length }
            [pscustomobject]@{ Name = $_.Name; Size = [double]$bytes }
        }
}

function Get-BenchServices {
    foreach ($name in @('Fax', 'XblGameSave', 'WSearch', 'SysMain')) {
        $service = Get-Service -Name $name -ErrorAction SilentlyContinue
        if ($service) {
            [pscustomobject]@{ Name = $service.Name; Status = $service.Status; StartType = $service.StartType }
        }
    }
}

function Get-BenchPower {
    & powercfg /list 2>$null | ForEach-Object {
        if ($_ -match '(?i)([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\s*\(([^)]*)\)') {
            [pscustomobject]@{ Guid = $Matches[1]; Name = $Matches[2].Trim() }
        }
    }
}

function Get-BenchAiApps {
    $pattern = '(?i)\b(ollama|lm studio|lmstudio|jan(\s+ai)?|anything\s*llm|open\s*webui|chatgpt|openai|claude(\s*code)?|anthropic|gemini|google\s+ai|cursor|windsurf|continue(\.dev)?|cline|roo\s*code|kilo\s*code|github\s+copilot|microsoft\s+copilot|tabnine|codeium|amazon\s+q|kiro|aider|codex|opencode|antigravity|trae|zed|openclaw|moltbot|deepseek|perplexity|msty|gpt4all|koboldcpp)\b'
    $found = @()
    foreach ($app in (Get-BenchApps)) {
        if ($app.Name -match $pattern) { $found += [pscustomobject]@{ Name = $app.Name; Kind = 'app' } }
    }
    foreach ($package in (Get-BenchStorePackages)) {
        if ($package.Name -match $pattern) { $found += [pscustomobject]@{ Name = $package.Name; Kind = 'appx' } }
    }
    $npm = Get-Command npm.cmd -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($npm) {
        try {
            $json = (& $npm.Source list --global --depth=0 --json 2>$null | Out-String) | ConvertFrom-Json -ErrorAction Stop
            foreach ($property in $json.dependencies.PSObject.Properties) {
                if ($property.Name -match '(?i)(codex|claude|gemini-cli|opencode|cline|copilot|aider|qwen)') {
                    $found += [pscustomobject]@{ Name = $property.Name; Kind = 'npm' }
                }
            }
        } catch { }
    }
    $found
}

function Get-BenchOverview {
    $null = Get-BenchApps
    $null = Get-BenchStorePackages
    $null = Get-BenchStartup
    $null = Get-BenchProcesses
    $null = Get-BenchTemp
    $null = Get-BenchServices
    $null = Get-BenchPower
}

$cases = @()
$cases += Measure-Case -Name 'registry.installed_apps' -Action { Get-BenchApps }
$cases += Measure-Case -Name 'registry.store_packages' -Action { Get-BenchStorePackages }
$cases += Measure-Case -Name 'registry.startup_entries' -Action { Get-BenchStartup }
$cases += Measure-Case -Name 'processes.list' -Action { Get-BenchProcesses }
$cases += Measure-Case -Name 'temp.scan' -Action { Get-BenchTemp }
$cases += Measure-Case -Name 'services.query' -Action { Get-BenchServices }
$cases += Measure-Case -Name 'power.list' -Action { Get-BenchPower }
$cases += Measure-Case -Name 'ai.scan' -Action { Get-BenchAiApps }
$cases += Measure-Case -Name 'overview.full' -Action { Get-BenchOverview }

$report = [pscustomobject]@{
    implementation = 'powershell'
    runtime        = "PowerShell $($PSVersionTable.PSVersion) on Windows $((Get-CimInstance Win32_OperatingSystem).Caption)"
    iterations     = $Iterations
    total_ms       = [math]::Round(($cases | Measure-Object -Property median_ms -Sum).Sum, 3)
    cases          = $cases
}

$json = $report | ConvertTo-Json -Depth 5
$json
if ($OutputPath) {
    $dir = Split-Path -Parent $OutputPath
    if ($dir -and -not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
    Set-Content -LiteralPath $OutputPath -Value $json -Encoding UTF8
}
