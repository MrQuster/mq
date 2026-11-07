<#
.SYNOPSIS
    Builds the standalone portable ZIP package for MQ Launcher on Windows.
.DESCRIPTION
    Packages mq.exe, portable.flag, mq.ico, run.bat, and README.txt into
    dist/mq-windows-x86_64-portable.zip.
.PARAMETER BinaryPath
    Optional explicit path to mq.exe. If omitted, checks target/release paths.
#>
[CmdletBinding()]
param(
    [string]$BinaryPath = ""
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Resolve-Path (Join-Path $scriptDir "..\..")
$distDir = Join-Path $repoRoot "dist"
$packageName = "mq-windows-x86_64-portable"
$archivePath = Join-Path $distDir "$packageName.zip"

Write-Host "=== Packaging MQ Windows Portable Archive ===" -ForegroundColor Cyan

if (-not $BinaryPath) {
    $candidates = @(
        (Join-Path $repoRoot "target\x86_64-pc-windows-msvc\release\mq.exe"),
        (Join-Path $repoRoot "target\release\mq.exe")
    )
    foreach ($cand in $candidates) {
        if (Test-Path $cand) {
            $BinaryPath = $cand
            break
        }
    }
}

if (-not $BinaryPath -or -not (Test-Path $BinaryPath)) {
    throw "mq.exe not found! Please build first: cargo build --release`nOr provide -BinaryPath"
}

Write-Host "Source binary: $BinaryPath" -ForegroundColor Gray

if (-not (Test-Path $distDir)) {
    New-Item -ItemType Directory -Path $distDir -Force | Out-Null
}

$tempStage = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
$stageTarget = Join-Path $tempStage $packageName

try {
    New-Item -ItemType Directory -Path $stageTarget -Force | Out-Null

    Copy-Item -Path $BinaryPath -Destination (Join-Path $stageTarget "mq.exe") -Force
    Copy-Item -Path (Join-Path $scriptDir "portable.flag") -Destination (Join-Path $stageTarget "portable.flag") -Force
    Copy-Item -Path (Join-Path $scriptDir "mq.ico") -Destination (Join-Path $stageTarget "mq.ico") -Force
    Copy-Item -Path (Join-Path $scriptDir "run.bat") -Destination (Join-Path $stageTarget "run.bat") -Force
    Copy-Item -Path (Join-Path $scriptDir "README-portable.txt") -Destination (Join-Path $stageTarget "README.txt") -Force

    if (Test-Path $archivePath) {
        Remove-Item -Path $archivePath -Force
    }

    Write-Host "Compressing to $archivePath..." -ForegroundColor Cyan
    Compress-Archive -Path $stageTarget -DestinationPath $archivePath -CompressionLevel Optimal

    $hash = (Get-FileHash -Path $archivePath -Algorithm SHA256).Hash.ToLower()
    Write-Host "[OK] Archive created: $archivePath" -ForegroundColor Green
    Write-Host "SHA-256: $hash" -ForegroundColor Yellow
}
finally {
    if (Test-Path $tempStage) {
        Remove-Item -Path $tempStage -Recurse -Force -ErrorAction SilentlyContinue
    }
}
