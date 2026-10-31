; pley's Windows installer, built with Inno Setup 6.7 or newer:
;   ISCC /DAppVersion=<version> /DSourceDir=<folder with pley.exe> /DFfmpegDir=<an FFmpeg build's bin folder>
;        [/DFfmpegLabel=<its name>] [/DArch=arm64] [/DFlavor=fel] deploy\windows\pley.iss
; scripts\build.ps1 -Installer builds it next to the release archive.
; README.md in this folder says what it sets up and how to install silently.

#if VER < EncodeVer(6, 7, 0)
  #error Inno Setup 6.7 or newer builds this installer
#endif
#ifndef AppVersion
  #error Pass /DAppVersion=<version>
#endif
#ifndef SourceDir
  #error Pass /DSourceDir=<folder with pley.exe and pley-transcoder.exe>
#endif
#ifndef Arch
  #define Arch "x64"
#endif
#if Arch == "arm64"
  #define ArchName "arm64"
  #define Architectures "arm64"
#else
  #define ArchName "x86_64"
  #define Architectures "x64compatible"
#endif
#ifndef OutputDir
  #define OutputDir "..\..\dist"
#endif
; The FFmpeg pley runs, which the installer carries: ffmpeg.exe, ffprobe.exe
; and the DLLs next to them (gyan.dev's build, or one built for Dolby Vision
; FEL, say).
#ifndef FfmpegDir
  #error Pass /DFfmpegDir=<an FFmpeg build's bin folder>
#endif
#ifndef FfmpegLabel
  #define FfmpegLabel "FFmpeg"
#endif
; A name for a build that differs from the release's, in its file name.
#ifdef Flavor
  #define Flavored "-" + Flavor
#else
  #define Flavored ""
#endif

