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

function Resolve-Iscc {
    $cmd = Get-Command iscc -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }

    $defaultPath = "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"
    if (Test-Path $defaultPath) { return $defaultPath }

    Write-Host "iscc not found; installing Inno Setup via Chocolatey..."
    choco install innosetup --no-progress -y
    if ($LASTEXITCODE -ne 0) {
        throw "choco install innosetup failed with exit code $LASTEXITCODE"
    }
    if (Test-Path $defaultPath) { return $defaultPath }
    throw "ISCC.exe not found even after installing Inno Setup"
}

function Invoke-CheckedProcess {
    param(
        [string]$FilePath,
        [string[]]$ArgumentList,
        [string]$Description
    )
    Write-Host "Running ${Description}: $FilePath $($ArgumentList -join ' ')"
    $proc = Start-Process -FilePath $FilePath -ArgumentList $ArgumentList -Wait -PassThru
    if ($proc.ExitCode -ne 0) {
        throw "$Description failed with exit code $($proc.ExitCode)"
    }
}

Push-Location $RepoRoot
$tempRoot = $null
try {
    $ver = Get-WorkspaceVersion -CargoTomlPath (Join-Path $RepoRoot "Cargo.toml")
    Write-Host "Testing Windows installer for MQ version $ver"

    $iscc = Resolve-Iscc
    $issPath = Join-Path $RepoRoot "packaging\windows\mq.iss"
    $distDir = Join-Path $RepoRoot "dist"
    New-Item -ItemType Directory -Force -Path $distDir | Out-Null

    # 1. Compile current version installer
    & $iscc "/DMyAppVersion=$ver" $issPath
    if ($LASTEXITCODE -ne 0) {
        throw "iscc failed compiling current version with exit code $LASTEXITCODE"
    }

    $setupExe = Join-Path $distDir "MQ-Setup-$ver-x64.exe"
    Assert-True (Test-Path $setupExe) "Expected installer at $setupExe"

    $baseTemp = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
    $tempRoot = Join-Path $baseTemp ("mq-inst-test-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
    $installDir = Join-Path $tempRoot "mq-inst"
    $oldDistDir = Join-Path $tempRoot "old-dist"
    New-Item -ItemType Directory -Force -Path $oldDistDir | Out-Null

    $dataDir = Join-Path $env:LOCALAPPDATA "MQ"
    $keepDir = Join-Path $dataDir "instances"
    $keepFile = Join-Path $keepDir "keep.txt"
    $uninstallRegPath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{9D7C52A4-325A-4B6A-B68C-7C4E15F59D18}_is1"

    # 2. Test upgrade-in-place: install previous version (2.2.0), create user data, upgrade to $ver
    $prevVer = "2.2.0"
    & $iscc "/DMyAppVersion=$prevVer" "/O$oldDistDir" $issPath
    if ($LASTEXITCODE -ne 0) {
        throw "iscc failed compiling previous version with exit code $LASTEXITCODE"
    }
    $oldSetupExe = Join-Path $oldDistDir "MQ-Setup-$prevVer-x64.exe"
    Assert-True (Test-Path $oldSetupExe) "Expected previous installer at $oldSetupExe"

    Invoke-CheckedProcess -FilePath $oldSetupExe -ArgumentList @(
        "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/DIR=$installDir"
    ) -Description "Install previous version $prevVer"

    New-Item -ItemType Directory -Force -Path $keepDir | Out-Null
    Set-Content -Path $keepFile -Value "preserve-across-upgrade" -Encoding UTF8

    Invoke-CheckedProcess -FilePath $setupExe -ArgumentList @(
        "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/DIR=$installDir"
    ) -Description "Upgrade in-place to $ver"

    Assert-True (Test-Path $keepFile) "User data file $keepFile must be preserved after upgrade"
    Assert-True ((Get-Content $keepFile -Raw).Trim() -eq "preserve-across-upgrade") "User data content must be intact after upgrade"

    # 3. Verify installed files, Start Menu shortcut, registry, .mrpack association, and --version
    $installedExe = Join-Path $installDir "mq.exe"
    $licensesFile = Join-Path $installDir "THIRD_PARTY_LICENSES.md"
    Assert-True (Test-Path $installedExe) "Installed mq.exe missing at $installedExe"
    Assert-True (Test-Path $licensesFile) "THIRD_PARTY_LICENSES.md missing at $licensesFile"

    $programsDir = [Environment]::GetFolderPath("Programs")
    $shortcut = Get-ChildItem -Path $programsDir -Filter "MQ.lnk" -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
    Assert-True ($null -ne $shortcut) "Start Menu shortcut MQ.lnk not found under $programsDir"

    Assert-True (Test-Path $uninstallRegPath) "Uninstall registry key not found at $uninstallRegPath"
    $regProps = Get-ItemProperty -Path $uninstallRegPath
    Assert-True ($regProps.Publisher -eq "MrQuster") "Registry Publisher expected 'MrQuster', got '$($regProps.Publisher)'"
    Assert-True ($regProps.DisplayVersion -eq $ver) "Registry DisplayVersion expected '$ver', got '$($regProps.DisplayVersion)'"

    $mrpackReg = "HKCU:\Software\Classes\.mrpack"
    Assert-True (Test-Path $mrpackReg) ".mrpack association key missing at $mrpackReg"
    $progId = (Get-ItemProperty -Path $mrpackReg).'(default)'
    Assert-True (-not [string]::IsNullOrWhiteSpace($progId)) ".mrpack ProgId is empty"
    $cmdKey = "HKCU:\Software\Classes\$progId\shell\open\command"
    Assert-True (Test-Path $cmdKey) ".mrpack open command key missing at $cmdKey"
    $openCmd = (Get-ItemProperty -Path $cmdKey).'(default)'
    Assert-True ($openCmd -like "*mq.exe*") ".mrpack open command '$openCmd' does not reference mq.exe"

    $verOutFile = Join-Path $tempRoot "mq-version.txt"
    $verProc = Start-Process -FilePath $installedExe -ArgumentList "--version" -NoNewWindow -Wait -PassThru -RedirectStandardOutput $verOutFile
    Assert-True ($verProc.ExitCode -eq 0) "mq.exe --version exited with code $($verProc.ExitCode)"
    $verOutput = if (Test-Path $verOutFile) { (Get-Content $verOutFile -Raw).Trim() } else { "" }
    $peProductVer = (Get-Item $installedExe).VersionInfo.ProductVersion
    Assert-True (($verOutput -like "*$ver*") -or ($peProductVer -like "*$ver*")) "mq.exe version check failed: stdout='$verOutput', PE='$peProductVer', expected '$ver'"

    # 4. Test silent uninstall without /PURGE=1 -> user data must be preserved
    $uninstaller = Join-Path $installDir "unins000.exe"
    Assert-True (Test-Path $uninstaller) "Uninstaller not found at $uninstaller"
    Invoke-CheckedProcess -FilePath $uninstaller -ArgumentList @(
        "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"
    ) -Description "Silent uninstall (preserve data)"

    Assert-True (Test-Path $keepFile) "User data $keepFile must be preserved after normal silent uninstall"
    Assert-True (-not (Test-Path $installedExe)) "mq.exe should be removed after uninstall"

    # 5. Re-install and test silent uninstall with /PURGE=1 -> data, files, registry all removed
    Invoke-CheckedProcess -FilePath $setupExe -ArgumentList @(
        "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/DIR=$installDir"
    ) -Description "Re-install $ver for /PURGE=1 test"

    Assert-True (Test-Path $keepFile) "keep.txt should still exist before /PURGE=1"
    $uninstaller = Join-Path $installDir "unins000.exe"
    Invoke-CheckedProcess -FilePath $uninstaller -ArgumentList @(
        "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/PURGE=1"
    ) -Description "Silent uninstall with /PURGE=1"

    Assert-True (-not (Test-Path $dataDir)) "Data directory $dataDir must be removed by /PURGE=1"
    Assert-True (-not (Test-Path $installedExe)) "mq.exe must be removed by /PURGE=1"
    Assert-True (-not (Test-Path $uninstallRegPath)) "Uninstall registry key must be removed by /PURGE=1"
    Assert-True (-not (Test-Path $mrpackReg)) ".mrpack registry association must be removed by /PURGE=1"

    Write-Host "ALL WINDOWS INSTALLER TESTS PASSED ($ver)" -ForegroundColor Green
}
finally {
    Pop-Location
    if ($tempRoot -and (Test-Path $tempRoot)) {
        Remove-Item -Recurse -Force $tempRoot -ErrorAction SilentlyContinue
    }
}
