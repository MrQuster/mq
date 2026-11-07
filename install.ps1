# ==============================================================================
#  MQ Launcher — 1-Command Universal Windows Installer
#  Usage:
#    irm https://github.com/MrQuster/mq/releases/latest/download/install.ps1 | iex
#  Advanced:
#    & ([scriptblock]::Create((irm https://github.com/MrQuster/mq/releases/latest/download/install.ps1))) -Portable
#    & ([scriptblock]::Create((irm https://github.com/MrQuster/mq/releases/latest/download/install.ps1))) -Uninstall
#    & ([scriptblock]::Create((irm https://github.com/MrQuster/mq/releases/latest/download/install.ps1))) -Version 2.2.0
# ==============================================================================

[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [string]$Version = "",

    [switch]$Portable,

    [switch]$Uninstall,

    [string]$InstallDir = "",

    [switch]$NoDesktopShortcut,

    [switch]$Force,

    [switch]$Help
)

$ErrorActionPreference = "Stop"

# Fallback parameter bindings via environment variables for pipe invocation
if (-not $Version -and $env:MQ_VERSION) { $Version = $env:MQ_VERSION }
if (-not $Version) { $Version = "latest" }

if (-not $Portable -and ($env:MQ_PORTABLE -eq "1" -or $env:MQ_PORTABLE -eq "true")) { $Portable = $true }
if (-not $Uninstall -and ($env:MQ_UNINSTALL -eq "1" -or $env:MQ_UNINSTALL -eq "true")) { $Uninstall = $true }
if (-not $InstallDir -and $env:MQ_INSTALL_DIR) { $InstallDir = $env:MQ_INSTALL_DIR }

# Display help if requested
if ($Help) {
    Write-Host @"
MQ Launcher — Windows Installer
Usage:
  install.ps1 [-Version <tag>] [-Portable] [-Uninstall] [-InstallDir <path>] [-NoDesktopShortcut] [-Force]

Parameters:
  -Version <tag>       Specify version to install (e.g. '2.2.0' or 'latest'). Default: latest
  -Portable            Install standalone portable package instead of system setup
  -Uninstall           Silently uninstall MQ Launcher (preserves %LOCALAPPDATA%\MQ user data)
  -InstallDir <path>   Custom installation target directory
  -NoDesktopShortcut   Do not create a shortcut on Desktop
  -Force               Ignore active running instances or skip checksum validation warnings
"@ -ForegroundColor Cyan
    exit 0
}

# 1. Enforce modern secure TLS protocols (TLS 1.2 & TLS 1.3)
try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    if ([Enum]::IsDefined([Net.SecurityProtocolType], "Tls13")) {
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls13
    }
} catch {
    # Fallback to direct bit flags if runtime enum is older
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor 3072 -bor 12288
}

# Repository details
$Repo = if ($env:MQ_REPO) { $env:MQ_REPO } else { "MrQuster/mq" }
$AppName = "MQ Launcher"
$DefaultInstallDir = Join-Path $env:LOCALAPPDATA "Programs\MQ"
$UserDataDir = Join-Path $env:LOCALAPPDATA "MQ"

# Print banner
Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "        MQ Launcher — Solid Edition (1-Command Windows Installer)                " -ForegroundColor Cyan
Write-Host "================================================================================" -ForegroundColor Cyan

# Helper function to close running instances
function Stop-RunningMQ {
    $processes = Get-Process -Name "mq" -ErrorAction SilentlyContinue
    if ($processes) {
        Write-Host "Closing running instances of $AppName..." -ForegroundColor Yellow
        foreach ($proc in $processes) {
            try {
                Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
            } catch {}
        }
        Start-Sleep -Milliseconds 300
    }
}

# Helper to download file safely with WebClient or Invoke-WebRequest
function Download-FileWithFallback {
    param(
        [string]$Url,
        [string]$Destination
    )
    try {
        Invoke-WebRequest -Uri $Url -OutFile $Destination -UseBasicParsing
    } catch {
        $wc = New-Object System.Net.WebClient
        $wc.DownloadFile($Url, $Destination)
    }
}

