#define AppVersion "0.1.20"
#define Bundle "QuickPeek-" + AppVersion + "-windows-x64"

[Setup]
AppId=Nex-Z.QuickPeek
AppName=QuickPeek
AppVersion={#AppVersion}
AppPublisher=Nex-Z
DefaultDirName={localappdata}\Programs\QuickPeek
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.22000
OutputDir=..\dist
OutputBaseFilename={#Bundle}-setup
SetupIconFile=..\assets\QuickPeek.ico
UninstallDisplayIcon={app}\quickpeek.exe
Compression=lzma2
SolidCompression=yes
DisableProgramGroupPage=yes

Uninstallable=not PortableUpdate
CreateUninstallRegKey=not PortableUpdate

[Files]
Source: "..\dist\{#Bundle}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Check: not PortableUpdate; Name: "{userprograms}\QuickPeek"; Filename: "{app}\quickpeek.exe"

[Run]
Filename: "{app}\quickpeek.exe"; Description: "Launch QuickPeek"; Flags: nowait postinstall skipifsilent

[Code]
function PortableUpdate: Boolean;
begin
  Result := ExpandConstant('{param:MODE|}') = 'portable';
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Value: String;
begin
  if (CurUninstallStep = usPostUninstall) and
    RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'QuickPeek', Value) and
    (CompareText(Value, '"' + ExpandConstant('{app}\quickpeek.exe') + '"') = 0) then
    RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'QuickPeek');
end;
