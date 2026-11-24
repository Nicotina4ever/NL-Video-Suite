; NL Video Suite installer (Inno Setup 6, per-user, no administrator rights).
;
; Build the portable bundle first, then compile this script with the bundle folder as source:
;   .\packaging\nl\portable.ps1 -Version 0.2.0
;   iscc /DMyAppVersion=0.2.0 /DSourceBundle=<bundle folder> packaging\nl\installer.iss
;
; It installs the executable, the copied ComfyUI workflows, ffmpeg/ffprobe when present and the
; documentation under %LOCALAPPDATA%\Programs\NL Video Suite, with Start Menu and optional desktop
; shortcuts and an uninstaller. AI services (ComfyUI, Ollama or an OpenAI-compatible server) are NOT
; installed or started: they stay external and are configured in Settings.

#ifndef MyAppVersion
  #define MyAppVersion "0.2.0"
#endif

#ifndef SourceBundle
  #define SourceBundle "..\out\NLVideoSuite-" + MyAppVersion + "-windows-x64"
#endif

[Setup]
AppId={{6F1F3B0E-4E42-4A7C-9A57-3B1A7D2C8E11}
AppName=NL Video Suite
AppVersion={#MyAppVersion}
AppVerName=NL Video Suite {#MyAppVersion}
AppPublisher=NL Video Suite
DefaultDirName={localappdata}\Programs\NL Video Suite
DefaultGroupName=NL Video Suite
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\out
OutputBaseFilename=NLVideoSuite-{#MyAppVersion}-windows-x64-setup
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\NL Video Suite.exe
LicenseFile=..\..\LICENSE-MIT

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked

[Files]
Source: "{#SourceBundle}\NL Video Suite.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceBundle}\README.txt"; DestDir: "{app}"; Flags: ignoreversion isreadme
Source: "{#SourceBundle}\DISTRIBUZIONE.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceBundle}\Avvia NL Video Suite.cmd"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceBundle}\workflows\*"; DestDir: "{app}\workflows"; Flags: ignoreversion recursesubdirs createallsubdirs skipifsourcedoesntexist
Source: "{#SourceBundle}\tools\*"; DestDir: "{app}\tools"; Flags: ignoreversion skipifsourcedoesntexist

[Icons]
Name: "{group}\NL Video Suite"; Filename: "{app}\NL Video Suite.exe"
Name: "{group}\Uninstall NL Video Suite"; Filename: "{uninstallexe}"
Name: "{autodesktop}\NL Video Suite"; Filename: "{app}\NL Video Suite.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\NL Video Suite.exe"; Description: "Start NL Video Suite"; Flags: nowait postinstall skipifsilent
