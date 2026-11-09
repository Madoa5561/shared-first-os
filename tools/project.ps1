param(
    [ValidateSet('build', 'run', 'test-boot', 'check', 'verify')]
    [string]$Action = 'verify',
    [string]$Distribution = 'Ubuntu'
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location -LiteralPath $root
$lock = Get-Content -Raw -LiteralPath "$PSScriptRoot/versions.json" | ConvertFrom-Json
$logDirectory = Join-Path $root ("artifacts/commands-" + (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
New-Item -ItemType Directory -Force -Path $logDirectory | Out-Null
$script:step = 0

function Invoke-Recorded([string]$Executable, [string[]]$ArgumentList) {
    $script:step++
    $stem = Join-Path $logDirectory ("{0:D2}" -f $script:step)
    Write-Host (ConvertTo-Json -Compress -InputObject (@($Executable) + $ArgumentList))
    $output = & $Executable @ArgumentList 2>&1
    $code = $LASTEXITCODE
    $output | Out-File -LiteralPath "$stem.log" -Encoding utf8
    $output | ForEach-Object { Write-Host $_ }
    @{ command = @($Executable) + $ArgumentList; exit_code = $code; cwd = $root } |
        ConvertTo-Json -Depth 5 | Out-File -LiteralPath "$stem.json" -Encoding utf8
    if ($code -ne 0) { throw "Command failed (exit $code); see $stem.log" }
}

function Invoke-QemuTool([string[]]$ToolArguments) {
    Invoke-Recorded 'wsl.exe' (@('-d', $Distribution, '--cd', $root, '--exec', 'python3', 'tools/qemu.py') + $ToolArguments)
}

function Build-Kernel([switch]$PanicImage, [switch]$FaultImage, [switch]$KernelOnly) {
    $buildArguments = @('build', '--locked', '--offline', '--release', '--target', 'x86_64-unknown-none', '--bin', 'shared-first-kernel')
    $variant = 'normal'
    if ($PanicImage) { $buildArguments += @('--features', 'boot-panic'); $variant = 'panic' }
    if ($FaultImage) { $buildArguments += @('--features', 'boot-unexpected-fault'); $variant = 'fault' }
    Invoke-Recorded 'cargo' $buildArguments
    if (-not $KernelOnly) { Invoke-QemuTool @('image', '--variant', $variant) }
}

try {
    $rustVersion = & rustc --version
    if ($LASTEXITCODE -ne 0 -or $rustVersion -notmatch ('^rustc ' + [regex]::Escape($lock.rust) + ' ')) {
        throw "Expected Rust $($lock.rust), got $rustVersion. See docs/BUILD.md."
    }
    Invoke-Recorded 'rustc' @('-vV')
    if ($Action -in @('check', 'verify')) {
        Invoke-Recorded 'cargo' @('fmt', '--check')
        Invoke-Recorded 'cargo' @('test', '--locked', '--offline', '--lib')
        Invoke-Recorded 'cargo' @('clippy', '--locked', '--offline', '--lib', '--tests', '--', '-D', 'warnings')
        Invoke-Recorded 'cargo' @('clippy', '--locked', '--offline', '--target', 'x86_64-unknown-none', '--bin', 'shared-first-kernel', '--all-features', '--', '-D', 'warnings')
        Invoke-Recorded 'wsl.exe' @('-d', $Distribution, '--cd', $root, '--exec', 'python3', '-B', '-m', 'unittest', 'discover', '-s', 'tools', '-p', 'test_*.py', '-v')
    }
    if ($Action -ne 'check') { Build-Kernel }
    if ($Action -in @('run', 'test-boot', 'verify')) { Invoke-QemuTool @('test', '--memory', '128') }
    if ($Action -eq 'verify') {
        Invoke-QemuTool @('test', '--memory', '256')
        Invoke-QemuTool @('test-timeout', '--timeout', '2')
        Build-Kernel -PanicImage
        Invoke-QemuTool @('test', '--variant', 'panic', '--expect-panic')
        Build-Kernel -FaultImage
        Invoke-QemuTool @('test', '--variant', 'fault', '--expect-fault')
        # Restore the normal ELF, retaining the exact ISO already boot-tested.
        Build-Kernel -KernelOnly
    }
    Write-Host "PASS: $Action; command evidence: $logDirectory"
} catch {
    Write-Error $_ -ErrorAction Continue
    exit 1
}
