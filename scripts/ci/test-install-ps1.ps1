[CmdletBinding()]
param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
)

$ErrorActionPreference = "Stop"

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) {
        throw "ASSERTION FAILED: $Message"
    }
}

function Get-WorkspaceVersion {
    param([string]$CargoTomlPath)
    $content = Get-Content -Path $CargoTomlPath -Raw
    if ($content -match '(?ms)^\[workspace\.package\].*?^version\s*=\s*"([^"]+)"') {
        return $Matches[1]
    }
    throw "Could not parse workspace.package.version from $CargoTomlPath"
}

function Write-Sha256Sums {
    param([string]$Dir)
    $lines = @()
    Get-ChildItem -Path $Dir -File | Where-Object { $_.Name -notin @("SHA256SUMS", "SHA256SUMS.minisig") } | ForEach-Object {
        $hash = (Get-FileHash -Path $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        $lines += "$hash  $($_.Name)"
    }
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllLines((Join-Path $Dir "SHA256SUMS"), $lines, $utf8NoBom)
}

function Get-FreeTcpPort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $port = $listener.LocalEndpoint.Port
    $listener.Stop()
    return $port
}

Push-Location $RepoRoot
$tempRoot = $null
$serverJob = $null
try {
    $ver = Get-WorkspaceVersion -CargoTomlPath (Join-Path $RepoRoot "Cargo.toml")
    $releaseExe = Join-Path $RepoRoot "target\release\mq.exe"
    Assert-True (Test-Path $releaseExe) "Release binary not found at $releaseExe"

    $installPs1 = Join-Path $RepoRoot "packaging\windows\install.ps1"
    Assert-True (Test-Path $installPs1) "install.ps1 not found at $installPs1"

    $baseTemp = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
    $tempRoot = Join-Path $baseTemp ("mq-ps1-test-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
    $artifactsDir = Join-Path $tempRoot "artifacts"
    $portableStage = Join-Path $tempRoot "portable-stage"
    $installDir = Join-Path $tempRoot "installed"
    $badInstallDir = Join-Path $tempRoot "bad-installed"
    New-Item -ItemType Directory -Force -Path $artifactsDir, $portableStage | Out-Null

    # Prepare local release artifacts
    Copy-Item $releaseExe -Destination (Join-Path $artifactsDir "mq-windows-x86_64.exe")

    Copy-Item $releaseExe -Destination (Join-Path $portableStage "mq.exe")
    Copy-Item (Join-Path $RepoRoot "packaging\windows\portable.flag") -Destination (Join-Path $portableStage "portable.flag")
    Copy-Item (Join-Path $RepoRoot "packaging\windows\run.bat") -Destination (Join-Path $portableStage "run.bat")
    Copy-Item (Join-Path $RepoRoot "packaging\windows\README-portable.txt") -Destination (Join-Path $portableStage "README.txt")
    Compress-Archive -Path "$portableStage\*" -DestinationPath (Join-Path $artifactsDir "mq-windows-x86_64-portable.zip") -Force

    $setupSrc = Join-Path $RepoRoot "dist\MQ-Setup-$ver-x64.exe"
    if (Test-Path $setupSrc) {
        Copy-Item $setupSrc -Destination (Join-Path $artifactsDir "MQ-Setup-$ver-x64.exe")
    }

    Write-Sha256Sums -Dir $artifactsDir

    # Start local HTTP server for -BaseUrl
    $port = Get-FreeTcpPort
    $baseUrl = "http://127.0.0.1:$port"
    $serverJob = Start-Job -ScriptBlock {
        param($RootDir, $ListenPort)
        $listener = [System.Net.HttpListener]::new()
        $listener.Prefixes.Add("http://127.0.0.1:$ListenPort/")
        $listener.Start()
        try {
            while ($listener.IsListening) {
                $ctx = $listener.GetContext()
                $relPath = $ctx.Request.Url.AbsolutePath.TrimStart('/')
                if ([string]::IsNullOrWhiteSpace($relPath)) { $relPath = "SHA256SUMS" }
                $filePath = Join-Path $RootDir $relPath
                if (Test-Path $filePath -PathType Leaf) {
                    $bytes = [System.IO.File]::ReadAllBytes($filePath)
                    $ctx.Response.StatusCode = 200
                    $ctx.Response.ContentLength64 = $bytes.Length
                    $ctx.Response.OutputStream.Write($bytes, 0, $bytes.Length)
                } else {
                    $ctx.Response.StatusCode = 404
                }
                $ctx.Response.OutputStream.Close()
            }
        } finally {
            $listener.Stop()
        }
    } -ArgumentList $artifactsDir, $port

    # Wait briefly for HTTP listener readiness
    $ready = $false
    for ($i = 0; $i -lt 20; $i++) {
        try {
            Invoke-WebRequest -Uri "$baseUrl/SHA256SUMS" -UseBasicParsing -TimeoutSec 2 | Out-Null
            $ready = $true
            break
        } catch {
            Start-Sleep -Milliseconds 150
        }
    }
    Assert-True $ready "Local HTTP artifact server failed to start on $baseUrl"

    $cmdInfo = Get-Command $installPs1
    $hasBaseUrl = $cmdInfo.Parameters.ContainsKey("BaseUrl")
    Assert-True $hasBaseUrl "install.ps1 must support -BaseUrl parameter for local artifact testing"
    $hasSilent = $cmdInfo.Parameters.ContainsKey("Silent")
    $hasInstallDir = $cmdInfo.Parameters.ContainsKey("InstallDir")
    $hasNoDesktop = $cmdInfo.Parameters.ContainsKey("NoDesktopShortcut")

    $targetInstallDir = if ($hasInstallDir) { $installDir } else { Join-Path $env:LOCALAPPDATA "MQ-Portable" }
    $installedExe = Join-Path $targetInstallDir "mq.exe"

    # 1. Successful install
    Write-Host "Test 1: Successful install via install.ps1 -BaseUrl $baseUrl"
    $args1 = @("-NoProfile", "-File", $installPs1, "-BaseUrl", $baseUrl, "-Version", $ver, "-Portable")
    if ($hasInstallDir) { $args1 += @("-InstallDir", $installDir) }
    if ($hasNoDesktop) { $args1 += "-NoDesktopShortcut" }
    if ($hasSilent) { $args1 += "-Silent" }
    & pwsh @args1
    Assert-True ($LASTEXITCODE -eq 0) "install.ps1 failed on valid install (exit $LASTEXITCODE)"
    Assert-True (Test-Path $installedExe) "mq.exe not found at $installedExe after install.ps1"
    $verOut = (& $installedExe --version 2>&1) -join "`n"
    Assert-True ($verOut -like "*$ver*") "Installed mq.exe --version '$verOut' does not contain '$ver'"

    # 2. Failure on corrupted SHA256SUMS
    Write-Host "Test 2: Corrupted SHA256SUMS must fail"
    $origSums = Get-Content -Path (Join-Path $artifactsDir "SHA256SUMS") -Raw
    Set-Content -Path (Join-Path $artifactsDir "SHA256SUMS") -Value "0000000000000000000000000000000000000000000000000000000000000000  mq-windows-x86_64-portable.zip`n0000000000000000000000000000000000000000000000000000000000000000  mq-windows-x86_64.exe" -Encoding UTF8
    $args2 = @("-NoProfile", "-File", $installPs1, "-BaseUrl", $baseUrl, "-Version", $ver, "-Portable")
    if ($hasInstallDir) { $args2 += @("-InstallDir", $badInstallDir) }
    if ($hasNoDesktop) { $args2 += "-NoDesktopShortcut" }
    if ($hasSilent) { $args2 += "-Silent" }
    $null = & pwsh @args2 2>&1
    $badExit = $LASTEXITCODE
    Assert-True ($badExit -ne 0) "install.ps1 unexpectedly succeeded with corrupted SHA256SUMS"
    if ($hasInstallDir) {
        Assert-True (-not (Test-Path (Join-Path $badInstallDir "mq.exe"))) "mq.exe must not be installed when SHA256SUMS fails"
    }

    # 3. Re-run (idempotent re-install) with restored SHA256SUMS
    Write-Host "Test 3: Re-run install.ps1 on existing directory"
    Set-Content -Path (Join-Path $artifactsDir "SHA256SUMS") -Value $origSums -NoNewline -Encoding UTF8
    & pwsh @args1
    Assert-True ($LASTEXITCODE -eq 0) "install.ps1 failed on re-run (exit $LASTEXITCODE)"
    Assert-True (Test-Path $installedExe) "mq.exe missing after re-run"

    # 4. Uninstall
    Write-Host "Test 4: Uninstall via install.ps1 -Uninstall"
    $args4 = @("-NoProfile", "-File", $installPs1, "-Uninstall")
    if ($hasInstallDir) { $args4 += @("-InstallDir", $installDir) }
    if ($hasSilent) { $args4 += "-Silent" }
    & pwsh @args4
    Assert-True ($LASTEXITCODE -eq 0) "install.ps1 -Uninstall failed (exit $LASTEXITCODE)"
    if ($hasInstallDir) {
        Assert-True (-not (Test-Path $installedExe)) "mq.exe still exists after -Uninstall"
    }

    Write-Host "ALL INSTALL.PS1 TESTS PASSED ($ver)" -ForegroundColor Green
}
finally {
    if ($serverJob) {
        Stop-Job -Job $serverJob -ErrorAction SilentlyContinue
        Remove-Job -Job $serverJob -Force -ErrorAction SilentlyContinue
    }
    Pop-Location
    if ($tempRoot -and (Test-Path $tempRoot)) {
        Remove-Item -Recurse -Force $tempRoot -ErrorAction SilentlyContinue
    }
}
