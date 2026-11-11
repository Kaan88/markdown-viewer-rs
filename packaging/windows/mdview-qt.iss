; Inno Setup script for mdview on Qt 6 (mdview-qt.exe with its Qt and MSVC runtime DLLs).
; build-installer-qt.ps1 first stages the files in target\qt-deploy (windeployqt + VC runtime).
; Written for Inno Setup 6.3+/7. Compiled with Inno Setup 7.1.1 (2026-11-10 in the VM, 2026-11-11 on
; the Windows PC).
; Build:  ISCC.exe /DAppVersion=0.2.0 mdview-qt.iss   (see build-installer-qt.ps1)

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

#define AppName "mdview-qt"
#define DisplayName "mdview (Qt)"
#define AppExe "mdview-qt.exe"
#define ProgId "mdview-qt.Markdown"

[Setup]
AppId={{4C1D7A52-8E0B-4B6F-9C3A-2F5D6E7B8A91}
AppName={#DisplayName}
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
MinVersion=10.0.17763
SetupIconFile=..\..\assets\mdview.ico
UninstallDisplayIcon={app}\mdview.ico
UninstallDisplayName={#DisplayName}
; Registers the associations and calls SHChangeNotify(SHCNE_ASSOCCHANGED) at the end.
ChangesAssociations=yes
; Used by the auto-updater (/SILENT /CLOSEAPPLICATIONS): close a running viewer, do not relaunch.
CloseApplications=yes
RestartApplications=no
OutputDir=..\..\target\installer
OutputBaseFilename=mdview-qt-setup-{#AppVersion}
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
Source: "..\..\target\qt-deploy\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#DisplayName}"; Filename: "{app}\{#AppExe}"; IconFilename: "{app}\mdview.ico"
Name: "{autodesktop}\{#DisplayName}"; Filename: "{app}\{#AppExe}"; IconFilename: "{app}\mdview.ico"; Tasks: desktopicon

[Registry]
; mdview is only OFFERED under "Open with". Nothing here writes an extension's default value
; (the "(Default)" of HKCU\Software\Classes\.md etc.), so the user's current choice is untouched.
Root: HKA; Subkey: "Software\Classes\{#ProgId}"; ValueType: string; ValueName: ""; ValueData: "Markdown document"; Flags: uninsdeletekey; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\{#ProgId}\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\mdview.ico"; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\{#ProgId}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""; Tasks: assoc

Root: HKA; Subkey: "Software\Classes\.md\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.markdown\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.mdown\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.mkd\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty; Tasks: assoc

Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#DisplayName}"; Flags: uninsdeletekey; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".md"; ValueData: ""; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".markdown"; ValueData: ""; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".mdown"; ValueData: ""; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".mkd"; ValueData: ""; Tasks: assoc

; Registered application: lists mdview in Settings > Apps > Default apps, where the user can make it
; the default for each Markdown extension.
Root: HKA; Subkey: "Software\{#AppName}"; Flags: uninsdeletekey; Tasks: assoc
Root: HKA; Subkey: "Software\{#AppName}\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#DisplayName}"; Tasks: assoc
Root: HKA; Subkey: "Software\{#AppName}\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "A native, read-only Markdown reader."; Tasks: assoc
Root: HKA; Subkey: "Software\{#AppName}\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\mdview.ico"; Tasks: assoc
Root: HKA; Subkey: "Software\{#AppName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".md"; ValueData: "{#ProgId}"; Tasks: assoc
Root: HKA; Subkey: "Software\{#AppName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".markdown"; ValueData: "{#ProgId}"; Tasks: assoc
Root: HKA; Subkey: "Software\{#AppName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".mdown"; ValueData: "{#ProgId}"; Tasks: assoc
Root: HKA; Subkey: "Software\{#AppName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".mkd"; ValueData: "{#ProgId}"; Tasks: assoc
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "{#AppName}"; ValueData: "Software\{#AppName}\Capabilities"; Flags: uninsdeletevalue; Tasks: assoc

[Run]
Filename: "{app}\{#AppExe}"; Description: "Launch mdview (Qt)"; Flags: nowait postinstall skipifsilent
; Opens Settings at mdview's Default apps page, where it can be made the default for .md files.
Filename: "ms-settings:defaultapps?registeredAppUser={#AppName}"; Description: "Choose mdview as the default app for Markdown files"; Flags: shellexec nowait postinstall skipifsilent unchecked; Tasks: assoc
