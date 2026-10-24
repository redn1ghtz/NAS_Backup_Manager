; Inno Setup 7. Open setup-x86.iss or setup-x64.iss, not this shared include.
#if Ver < EncodeVer(7, 0, 0)
  #error Inno Setup 7 or newer is required.
#endif
#define AppVersion "0.2.0-alpha"
#ifndef ReleaseRoot
  #define ReleaseRoot AddBackslash(SourcePath) + "..\..\releases\" + AppVersion
#endif
#define Payload AddBackslash(ReleaseRoot) + "portable-" + TargetArchitecture
#ifndef QAId
  #define ProductId "org.nasbackupmanager.desktop"
  #define ProductName "NAS Backup Manager"
#else
  #define ProductId "org.nasbackupmanager.qa." + QAId
  #define ProductName "NAS Backup Manager QA " + QAId
#endif

[Setup]
AppId={#ProductId}
AppName={#ProductName}
AppVersion={#AppVersion}
AppPublisher=redn1ghtz
AppPublisherURL=https://github.com/redn1ghtz
AppSupportURL=https://github.com/redn1ghtz/NAS_Backup_Manager
AppUpdatesURL=https://github.com/redn1ghtz/NAS_Backup_Manager/releases
DefaultDirName={localappdata}\Programs\{#ProductName}
DefaultGroupName={#ProductName}
PrivilegesRequired=lowest
; Both installers share one per-user installation/registry identity. Payload
; architecture is checked separately; cross-architecture upgrades reuse the folder.
SetupArchitecture=x86
ArchitecturesInstallIn64BitMode=
#if TargetArchitecture == "x64"
ArchitecturesAllowed=x64compatible
#else
ArchitecturesAllowed=x86compatible
#endif
MinVersion=10.0
VersionInfoVersion=0.2.0.0
VersionInfoProductVersion=0.2.0.0
VersionInfoProductTextVersion={#AppVersion}
VersionInfoTextVersion={#AppVersion}
OutputDir={#ReleaseRoot}\installers
OutputBaseFilename=NAS-Backup-Manager-{#AppVersion}-setup-{#TargetArchitecture}
#ifdef QAId
Compression=none
SolidCompression=no
#else
Compression=lzma2
SolidCompression=yes
#endif
WizardStyle=modern
ShowLanguageDialog=yes
LicenseFile={#Payload}\LICENSE
CloseApplications=no
RestartApplications=no
UninstallDisplayIcon={app}\NAS Backup Manager.exe
DisableProgramGroupPage=yes
Uninstallable=yes

[Languages]
Name: "ru"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
ru.CloseApp=Закройте NAS Backup Manager и дождитесь завершения фоновой синхронизации.
en.CloseApp=Close NAS Backup Manager and wait for background synchronization to finish.
ru.DisableSchedule=Перед обновлением отключите расписания в приложении.
en.DisableSchedule=Disable the application's schedules before updating.
ru.NotPortable=Выберите другую папку: здесь находится portable-версия.
en.NotPortable=Choose another folder: this folder contains a portable version.
ru.CleanupFailed=Не удалось безопасно удалить задания планировщика. Программа сохранена. Дождитесь завершения синхронизации и повторите удаление.
en.CleanupFailed=Scheduled-task cleanup could not finish safely. The application is preserved. Wait for synchronization to finish and retry uninstalling.
ru.SchedulerFailed=Не удалось проверить планировщик заданий. Повторите попытку после устранения ошибки.
en.SchedulerFailed=Could not inspect Task Scheduler. Resolve the error and retry.

[Files]
; Explicit allowlist: no portable.flag, user databases, backups or data/ included.
Source: "{#Payload}\NAS Backup Manager.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Payload}\nas-backup.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Payload}\nas-backup-notify.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Payload}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Payload}\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Payload}\USER_GUIDE.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Payload}\ADVANCED.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Payload}\TOOL_SOURCES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Payload}\tools\age.exe"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "{#Payload}\tools\age-keygen.exe"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "{#Payload}\tools\rclone.exe"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "{#Payload}\tools\AGE-LICENSE"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "{#Payload}\tools\RCLONE-COPYING"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "{#Payload}\tools\notices\*"; DestDir: "{app}\tools\notices"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#Payload}\INSTALLER.md"; DestDir: "{app}"; DestName: "README.md"; Flags: ignoreversion
Source: "{#Payload}\licenses\*"; DestDir: "{app}\licenses"; Flags: ignoreversion recursesubdirs createallsubdirs skipifsourcedoesntexist

[Icons]
Name: "{group}\{#ProductName}"; Filename: "{app}\NAS Backup Manager.exe"
Name: "{group}\{cm:UninstallProgram,{#ProductName}}"; Filename: "{uninstallexe}"

[Code]
function InstalledCli: String;
begin
  Result := ExpandConstant('{app}\nas-backup.exe');
end;

function StatePath: String;
begin
#ifdef QAId
  Result := ExpandConstant('{app}\qa-state.sqlite');
#else
  Result := ExpandConstant('{userappdata}\org.nasbackupmanager.desktop\state.sqlite');
#endif
end;

function OwnTask(Task: Variant): Boolean;
var I: Integer; Action: Variant;
begin
  Result := False;
  if Pos('NASBackupManager-', String(Task.Name)) <> 1 then Exit;
  if Task.Definition.Actions.Count <> 1 then Exit;
  for I := 1 to Task.Definition.Actions.Count do begin
    Action := Task.Definition.Actions.Item(I);
    try
      if CompareText(RemoveQuotes(String(Action.Path)), InstalledCli) = 0 then Result := True;
    except
      Result := False; // Non-executable actions have no Path and are never owned.
    end;
  end;
end;

function TaskFolder: Variant;
var Service: Variant;
begin
  Service := CreateOleObject('Schedule.Service');
  Service.Connect();
  Result := Service.GetFolder('\');
end;

function OwnTasks(Folder: Variant): TStringList;
var Tasks, Task: Variant; I: Integer;
begin
  Result := TStringList.Create;
  try
    Tasks := Folder.GetTasks(1);
    for I := 1 to Tasks.Count do begin
      Task := Tasks.Item(I);
      if OwnTask(Task) then Result.Add(String(Task.Name));
    end;
  except
    Result.Free;
    RaiseException(CustomMessage('SchedulerFailed'));
  end;
end;

function OwnProcessRunning: Boolean;
var Locator, Services, Processes, Process: Variant; I: Integer; FileName: String;
begin
  Result := False;
  Locator := CreateOleObject('WbemScripting.SWbemLocator');
  Services := Locator.ConnectServer('', 'root\CIMV2');
  Processes := Services.ExecQuery('SELECT ExecutablePath FROM Win32_Process WHERE Name=''NAS Backup Manager.exe'' OR Name=''nas-backup.exe'' OR Name=''nas-backup-notify.exe''');
  for I := 0 to Processes.Count - 1 do begin
    Process := Processes.ItemIndex(I);
    if not VarIsNull(Process.ExecutablePath) then begin
      FileName := String(Process.ExecutablePath);
      if (CompareText(FileName, InstalledCli) = 0) or
        (CompareText(FileName, ExpandConstant('{app}\NAS Backup Manager.exe')) = 0) or
        (CompareText(FileName, ExpandConstant('{app}\nas-backup-notify.exe')) = 0) then Result := True;
    end;
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var Folder: Variant; Names: TStringList;
begin
  Result := '';
  try
    if FileExists(ExpandConstant('{app}\portable.flag')) then begin Result := CustomMessage('NotPortable'); Exit; end;
    if OwnProcessRunning then begin Result := CustomMessage('CloseApp'); Exit; end;
    Folder := TaskFolder;
    Names := OwnTasks(Folder);
    try
      if Names.Count > 0 then Result := CustomMessage('DisableSchedule');
    finally Names.Free; end;
  except Result := CustomMessage('SchedulerFailed'); end;
end;

function InitializeUninstall: Boolean;
begin
  Result := False;
  try
    Result := not OwnProcessRunning;
    if not Result then SuppressibleMsgBox(CustomMessage('CloseApp'), mbError, MB_OK, IDOK);
  except SuppressibleMsgBox(CustomMessage('SchedulerFailed'), mbError, MB_OK, IDOK); end;
end;

procedure InitializeUninstallProgressForm;
var Folder, Task: Variant; Names: TStringList; I, Code: Integer;
begin
  // Cleanup runs after uninstall confirmation, before file removal. Never kill a
  // worker or delete a task based on a name/prefix alone.
  try
    if OwnProcessRunning then RaiseException(CustomMessage('CloseApp'));
    Folder := TaskFolder;
    Names := OwnTasks(Folder);
    try
      for I := 0 to Names.Count - 1 do begin
        Task := Folder.GetTask(Names[I]);
        if not OwnTask(Task) then RaiseException(CustomMessage('CleanupFailed'));
        Task.Enabled := False;
        if Task.State = 4 then RaiseException(CustomMessage('CloseApp'));
      end;
      if not Exec(InstalledCli, '--state "' + StatePath + '" uninstall-mark-schedules-disabled',
        ExpandConstant('{app}'), SW_HIDE, ewWaitUntilTerminated, Code) then RaiseException(CustomMessage('CleanupFailed'));
      if Code <> 0 then RaiseException(CustomMessage('CleanupFailed'));
      for I := 0 to Names.Count - 1 do begin
        Task := Folder.GetTask(Names[I]);
        if not OwnTask(Task) then RaiseException(CustomMessage('CleanupFailed'));
        Folder.DeleteTask(Names[I], 0);
      end;
    finally Names.Free; end;
  except
    RaiseException(CustomMessage('CleanupFailed'));
  end;
end;
