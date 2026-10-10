; Windows installer built from a release binary with Inno Setup 6.3 or newer:
;
;   iscc /DVersion=0.1.0 /DArch=x86_64 /DBinary=...\wavo.exe ^
;        /DOutputDir=dist packaging\windows\wavo.iss
;
; Arch matches the Rust target: x86_64 or aarch64. Installation uses the
; current user's Programs folder and does not need administrator rights.
; Updates close a running copy before replacing it.

#ifndef Version
  #error Version must be defined on the ISCC command line
#endif
#ifndef Arch
  #error Arch must be defined on the ISCC command line (x86_64 or aarch64)
#endif
#ifndef NumericVersion
  #define NumericVersion Version
#endif
#ifndef Binary
  #error Binary must be defined on the ISCC command line
#endif
#ifndef OutputDir
  #error OutputDir must be defined on the ISCC command line
#endif
#if Arch == "aarch64"
  #define InnoArch "arm64"
#else
  #define InnoArch "x64compatible"
#endif

#define AppName "WAVO"
#define AppExeName "wavo.exe"

[Setup]
; A fresh AppId: WAVO is its own program, installed alongside ZapFast, and
; Windows tells an update from a new program by this id.
AppId={{21EAB135-4D06-45C2-A35F-6533AAF1D982}
AppName={#AppName}
AppVersion={#Version}
AppVerName={#AppName} {#Version}
AppPublisher=WAVO
AppPublisherURL=https://github.com/abhinavdatta/wavo
AppSupportURL=https://github.com/abhinavdatta/wavo/issues
AppUpdatesURL=https://github.com/abhinavdatta/wavo/releases
DefaultDirName={localappdata}\Programs\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed={#InnoArch}
ArchitecturesInstallIn64BitMode={#InnoArch}
MinVersion=10.0
LicenseFile=..\..\LICENSE
OutputDir={#OutputDir}
OutputBaseFilename=wavo-v{#Version}-{#Arch}-pc-windows-msvc-setup
SetupIconFile=wavo.ico
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
UninstallDisplayIcon={app}\{#AppExeName}
VersionInfoVersion={#NumericVersion}.0

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked

[Files]
Source: "{#Binary}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\THIRD-PARTY-NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion

Source: "wavo-installer.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"; AppUserModelID: "io.github.wavo.Wavo"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Tasks: desktopicon; AppUserModelID: "io.github.wavo.Wavo"

[Run]
Filename: "{app}\{#AppExeName}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent
