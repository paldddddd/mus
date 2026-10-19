; =====================================================================
; wallmus Inno Setup 6 Script
; Per-user Windows live wallpaper engine with music widget
; =====================================================================

#define MyAppName "wallmus"
#define MyAppVersion "0.1.0"
#define MyAppPublisher "wallmus Team"
#define MyAppURL "https://github.com/wallmus/wallmus"
#define MyAppExeName "wallmus.exe"
#define MyUiExeName "wallmus-ui.exe"

[Setup]
; App Identity
AppId={{E63A7F41-893C-4C8D-9150-13D416629BCB}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}

; Strict Per-User Installation (Zero Administrator Privileges Required)
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog

; Installation Target: %LOCALAPPDATA%\Programs\wallmus
DefaultDirName={localappdata}\Programs\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes

; Output Settings
OutputDir=..\dist
OutputBaseFilename=wallmus-setup-v{#MyAppVersion}
SetupIconFile=compiler:SetupClassicIcon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2/ultra64
SolidCompression=yes
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64

; License & UI Styling
LicenseFile=..\LICENSE.txt
WizardStyle=modern

[Languages]
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "autostart"; Description: "Запускать wallmus вместе с Windows"; GroupDescription: "Автозапуск:"; Flags: unchecked

[Files]
; Core Binaries (from release build or dist directory)
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\{#MyUiExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\wallmus-htmlhost.exe"; DestDir: "{app}"; Flags: ignoreversion

; Third-Party Runtime Dependencies (LGPL 2.1)
Source: "..\deps\libmpv-2.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\deps\ffmpeg.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\deps\ffprobe.exe"; DestDir: "{app}"; Flags: ignoreversion

; Documentation & Licenses
Source: "..\LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyUiExeName}"; Comment: "Управление живыми обоями wallmus"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyUiExeName}"; Tasks: desktopicon

[Registry]
; Autostart task write to HKCU Run
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "{#MyAppName}"; ValueData: """{app}\{#MyAppExeName}"""; Tasks: autostart; Flags: uninsdeletevalue

[Run]
; Launch daemon automatically on installation finish
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[Code]
// Check if Microsoft WebView2 Runtime is installed
function IsWebView2Installed(): Boolean;
var
  VersionStr: String;
begin
  Result := RegQueryStringValue(HKEY_CURRENT_USER,
    'Software\Microsoft\EdgeUpdate\Clients\{F3017226-2347-470A-AE31-417646033875}',
    'pv', VersionStr) or
    RegQueryStringValue(HKEY_LOCAL_MACHINE,
    'SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-2347-470A-AE31-417646033875}',
    'pv', VersionStr);
end;

// Gracefully close any existing wallmus instance prior to setup
function InitializeSetup(): Boolean;
var
  ErrorCode: Integer;
begin
  Result := True;
  
  // Close running daemon if active
  Exec('taskkill.exe', '/F /IM wallmus.exe', '', SW_HIDE, ewWaitUntilTerminated, ErrorCode);
  Exec('taskkill.exe', '/F /IM wallmus-ui.exe', '', SW_HIDE, ewWaitUntilTerminated, ErrorCode);
  Exec('taskkill.exe', '/F /IM wallmus-htmlhost.exe', '', SW_HIDE, ewWaitUntilTerminated, ErrorCode);

  if not IsWebView2Installed() then
  begin
    MsgBox('Примечание: Для отображения HTML5/WebGL обоев требуется Microsoft WebView2 Runtime.' + #13#10 +
           'Видео и шейдерные обои будут работать автономно.', mbInformation, MB_OK);
  end;
end;

// Clean up running processes before uninstalling
function InitializeUninstall(): Boolean;
var
  ErrorCode: Integer;
begin
  Result := True;
  Exec('taskkill.exe', '/F /IM wallmus.exe', '', SW_HIDE, ewWaitUntilTerminated, ErrorCode);
  Exec('taskkill.exe', '/F /IM wallmus-ui.exe', '', SW_HIDE, ewWaitUntilTerminated, ErrorCode);
  Exec('taskkill.exe', '/F /IM wallmus-htmlhost.exe', '', SW_HIDE, ewWaitUntilTerminated, ErrorCode);
end;

// Prompt user whether to remove database & configs upon uninstallation
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  AppDataPath: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    if MsgBox('Удалить сохранённую библиотеку обоев и историю настроек в %APPDATA%\wallmus?',
              mbConfirmation, MB_YESNO) = IDYES then
    begin
      AppDataPath := ExpandConstant('{userappdata}\wallmus');
      DelTree(AppDataPath, True, True, True);
    end;
  end;
end;
