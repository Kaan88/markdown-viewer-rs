; Inno Setup script for mdview (WinUI 3 shell + mdview-engine helper).
; Written for Inno Setup 6.3+/7. Compiled with Inno Setup 7.1.1 (2026-11-10 in the VM, 2026-11-11 on
; the Windows PC).
; Build:  ISCC.exe /DAppVersion=0.2.0 mdview.iss   (see build-installer.ps1)

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
; Numeric a.b.c.0 for the exe version resource (strips any -prerelease suffix).
#define Dash Pos("-", AppVersion)
#if Dash > 0
  #define NumVer Copy(AppVersion, 1, Dash - 1)
#else
  #define NumVer AppVersion
#endif

#define AppName "mdview"
#define AppExe "mdview-winui.exe"
#define EngineExe "mdview-engine.exe"
#define ProgId "mdview.Markdown"
#define RuntimeMin "2.6.4"
#define RuntimeUrl "https://aka.ms/windowsappsdk/2.6/latest/windowsappruntimeinstall-x64.exe"

[Setup]
AppId={{95434ABD-337F-42B7-9A13-05FD98FD2F81}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=Kaan88
AppPublisherURL=https://github.com/Kaan88/markdown-viewer-rs
VersionInfoVersion={#NumVer}.0
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
; Per-user by default; the user may pick "for all users" in the dialog.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.22000
SetupIconFile=..\..\assets\mdview.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
; Registers the associations and calls SHChangeNotify(SHCNE_ASSOCCHANGED) at the end.
ChangesAssociations=yes
; Used by the auto-updater (/SILENT /CLOSEAPPLICATIONS): close a running viewer, do not relaunch.
CloseApplications=yes
RestartApplications=no
OutputDir=..\..\target\installer
OutputBaseFilename=mdview-setup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
; Checked by default. Windows does not let an installer take the default handler for an extension
; (the per-user choice is protected), so mdview registers as a handler and as an application in
; Settings > Default apps; the last page offers to open that page so the user can pick it.
Name: "assoc"; Description: "Register mdview for Markdown files (.md, .markdown, .mdown, .mkd)"; GroupDescription: "File types:"
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "..\..\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\target\release\{#EngineExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Registry]
; mdview is only OFFERED under "Open with". Nothing here writes an extension's default value
; (the "(Default)" of HKCU\Software\Classes\.md etc.), so the user's current choice is untouched.
Root: HKA; Subkey: "Software\Classes\{#ProgId}"; ValueType: string; ValueName: ""; ValueData: "Markdown document"; Flags: uninsdeletekey; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\{#ProgId}\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExe},0"; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\{#ProgId}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""; Tasks: assoc

Root: HKA; Subkey: "Software\Classes\.md\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.markdown\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.mdown\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.mkd\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty; Tasks: assoc

Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#AppName}"; Flags: uninsdeletekey; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".md"; ValueData: ""; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".markdown"; ValueData: ""; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".mdown"; ValueData: ""; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".mkd"; ValueData: ""; Tasks: assoc

; Registered application: lists mdview in Settings > Apps > Default apps, where the user can make it
; the default for each Markdown extension.
Root: HKA; Subkey: "Software\{#AppName}"; Flags: uninsdeletekey; Tasks: assoc
Root: HKA; Subkey: "Software\{#AppName}\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#AppName}"; Tasks: assoc
Root: HKA; Subkey: "Software\{#AppName}\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "A native, read-only Markdown reader."; Tasks: assoc
Root: HKA; Subkey: "Software\{#AppName}\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\{#AppExe},0"; Tasks: assoc
Root: HKA; Subkey: "Software\{#AppName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".md"; ValueData: "{#ProgId}"; Tasks: assoc
Root: HKA; Subkey: "Software\{#AppName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".markdown"; ValueData: "{#ProgId}"; Tasks: assoc
Root: HKA; Subkey: "Software\{#AppName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".mdown"; ValueData: "{#ProgId}"; Tasks: assoc
Root: HKA; Subkey: "Software\{#AppName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".mkd"; ValueData: "{#ProgId}"; Tasks: assoc
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "{#AppName}"; ValueData: "Software\{#AppName}\Capabilities"; Flags: uninsdeletevalue; Tasks: assoc

[Run]
Filename: "{app}\{#AppExe}"; Description: "Launch mdview"; Flags: nowait postinstall skipifsilent
; Opens Settings at mdview's Default apps page, where it can be made the default for .md files.
Filename: "ms-settings:defaultapps?registeredAppUser={#AppName}"; Description: "Choose mdview as the default app for Markdown files"; Flags: shellexec nowait postinstall skipifsilent unchecked; Tasks: assoc

[Code]
var
  RuntimeInstaller: String;   // path of the downloaded installer, '' when none is needed
  DownloadPage: TDownloadWizardPage;

function OnDownloadProgress(const Url, FileName: String; const Progress, ProgressMax: Int64): Boolean;
begin
  if Progress = ProgressMax then
    Log(Format('Downloaded %s', [FileName]));
  Result := True;
end;

procedure InitializeWizard;
begin
  DownloadPage := CreateDownloadPage(SetupMessage(msgWizardPreparing),
    SetupMessage(msgPreparingDesc), @OnDownloadProgress);
end;

// Runs a PowerShell command, returns its exit code (-1 if it could not be started).
function RunPowerShell(const Command: String): Integer;
var
  Code: Integer;
begin
  if not Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
      '-NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "' + Command + '"',
      '', SW_HIDE, ewWaitUntilTerminated, Code) then
    Code := -1;
  Result := Code;
end;

// Detection: the framework package Microsoft.WindowsAppRuntime.2 (family
// Microsoft.WindowsAppRuntime.2_8wekyb3d8bbwe, the one the bootstrap in the app looks for)
// at version >= 2.6.4.0 for x64. Get-AppxPackage is the supported public API for this; the
// package has no stable registry key. Without -AllUsers it answers for the current user, which
// is what the app's bootstrap needs. Exit 0 = found.
function RuntimeInstalled: Boolean;
begin
  Result := RunPowerShell(
    '$p = Get-AppxPackage -Name Microsoft.WindowsAppRuntime.2 -ErrorAction SilentlyContinue | ' +
    'Where-Object { $_.Architecture -eq ''X64'' -and [version]$_.Version -ge [version]''{#RuntimeMin}.0'' }; ' +
    'if ($p) { exit 0 } else { exit 1 }') = 0;
end;

// The downloaded file must carry a valid Authenticode signature whose signer is Microsoft.
function SignedByMicrosoft(const FileName: String): Boolean;
var
  Quoted: String;
begin
  Quoted := FileName;
  StringChangeEx(Quoted, '''', '''''', True);
  Result := RunPowerShell(
    '$s = Get-AuthenticodeSignature -LiteralPath ''' + Quoted + '''; ' +
    'if ($s.Status -eq ''Valid'' -and $s.SignerCertificate -and ' +
    '$s.SignerCertificate.Subject -match ''(^|, )O=Microsoft Corporation(,|$)'') { exit 0 } else { exit 1 }') = 0;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Target: String;
  Code: Integer;
begin
  Result := '';
  RuntimeInstaller := '';
  if RuntimeInstalled then
    Exit;

  Target := ExpandConstant('{tmp}\windowsappruntimeinstall-x64.exe');
  DownloadPage.Clear;
  DownloadPage.Add('{#RuntimeUrl}', 'windowsappruntimeinstall-x64.exe', '');
  DownloadPage.Show;
  try
    try
      DownloadPage.Download;
    except
      Result := 'mdview needs the Windows App Runtime {#RuntimeMin} or newer, and downloading it failed: ' +
        GetExceptionMessage + #13#10 +
        'Install it from https://aka.ms/windowsappsdk/2.6/latest/windowsappruntimeinstall-x64.exe and run Setup again.';
      Exit;
    end;
  finally
    DownloadPage.Hide;
  end;

  if not SignedByMicrosoft(Target) then
  begin
    DeleteFile(Target);
    Result := 'The downloaded Windows App Runtime installer is not signed by Microsoft Corporation, so it was not run.';
    Exit;
  end;

  // The runtime installs machine-wide, so ask for elevation even in a per-user setup.
  if not ShellExec('runas', Target, '--quiet', '', SW_HIDE, ewWaitUntilTerminated, Code) or (Code <> 0) then
  begin
    Result := 'Installing the Windows App Runtime failed (exit code ' + IntToStr(Code) + ').';
    Exit;
  end;
  RuntimeInstaller := Target;
end;
