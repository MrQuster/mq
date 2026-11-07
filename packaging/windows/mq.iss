; =====================================================================
; MQ Launcher — Inno Setup Script
; High-Performance Multi-Threaded Minecraft Launcher
; Target: MQ-Setup-<version>.exe
; =====================================================================

#ifndef MyAppVersion
  #define MyAppVersion "2.2.0"
#endif

#ifndef MyAppName
  #define MyAppName "MQ"
#endif

#ifndef MyAppPublisher
  #define MyAppPublisher "MrQuster"
#endif

#ifndef MyAppURL
  #define MyAppURL "https://github.com/MrQuster/mq"
#endif

#ifndef MyAppExeName
  #define MyAppExeName "mq.exe"
#endif

#ifndef AppSourceDir
  #define AppSourceDir "..\..\target\release"
#endif

#ifndef AppOutputDir
  #define AppOutputDir "..\..\dist"
#endif

[Setup]
; Unique application GUID for per-user installation tracking & updates
AppId={{9D7C52A4-325A-4B6A-B68C-7C4E15F59D18}}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
AppUpdatesURL={#MyAppURL}/releases

; Per-user installation to %LOCALAPPDATA%\Programs\MQ (no UAC / elevation needed)
DefaultDirName={localappdata}\Programs\{#MyAppName}
DefaultGroupName={#MyAppName}
AllowNoIcons=yes
OutputDir={#AppOutputDir}
OutputBaseFilename=MQ-Setup-{#MyAppVersion}
SetupIconFile=mq.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName} Launcher

; Compression settings
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern

; Strict per-user mode — zero admin privileges required
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=commandline dialog
UsedUserAreasWarning=no

; Automatic process handling & silent capabilities
CloseApplications=yes
CloseApplicationsFilter=mq.exe
RestartApplications=no
DisableProgramGroupPage=yes
DisableWelcomePage=no
DisableDirPage=auto

; 64-bit architecture enforcement
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"

[CustomMessages]
english.CreateDesktopIcon=Create a &desktop shortcut
english.AdditionalIcons=Additional shortcuts:
english.LaunchProgram=Launch %1
english.AutoStartOption=Start %1 automatically on login
english.PreserveDataInfo=Note: All game instances, mod collections, and accounts in %LOCALAPPDATA%\MQ are preserved.
english.ClosingRunningProcess=Closing running instances of %1...

russian.CreateDesktopIcon=Создать &ярлык на рабочем столе
russian.AdditionalIcons=Дополнительные ярлыки:
russian.LaunchProgram=Запустить %1
russian.AutoStartOption=Автозапуск %1 при входе в Windows
russian.PreserveDataInfo=Примечание: Все сборки игры, моды и аккаунты в %LOCALAPPDATA%\MQ сохраняются.
russian.ClosingRunningProcess=Завершение запущенных процессов %1...

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; Main executable
Source: "{#AppSourceDir}\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
; Application icon
Source: "mq.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
; Start Menu shortcut
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\mq.ico"
; Optional Desktop shortcut
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\mq.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Clean up runtime artifacts within the program installation directory
Type: files; Name: "{app}\{#MyAppExeName}.old"
Type: files; Name: "{app}\*.log"
Type: files; Name: "{app}\*.tmp"
; Clean up app directory if empty (preserves user data directory in %LOCALAPPDATA%\MQ)
Type: dirifempty; Name: "{app}"

[Code]
// Helper function to safely terminate running instances of mq.exe
function TerminateMQProcess(): Boolean;
var
  ResultCode: Integer;
begin
  // Execute Windows taskkill quietly to close any active mq.exe process
  Exec('taskkill.exe', '/f /im mq.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Result := True;
end;

function InitializeSetup(): Boolean;
begin
  // Terminate any running MQ instances before installation begins
  TerminateMQProcess();
  // Brief pause to allow Windows to release file locks on mq.exe
  Sleep(300);
  Result := True;
end;

function InitializeUninstall(): Boolean;
begin
  // Terminate running MQ instances before uninstallation begins
  TerminateMQProcess();
  Sleep(300);
  Result := True;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
  begin
    // User data residing in %LOCALAPPDATA%\MQ and %APPDATA%\MQ is intentionally untouched
    // to safeguard instances, worlds, mod configurations, and credentials.
  end;
end;
