# Pley Media Server on Windows

The Windows installer sets up Pley as a Windows service or in the
notification area, with FFmpeg, a firewall rule and an uninstaller. It needs
Windows 10 1809 or newer and an administrator. To run Pley without
installing it, use the release's `.zip` instead
([Running Pley](../README.md#windows)).

## Install

1. Download `pley-server-<version>-windows-x86_64-setup.exe` (or `-arm64-`)
   from the [releases page](https://github.com/pley-media-server/pley/releases).
2. Run it. Setup isn't signed, so SmartScreen warns: choose
   **More info → Run anyway**.
3. Keep the defaults, or change them ([Setup's choices](#setups-choices)).
4. On the last page, open the setup link to create the administrator
   account; setup then asks about your server and your libraries, or
   brings over a Plex server
   ([First start](../../docs/CONFIGURATION.md#first-start)). If you started
   from existing data, sign in as before.

Later, the Start menu's **Pley Media Server** opens Pley, and
**Pley Media Server logs** opens its logs. A service has no icon in the
notification area.

To update, run the new version's setup: it keeps your choices and restarts
Pley. To uninstall, use **Settings → Apps → Pley Media Server**; it asks
whether to delete the data folder too (no by default).

## Setup's choices

| Page | What you choose | Default |
|---|---|---|
| How Pley runs | Service, service as a user of its own, or notification area (below) | Service |
| Data folder | Where Pley keeps its database, artwork and settings; optionally a Pley you already use to copy from (stop it first) | `C:\ProgramData\pley`, or `%LOCALAPPDATA%\pley` in the notification area |
| Network | Only this computer, this computer and the networks you tick, or every network; the port; the Plex-compatible API | Only this computer (`127.0.0.1`), port 7539, Plex-compatible API on |
| Windows Firewall | The kinds of network to let Pley through on, from which devices, and whether Pley's apps can find the server | Private and domain networks, devices on them, apps can find it |

How Pley runs:

- **As a service with an account of its own** (recommended): for media on
  this computer. It starts with Windows, before anyone signs in, as
  `NT SERVICE\pley`, an account with no password that can't sign in.
- **As a service, as a user of its own**: for media on a NAS. Setup creates
  a standard user (`pley`) and can save its sign-in to your NAS. Add
  libraries by network path, such as `\\nas\media\Movies`.
- **In the notification area, as you**: runs with your account (network
  drives included) while you are signed in.
- **Let the service write only in its data folder** (on by default): the
  service can change nothing else and keeps only the privileges it needs.

Good to know:

- **Only this computer** suits a reverse proxy on the same computer that
  adds HTTPS (Caddy, nginx, IIS). Pick networks to let other devices in.
- A network adapter that isn't up when Pley starts (Wi-Fi, Tailscale) is
  retried every 30 seconds.
- You can change where Pley listens later in **Settings → Network**. The
  firewall rule follows, for the kinds of network you chose here. It lets
  Pley's TCP and UDP through on any port: the web port, the apps' broadcast
  and Bonjour's (UDP 5353), by which Apple's apps find the server once
  Bonjour is turned on in **Settings → Network**.
- The Jellyfin-compatible API, for Jellyfin's apps, is off. **Settings →
  Network** turns it on, on Pley's port or one of its own.
- FFmpeg comes with the installer, in `ffmpeg\` next to Pley. Releases carry
  gyan.dev's full build (NVENC, Quick Sync, AMF, Vulkan tone mapping). The
  ARM64 installer carries the same x64 build, which Windows emulates, so on
  Windows on Arm Pley transcodes on the CPU.
- The service starts with Windows (delayed), restarts when it stops on its
  own, and has 50 seconds to stop at shutdown.

## Media folders

A service reads your media as `NT SERVICE\pley` (or its own user), not as
you.

- A folder on another drive (`D:\Movies`) is usually readable by every
  account, services included.
- A folder in your user folder (`C:\Users\<you>\Videos`) isn't. Grant read
  access from an administrator's terminal, then scan the library again:

  ```
  icacls "C:\Users\<you>\Videos" /grant "NT SERVICE\pley:(OI)(CI)RX"
  ```

  `(OI)(CI)` covers what is inside, and `RX` is read only. For the
  service's own user, use its name (`pley`) instead. In Explorer, the same
  is **Properties → Security → Edit → Add**, `NT SERVICE\pley` on this
  computer, **Read & execute**.
- A drive letter mapped to a NAS exists only for you. Choose **As a service,
  as a user of its own** and add libraries by network path.

## Install without the wizard

```
pley-server-0.0.1-windows-x86_64-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /MODE=service /LISTEN=192.168.1.10
```

| Option | Values | Default |
|---|---|---|
| `/MODE=` | `service`, `user` (a user of its own) or `tray` | `service` |
| `/USER=` | The user's name, with `user` | `pley` |
| `/SHARE=`, `/SHAREUSER=` | The NAS and its user name, with `user` (password: below) | none |
| `/RESTRICT=` | `yes` or `no`: write only in its data folder | `yes` |
| `/AUTOSTART=` | `yes` or `no`: start at sign-in, with `tray` | `yes` |
| `/DATADIR=` | The data folder | as above |
| `/COPYDATA=` | A data folder to copy first | none |
| `/LISTEN=` | `local`, `all`, or the addresses of the networks to listen on, separated by commas | `local` |
| `/PORT=` | The port | `7539` |
| `/PLEX=` | `yes` or `no`: the Plex-compatible API, on Pley's port | `yes` |
| `/FIREWALL=` | `private`, `domain`, `public` (separated by commas), or `none` | `private,domain` |
| `/FROM=` | `subnet` (devices on those networks) or `any` (a VPN too) | `subnet` |
| `/DISCOVERY=` | `yes` or `no`: Pley's apps can find the server | `yes` |

- An update keeps your last choices for options you leave out.
- Inno Setup's own options work too, such as `/DIR=` and `/LOG=<file>`.
- A silent install stops when the port is taken.
- A silent uninstall (`unins000.exe /VERYSILENT`) keeps the data folder
  unless you add `/DELETEDATA`.

Setup logs its command line, so the NAS password goes in the environment
variable `PLEY_SHARE_PASSWORD`. Set it in an administrator's PowerShell and
run Setup from there:

```
$env:PLEY_SHARE_PASSWORD = [Net.NetworkCredential]::new('', (Read-Host 'NAS password' -AsSecureString)).Password
.\pley-server-0.0.1-windows-x86_64-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /MODE=user /SHARE=\\nas /SHAREUSER=media
```

## By hand: `pley service`

What Setup runs, from an administrator's terminal:

```
pley --data-dir C:\ProgramData\pley --bind 127.0.0.1:7539 --plex-bind web ^
  --ffmpeg "C:\Program Files\pley-server\ffmpeg\ffmpeg.exe" ^
  --ffprobe "C:\Program Files\pley-server\ffmpeg\ffprobe.exe" service install
```

It saves the options in `pley.toml` in the data folder, installs or updates
the service, locks the data folder down, starts Pley, and prints its address
and setup link.

- `pley service install --help` lists the other options, such as
  `--account user`, `--unrestricted`, `--share` and `--dry-run`.
- `pley service start`, `stop` and `uninstall` do what they say.
- `pley save-config` saves the same options for a Pley that isn't a service.

## Security

- **Accounts.** `NT SERVICE\pley` can't sign in. The service's own user is a
  standard user with a random password, reset by each install; its NAS
  sign-in is in its own Credential Manager. Setup only changes or deletes a
  user it created.
- **Writes and privileges.** Unless you turn it off, the service can change
  only its data folder, and keeps two privileges: traversing folders, and
  the shared objects GPU drivers create. Impersonation is removed.
- **The data folder** (database, the key that seals two-step sign-in
  secrets, logs) is owned by the administrators and closed to other users.
  Setup refuses one that is, or contains, a link or junction. It sets the
  owner and permissions of every file in it only when the folder itself
  isn't set up that way already for the same accounts: on an update, the
  files inherit the folder's, and a library's artwork and seek previews
  are hundreds of thousands of files. `pley service install
  --reset-permissions` sets every file's again.
- **RedirectionGuard is off** for Setup, so the Pley it starts can follow
  WinGet's FFmpeg links and junctions in media folders. Setup writes only in
  Program Files and its own temporary folder.
- **Provenance.** Each release's installer has build provenance:
  `gh attestation verify <file> --repo pley-media-server/pley`.

[SECURITY.md](../../SECURITY.md) covers the server itself.

## Troubleshooting

- **Logs**: `logs\pley.log` in the data folder. The service is
  **Pley Media Server** in `services.msc`.
- **No GPU encoder** on the last page although the computer has one: run
  Setup again and turn off **Let the service write only in its data
  folder**.
- **A NAS isn't reached**: choose **As a service, as a user of its own**,
  name the NAS the same way the library paths do (`\\nas`, or its address,
  in both), and give a user the NAS knows.
- **The port is taken** by another Pley or program: stop it, or choose
  another port.

## Building the installer

You need [Inno Setup](https://jrsoftware.org/isinfo.php) 6.7 or newer
(`winget install JRSoftware.InnoSetup`), plus Rust and Bun:

```
powershell -ExecutionPolicy Bypass -File scripts\build.ps1 -Archive -Installer
```

Or, from a release folder you already built:
`scripts\build-installer.ps1 -Release dist\pley-server-<version>-windows-x86_64`.

- **`-Ffmpeg <bin folder>`** picks the FFmpeg build to carry (default: the
  one on `PATH`, following WinGet's link). Setup takes its DLLs and, from
  beside `bin`, its `LICENSE*`, `README.txt` and `source\` (a patched build's
  patch and script, which the GPL requires). Releases use gyan.dev's full
  build, at the version and checksum in `.github/workflows/server.yml`.
- **`-Flavor <name>`** names an installer built with a different FFmpeg:
  `-Flavor custom` makes `pley-server-<version>-custom-windows-x86_64-setup.exe`.
- `scripts\build.ps1 -Installer` passes `-Ffmpeg` and `-Flavor` on.
- `ISCC /DTest …` builds an installer that installs in the notification area
  without administrator rights, for trying Setup on a development computer.
