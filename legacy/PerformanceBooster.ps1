# PerformanceBooster v2.4 (LEGACY - superseded by the Rust/Tauri app)
# Kept for reference and for the benchmark comparison in bench/bench.ps1.
param(
    [string]$Action = ''
)

[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$ErrorActionPreference = 'Continue'

# The embedded script (ps2exe) has no file path of its own, so the running host
# process is the reliable way to know whether this is the .exe or the .ps1.
$scriptFile = $MyInvocation.MyCommand.Definition
if (-not $scriptFile) { $scriptFile = $MyInvocation.MyCommand.Path }
$hostExe = $null
try { $hostExe = [System.Diagnostics.Process]::GetCurrentProcess().MainModule.FileName } catch {}
if (-not $hostExe) { $hostExe = [System.Environment]::GetCommandLineArgs()[0] }
$isExe = $hostExe -and ($hostExe -match '\.exe$') -and ($hostExe -notmatch '(?i)\\(powershell|pwsh)\.exe$')

$dataDir = Join-Path $env:LOCALAPPDATA 'PCBooster'
$stateFile = Join-Path $dataDir 'restore-state.json'
$logFile = Join-Path $dataDir 'PCBooster_Log.txt'
$legacyLogFile = Join-Path ([System.IO.Path]::GetTempPath()) 'PCBooster_Log.txt'

try {
    New-Item -ItemType Directory -Path $dataDir -Force | Out-Null
    # v2.3 kept the log in %TEMP%, where the app's own temp cleanup could delete it.
    if ((Test-Path -LiteralPath $legacyLogFile) -and -not (Test-Path -LiteralPath $logFile)) {
        Move-Item -LiteralPath $legacyLogFile -Destination $logFile -Force
    }
} catch {}

function Write-Log {
    param([string]$Message)
    try { "$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') - $Message" | Add-Content -LiteralPath $logFile -Encoding UTF8 } catch {}
}

function Test-Admin {
    try {
        $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
        $principal = New-Object Security.Principal.WindowsPrincipal($identity)
        return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    } catch { return $false }
}

# Re-launches this app in an elevated console and waits for it to finish.
# Only actions that really need administrator rights use this.
function Start-ElevatedInstance {
    param([Parameter(Mandatory)][string]$TargetAction)
    Write-Host "`nThao tác này cần quyền quản trị. Đang mở cửa sổ UAC..." -ForegroundColor Yellow
    try {
        if ($isExe) {
            Start-Process -FilePath $hostExe -ArgumentList @('-Action', $TargetAction) -Verb RunAs -Wait -ErrorAction Stop
        } else {
            $launcher = (Get-Process -Id $PID).Path
            $argLine = '-NoProfile -ExecutionPolicy Bypass -File "{0}" -Action {1}' -f $scriptFile, $TargetAction
            Start-Process -FilePath $launcher -ArgumentList $argLine -Verb RunAs -Wait -ErrorAction Stop
        }
        return $true
    } catch {
        Write-Host "Không thể nâng quyền quản trị: $($_.Exception.Message)" -ForegroundColor Red
        return $false
    }
}

function Show-Menu {
    Clear-Host
    Write-Host '============================================' -ForegroundColor Cyan
    Write-Host '        PERFORMANCE BOOSTER v2.4' -ForegroundColor Yellow
    Write-Host '============================================' -ForegroundColor Cyan
    Write-Host '1. Boost PC (chọn ứng dụng/dịch vụ muốn tối ưu)'
    Write-Host '2. Khôi phục cấu hình đã lưu'
    Write-Host '3. Dọn tệp tạm cũ'
    Write-Host '4. Xem log'
    Write-Host '5. Remove AI (gỡ ứng dụng AI / coding agent)'
    Write-Host '6. Quét malware bằng Microsoft Defender'
    Write-Host '7. Xem ứng dụng và tài nguyên đang dùng'
    Write-Host '============================================' -ForegroundColor Cyan
    if (-not (Test-Admin)) { Write-Host 'Đang chạy không có quyền quản trị: mục 1 và 2 sẽ yêu cầu nâng quyền khi cần.' -ForegroundColor DarkYellow }
    Write-Host "Nhập lựa chọn (1-7) hoặc Q để thoát: " -NoNewline
}

function Get-InstalledApps {
    # Read values with RegistryKey.GetValue instead of Get-ItemProperty: some
    # uninstall keys hold values whose type cannot be cast, which made
    # Get-ItemProperty throw InvalidCastException and print a raw error.
    $roots = @(
        'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKCU:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'
    )
    $apps = @()
    foreach ($root in $roots) {
        if (-not (Test-Path -LiteralPath $root)) { continue }
        $keys = @(Get-ChildItem -LiteralPath $root -ErrorAction SilentlyContinue)
        foreach ($key in $keys) {
            try {
                $reg = Get-Item -LiteralPath $key.PSPath -ErrorAction Stop
                $name = [string]$reg.GetValue('DisplayName')
                if ([string]::IsNullOrWhiteSpace($name)) { continue }
                $uninstall = [string]$reg.GetValue('UninstallString')
                $quiet = [string]$reg.GetValue('QuietUninstallString')
                if (-not $uninstall -and -not $quiet) { continue }
                $sizeRaw = $reg.GetValue('EstimatedSize')
                $sizeMB = $null
                if ($sizeRaw) { $sizeMB = [math]::Round(([double]$sizeRaw / 1024), 1) }
                $apps += [pscustomobject]@{
                    Name = $name
                    Version = [string]$reg.GetValue('DisplayVersion')
                    Publisher = [string]$reg.GetValue('Publisher')
                    SizeMB = $sizeMB
                    Uninstall = $uninstall
                    QuietUninstall = $quiet
                    Kind = 'app'
                }
            } catch {
                Write-Log "Skipped uninstall entry $($key.PSChildName): $($_.Exception.Message)"
            }
        }
    }
    $apps | Sort-Object Name, Version -Unique
}

function Get-AIApps {
    # Include normal uninstall entries, Store apps, native CLI files and global npm agents.
    $pattern = '(?i)\b(ollama|lm studio|lmstudio|jan(\s+ai)?|anything\s*llm|open\s*webui|chatgpt|openai|claude(\s*code)?|anthropic|gemini|google\s+ai|cursor|windsurf|continue(\.dev)?|cline|roo\s*code|kilo\s*code|github\s+copilot|microsoft\s+copilot|tabnine|codeium|amazon\s+q|kiro|aider|codex|opencode|antigravity|trae|zed|openclaw|moltbot|deepseek|perplexity|msty|gpt4all|koboldcpp)\b'
    $apps = @(Get-InstalledApps | Where-Object { $_.Name -match $pattern })

    # Store/MSIX apps (for example Claude and ChatGPT) do not always have uninstall registry entries.
    try {
        $apps += @(Get-AppxPackage -ErrorAction Stop | Where-Object {
            -not $_.IsFramework -and -not $_.NonRemovable -and (($_.Name -match $pattern) -or ($_.PackageFullName -match $pattern))
        } | ForEach-Object {
            [pscustomobject]@{
                Name = "Store app: $($_.Name)"
                Version = [string]$_.Version
                Publisher = [string]$_.Publisher
                SizeMB = $null
                Uninstall = $null
                Kind = 'appx'
                PackageFullName = [string]$_.PackageFullName
                PackageFamilyName = [string]$_.PackageFamilyName
            }
        })
    } catch { Write-Log "Could not inspect Store apps: $_" }

    # Claude Code's native Windows installer places the executable here rather than in npm.
    $nativeClaude = Join-Path $env:USERPROFILE '.local\bin\claude.exe'
    if (Test-Path -LiteralPath $nativeClaude -PathType Leaf) {
        $apps += [pscustomobject]@{
            Name = 'Claude Code (native CLI)'
            Version = ''
            Publisher = 'Native user install'
            SizeMB = [math]::Round((Get-Item -LiteralPath $nativeClaude).Length / 1MB, 1)
            Uninstall = $nativeClaude
            Kind = 'native-cli'
        }
    }
    $packagePattern = '(?i)(^|/|[._-])(codex|claude(-code)?|gemini-cli|opencode(-ai)?|cline|roo-code|kilo-code|copilot|aider|qwen-code|mistral-vibe|open-interpreter|openclaw|amp)(/|[._-]|$)'
    foreach ($managerName in @('npm.cmd','pnpm.cmd')) {
        $manager = Get-Command $managerName -ErrorAction SilentlyContinue | Select-Object -First 1
        if (-not $manager) { continue }
        try {
            $json = (& $manager.Source list --global --depth=0 --json 2>$null | Out-String) | ConvertFrom-Json -ErrorAction Stop
            foreach ($root in @($json)) {
                foreach ($package in @($root.dependencies.PSObject.Properties)) {
                    if ($package.Name -match $packagePattern) {
                        $kind = if ($managerName -eq 'npm.cmd') { 'npm' } else { 'pnpm' }
                        $apps += [pscustomobject]@{
                            Name = "$kind global: $($package.Name)"
                            Version = [string]$package.Value.version
                            Publisher = "$kind global"
                            SizeMB = $null
                            Uninstall = $null
                            Kind = $kind
                            Package = [string]$package.Name
                        }
                    }
                }
            }
        } catch { Write-Log "Could not inspect global packages with $managerName : $_" }
    }

    # Bun lists global packages as a tree; parse only package/version rows, never run package code.
    $bun = Get-Command bun.exe,bun -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($bun) {
        try {
            foreach ($line in (& $bun.Source pm ls -g 2>$null)) {
                if ($line -match '(?<package>@[^/\s]+/[^@\s]+|[a-z0-9._-]+)@(?<version>\d[^\s]*)') {
                    $packageName = $Matches['package']
                    if ($packageName -match $packagePattern) {
                        $apps += [pscustomobject]@{
                            Name = "bun global: $packageName"
                            Version = $Matches['version']
                            Publisher = 'bun global'
                            SizeMB = $null
                            Uninstall = $null
                            Kind = 'bun'
                            Package = $packageName
                        }
                    }
                }
            }
        } catch { Write-Log "Could not inspect global bun packages: $_" }
    }
    $apps | Sort-Object Name, Version -Unique
}

function Start-AppUninstaller {
    param([Parameter(Mandatory)]$App)
    if ($App.Kind -eq 'appx') {
        Remove-AppxPackage -Package $App.PackageFullName -ErrorAction Stop
        return
    }
    if ($App.Kind -eq 'native-cli') {
        Remove-Item -LiteralPath $App.Uninstall -Force -ErrorAction Stop
        return
    }
    if ($App.Kind -in @('npm','pnpm')) {
        $package = [string]$App.Package
        if ($package -notmatch '^(@[a-z0-9._-]+/)?[a-z0-9._-]+$') { throw 'Tên npm package không hợp lệ.' }
        $managerName = if ($App.Kind -eq 'npm') { 'npm.cmd' } else { 'pnpm.cmd' }
        $manager = Get-Command $managerName -ErrorAction Stop | Select-Object -First 1
        & $manager.Source uninstall --global $package
        if ($LASTEXITCODE -ne 0) { throw "$managerName uninstall kết thúc với mã $LASTEXITCODE." }
        return
    }
    if ($App.Kind -eq 'bun') {
        $package = [string]$App.Package
        if ($package -notmatch '^(@[a-z0-9._-]+/)?[a-z0-9._-]+$') { throw 'Tên bun package không hợp lệ.' }
        $manager = Get-Command bun.exe,bun -ErrorAction Stop | Select-Object -First 1
        & $manager.Source remove --global $package
        if ($LASTEXITCODE -ne 0) { throw "bun remove kết thúc với mã $LASTEXITCODE." }
        return
    }
    $command = ([string]$App.Uninstall).Trim()
    if (-not $command) { $command = ([string]$App.QuietUninstall).Trim() }
    if (-not $command) { throw 'Lệnh gỡ cài đặt không được hỗ trợ an toàn.' }
    if ($command -match '(?i)msiexec(?:\.exe)?\s+.*?/(?:i|x)\s*(\{[0-9a-f-]{36}\})') {
        $msi = Join-Path $env:SystemRoot 'System32\msiexec.exe'
        Start-Process -FilePath $msi -ArgumentList "/x $($Matches[1])" -Wait
        return
    }

    # Only launch a real executable directly; never pass registry text through cmd.exe.
    if ($command -match '^"([^"]+\.exe)"\s*(.*)$') { $exe = $Matches[1]; $args = $Matches[2] }
    elseif ($command -match '^(.+?\.exe)\s*(.*)$') { $exe = $Matches[1].Trim(); $args = $Matches[2] }
    else { throw 'Lệnh gỡ cài đặt không được hỗ trợ an toàn.' }
    if (-not [System.IO.Path]::IsPathRooted($exe) -or -not (Test-Path -LiteralPath $exe -PathType Leaf)) {
        throw 'Không tìm thấy chương trình gỡ cài đặt trên máy.'
    }
    Start-Process -FilePath $exe -ArgumentList $args -Wait
}

function Get-AIResidualPaths {
    param([Parameter(Mandatory)]$App)
    $identity = "$($App.Name) $($App.Package) $($App.PackageFullName)"
    $paths = @()
    if ($identity -match '(?i)claude|anthropic') {
        $paths += @((Join-Path $env:USERPROFILE '.claude'), (Join-Path $env:USERPROFILE '.local\bin\claude.exe'), (Join-Path $env:APPDATA 'Claude'), (Join-Path $env:LOCALAPPDATA 'Claude'), (Join-Path $env:LOCALAPPDATA 'Programs\Claude'))
    }
    if ($identity -match '(?i)chatgpt') {
        $paths += @((Join-Path $env:APPDATA 'ChatGPT'), (Join-Path $env:LOCALAPPDATA 'ChatGPT'))
    }
    if ($identity -match '(?i)opencode') {
        $paths += @((Join-Path $env:APPDATA 'opencode'), (Join-Path $env:LOCALAPPDATA 'opencode'), (Join-Path $env:USERPROFILE '.config\opencode'), (Join-Path $env:USERPROFILE '.local\share\opencode'), (Join-Path $env:USERPROFILE '.opencode'))
    }
    if ($identity -match '(?i)gemini') { $paths += (Join-Path $env:USERPROFILE '.gemini') }
    if ($identity -match '(?i)codex|openai') { $paths += (Join-Path $env:USERPROFILE '.codex') }
    if ($identity -match '(?i)ollama') { $paths += @((Join-Path $env:USERPROFILE '.ollama'), (Join-Path $env:LOCALAPPDATA 'Ollama')) }
    if ($identity -match '(?i)lm.?studio') { $paths += @((Join-Path $env:USERPROFILE '.lmstudio'), (Join-Path $env:APPDATA 'LM Studio'), (Join-Path $env:LOCALAPPDATA 'LM Studio')) }
    if ($identity -match '(?i)cursor') { $paths += @((Join-Path $env:APPDATA 'Cursor'), (Join-Path $env:USERPROFILE '.cursor')) }
    if ($App.Kind -eq 'appx' -and $App.PackageFamilyName) {
        $paths += (Join-Path (Join-Path $env:LOCALAPPDATA 'Packages') $App.PackageFamilyName)
    }
    @($paths | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -Unique)
}

function Remove-AIResidualData {
    param([Parameter(Mandatory)]$App)
    $paths = @(Get-AIResidualPaths -App $App)
    if ($paths.Count -eq 0) { return }
    Write-Host "`nTìm thấy dữ liệu còn lại của $($App.Name):" -ForegroundColor Yellow
    $paths | ForEach-Object { Write-Host " - $_" }
    Write-Host 'Có thể gồm đăng nhập, cấu hình, lịch sử hội thoại, plugin hoặc model đã tải.' -ForegroundColor Yellow
    if ((Read-Host 'Xóa toàn bộ các đường dẫn trên? (Y/N)') -notmatch '^(y|yes|c|co)$') { return }
    foreach ($path in $paths) {
        $resolved = [System.IO.Path]::GetFullPath($path)
        $profileRoot = [System.IO.Path]::GetFullPath($env:USERPROFILE).TrimEnd('\') + '\'
        if (-not $resolved.StartsWith($profileRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
            Write-Host "Bỏ qua đường dẫn ngoài hồ sơ người dùng: $resolved" -ForegroundColor Red
            continue
        }
        try {
            Remove-Item -LiteralPath $resolved -Recurse -Force -ErrorAction Stop
            Write-Log "Removed residual data: $resolved"
            Write-Host "Đã xóa: $resolved" -ForegroundColor Green
        } catch {
            Write-Host "Không xóa được $resolved : $_" -ForegroundColor DarkYellow
            Write-Log "Could not remove residual data $resolved : $_"
        }
    }
}

function Remove-AI {
    Write-Host "`nĐang quét registry, ứng dụng Store, CLI Claude native và npm global..."
    $apps = @(Get-AIApps)
    if ($apps.Count -eq 0) {
        Write-Host 'Không tìm thấy ứng dụng phù hợp trong các nguồn cài đặt đã quét.' -ForegroundColor DarkYellow
        Read-Host 'Nhấn Enter để tiếp tục'; return
    }
    for ($i = 0; $i -lt $apps.Count; $i++) {
        $version = if ($apps[$i].Version) { " (v$($apps[$i].Version))" } else { '' }
        Write-Host ("{0}. {1}{2} — {3}" -f ($i + 1), $apps[$i].Name, $version, $apps[$i].Publisher)
    }
    Write-Host "`nChỉ nhập số của ứng dụng bạn muốn gỡ, cách nhau bằng dấu phẩm. Enter để hủy."
    $selection = Read-Host 'Lựa chọn'
    if ([string]::IsNullOrWhiteSpace($selection)) { return }
    $indexes = @($selection -split '[,; ]+' | ForEach-Object {
        $n = 0
        if ([int]::TryParse($_, [ref]$n) -and $n -ge 1 -and $n -le $apps.Count) { $n - 1 }
    } | Select-Object -Unique)
    if ($indexes.Count -eq 0) { Write-Host 'Lựa chọn không hợp lệ.' -ForegroundColor Red; Read-Host 'Nhấn Enter'; return }
    foreach ($index in $indexes) {
        $app = $apps[$index]
        $confirm = Read-Host "Mở trình gỡ cài đặt cho '$($app.Name)'? (Y/N)"
        if ($confirm -notmatch '^(y|yes|c|co)$') { continue }
        try {
            Start-AppUninstaller -App $app
            Write-Log "Uninstaller launched for $($app.Name)"
            Write-Host "Đã xử lý trình gỡ cài đặt của $($app.Name)." -ForegroundColor Green
            Remove-AIResidualData -App $app
        } catch {
            Write-Host "Không thể gỡ $($app.Name): $_" -ForegroundColor Red
            Write-Log "Uninstall failed for $($app.Name): $_"
        }
    }
    Read-Host 'Nhấn Enter để tiếp tục'
}

function Get-RestoreState {
    if (Test-Path -LiteralPath $stateFile) {
        try { return Get-Content -LiteralPath $stateFile -Raw | ConvertFrom-Json } catch { Write-Log "Restore state unreadable: $_" }
    }
    return $null
}

function Save-RestoreState {
    param($State)
    New-Item -ItemType Directory -Path $dataDir -Force | Out-Null
    $State | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $stateFile -Encoding UTF8
}

# powercfg output is localized, so match the GUID and the name in parentheses
# instead of the English "Power Scheme GUID:" label.
$script:GuidPattern = '(?i)([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})'
$script:HighPerformanceGuid = '8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c'
$script:BalancedGuid = '381b4222-f694-41f0-9685-ff5bb260df2e'

function Get-PowerSchemes {
    $schemes = @()
    foreach ($line in (& powercfg /list 2>$null)) {
        if ($line -match "$script:GuidPattern\s*\(([^)]*)\)") {
            $schemes += [pscustomobject]@{
                Guid = $Matches[1]
                Name = $Matches[2].Trim()
                Active = ($line -match '\*\s*$')
            }
        }
    }
    $schemes
}

function Get-ActivePowerSchemeGuid {
    foreach ($line in (& powercfg /getactivescheme 2>$null)) {
        if ($line -match $script:GuidPattern) { return $Matches[1] }
    }
    $null
}

function Get-HighPerformanceScheme {
    $schemes = @(Get-PowerSchemes)
    $high = $schemes | Where-Object {
        $_.Guid -eq $script:HighPerformanceGuid -or $_.Name -match '(?i)high performance|hiệu suất cao'
    } | Select-Object -First 1
    if ($high) { return $high }

    # The High performance scheme is hidden by default on many machines.
    $created = $null
    foreach ($line in (& powercfg /duplicatescheme $script:HighPerformanceGuid 2>&1)) {
        if ($line -match $script:GuidPattern) { $created = $Matches[1]; break }
    }
    if (-not $created) { return $null }
    Write-Log "Created High performance power scheme $created"
    Get-PowerSchemes | Where-Object { $_.Guid -eq $created } | Select-Object -First 1
}

function Invoke-Boost {
    Write-Host "`nBoost chỉ tác động sau khi bạn chọn và xác nhận từng mục."
    $allProcesses = @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle } | Sort-Object ProcessName)
    if ($allProcesses.Count -gt 0) {
        Write-Host "`nỨng dụng đang mở:"
        for ($i = 0; $i -lt $allProcesses.Count; $i++) { Write-Host ("{0}. {1} — {2}" -f ($i + 1), $allProcesses[$i].ProcessName, $allProcesses[$i].MainWindowTitle) }
        $pick = Read-Host 'Nhập số ứng dụng cần đóng, phân tách bằng dấu phẩm (Enter để bỏ qua)'
        foreach ($value in ($pick -split '[,; ]+')) {
            $number = 0
            if ([int]::TryParse($value, [ref]$number) -and $number -ge 1 -and $number -le $allProcesses.Count) {
                $proc = $allProcesses[$number - 1]
                if ((Read-Host "Đóng '$($proc.ProcessName)'? (Y/N)") -match '^(y|yes|c|co)$') {
                    try { Stop-Process -Id $proc.Id -ErrorAction Stop; Write-Log "Closed process $($proc.ProcessName) ($($proc.Id))" } catch { Write-Host "Không thể đóng: $_" -ForegroundColor DarkYellow }
                }
            }
        }
    }

    $optional = @('Fax', 'XblGameSave', 'WSearch', 'SysMain')
    Write-Host "`nDịch vụ tùy chọn (chỉ tắt nếu bạn xác nhận):"
    $available = @($optional | ForEach-Object { Get-Service -Name $_ -ErrorAction SilentlyContinue } | Where-Object { $_ })
    if ($available.Count) {
        for ($i = 0; $i -lt $available.Count; $i++) { Write-Host ("{0}. {1} (hiện: {2})" -f ($i + 1), $available[$i].DisplayName, $available[$i].Status) }
        $pick = Read-Host 'Chọn số dịch vụ muốn tắt, phân tách bằng dấu phẩm (Enter để bỏ qua)'
        $state = Get-RestoreState
        if (-not $state) { $state = [pscustomobject]@{ Services = @(); PowerPlan = $null } }
        foreach ($value in ($pick -split '[,; ]+')) {
            $number = 0
            if ([int]::TryParse($value, [ref]$number) -and $number -ge 1 -and $number -le $available.Count) {
                $svc = $available[$number - 1]
                if ((Read-Host "Tắt '$($svc.DisplayName)'? (Y/N)") -match '^(y|yes|c|co)$') {
                    try {
                        $wmi = Get-CimInstance Win32_Service -Filter "Name='$($svc.Name)'"
                        $prior = @($state.Services | Where-Object { $_.Name -eq $svc.Name })
                        if ($prior.Count -eq 0) {
                            $state.Services += [pscustomobject]@{ Name = $svc.Name; StartMode = $wmi.StartMode; WasRunning = ($svc.Status -eq 'Running') }
                        }
                        Save-RestoreState $state
                        Stop-Service -Name $svc.Name -Force -ErrorAction SilentlyContinue
                        Set-Service -Name $svc.Name -StartupType Disabled -ErrorAction Stop
                        Write-Log "Disabled optional service $($svc.Name)"
                    } catch { Write-Host "Không thể thay đổi dịch vụ: $_" -ForegroundColor DarkYellow }
                }
            }
        }
    }

    $active = Get-ActivePowerSchemeGuid
    $high = Get-HighPerformanceScheme
    if ($high -and $active -and $high.Guid -ne $active) {
        if ((Read-Host "Chuyển sang gói nguồn '$($high.Name)'? (Y/N)") -match '^(y|yes|c|co)$') {
            $state = Get-RestoreState
            if (-not $state) { $state = [pscustomobject]@{ Services = @(); PowerPlan = $null } }
            if (-not $state.PowerPlan) { $state.PowerPlan = $active }
            try {
                Save-RestoreState $state
                & powercfg /setactive $high.Guid | Out-Null
                if ($LASTEXITCODE -eq 0) { Write-Log "Changed power plan to $($high.Name)" }
                else { Write-Host 'Windows không áp dụng được gói nguồn này.' -ForegroundColor DarkYellow }
            } catch { Write-Host "Không thể đổi gói nguồn: $_" -ForegroundColor DarkYellow }
        }
    }
    Write-Host "`nHoàn tất. Cấu hình dịch vụ đã thay đổi được lưu để khôi phục." -ForegroundColor Green
    Read-Host 'Nhấn Enter để tiếp tục'
}

function Restore-Normal {
    $state = Get-RestoreState
    $hasServices = $state -and $state.Services -and $state.Services.Count -gt 0
    $hasPowerPlan = $state -and $state.PowerPlan
    if (-not $hasServices -and -not $hasPowerPlan) {
        Write-Host 'Không có thay đổi dịch vụ nào được lưu để khôi phục.' -ForegroundColor DarkYellow
    } else {
        $restoreFailed = $false
        foreach ($item in @($state.Services)) {
            try {
                $startup = switch ($item.StartMode) { 'Auto' { 'Automatic' } 'Manual' { 'Manual' } 'Disabled' { 'Disabled' } default { $null } }
                if ($startup) { Set-Service -Name $item.Name -StartupType $startup -ErrorAction Stop }
                if ($item.WasRunning) { Start-Service -Name $item.Name -ErrorAction SilentlyContinue }
                Write-Host "Đã khôi phục $($item.Name)"; Write-Log "Restored service $($item.Name) to $($item.StartMode)"
            } catch { $restoreFailed = $true; Write-Host "Không thể khôi phục $($item.Name): $_" -ForegroundColor DarkYellow; Write-Log "Restore failed for $($item.Name): $_" }
        }
        if ($state.PowerPlan) {
            & powercfg /setactive $state.PowerPlan | Out-Null
            if ($LASTEXITCODE -eq 0) { Write-Host 'Đã khôi phục gói nguồn trước đó.'; Write-Log "Restored power plan $($state.PowerPlan)" }
            else { $restoreFailed = $true; Write-Host 'Không thể khôi phục gói nguồn trước đó.' -ForegroundColor DarkYellow }
        }
        if ($restoreFailed) { Write-Host 'Thông tin khôi phục vẫn được lưu để có thể thử lại.' -ForegroundColor Yellow }
        else { Remove-Item -LiteralPath $stateFile -Force -ErrorAction SilentlyContinue }
    }
    Read-Host 'Nhấn Enter để tiếp tục'
}

function Clean-System {
    Write-Host "`nDọn tệp trong thư mục TEMP của tài khoản, chỉ những tệp cũ hơn 7 ngày."
    $cutoff = (Get-Date).AddDays(-7)
    $items = @(Get-ChildItem -LiteralPath $env:TEMP -Force -ErrorAction SilentlyContinue | Where-Object { $_.LastWriteTime -lt $cutoff })
    $bytes = ($items | Measure-Object -Property Length -Sum).Sum
    if (-not $bytes) { $bytes = 0 }
    Write-Host ("Tìm thấy {0} mục, khoảng {1:N1} MB." -f $items.Count, ($bytes / 1MB))
    $confirm = Read-Host 'Xóa các mục này? (Y/N)'
    if ($items.Count -and $confirm -match '^(y|yes|c|co)$') {
        $removed = 0
        foreach ($item in $items) {
            try { Remove-Item -LiteralPath $item.FullName -Recurse -Force -ErrorAction Stop; $removed++ } catch { Write-Log "Skipped locked temp item $($item.FullName)" }
        }
        Write-Host "Đã dọn $removed mục. Tệp đang được sử dụng sẽ được giữ lại." -ForegroundColor Green
        Write-Log "Cleaned $removed old temp items"
    }
    Read-Host 'Nhấn Enter để tiếp tục'
}

function View-Log {
    Write-Host "`nLog: $logFile"
    if (Test-Path -LiteralPath $logFile) { Get-Content -LiteralPath $logFile -Tail 200 } else { Write-Host 'Chưa có log.' -ForegroundColor DarkYellow }
    Read-Host 'Nhấn Enter để tiếp tục'
}

function Show-MalwareScan {
    Write-Host "`nQuét malware bằng Microsoft Defender (không gửi file lên API)."
    try {
        $status = Get-MpComputerStatus -ErrorAction Stop
        Write-Host ("Defender: {0}; bảo vệ thời gian thực: {1}; cập nhật chữ ký: {2}" -f $status.AMRunningMode, $status.RealTimeProtectionEnabled, $status.AntivirusSignatureLastUpdated)
        if (-not $status.AntivirusEnabled -or $status.AMRunningMode -eq 'Passive') {
            Write-Host 'Defender không ở chế độ antivirus chủ động. Máy có thể đang dùng antivirus khác.' -ForegroundColor Yellow
        }
    } catch {
        Write-Host 'Không đọc được trạng thái Microsoft Defender trên máy này.' -ForegroundColor Yellow
        Write-Log "Defender status unavailable: $_"
        Read-Host 'Nhấn Enter để tiếp tục'; return
    }

    Write-Host '1. Quét nhanh'
    Write-Host '2. Quét toàn bộ máy (có thể lâu và dùng nhiều tài nguyên)'
    Write-Host '3. Quét một tệp hoặc thư mục'
    $choice = Read-Host 'Chọn kiểu quét (Enter để hủy)'
    try {
        switch ($choice) {
            '1' { Start-MpScan -ScanType QuickScan -ErrorAction Stop; Write-Log 'Started Microsoft Defender quick scan' }
            '2' {
                if ((Read-Host 'Quét toàn bộ máy có thể mất nhiều thời gian. Tiếp tục? (Y/N)') -match '^(y|yes|c|co)$') {
                    Start-MpScan -ScanType FullScan -ErrorAction Stop
                    Write-Log 'Started Microsoft Defender full scan'
                }
            }
            '3' {
                $path = Read-Host 'Đường dẫn tệp/thư mục cần quét'
                if (-not [string]::IsNullOrWhiteSpace($path) -and (Test-Path -LiteralPath $path)) {
                    Start-MpScan -ScanType CustomScan -ScanPath (Resolve-Path -LiteralPath $path).Path -ErrorAction Stop
                    Write-Log "Started Microsoft Defender custom scan: $path"
                } elseif (-not [string]::IsNullOrWhiteSpace($path)) { Write-Host 'Không tìm thấy đường dẫn.' -ForegroundColor Red }
            }
            default { Write-Host 'Đã hủy quét.' }
        }
        if ($choice -in @('1','2','3')) { Write-Host 'Đã gửi yêu cầu quét cho Defender. Tiến độ/kết quả đầy đủ xem trong Windows Security.' -ForegroundColor Green }
    } catch {
        Write-Host "Không khởi chạy được quét Defender: $_" -ForegroundColor Red
        Write-Log "Defender scan failed: $_"
    }
    $detections = @(Get-MpThreatDetection -ErrorAction SilentlyContinue | Sort-Object InitialDetectionTime -Descending | Select-Object -First 10)
    if ($detections.Count -gt 0) {
        Write-Host "`nTối đa 10 phát hiện gần đây (có thể gồm lịch sử đã xử lý):" -ForegroundColor Yellow
        $detections | Select-Object @{N='Phát hiện';E={$_.ThreatName}}, @{N='Thời gian';E={$_.InitialDetectionTime}}, @{N='Đã xử lý';E={$_.ActionSuccess}}, @{N='Tài nguyên';E={($_.Resources -join '; ')}} | Format-Table -AutoSize -Wrap
    }
    Read-Host 'Nhấn Enter để tiếp tục'
}

function Show-ResourceReport {
    Write-Host "`nĐo CPU trong 2 giây và đọc RAM hiện tại; chỉ báo cáo, không tự đóng ứng dụng."
    $first = @{}
    foreach ($proc in Get-Process -ErrorAction SilentlyContinue) {
        try { $first[$proc.Id] = [double]$proc.CPU } catch { $first[$proc.Id] = 0.0 }
    }
    Start-Sleep -Seconds 2
    $cpuCount = [math]::Max(1, [Environment]::ProcessorCount)
    $rows = @(Get-Process -ErrorAction SilentlyContinue | ForEach-Object {
        $cpuNow = 0.0
        try { if ($_.CPU) { $cpuNow = [double]$_.CPU } } catch {}
        $cpuPercent = 0.0
        if ($first.ContainsKey($_.Id) -and $cpuNow -ge $first[$_.Id]) { $cpuPercent = (($cpuNow - $first[$_.Id]) / 2 / $cpuCount) * 100 }
        [pscustomobject]@{ Process = $_.ProcessName; PID = $_.Id; CPUPercent = [math]::Round($cpuPercent, 1); MemoryMB = [math]::Round($_.WorkingSet64 / 1MB, 1); Window = $_.MainWindowTitle }
    })
    Write-Host "`nTop tiến trình theo RAM (ảnh chụp tại thời điểm đo):"
    $rows | Sort-Object MemoryMB -Descending | Select-Object -First 12 Process, PID, MemoryMB, CPUPercent, Window | Format-Table -AutoSize -Wrap
    Write-Host "`nTop tiến trình theo CPU trung bình trong 2 giây:"
    $rows | Sort-Object CPUPercent -Descending | Select-Object -First 12 Process, PID, CPUPercent, MemoryMB, Window | Format-Table -AutoSize -Wrap

    $apps = @(Get-InstalledApps | Where-Object { $_.SizeMB -gt 0 } | Sort-Object SizeMB -Descending | Select-Object -First 15)
    Write-Host "`nỨng dụng có dung lượng ước tính trong Windows (EstimatedSize):"
    if ($apps.Count) { $apps | Select-Object Name, Version, SizeMB, Publisher | Format-Table -AutoSize -Wrap }
    else { Write-Host 'Windows không cung cấp dung lượng cài đặt cho các ứng dụng đã đăng ký.' -ForegroundColor DarkYellow }
    Write-Host 'Dung lượng ứng dụng là số ước tính do bộ cài đăng ký; tiến trình có nhiều RAM/CPU không mặc định là malware.' -ForegroundColor DarkYellow
    Read-Host 'Nhấn Enter để tiếp tục'
}

function Invoke-NamedAction {
    param([Parameter(Mandatory)][string]$Name)
    switch ($Name.ToLowerInvariant()) {
        'boost' { Invoke-Boost }
        'restore' { Restore-Normal }
        default { Write-Host "Hành động không hợp lệ: $Name" -ForegroundColor Red }
    }
}

# Elevated hand-off: the parent process starts the app again with -Action and
# the elevated instance runs only that action, then exits.
if ($Action) {
    if (Test-Admin) {
        Invoke-NamedAction -Name $Action
    } elseif (Start-ElevatedInstance -TargetAction $Action) {
        exit 0
    } else {
        Read-Host 'Nhấn Enter để thoát'
        exit 1
    }
    exit 0
}

while ($true) {
    Show-Menu
    $choice = Read-Host
    if ($choice -match '^[Qq]$') {
        Write-Host "`nTạm biệt!"
        if ($isExe) { Read-Host 'Nhấn Enter để đóng cửa sổ' }
        break
    }

    # Options 1 and 2 change services, which needs administrator rights.
    $adminAction = switch ($choice) { '1' { 'boost' } '2' { 'restore' } default { $null } }
    if ($adminAction -and -not (Test-Admin)) {
        if (-not (Start-ElevatedInstance -TargetAction $adminAction)) {
            Write-Host 'Bỏ qua thao tác vì không có quyền quản trị.' -ForegroundColor Red
            Read-Host 'Nhấn Enter để tiếp tục'
        }
        continue
    }

    switch ($choice) {
        '1' { Invoke-Boost }
        '2' { Restore-Normal }
        '3' { Clean-System }
        '4' { View-Log }
        '5' { Remove-AI }
        '6' { Show-MalwareScan }
        '7' { Show-ResourceReport }
        default { Write-Host 'Lựa chọn không hợp lệ.' -ForegroundColor Red; Read-Host 'Nhấn Enter để tiếp tục' }
    }
}
