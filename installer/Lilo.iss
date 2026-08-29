#ifndef MyAppVersion
  #error MyAppVersion must be provided by the packaging script
#endif

#ifndef MySourceExe
  #error MySourceExe must be provided by the packaging script
#endif

#ifndef MyOutputDir
  #error MyOutputDir must be provided by the packaging script
#endif

#define MyAppName "Lilo"
#define MyAppPublisher "HellterEnjoy"
#define MyAppUrl "https://github.com/HellterEnjoy/Lilo"

[Setup]
AppId={{256F9EB3-4565-49E5-82C0-08FE0E03F6B3}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppUrl}
AppSupportURL={#MyAppUrl}/issues
AppUpdatesURL={#MyAppUrl}/releases
DefaultDirName={localappdata}\Programs\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
LicenseFile=..\LICENSE
OutputDir={#MyOutputDir}
OutputBaseFilename=Lilo-{#MyAppVersion}-windows-x64-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
UninstallDisplayIcon={app}\Lilo.exe
CloseApplications=yes
RestartApplications=no

[Files]
Source: "{#MySourceExe}"; DestDir: "{app}"; DestName: "Lilo.exe"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\CHANGELOG.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\PRIVACY.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Lilo"; Filename: "{app}\Lilo.exe"
Name: "{autodesktop}\Lilo"; Filename: "{app}\Lilo.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked

[Run]
Filename: "{app}\Lilo.exe"; Description: "Launch Lilo"; Flags: nowait postinstall skipifsilent