# ------------------------------------------------------------------------------
# Action: UNINSTALL
# ------------------------------------------------------------------------------
if ($Uninstall) {
    Write-Host "[1/2] Searching for existing installation of $AppName..." -ForegroundColor Cyan

    Stop-RunningMQ

    $uninstalled = $false

    # Check Windows Registry for Inno Setup uninstaller
    $registryKeys = @(
        "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*",
        "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*"
    )
    foreach ($regKey in $registryKeys) {
        $match = Get-ItemProperty -Path $regKey -ErrorAction SilentlyContinue |
            Where-Object { $_.DisplayName -like "*MQ*" -and $_.UninstallString } |
            Select-Object -First 1

        if ($match) {
            $rawCmd = $match.UninstallString.Trim('"')
            if (Test-Path $rawCmd) {
                Write-Host "Running uninstaller: $rawCmd" -ForegroundColor Yellow
                $proc = Start-Process -FilePath $rawCmd -ArgumentList "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART" -Wait -PassThru -NoNewWindow
                $uninstalled = $true
                break
            }
        }
    }

    # Fallback to default unins000.exe
    if (-not $uninstalled) {
        $defaultUnins = Join-Path $DefaultInstallDir "unins000.exe"
        if (Test-Path $defaultUnins) {
            Write-Host "Running default uninstaller: $defaultUnins" -ForegroundColor Yellow
            $proc = Start-Process -FilePath $defaultUnins -ArgumentList "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART" -Wait -PassThru -NoNewWindow
            $uninstalled = $true
        }
    }

    # If portable was installed in custom or default portable dir
    $portableDir = if ($InstallDir) { $InstallDir } else { Join-Path $env:LOCALAPPDATA "Programs\MQ-Portable" }
    if (Test-Path $portableDir) {
        Write-Host "Removing portable installation at $portableDir..." -ForegroundColor Yellow
        Remove-Item -Path $portableDir -Recurse -Force -ErrorAction SilentlyContinue
        $uninstalled = $true
    }

    # Clean up shortcuts if leftover
    $shortcuts = @(
        (Join-Path ([Environment]::GetFolderPath("Programs")) "MQ.lnk"),
        (Join-Path ([Environment]::GetFolderPath("Desktop")) "MQ.lnk")
    )
    foreach ($sc in $shortcuts) {
        if (Test-Path $sc) {
            Remove-Item -Path $sc -Force -ErrorAction SilentlyContinue
        }
    }

    Write-Host "`n[2/2] Uninstall complete!" -ForegroundColor Green
    Write-Host "User game instances and account data preserved at: $UserDataDir" -ForegroundColor Gray
    exit 0
}

# ------------------------------------------------------------------------------
# Action: INSTALL (Setup or Portable)
# ------------------------------------------------------------------------------

# Resolve download base URL
$BaseDownloadUrl = if ($Version -eq "latest") {
    "https://github.com/$Repo/releases/latest/download"
} else {
    $tag = if ($Version.StartsWith("v")) { $Version } else { "v$Version" }
    "https://github.com/$Repo/releases/download/$tag"
}

