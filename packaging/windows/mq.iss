; =====================================================================
; MQ Launcher — Inno Setup 6 Script
; High-Performance Multi-Threaded Minecraft Launcher
; Target: MQ-Setup-<version>-x64.exe
; =====================================================================

#ifndef MyAppVersion
  #define MyAppVersion GetEnv("APP_VERSION")
#endif
#if MyAppVersion == ""
  #sub ReadCargoVersion
    #define private Line Trim(FileRead(CargoFile))
    #if Pos('version = "', Line) == 1 && MyAppVersion == ""
      #define private Rest Copy(Line, 12, 64)
      #expr MyAppVersion = Copy(Rest, 1, Pos('"', Rest) - 1)
    #endif
  #endsub
  #define CargoFile FileOpen("..\..\Cargo.toml")
  #if CargoFile
    #for {0; !FileEof(CargoFile) && MyAppVersion == ""; 0} ReadCargoVersion
    #expr FileClose(CargoFile)
  #endif
  #if MyAppVersion == ""
    #expr MyAppVersion = "2.6.0"
  #endif
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
AppId={{9D7C52A4-325A-4B6A-B68C-7C4E15F59D18}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
AppUpdatesURL={#MyAppURL}/releases

; Windows 10+ 64-bit
MinVersion=10.0
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

; Per-user installation to %LOCALAPPDATA%\Programs\MQ by default (no UAC needed)
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
AllowNoIcons=yes
OutputDir={#AppOutputDir}
OutputBaseFilename=MQ-Setup-{#MyAppVersion}-x64
SetupIconFile=mq.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName} Launcher

#if FileExists("..\..\LICENSE")
LicenseFile=..\..\LICENSE
#endif

; Modern wizard appearance (matte graphite + emerald assets)
WizardStyle=modern
WizardImageFile=wizard-large.bmp
WizardSmallImageFile=wizard-small.bmp

; Compression settings
Compression=lzma2/ultra64
SolidCompression=yes

; Strict per-user mode by default, with optional dialog override
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
UsedUserAreasWarning=no

; File associations (.mrpack only; .zip is never intercepted)
ChangesAssociations=yes

; Automatic process handling for in-place updates (only mq.exe, never java.exe/javaw.exe)
CloseApplications=yes
CloseApplicationsFilter=mq.exe
RestartApplications=no
DisableProgramGroupPage=yes
DisableWelcomePage=no
DisableDirPage=auto

[Languages]
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "ukrainian"; MessagesFile: "compiler:Languages\Ukrainian.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
english.CreateStartMenuIcon=Create a &Start Menu shortcut
english.CreateDesktopIcon=Create a &desktop shortcut
english.AdditionalIcons=Additional shortcuts:
english.FileAssociations=File associations:
english.AssocMrpack=Associate .mrpack files with %1
english.MrpackFileDescription=Modrinth Modpack
english.LaunchProgram=Launch %1
english.ClosingRunningProcess=Closing running instances of %1...
english.UninstallPurgePrompt=Delete launcher data (instances, settings, accounts)?%n%nPaths:%n  %1%n  %2%n%nTotal size: %3 MB

russian.CreateStartMenuIcon=Создать ярлык в меню «&Пуск»
russian.CreateDesktopIcon=Создать &ярлык на рабочем столе
russian.AdditionalIcons=Дополнительные ярлыки:
russian.FileAssociations=Ассоциации файлов:
russian.AssocMrpack=Ассоциировать файлы .mrpack с %1
russian.MrpackFileDescription=Модпак Modrinth
russian.LaunchProgram=Запустить %1
russian.ClosingRunningProcess=Завершение запущенных процессов %1...
russian.UninstallPurgePrompt=Удалить данные лаунчера (инстансы, настройки, аккаунты)?%n%nПути:%n  %1%n  %2%n%nОбщий размер: %3 МБ

ukrainian.CreateStartMenuIcon=Створити ярлик у меню «&Пуск»
ukrainian.CreateDesktopIcon=Створити &ярлик на робочому столі
ukrainian.AdditionalIcons=Додаткові ярлики:
ukrainian.FileAssociations=Асоціації файлів:
ukrainian.AssocMrpack=Асоціювати файли .mrpack з %1
ukrainian.MrpackFileDescription=Модпак Modrinth
ukrainian.LaunchProgram=Запустити %1
ukrainian.ClosingRunningProcess=Завершення запущених процесів %1...
ukrainian.UninstallPurgePrompt=Видалити дані лаунчера (інстанси, налаштування, акаунти)?%n%nШляхи:%n  %1%n  %2%n%nЗагальний розмір: %3 МБ

[Tasks]
Name: "startmenu"; Description: "{cm:CreateStartMenuIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "assocmrpack"; Description: "{cm:AssocMrpack,{#MyAppName}}"; GroupDescription: "{cm:FileAssociations}"

[Files]
Source: "{#AppSourceDir}\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "mq.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\THIRD_PARTY_LICENSES.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\mq.ico"; Tasks: startmenu
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\mq.ico"; Tasks: desktopicon

[Registry]
; Register .mrpack association only (never intercept .zip)
Root: HKA; Subkey: "Software\Classes\.mrpack"; ValueType: string; ValueName: ""; ValueData: "MQ.Modpack"; Flags: uninsdeletekeyifempty uninsdeletevalue; Tasks: assocmrpack
Root: HKA; Subkey: "Software\Classes\MQ.Modpack"; ValueType: string; ValueName: ""; ValueData: "{cm:MrpackFileDescription}"; Flags: uninsdeletekey; Tasks: assocmrpack
Root: HKA; Subkey: "Software\Classes\MQ.Modpack\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\mq.ico,0"; Tasks: assocmrpack
Root: HKA; Subkey: "Software\Classes\MQ.Modpack\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""; Tasks: assocmrpack

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Clean up runtime artifacts within the program installation directory
Type: files; Name: "{app}\{#MyAppExeName}.old"
Type: files; Name: "{app}\*.log"
Type: files; Name: "{app}\*.tmp"
; Clean up app directory if empty (user data in %LOCALAPPDATA%\MQ and %APPDATA%\MQ is handled in [Code])
Type: dirifempty; Name: "{app}"

[Code]
// Safely terminate only running instances of mq.exe (never touch java.exe / javaw.exe)
function TerminateMQProcess(): Boolean;
var
  ResultCode: Integer;
begin
  Exec('taskkill.exe', '/im mq.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Sleep(200);
  Exec('taskkill.exe', '/f /im mq.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Sleep(200);
  Result := True;
end;

function GetDirSizeBytes(const DirPath: string): Extended;
var
  FindRec: TFindRec;
  FullPath: string;
begin
  Result := 0.0;
  if not DirExists(DirPath) then
    Exit;

  if FindFirst(AddBackslash(DirPath) + '*', FindRec) then
  begin
    try
      repeat
        if (FindRec.Name <> '.') and (FindRec.Name <> '..') then
        begin
          FullPath := AddBackslash(DirPath) + FindRec.Name;
          if (FindRec.Attributes and FILE_ATTRIBUTE_DIRECTORY) <> 0 then
            Result := Result + GetDirSizeBytes(FullPath)
          else
            Result := Result + (FindRec.SizeHigh * 4294967296.0) + FindRec.SizeLow;
        end;
      until not FindNext(FindRec);
    finally
      FindClose(FindRec);
    end;
  end;
end;

function InitializeSetup(): Boolean;
begin
  TerminateMQProcess();
  Result := True;
end;

function PrepareToInstall(var NeedsRestart: Boolean): string;
begin
  TerminateMQProcess();
  Result := '';
end;

function InitializeUninstall(): Boolean;
begin
  TerminateMQProcess();
  Result := True;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  LocalDataDir: string;
  RoamingDataDir: string;
  TotalBytes: Extended;
  SizeMBStr: string;
  PurgeParam: string;
  ShouldPurge: Boolean;
  PromptMsg: string;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    LocalDataDir := ExpandConstant('{localappdata}\MQ');
    RoamingDataDir := ExpandConstant('{userappdata}\MQ');
    if (not DirExists(LocalDataDir)) and (not DirExists(RoamingDataDir)) then
      Exit;

    PurgeParam := ExpandConstant('{param:PURGE|0}');
    ShouldPurge := (PurgeParam = '1');

    if (not ShouldPurge) and (not UninstallSilent) then
    begin
      TotalBytes := GetDirSizeBytes(LocalDataDir) + GetDirSizeBytes(RoamingDataDir);
      SizeMBStr := Format('%.1f', [TotalBytes / 1048576.0]);
      PromptMsg := FmtMessage(CustomMessage('UninstallPurgePrompt'), [LocalDataDir, RoamingDataDir, SizeMBStr]);
      if MsgBox(PromptMsg, mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        ShouldPurge := True;
    end;

    if ShouldPurge then
    begin
      if DirExists(LocalDataDir) then
        DelTree(LocalDataDir, True, True, True);
      if DirExists(RoamingDataDir) then
        DelTree(RoamingDataDir, True, True, True);
    end;
  end;
end;