[Setup]
AppId={{6D1B6F0C-3F1E-4C55-9A0D-2B7E3C5A9F41}
; "Pley Media Server", apart from the desktop app ("Pley") in Programs, the
; Start menu and Services; "pley-server" in its folder and a release's files.
AppName=Pley Media Server
AppVersion={#AppVersion}
AppVerName=Pley Media Server {#AppVersion}
AppPublisher=the Pley authors
AppPublisherURL=https://github.com/pley-media-server/pley
AppSupportURL=https://github.com/pley-media-server/pley/issues
AppUpdatesURL=https://github.com/pley-media-server/pley/releases
AppCopyright=Copyright (c) 2026 the Pley authors
DefaultDirName={autopf}\pley-server
DisableProgramGroupPage=yes
#ifdef Test
; /DTest: a build that runs without administrator rights, to try the
; notification area install where nothing else may change (no shortcuts, no
; start at sign-in).
PrivilegesRequired=lowest
#else
; Services, user accounts, the firewall and Program Files.
PrivilegesRequired=admin
#endif
ArchitecturesAllowed={#Architectures}
ArchitecturesInstallIn64BitMode={#Architectures}
MinVersion=10.0.17763
WizardStyle=modern dynamic
SetupIconFile=..\..\apps\server\assets\pley.ico
; The logo inset in the pages' top right corner, one per DPI
; (scripts/render-icons.mjs).
WizardSmallImageFile=setup-small-*.png
UninstallDisplayIcon={app}\pley.exe
UninstallDisplayName=Pley Media Server
OutputDir={#OutputDir}
OutputBaseFilename=pley-server-{#AppVersion}{#Flavored}-windows-{#ArchName}-setup
Compression=lzma2/max
SolidCompression=yes
; pley is stopped by PrepareToInstall, which knows how.
CloseApplications=no
SetupLogging=yes
; Programs Setup starts inherit RedirectionGuard: the pley it starts couldn't
; follow the links WinGet puts FFmpeg behind, nor junctions in media folders,
; until it restarts. Setup writes in Program Files and its own temporary
; folder only; pley refuses links in the data folder it protects (ProgramData).
RedirectionGuard=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
; Extracted on its own first to stop the pley being updated (PrepareToInstall).
Source: "{#SourceDir}\pley.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\*"; Excludes: "pley.exe"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
; FFmpeg in a folder of its own, emptied first (CurStepChanged), with its
; license, README and source (a patched build's) when the build has them
; beside its bin folder.
Source: "{#FfmpegDir}\ffmpeg.exe"; DestDir: "{app}\ffmpeg"; Flags: ignoreversion
Source: "{#FfmpegDir}\ffprobe.exe"; DestDir: "{app}\ffmpeg"; Flags: ignoreversion
Source: "{#FfmpegDir}\*.dll"; DestDir: "{app}\ffmpeg"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#FfmpegDir}\..\LICENSE*"; DestDir: "{app}\ffmpeg"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#FfmpegDir}\..\README.txt"; DestDir: "{app}\ffmpeg"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#FfmpegDir}\..\source\*"; DestDir: "{app}\ffmpeg\source"; Flags: ignoreversion skipifsourcedoesntexist

#ifndef Test
[Icons]
Name: "{autoprograms}\Pley Media Server"; Filename: "{app}\pley.exe"; Parameters: "--data-dir ""{code:DataDir}"""; Check: IsTray
Name: "{autoprograms}\Pley Media Server"; Filename: "{code:WebAddress}"; Check: not IsTray
Name: "{autoprograms}\Pley Media Server logs"; Filename: "{code:DataDir}\logs"

; The other way's, when pley ran otherwise before: pley itself (a .lnk) in
; the notification area, its page (a .url) for a service.
[InstallDelete]
Type: files; Name: "{autoprograms}\Pley Media Server.url"; Check: IsTray
Type: files; Name: "{autoprograms}\Pley Media Server.lnk"; Check: not IsTray
; Their names before "Pley Media Server".
Type: files; Name: "{autoprograms}\Pley Server.url"
Type: files; Name: "{autoprograms}\Pley Server.lnk"
Type: files; Name: "{autoprograms}\Pley Server logs.lnk"
; The shortcuts' names before the capital P.
Type: files; Name: "{autoprograms}\pley-server.url"
Type: files; Name: "{autoprograms}\pley-server.lnk"
Type: files; Name: "{autoprograms}\pley-server logs.lnk"
#endif

[Run]
Filename: "{code:OpenLink}"; Description: "Open Pley in the browser"; Flags: postinstall shellexec nowait skipifsilent runasoriginaluser; Check: HasOpenLink

[UninstallDelete]
Type: filesandordirs; Name: "{app}\ffmpeg"

[Code]
const
  // What the last install set up, for the next install and the uninstaller.
  StateKey = 'Software\pley\Setup';
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';

var
  ModePage, NetworkPage: TWizardPage;
  ServiceRadio, UserRadio, TrayRadio: TNewRadioButton;
  RestrictCheck, AutostartCheck: TNewCheckBox;
  UserPage: TInputQueryWizardPage;
  DataPage: TInputDirWizardPage;
  CopyCheck: TNewCheckBox;
  CopyEdit: TNewEdit;
  CopyButton: TNewButton;
  // Network: this computer only, chosen networks (adapters), or every one.
  LocalRadio, AdaptersRadio, LanRadio: TNewRadioButton;
  AdapterList: TNewCheckListBox;
  // This computer's IPv4 addresses (and any an earlier install chose), their
  // adapters' names and the kind of network Windows takes each for, in
  // AdapterList's order.
  AdapterIps, AdapterNames, AdapterKinds: TStringList;
  PortEdit: TNewEdit;
  DiscoveryCheck: TNewCheckBox;
  FirewallPage: TWizardPage;
  PrivateCheck, DomainCheck, PublicCheck: TNewCheckBox;
  SubnetRadio, AnyRadio: TNewRadioButton;
  // The last install's, from StateKey.
  PrevMode, PrevDataDir, PrevUser: String;
  // What pley said once it ran (HasAccounts: it was set up already), and
  // what went wrong.
  Running, SetupLink, FfmpegInfo, Failure: String;
  HasAccounts: Boolean;
  // Uninstall.
  DeleteData: Boolean;

function SetEnvironmentVariable(Name, Value: String): BOOL;
  external 'SetEnvironmentVariableW@kernel32.dll stdcall';

function PowerShell: String;
begin
  Result := ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe');
end;

// ---- the choices ----

function IsTray: Boolean;
begin
  Result := TrayRadio.Checked;
end;

function ModeName: String;
begin
  if ServiceRadio.Checked then
    Result := 'service'
  else if UserRadio.Checked then
    Result := 'user'
  else
    Result := 'tray';
end;

function DataDir(Param: String): String;
begin
  Result := RemoveBackslashUnlessRoot(Trim(DataPage.Values[0]));
end;

function Port: String;
begin
  Result := Trim(PortEdit.Text);
end;

// The addresses of the networks chosen, separated by commas.
function ChosenIps: String;
var
  I: Integer;
begin
  Result := '';
  if AdaptersRadio.Checked then
    for I := 0 to AdapterIps.Count - 1 do
      if AdapterList.Checked[I] then
      begin
        if Result <> '' then
          Result := Result + ',';
        Result := Result + AdapterIps.Strings[I];
      end;
end;

// Where pley listens on port P: everywhere, or this computer and the
// networks chosen. 127.0.0.1 comes first: pley must listen there as it
// starts, and an adapter's address may come later (pley tries again).
function AddressesWith(const P: String): String;
var
  I: Integer;
begin
  if LanRadio.Checked then
    Result := '0.0.0.0:' + P
  else
  begin
    Result := '127.0.0.1:' + P;
    if AdaptersRadio.Checked then
      for I := 0 to AdapterIps.Count - 1 do
        if AdapterList.Checked[I] then
          Result := Result + ',' + AdapterIps.Strings[I] + ':' + P;
  end;
end;

function BindAddress: String;
begin
  Result := AddressesWith(Port);
end;

// Whether other devices may reach pley.
function Reachable: Boolean;
begin
  Result := LanRadio.Checked or (ChosenIps <> '');
end;

// pley answers on the networks it listens on only, now or once it listens
// on some (Settings, Network).
function Discovery: Boolean;
begin
  Result := DiscoveryCheck.Checked;
end;

// The Windows Firewall profiles the rule is for, such as "private,domain".
function Profiles: String;
begin
  Result := '';
  if PrivateCheck.Checked then
    Result := Result + ',private';
  if DomainCheck.Checked then
    Result := Result + ',domain';
  if PublicCheck.Checked then
    Result := Result + ',public';
  if Result <> '' then
    Delete(Result, 1, 1);
end;

// "private network", from Windows' NetworkCategory.
function KindText(const Kind: String): String;
begin
  if Kind = 'Private' then
    Result := 'private network'
  else if Kind = 'Public' then
    Result := 'public network'
  else if Kind = 'DomainAuthenticated' then
    Result := 'domain network'
  else
    Result := '';
end;

function WebAddress(Param: String): String;
begin
  Result := 'http://localhost:' + Port;
end;

function OpenLink(Param: String): String;
begin
  if SetupLink <> '' then
    Result := SetupLink
  else if Running <> '' then
    Result := Running
  else
    Result := WebAddress('');
end;

function HasOpenLink: Boolean;
begin
  Result := (Failure = '') and (Running <> '');
end;

function ShareServer: String;
begin
  Result := Trim(UserPage.Values[1]);
  while (Length(Result) > 0) and ((Result[1] = '\') or (Result[1] = '/')) do
    Delete(Result, 1, 1);
  if Pos('\', Result) > 0 then
    Result := Copy(Result, 1, Pos('\', Result) - 1);
end;

function YesNo(Value: Boolean): String;
begin
  if Value then Result := 'yes' else Result := 'no';
end;

function TrueFalse(Value: Boolean): String;
begin
  if Value then Result := 'true' else Result := 'false';
end;

// A command line option of Setup, such as /MODE=user, or Default.
function CmdParam(const Name, Default: String): String;
var
  I: Integer;
  Arg: String;
begin
  Result := Default;
  for I := 1 to ParamCount do
  begin
    Arg := ParamStr(I);
    if CompareText(Copy(Arg, 1, Length(Name) + 2), '/' + Name + '=') = 0 then
      Result := Copy(Arg, Length(Name) + 3, MaxInt);
  end;
end;

function Flag(const Name: String; const Default: Boolean): Boolean;
var
  Value: String;
begin
  Value := Lowercase(CmdParam(Name, YesNo(Default)));
  Result := (Value = 'yes') or (Value = 'true') or (Value = '1');
end;

// ---- the pages ----

function AddText(Page: TWizardPage; var Top: Integer; const Indent: Integer; const Text: String): TNewStaticText;
begin
  Result := TNewStaticText.Create(Page);
  Result.Parent := Page.Surface;
  Result.Left := Indent;
  Result.Top := Top;
  Result.Width := Page.SurfaceWidth - Indent;
  Result.AutoSize := False;
  Result.WordWrap := True;
  Result.Caption := Text;
  Result.AdjustHeight;
  Top := Top + Result.Height + ScaleY(6);
end;

function AddRadio(Page: TWizardPage; var Top: Integer; const Caption, Text: String): TNewRadioButton;
begin
  Result := TNewRadioButton.Create(Page);
  Result.Parent := Page.Surface;
  Result.Caption := Caption;
  Result.Left := 0;
  Result.Top := Top;
  Result.Width := Page.SurfaceWidth;
  Result.Height := ScaleY(17);
  Top := Top + Result.Height + ScaleY(2);
  if Text <> '' then
    AddText(Page, Top, ScaleX(18), Text);
  Top := Top + ScaleY(4);
end;

function AddCheck(Page: TWizardPage; var Top: Integer; const Caption, Text: String): TNewCheckBox;
begin
  Result := TNewCheckBox.Create(Page);
  Result.Parent := Page.Surface;
  Result.Caption := Caption;
  Result.Left := 0;
  Result.Top := Top;
  Result.Width := Page.SurfaceWidth;
  Result.Height := ScaleY(17);
  Top := Top + Result.Height + ScaleY(2);
  if Text <> '' then
    AddText(Page, Top, ScaleX(18), Text);
  Top := Top + ScaleY(4);
end;

// A folder box with a Browse button, indented under a choice.
procedure AddFolder(Page: TWizardPage; var Top: Integer; var Edit: TNewEdit; var Button: TNewButton; Click: TNotifyEvent);
begin
  Button := TNewButton.Create(Page);
  Button.Parent := Page.Surface;
  Button.Caption := SetupMessage(msgButtonWizardBrowse);
  Button.Width := ScaleX(75);
  Button.Height := ScaleY(23);
  Button.Left := Page.SurfaceWidth - Button.Width;
  Button.Top := Top;
  Button.OnClick := Click;
  Edit := TNewEdit.Create(Page);
  Edit.Parent := Page.Surface;
  Edit.Left := ScaleX(18);
  Edit.Top := Top + ScaleY(1);
  Edit.Width := Button.Left - ScaleX(8) - Edit.Left;
  Top := Top + Button.Height + ScaleY(10);
end;

procedure Browse(Edit: TNewEdit; const Prompt: String);
var
  Dir: String;
begin
  Dir := Edit.Text;
  if BrowseForFolder(Prompt, Dir, False) then
    Edit.Text := Dir;
end;

procedure CopyButtonClick(Sender: TObject);
begin
  Browse(CopyEdit, 'The data folder of the Pley you used until now:');
  CopyCheck.Checked := True;
end;

procedure UpdateControls(Sender: TObject);
begin
  RestrictCheck.Enabled := not TrayRadio.Checked;
  AutostartCheck.Enabled := TrayRadio.Checked;
  AdapterList.Enabled := AdaptersRadio.Checked;
  CopyEdit.Enabled := CopyCheck.Checked;
  CopyButton.Enabled := CopyCheck.Checked;
end;

// Ticking a network chooses "these networks".
procedure AdapterClicked(Sender: TObject);
begin
  AdaptersRadio.Checked := True;
  UpdateControls(nil);
end;

// This computer's networks: its IPv4 addresses (not loopback nor
// link-local), each with its adapter's name and the kind of network Windows
// takes it for.
procedure FindAdapters;
var
  Code, I, Bar: Integer;
  Output: TExecOutput;
  Line, Rest: String;
begin
  AdapterIps := TStringList.Create;
  AdapterNames := TStringList.Create;
  AdapterKinds := TStringList.Create;
  try
    if not ExecAndCaptureOutput(PowerShell, '-NoProfile -ExecutionPolicy Bypass -Command "' +
      '$kinds = @{}; Get-NetConnectionProfile -ErrorAction SilentlyContinue | ' +
      'ForEach-Object { $kinds[$_.InterfaceAlias] = [string]$_.NetworkCategory }; ' +
      'Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue | ' +
      'Where-Object { $_.IPAddress -notlike ''127.*'' -and $_.IPAddress -notlike ''169.254.*'' } | ' +
      'ForEach-Object { $_.IPAddress + ''|'' + $_.InterfaceAlias + ''|'' + $kinds[$_.InterfaceAlias] }"',
      '', SW_HIDE, ewWaitUntilTerminated, Code, Output) then
      Exit;
  except
    Log(GetExceptionMessage);
    Exit;
  end;
  for I := 0 to GetArrayLength(Output.StdOut) - 1 do
  begin
    Line := Trim(Output.StdOut[I]);
    Bar := Pos('|', Line);
    if Bar = 0 then
      Continue;
    AdapterIps.Add(Copy(Line, 1, Bar - 1));
    Rest := Copy(Line, Bar + 1, MaxInt);
    Bar := Pos('|', Rest);
    AdapterNames.Add(Copy(Rest, 1, Bar - 1));
    AdapterKinds.Add(Copy(Rest, Bar + 1, MaxInt));
    Log('Network: ' + Line);
  end;
end;

// A network in AdapterList: "Ethernet (192.168.1.10)", "private network".
procedure AddAdapter(const Ip, Name, Kind: String);
var
  Caption: String;
begin
  if Name = '' then
    Caption := Ip + ' (not on this computer now)'
  else
    Caption := Name + ' (' + Ip + ')';
  AdapterList.AddCheckBox(Caption, KindText(Kind), 0, False, True, False, False, nil);
end;

procedure CreatePages;
var
  Top, I: Integer;
  PortLabel: TNewStaticText;
  Networks: String;
begin
  ModePage := CreateCustomPage(wpSelectDir, 'How Pley runs', 'When Pley starts and which account it runs as.');
  Top := 0;
  ServiceRadio := AddRadio(ModePage, Top, 'As a service with an account of its own (recommended)',
    'Starts with Windows, before anyone signs in. Its account, NT SERVICE\pley, has no password and can''t sign in. ' +
    'For media on this computer.');
  UserRadio := AddRadio(ModePage, Top, 'As a service, as a user of its own',
    'For media on a NAS or another computer. Setup creates a standard user, whose password nobody needs, ' +
    'and can save its sign-in to your NAS.');
  TrayRadio := AddRadio(ModePage, Top, 'In the notification area, as you',
    'Runs while you are signed in, with your account: it reaches what you do, network drives included.');
  RestrictCheck := AddCheck(ModePage, Top, 'Let the service write only in its data folder (recommended)',
    'Windows stops it changing anything else and takes away privileges it doesn''t need. ' +
    'If hardware transcoding doesn''t work, run Setup again and turn this off.');
  AutostartCheck := AddCheck(ModePage, Top, 'Start Pley when I sign in', '');
  ServiceRadio.OnClick := @UpdateControls;
  UserRadio.OnClick := @UpdateControls;
  TrayRadio.OnClick := @UpdateControls;

  UserPage := CreateInputQueryPage(ModePage.ID, 'Pley''s user', 'The user the service runs as, and its sign-in to your NAS',
    'Setup creates this standard user, or updates the one it made before. Its password is random: nobody signs in as it.' + #13#10#13#10 +
    'For media on a NAS, enter the NAS, and a user and password that may read your media there. ' +
    'In Pley, add libraries by their network paths, such as \\nas\media\Movies.');
  UserPage.Add('&User:', False);
  UserPage.Add('&NAS or computer with your media (optional), such as \\nas:', False);
  UserPage.Add('User &name there:', False);
  UserPage.Add('&Password there:', True);

  DataPage := CreateInputDirPage(UserPage.ID, 'Data folder', 'Where Pley keeps its database, artwork and settings',
    'Pley keeps its database, artwork, logs and options (pley.toml) in this folder. For a service, only the service, ' +
    'administrators and you can open it.', False, '');
  DataPage.Add('');
  Top := DataPage.Buttons[0].Top + DataPage.Buttons[0].Height + ScaleY(16);
  CopyCheck := AddCheck(DataPage, Top, 'Copy the data of the Pley I used until now, from this folder:',
    'Its accounts, libraries, watch history and artwork. That Pley must be stopped. Nothing is copied over existing data.');
  AddFolder(DataPage, Top, CopyEdit, CopyButton, @CopyButtonClick);
  CopyCheck.OnClick := @UpdateControls;

  NetworkPage := CreateCustomPage(DataPage.ID, 'Network', 'Where Pley listens');
  Top := 0;
  LocalRadio := AddRadio(NetworkPage, Top, 'Only this computer (recommended with a reverse proxy)',
    'On 127.0.0.1, for a reverse proxy here that serves Pley to others with HTTPS.');
  AdaptersRadio := AddRadio(NetworkPage, Top, 'This computer and these networks:', '');
  Top := Top - ScaleY(4);
  AdapterList := TNewCheckListBox.Create(NetworkPage);
  AdapterList.Parent := NetworkPage.Surface;
  AdapterList.Left := ScaleX(18);
  AdapterList.Top := Top;
  AdapterList.Width := NetworkPage.SurfaceWidth - ScaleX(18);
  AdapterList.Height := ScaleY(56);
  AdapterList.Flat := True;
  for I := 0 to AdapterIps.Count - 1 do
    AddAdapter(AdapterIps.Strings[I], AdapterNames.Strings[I], AdapterKinds.Strings[I]);
  AdapterList.OnClickCheck := @AdapterClicked;
  Top := Top + AdapterList.Height + ScaleY(8);
  LanRadio := AddRadio(NetworkPage, Top, 'Every network, those that come later too', '');
  LocalRadio.OnClick := @UpdateControls;
  AdaptersRadio.OnClick := @UpdateControls;
  LanRadio.OnClick := @UpdateControls;
  PortLabel := TNewStaticText.Create(NetworkPage);
  PortLabel.Parent := NetworkPage.Surface;
  PortLabel.Caption := '&Port:';
  PortLabel.Left := 0;
  PortLabel.Top := Top + ScaleY(3);
  PortEdit := TNewEdit.Create(NetworkPage);
  PortEdit.Parent := NetworkPage.Surface;
  PortEdit.Left := PortLabel.Width + ScaleX(8);
  PortEdit.Top := Top;
  PortEdit.Width := ScaleX(60);
  PortLabel.FocusControl := PortEdit;
  Top := Top + PortEdit.Height + ScaleY(14);

  FirewallPage := CreateCustomPage(NetworkPage.ID, 'Windows Firewall', 'Which devices may reach Pley');
  Top := 0;
  Networks := '';
  for I := 0 to AdapterIps.Count - 1 do
    if KindText(AdapterKinds.Strings[I]) <> '' then
      Networks := Networks + #13#10 + '   ' + AdapterNames.Strings[I] + ' (' + AdapterIps.Strings[I] + '): ' +
        KindText(AdapterKinds.Strings[I]);
  if Networks <> '' then
    AddText(FirewallPage, Top, 0, 'This computer''s networks:' + Networks);
  AddText(FirewallPage, Top, 0, 'Where Pley listens on networks, chosen here or later in Pley (Settings, Network), ' +
    'let it through on networks Windows takes for:');
  PrivateCheck := AddCheck(FirewallPage, Top, 'Private (home)', '');
  DomainCheck := AddCheck(FirewallPage, Top, 'Domain (a workplace''s)', '');
  PublicCheck := AddCheck(FirewallPage, Top, 'Public (cafes, hotels: not recommended)', '');
  AddText(FirewallPage, Top, 0, 'From:');
  SubnetRadio := AddRadio(FirewallPage, Top, 'Devices on those networks only', '');
  AnyRadio := AddRadio(FirewallPage, Top, 'Any address, through a VPN such as Tailscale too', '');
  DiscoveryCheck := AddCheck(FirewallPage, Top, 'Let the Pley apps on the network find this server', '');
end;

// Ticks the networks of Ips (addresses separated by commas), adding those
// this computer doesn't have now (a VPN's, say).
procedure CheckAdapters(Ips: String);
var
  Comma, Index: Integer;
  Ip: String;
begin
  Ips := Ips + ',';
  while Ips <> '' do
  begin
    Comma := Pos(',', Ips);
    Ip := Trim(Copy(Ips, 1, Comma - 1));
    Delete(Ips, 1, Comma);
    if Ip = '' then
      Continue;
    Index := AdapterIps.IndexOf(Ip);
    if Index < 0 then
    begin
      AdapterIps.Add(Ip);
      AdapterNames.Add('');
      AdapterKinds.Add('');
      AddAdapter(Ip, '', '');
      Index := AdapterIps.Count - 1;
    end;
    AdapterList.Checked[Index] := True;
  end;
end;

// The choices of the last install (or of Setup's command line).
procedure LoadChoices;
var
  Mode, Listen, Ips, Firewall: String;
begin
  Mode := Lowercase(CmdParam('MODE', GetPreviousData('Mode', 'service')));
  ServiceRadio.Checked := Mode = 'service';
  UserRadio.Checked := Mode = 'user';
  TrayRadio.Checked := Mode = 'tray';
  if not (UserRadio.Checked or TrayRadio.Checked) then
    ServiceRadio.Checked := True;
  RestrictCheck.Checked := Flag('RESTRICT', GetPreviousData('Restrict', 'yes') = 'yes');
  AutostartCheck.Checked := Flag('AUTOSTART', GetPreviousData('Autostart', 'yes') = 'yes');

  UserPage.Values[0] := CmdParam('USER', GetPreviousData('User', 'pley'));
  UserPage.Values[1] := CmdParam('SHARE', GetPreviousData('Share', ''));
  UserPage.Values[2] := CmdParam('SHAREUSER', GetPreviousData('ShareUser', ''));
  // From the environment, as pley reads it: Setup's log keeps its command line.
  UserPage.Values[3] := GetEnv('PLEY_SHARE_PASSWORD');

  DataPage.Values[0] := CmdParam('DATADIR', GetPreviousData('DataDir', ''));
  CopyEdit.Text := CmdParam('COPYDATA', '');
  CopyCheck.Checked := CopyEdit.Text <> '';

  // local, all, or the addresses of the networks chosen (/LISTEN=10.0.0.5,100.64.0.7).
  Listen := Lowercase(Trim(CmdParam('LISTEN', GetPreviousData('Listen', 'local'))));
  Ips := GetPreviousData('Addresses', '');
  if (Listen <> 'local') and (Listen <> 'all') and (Listen <> 'adapters') then
  begin
    Ips := Listen;
    Listen := 'adapters';
  end;
  LocalRadio.Checked := Listen = 'local';
  LanRadio.Checked := Listen = 'all';
  AdaptersRadio.Checked := Listen = 'adapters';
  if AdaptersRadio.Checked then
    CheckAdapters(Ips);
  PortEdit.Text := CmdParam('PORT', GetPreviousData('Port', '7539'));

  // Private and domain networks, from devices on them, unless told otherwise.
  Firewall := Lowercase(CmdParam('FIREWALL', GetPreviousData('Firewall', 'private,domain')));
  PrivateCheck.Checked := Pos('private', Firewall) > 0;
  DomainCheck.Checked := Pos('domain', Firewall) > 0;
  PublicCheck.Checked := Pos('public', Firewall) > 0;
  AnyRadio.Checked := Lowercase(CmdParam('FROM', GetPreviousData('From', 'subnet'))) = 'any';
  SubnetRadio.Checked := not AnyRadio.Checked;
  DiscoveryCheck.Checked := Flag('DISCOVERY', GetPreviousData('Discovery', 'yes') = 'yes');
  UpdateControls(nil);
end;

// The NAS password on Setup's command line would stay in its log.
function InitializeSetup: Boolean;
begin
  Result := CmdParam('SHAREPASSWORD', '') = '';
  if not Result then
    SuppressibleMsgBox('Setup doesn''t take /SHAREPASSWORD=: its log would keep the password. Put it in the ' +
      'environment variable PLEY_SHARE_PASSWORD instead, in the administrator''s terminal that runs Setup.',
      mbError, MB_OK, IDOK);
end;

procedure InitializeWizard;
begin
  RegQueryStringValue(HKA, StateKey, 'Mode', PrevMode);
  RegQueryStringValue(HKA, StateKey, 'DataDir', PrevDataDir);
  RegQueryStringValue(HKA, StateKey, 'CreatedUser', PrevUser);
  FindAdapters;
  CreatePages;
  LoadChoices;
end;

procedure RegisterPreviousData(PreviousDataKey: Integer);
begin
  SetPreviousData(PreviousDataKey, 'Mode', ModeName);
  SetPreviousData(PreviousDataKey, 'Restrict', YesNo(RestrictCheck.Checked));
  SetPreviousData(PreviousDataKey, 'Autostart', YesNo(AutostartCheck.Checked));
  SetPreviousData(PreviousDataKey, 'User', UserPage.Values[0]);
  SetPreviousData(PreviousDataKey, 'Share', UserPage.Values[1]);
  SetPreviousData(PreviousDataKey, 'ShareUser', UserPage.Values[2]);
  SetPreviousData(PreviousDataKey, 'DataDir', DataDir(''));
  if LanRadio.Checked then
    SetPreviousData(PreviousDataKey, 'Listen', 'all')
  else if AdaptersRadio.Checked then
    SetPreviousData(PreviousDataKey, 'Listen', 'adapters')
  else
    SetPreviousData(PreviousDataKey, 'Listen', 'local');
  SetPreviousData(PreviousDataKey, 'Addresses', ChosenIps);
  SetPreviousData(PreviousDataKey, 'Port', Port);
  if Profiles = '' then
    SetPreviousData(PreviousDataKey, 'Firewall', 'none')
  else
    SetPreviousData(PreviousDataKey, 'Firewall', Profiles);
  if AnyRadio.Checked then
    SetPreviousData(PreviousDataKey, 'From', 'any')
  else
    SetPreviousData(PreviousDataKey, 'From', 'subnet');
  SetPreviousData(PreviousDataKey, 'Discovery', YesNo(DiscoveryCheck.Checked));
end;

function ShouldSkipPage(PageID: Integer): Boolean;
begin
  Result := (PageID = UserPage.ID) and not UserRadio.Checked;
end;

// Whether Name, at the top of a data folder, is what an install that
// stopped short leaves there (logs, tmp) or, with All, anything pley keeps
// there: its database and the files beside it, options, key, lock and the
// folders it fills.
function IsPleys(const Name: String; const All: Boolean): Boolean;
var
  N: String;
begin
  N := Lowercase(Name);
  Result := (N = 'logs') or (N = 'tmp');
  if All and not Result then
    Result := (Copy(N, 1, 5) = 'pley.') or (N = 'secret.key') or (N = 'server-id') or
      (N = 'plex-machine-id') or (N = 'transcode') or
      (N = 'cache') or (N = 'metadata') or (N = 'media') or (N = 'avatars') or (N = 'backups');
end;

// Whether Dir (missing, empty or not) holds nothing but IsPleys(All).
function HoldsOnlyPleys(const Dir: String; const All: Boolean): Boolean;
var
  FindRec: TFindRec;
begin
  Result := True;
  if FindFirst(AddBackslash(Dir) + '*', FindRec) then
  begin
    try
      repeat
        if (FindRec.Name <> '.') and (FindRec.Name <> '..') then
          Result := IsPleys(FindRec.Name, All);
      until not Result or not FindNext(FindRec);
    finally
      FindClose(FindRec);
    end;
  end;
end;

// A data folder as `pley service install` takes one, for every way pley
// runs (the uninstaller may delete it): new or empty, pley's, or what an
// install that stopped short left.
function DataDirFree(const Dir: String): Boolean;
begin
  Result := FileExists(AddBackslash(Dir) + 'pley.db') or FileExists(AddBackslash(Dir) + 'pley.toml') or
    FileExists(AddBackslash(Dir) + 'secret.key') or HoldsOnlyPleys(Dir, False);
end;

function DefaultDataDir: String;
begin
  if TrayRadio.Checked then
    Result := ExpandConstant('{localappdata}\pley')
  else
    Result := ExpandConstant('{commonappdata}\pley');
end;

function ValidPort: Boolean;
var
  N: Integer;
begin
  N := StrToIntDef(Port, 0);
  Result := (N > 0) and (N < 65536) and (IntToStr(N) = Port);
end;

// The program listening on TCP port Number, other than the pley installed
// here (which Setup stops); empty when there is none.
function PortOwner(const Number: String): String;
var
  Code: Integer;
  Output: TExecOutput;
begin
  Result := '';
  try
    if ExecAndCaptureOutput(PowerShell, '-NoProfile -ExecutionPolicy Bypass -Command "' +
      '$c = Get-NetTCPConnection -State Listen -LocalPort ' + Number + ' -ErrorAction SilentlyContinue | Select-Object -First 1; ' +
      'if ($c) { $p = Get-Process -Id $c.OwningProcess -ErrorAction SilentlyContinue; if ($p.Path) { $p.Path } else { $p.ProcessName } }"',
      '', SW_HIDE, ewWaitUntilTerminated, Code, Output) and (GetArrayLength(Output.StdOut) > 0) then
      Result := Trim(Output.StdOut[0]);
  except
    Log(GetExceptionMessage);
  end;
  if CompareText(Result, ExpandConstant('{app}\pley.exe')) = 0 then
    Result := '';
end;

// Whether to go on although a port pley needs is taken: no on silent installs.
function PortsFree: Boolean;
var
  Owner, Taken: String;
begin
  Result := True;
  Owner := PortOwner(Port);
  Taken := Port;
  if Owner <> '' then
    Result := SuppressibleMsgBox('Port ' + Taken + ' is taken by ' + Owner + '. Pley can''t listen there while it is: ' +
      'stop that program, or choose another port.' + #13#10#13#10 + 'Go on anyway?', mbError, MB_YESNO or MB_DEFBUTTON2, IDNO) = IDYES;
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var
  Dir, Other: String;
begin
  Result := True;
  if CurPageID = ModePage.ID then
  begin
    // The data folder follows the mode until it is chosen.
    Dir := DataDir('');
    if (Dir = '') or (CompareText(Dir, ExpandConstant('{localappdata}\pley')) = 0)
      or (CompareText(Dir, ExpandConstant('{commonappdata}\pley')) = 0) then
      DataPage.Values[0] := DefaultDataDir;
  end
  else if CurPageID = UserPage.ID then
  begin
    if Trim(UserPage.Values[0]) = '' then
    begin
      SuppressibleMsgBox('Enter the name of the user Pley runs as, such as pley.', mbError, MB_OK, IDOK);
      Result := False;
    end
    else if (ShareServer <> '') and (Trim(UserPage.Values[2]) = '') then
    begin
      SuppressibleMsgBox('Enter the user name Pley signs in to ' + ShareServer + ' with.', mbError, MB_OK, IDOK);
      Result := False;
    end;
  end
  else if CurPageID = DataPage.ID then
  begin
    Dir := DataDir('');
    Other := RemoveBackslashUnlessRoot(Trim(CopyEdit.Text));
    if (Length(Dir) <= 3) then
    begin
      SuppressibleMsgBox('Choose a folder for Pley''s data, not a whole drive.', mbError, MB_OK, IDOK);
      Result := False;
    end
    else if not DataDirFree(Dir) then
    begin
      SuppressibleMsgBox(Dir + ' has other files in it than Pley''s: choose an empty or a new folder for Pley''s data.',
        mbError, MB_OK, IDOK);
      Result := False;
    end
    else if CopyCheck.Checked and not FileExists(AddBackslash(Other) + 'pley.db') then
    begin
      SuppressibleMsgBox(Other + ' isn''t a Pley data folder: it has no pley.db.', mbError, MB_OK, IDOK);
      Result := False;
    end
    else if CopyCheck.Checked and ((CompareText(Other, Dir) = 0) or FileExists(AddBackslash(Dir) + 'pley.db')) then
    begin
      SuppressibleMsgBox(Dir + ' has Pley''s data already: nothing is copied over it. Choose another data folder.', mbError, MB_OK, IDOK);
      Result := False;
    end
    else if not CopyCheck.Checked and (PrevDataDir <> '') and (CompareText(PrevDataDir, Dir) <> 0)
      and FileExists(AddBackslash(PrevDataDir) + 'pley.db') and not FileExists(AddBackslash(Dir) + 'pley.db') then
    begin
      if SuppressibleMsgBox('Pley kept its data in ' + PrevDataDir + ' until now. Copy it into ' + Dir + '?',
        mbConfirmation, MB_YESNO, IDNO) = IDYES then
      begin
        CopyEdit.Text := PrevDataDir;
        CopyCheck.Checked := True;
        UpdateControls(nil);
      end;
    end;
  end
  else if CurPageID = NetworkPage.ID then
  begin
    if AdaptersRadio.Checked and (ChosenIps = '') then
    begin
      SuppressibleMsgBox('Tick the networks Pley listens on, or choose "Only this computer".', mbError, MB_OK, IDOK);
      Result := False;
    end
    else if not ValidPort then
    begin
      SuppressibleMsgBox('The port is a number from 1 to 65535, such as 7539.', mbError, MB_OK, IDOK);
      Result := False;
    end
    else
      Result := PortsFree;
  end
  else if (CurPageID = FirewallPage.ID) and (Profiles = '') then
    Result := SuppressibleMsgBox('Pley won''t be let through Windows Firewall, so only this computer will reach it ' +
      '(unless another rule lets it). Go on?', mbConfirmation, MB_YESNO, IDYES) = IDYES;
end;

function UpdateReadyMemo(Space, NewLine, MemoUserInfoInfo, MemoDirInfo, MemoTypeInfo, MemoComponentsInfo, MemoGroupInfo, MemoTasksInfo: String): String;
var
  S: String;
begin
  S := MemoDirInfo + NewLine + NewLine + 'How Pley runs:' + NewLine + Space;
  if ServiceRadio.Checked then
    S := S + 'As a service, as NT SERVICE\pley'
  else if UserRadio.Checked then
    S := S + 'As a service, as the user ' + Trim(UserPage.Values[0])
  else
    S := S + 'In the notification area, as you';
  if not TrayRadio.Checked and RestrictCheck.Checked then
    S := S + ', writing only in its data folder';
  if TrayRadio.Checked and AutostartCheck.Checked then
    S := S + ', from when you sign in';
  if UserRadio.Checked and (ShareServer <> '') then
    S := S + NewLine + Space + 'Signs in to \\' + ShareServer + ' as ' + Trim(UserPage.Values[2]);
  S := S + NewLine + NewLine + 'Data folder:' + NewLine + Space + DataDir('');
  if CopyCheck.Checked then
    S := S + NewLine + Space + 'With the data copied from ' + Trim(CopyEdit.Text);
  S := S + NewLine + NewLine + 'Network:' + NewLine + Space;
  if LanRadio.Checked then
    S := S + 'Every network, port ' + Port
  else if Reachable then
    S := S + 'This computer and ' + ChosenIps + ', port ' + Port
  else
    S := S + 'This computer only (127.0.0.1:' + Port + ')';
  if Profiles = '' then
    S := S + NewLine + Space + 'Not let through Windows Firewall'
  else if AnyRadio.Checked then
    S := S + NewLine + Space + 'Let through Windows Firewall on ' + Profiles + ' networks, from any address'
  else
    S := S + NewLine + Space + 'Let through Windows Firewall on ' + Profiles + ' networks, from devices on them';
  if Reachable and Discovery then
    S := S + NewLine + Space + 'Found by the Pley apps on the network';
  S := S + NewLine + NewLine + 'FFmpeg:' + NewLine + Space + '{#FfmpegLabel}';
  Result := S;
end;

// ---- installing ----

// Closes the pley running from Dir in the notification area (as Quit
// would), then ends what is left of it.
procedure StopProgramsIn(const Dir: String);
var
  Code: Integer;
  Script: String;
begin
  Script := '$d = ''' + AddBackslash(Dir) + '''; ' +
    '$p = @(Get-Process pley, pley-transcoder -ErrorAction SilentlyContinue | ' +
    'Where-Object { $_.Path -and $_.Path.StartsWith($d, [StringComparison]::OrdinalIgnoreCase) }); ' +
    'if ($p.Count -eq 0) { exit 0 }; ' +
    '$p | ForEach-Object { taskkill.exe /PID $_.Id 2>&1 | Out-Null }; ' +
    'for ($i = 0; $i -lt 40 -and @($p | Where-Object { -not $_.HasExited }).Count -gt 0; $i++) { Start-Sleep -Milliseconds 250 }; ' +
    '$p | Where-Object { -not $_.HasExited } | Stop-Process -Force -ErrorAction SilentlyContinue';
  Exec(PowerShell, '-NoProfile -ExecutionPolicy Bypass -Command "' + Script + '"', '', SW_HIDE, ewWaitUntilTerminated, Code);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Code: Integer;
begin
  Result := '';
  // The service and a pley in the notification area keep their programs
  // open: stopped, so that they can be replaced.
  ExtractTemporaryFile('pley.exe');
  if not Exec(ExpandConstant('{tmp}\pley.exe'), 'service stop', '', SW_HIDE, ewWaitUntilTerminated, Code) or (Code <> 0) then
    Result := 'The Pley service didn''t stop. Stop it in Services (services.msc), then try again.';
  if (Result = '') and FileExists(ExpandConstant('{app}\pley.exe')) then
    StopProgramsIn(ExpandConstant('{app}'));
end;

procedure SetStatus(const Text: String);
begin
  WizardForm.StatusLabel.Caption := Text;
  WizardForm.FilenameLabel.Caption := '';
end;

// Runs pley.exe with Params, logging what it says, and reading the lines
// `pley service install` and `report` end with. Its error on failure.
function RunPley(const Params: String): String;
var
  Code, I: Integer;
  Output: TExecOutput;
  Line: String;
begin
  Result := '';
  Log('pley.exe ' + Params);
  try
    if not ExecAndCaptureOutput(ExpandConstant('{app}\pley.exe'), Params, ExpandConstant('{app}'), SW_HIDE,
      ewWaitUntilTerminated, Code, Output) then
      Code := -1;
  except
    Code := -1;
    Result := GetExceptionMessage;
  end;
  for I := 0 to GetArrayLength(Output.StdOut) - 1 do
  begin
    Line := Output.StdOut[I];
    Log('  ' + Line);
    if Copy(Line, 1, 9) = 'running: ' then
      Running := Copy(Line, 10, MaxInt)
    else if Copy(Line, 1, 7) = 'setup: ' then
      SetupLink := Copy(Line, 8, MaxInt)
    else if Copy(Line, 1, 8) = 'ffmpeg: ' then
      FfmpegInfo := Copy(Line, 9, MaxInt)
    else if Line = 'accounts: yes' then
      HasAccounts := True;
  end;
  for I := 0 to GetArrayLength(Output.StdErr) - 1 do
  begin
    Log('  ' + Output.StdErr[I]);
    if Trim(Output.StdErr[I]) <> '' then
      Result := Result + Output.StdErr[I] + #13#10;
  end;
  if (Code <> 0) and (Result = '') then
    Result := 'pley.exe ended with code ' + IntToStr(Code);
  if Code = 0 then
    Result := '';
end;

// The ffmpeg and ffprobe options for pley: the FFmpeg Setup installed.
function FfmpegOptions: String;
begin
  Result := ' --ffmpeg ' + AddQuotes(ExpandConstant('{app}\ffmpeg\ffmpeg.exe')) +
    ' --ffprobe ' + AddQuotes(ExpandConstant('{app}\ffmpeg\ffprobe.exe'));
end;

// Lets pley through Windows Firewall on the kinds of network chosen. pley
// decides where it listens and on which ports, which its Settings change:
// the rule is for pley, wherever it listens. Its UDP half takes in
// discovery's broadcast on the web port and Bonjour's queries on 5353
// (mDNS), so neither needs a rule of its own.
procedure Firewall;
var
  Code: Integer;
  Netsh, Rule: String;
begin
  Netsh := ExpandConstant('{sys}\netsh.exe');
  Exec(Netsh, 'advfirewall firewall delete rule name="pley"', '', SW_HIDE, ewWaitUntilTerminated, Code);
  if Profiles = '' then
    Exit;
  Rule := 'advfirewall firewall add rule name="pley" dir=in action=allow profile=' + Profiles + ' program="' +
    ExpandConstant('{app}\pley.exe') + '"';
  // From devices on those networks, unless any address may.
  if SubnetRadio.Checked then
    Rule := Rule + ' remoteip=localsubnet';
  Exec(Netsh, Rule + ' protocol=TCP', '', SW_HIDE, ewWaitUntilTerminated, Code);
  Exec(Netsh, Rule + ' protocol=UDP', '', SW_HIDE, ewWaitUntilTerminated, Code);
end;

function RunCommand: String;
begin
  Result := '"' + ExpandConstant('{app}\pley.exe') + '" --data-dir "' + DataDir('') + '"';
end;

// Removes the start at sign-in of the pley installed here (not another's).
procedure RemoveAutostart;
var
  Value: String;
begin
  if RegQueryStringValue(HKCU, RunKey, 'pley', Value) and (Pos(Lowercase(ExpandConstant('{app}\')), Lowercase(Value)) > 0) then
    RegDeleteValue(HKCU, RunKey, 'pley');
end;

procedure InstallService(const Options: String);
var
  Params, User: String;
begin
  SetStatus('Setting up the Pley service...');
  RemoveAutostart;
  User := Trim(UserPage.Values[0]);
  Params := Options + ' service install --discovery ' + TrueFalse(Discovery) + ' --grant ' + AddQuotes(GetUserNameString);
  if UserRadio.Checked then
  begin
    Params := Params + ' --account user --create-user --user ' + AddQuotes(User);
    if ShareServer <> '' then
    begin
      Params := Params + ' --share ' + AddQuotes(ShareServer) + ' --share-user ' + AddQuotes(Trim(UserPage.Values[2]));
      SetEnvironmentVariable('PLEY_SHARE_PASSWORD', UserPage.Values[3]);
    end;
  end;
  if not RestrictCheck.Checked then
    Params := Params + ' --unrestricted';
  if CopyCheck.Checked then
    Params := Params + ' --copy-data-from ' + AddQuotes(RemoveBackslashUnlessRoot(Trim(CopyEdit.Text)));
  // The user an earlier install made, when the service no longer runs as it.
  if (PrevUser <> '') and not (UserRadio.Checked and (CompareText(PrevUser, User) = 0)) then
    Params := Params + ' --remove-user ' + AddQuotes(PrevUser);
  Failure := RunPley(Params);
  SetEnvironmentVariable('PLEY_SHARE_PASSWORD', '');
end;

procedure InstallTray(const Options: String);
var
  Code: Integer;
begin
  SetStatus('Setting up Pley...');
  if (PrevMode = 'service') or (PrevMode = 'user') then
  begin
    if PrevUser <> '' then
      Failure := RunPley('service uninstall --delete-user ' + AddQuotes(PrevUser))
    else
      Failure := RunPley('service uninstall');
  end;
  if (Failure = '') and CopyCheck.Checked then
    Failure := RunPley('--data-dir ' + AddQuotes(DataDir('')) + ' service copy-data --from ' +
      AddQuotes(RemoveBackslashUnlessRoot(Trim(CopyEdit.Text))));
  if Failure = '' then
    Failure := RunPley(Options + ' save-config --discovery ' + TrueFalse(Discovery));
  if Failure <> '' then
    Exit;
#ifndef Test
  if AutostartCheck.Checked then
    RegWriteStringValue(HKCU, RunKey, 'pley', RunCommand)
  else
    RemoveAutostart;
#endif
  SetStatus('Starting Pley...');
  if ExecAsOriginalUser(ExpandConstant('{app}\pley.exe'), '--data-dir ' + AddQuotes(DataDir('')), '', SW_HIDE, ewNoWait, Code) then
    Failure := RunPley('--data-dir ' + AddQuotes(DataDir('')) + ' service report')
  else
    Failure := 'Pley didn''t start: ' + SysErrorMessage(Code);
end;

procedure SaveState;
var
  Created: String;
begin
  RegWriteStringValue(HKA, StateKey, 'Mode', ModeName);
  RegWriteStringValue(HKA, StateKey, 'DataDir', DataDir(''));
  if UserRadio.Checked then
    Created := Trim(UserPage.Values[0])
  else
    Created := '';
  RegWriteStringValue(HKA, StateKey, 'CreatedUser', Created);
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  Options: String;
begin
  // The FFmpeg it carries replaces the one there was, DLLs and all.
  if CurStep = ssInstall then
    DelTree(ExpandConstant('{app}\ffmpeg'), True, True, True);
  if CurStep <> ssPostInstall then
    Exit;
  Options := '--data-dir ' + AddQuotes(DataDir('')) + ' --bind ' + BindAddress + FfmpegOptions;
  SetStatus('Opening Windows Firewall...');
  Firewall;
  if Failure = '' then
  begin
    if TrayRadio.Checked then
      InstallTray(Options)
    else
      InstallService(Options);
  end;
  if Failure = '' then
    SaveState
  else
    SuppressibleMsgBox('Pley is installed, but it isn''t running:' + #13#10#13#10 + Failure, mbError, MB_OK, IDOK);
end;

// What the finish page says about FFmpeg: its encoders, and a hint when
// none uses the GPU.
function FfmpegSummary: String;
var
  Encoders: String;
  Start: Integer;
begin
  Result := '';
  if FfmpegInfo = '' then
    Exit;
  if Pos('not found', FfmpegInfo) > 0 then
  begin
    Result := 'Pley''s FFmpeg doesn''t work here, so Pley can''t transcode: its log says why.';
    Exit;
  end;
  Start := Pos('encoders [', FfmpegInfo);
  if Start = 0 then
    Exit;
  Encoders := Copy(FfmpegInfo, Start + 10, MaxInt);
  Encoders := Copy(Encoders, 1, Pos(']', Encoders) - 1);
  Result := 'FFmpeg encoders: ' + Encoders + '.';
  if (Pos('nvenc', Encoders) = 0) and (Pos('qsv', Encoders) = 0) and (Pos('amf', Encoders) = 0) then
  begin
    Result := Result + ' None of them uses a GPU.';
    if not TrayRadio.Checked and RestrictCheck.Checked then
      Result := Result + ' If this computer has one, run Setup again and turn off "Let the service write only in its data folder".';
  end;
end;

procedure CurPageChanged(CurPageID: Integer);
var
  Text: String;
begin
  if CurPageID <> wpFinished then
    Exit;
  if Failure <> '' then
    Text := 'Pley is installed, but it isn''t running:' + #13#10#13#10 + Failure + #13#10 +
      'Its log is in ' + DataDir('') + '\logs.'
  else
  begin
    Text := 'Pley is running at ' + Running + '.';
    if not TrayRadio.Checked then
      Text := Text + ' It runs as a Windows service, in the background: no icon in the notification area. ' +
        'Open it from the Start menu (Pley Media Server).';
    if SetupLink <> '' then
      Text := Text + #13#10#13#10 + 'Open it to create your administrator account: the link holds the setup code (a new ' +
        'one is made each time Pley starts until then).'
    else if HasAccounts then
      Text := Text + #13#10#13#10 + 'Sign in with your Pley account: this Pley is set up already.'
    else
      Text := Text + #13#10#13#10 + 'Its setup code is in its log: ' + DataDir('') + '\logs\pley.log ("setup?code=").';
    if FfmpegSummary <> '' then
      Text := Text + #13#10#13#10 + FfmpegSummary;
    if LocalRadio.Checked then
      Text := Text + #13#10#13#10 + 'Point your reverse proxy at http://127.0.0.1:' + Port + '.';
  end;
  WizardForm.FinishedLabel.Caption := Text;
  WizardForm.FinishedLabel.AdjustHeight;
end;

// ---- uninstalling ----

function InitializeUninstall: Boolean;
var
  I: Integer;
begin
  RegQueryStringValue(HKA, StateKey, 'Mode', PrevMode);
  RegQueryStringValue(HKA, StateKey, 'DataDir', PrevDataDir);
  RegQueryStringValue(HKA, StateKey, 'CreatedUser', PrevUser);
  DeleteData := False;
  if UninstallSilent then
  begin
    for I := 1 to ParamCount do
      if CompareText(ParamStr(I), '/DELETEDATA') = 0 then
        DeleteData := True;
  end
  else if (PrevDataDir <> '') and DirExists(PrevDataDir) then
  begin
    if HoldsOnlyPleys(PrevDataDir, True) then
      DeleteData := MsgBox('Delete Pley''s data too?' + #13#10#13#10 + PrevDataDir + ' holds its database (accounts, libraries, ' +
        'watch history), artwork and settings. Choose No to keep it for a later install.',
        mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES
    else
      MsgBox(PrevDataDir + ', Pley''s data folder, holds other files too, so it stays as it is. Delete Pley''s ' +
        'files there (pley.db and the rest) yourself if you no longer need them.', mbInformation, MB_OK);
  end;
  Result := True;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Code: Integer;
  Params: String;
begin
  if CurUninstallStep = usUninstall then
  begin
    if (PrevMode = 'service') or (PrevMode = 'user') then
    begin
      Params := 'service uninstall';
      if PrevUser <> '' then
        Params := Params + ' --delete-user ' + AddQuotes(PrevUser);
      Exec(ExpandConstant('{app}\pley.exe'), Params, '', SW_HIDE, ewWaitUntilTerminated, Code);
    end;
    StopProgramsIn(ExpandConstant('{app}'));
    RemoveAutostart;
    Exec(ExpandConstant('{sys}\netsh.exe'), 'advfirewall firewall delete rule name="pley"', '', SW_HIDE, ewWaitUntilTerminated, Code);
  end
  else if CurUninstallStep = usPostUninstall then
  begin
    // Never a folder with other things in it too (chosen before Setup refused those).
    if DeleteData and (Length(PrevDataDir) > 3) then
    begin
      if HoldsOnlyPleys(PrevDataDir, True) then
        DelTree(PrevDataDir, True, True, True)
      else
        Log(PrevDataDir + ' holds other files than Pley''s: it stays');
    end;
    RegDeleteKeyIncludingSubkeys(HKA, StateKey);
    RegDeleteKeyIfEmpty(HKA, 'Software\pley');
  end;
end;