$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("mq-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $TempDir -Force | Out-Null

try {
    # Step 1: Download SHA256SUMS
    Write-Host "[1/4] Fetching checksums from GitHub Releases ($Version)..." -ForegroundColor Cyan
    $SumsFile = Join-Path $TempDir "SHA256SUMS"
    $SumsUrl = "$BaseDownloadUrl/SHA256SUMS"

    $checksums = @{}
    try {
        Download-FileWithFallback -Url $SumsUrl -Destination $SumsFile
        if (Test-Path $SumsFile) {
            Get-Content -Path $SumsFile | ForEach-Object {
                $line = $_.Trim()
                if ($line -and -not $line.StartsWith("#")) {
                    if ($line -match '^([a-fA-F0-9]{64})\s+\*?(.+)$') {
                        $hash = $matches[1].ToLower()
                        $fname = [System.IO.Path]::GetFileName($matches[2].Trim())
                        $checksums[$fname] = $hash
                    }
                }
            }
        }
    } catch {
        if (-not $Force) {
            Write-Host "Notice: Could not retrieve SHA256SUMS directly (using latest release artifacts)." -ForegroundColor Yellow
        }
    }

    # Step 2: Determine target artifact name & download
    Write-Host "[2/4] Downloading $AppName payload..." -ForegroundColor Cyan

    if ($Portable) {
        # Portable mode
        $artifactName = "mq-windows-x86_64-portable.zip"
        foreach ($k in $checksums.Keys) {
            if ($k -match '(?i)mq-.*portable.*\.zip$') {
                $artifactName = $k
                break
            }
        }

        $downloadUrl = "$BaseDownloadUrl/$artifactName"
        $downloadFile = Join-Path $TempDir $artifactName

        Write-Host "Downloading $downloadUrl..." -ForegroundColor Gray
        Download-FileWithFallback -Url $downloadUrl -Destination $downloadFile

        # Step 3: Verify SHA-256
        Write-Host "[3/4] Verifying SHA-256 integrity..." -ForegroundColor Cyan
        $computedHash = (Get-FileHash -Path $downloadFile -Algorithm SHA256).Hash.ToLower()

        if ($checksums.ContainsKey($artifactName)) {
            $expectedHash = $checksums[$artifactName]
            if ($computedHash -ne $expectedHash) {
                throw "SHA-256 mismatch for $artifactName!`nExpected: $expectedHash`nActual:   $computedHash"
            }
            Write-Host "[OK] SHA-256 verified: $computedHash" -ForegroundColor Green
        } else {
            Write-Host "Warning: $artifactName not found in SHA256SUMS; computed: $computedHash" -ForegroundColor Yellow
        }

        # Step 4: Extract portable package
        Write-Host "[4/4] Extracting portable package..." -ForegroundColor Cyan
        Stop-RunningMQ

        $targetDir = if ($InstallDir) { $InstallDir } else {
            Join-Path $env:LOCALAPPDATA "Programs\MQ-Portable"
        }
        if (-not (Test-Path $targetDir)) {
            New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
        }

        Expand-Archive -Path $downloadFile -DestinationPath $targetDir -Force

        # Ensure portable.flag is in place
        $flagPath = Join-Path $targetDir "portable.flag"
        if (-not (Test-Path $flagPath)) {
            Set-Content -Path $flagPath -Value "# MQ Portable Mode" -Encoding UTF8
        }

        Write-Host "`n================================================================================" -ForegroundColor Green
        Write-Host " MQ LAUNCHER (PORTABLE) SUCCESSFULLY INSTALLED!" -ForegroundColor Green
        Write-Host "================================================================================" -ForegroundColor Green
        Write-Host " Directory: $targetDir" -ForegroundColor Cyan
        Write-Host " Run:       $(Join-Path $targetDir 'mq.exe')" -ForegroundColor Cyan
        Write-Host "================================================================================`n" -ForegroundColor Green

    } else {
        # Standard Inno Setup mode
        $artifactName = "MQ-Setup.exe"
        foreach ($k in $checksums.Keys) {
            if ($k -match '(?i)^MQ-Setup.*\.exe$') {
                $artifactName = $k
                break
            }
        }

        $downloadUrl = "$BaseDownloadUrl/$artifactName"
        $downloadFile = Join-Path $TempDir $artifactName

        Write-Host "Downloading $downloadUrl..." -ForegroundColor Gray
        Download-FileWithFallback -Url $downloadUrl -Destination $downloadFile

        # Step 3: Verify SHA-256
        Write-Host "[3/4] Verifying SHA-256 integrity..." -ForegroundColor Cyan
        $computedHash = (Get-FileHash -Path $downloadFile -Algorithm SHA256).Hash.ToLower()

        if ($checksums.ContainsKey($artifactName)) {
            $expectedHash = $checksums[$artifactName]
            if ($computedHash -ne $expectedHash) {
                throw "SHA-256 mismatch for $artifactName!`nExpected: $expectedHash`nActual:   $computedHash"
            }
            Write-Host "[OK] SHA-256 verified: $computedHash" -ForegroundColor Green
        } else {
            Write-Host "Warning: $artifactName not found in SHA256SUMS; computed: $computedHash" -ForegroundColor Yellow
        }

        # Step 4: Run silent Inno Setup installation
        Write-Host "[4/4] Executing silent installation..." -ForegroundColor Cyan
        Stop-RunningMQ

        $setupArgs = @(
            "/VERYSILENT",
            "/SUPPRESSMSGBOXES",
            "/NORESTART",
            "/SP-"
        )
        if ($InstallDir) {
            $setupArgs += "/DIR=$InstallDir"
        }
        if ($NoDesktopShortcut) {
            $setupArgs += "/TASKS=!desktopicon"
        }

        $proc = Start-Process -FilePath $downloadFile -ArgumentList $setupArgs -Wait -PassThru -NoNewWindow
        if ($proc.ExitCode -ne 0) {
            throw "Installation failed with exit code $($proc.ExitCode)"
        }

        $installedExe = if ($InstallDir) { Join-Path $InstallDir "mq.exe" } else { Join-Path $DefaultInstallDir "mq.exe" }

        Write-Host "`n================================================================================" -ForegroundColor Green
        Write-Host " MQ LAUNCHER SUCCESSFULLY INSTALLED!" -ForegroundColor Green
        Write-Host "================================================================================" -ForegroundColor Green
        Write-Host " Executable: $installedExe" -ForegroundColor Cyan
        Write-Host " Start Menu: MQ" -ForegroundColor Cyan
        Write-Host " User Data:  $UserDataDir (Preserved on uninstalls and updates)" -ForegroundColor Gray
        Write-Host "================================================================================`n" -ForegroundColor Green
    }
}
finally {
    # Clean up temporary downloads
    if (Test-Path $TempDir) {
        Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}
