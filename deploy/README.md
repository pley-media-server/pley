# Running Pley

Pick the guide for where Pley will run:

| Where | How | Guide |
|---|---|---|
| A Linux server or NAS, with Docker | `./setup.sh`, then a reverse proxy | [docker/README.md](docker/README.md) |
| Unraid | A template from Docker → Add Container | [unraid/README.md](unraid/README.md) |
| Windows | The installer: a service, or in the notification area | [windows/README.md](windows/README.md) |
| Synology | A package, from Package Center's Manual Install | [synology/README.md](synology/README.md) |
| Linux without Docker | The release archive and a systemd unit | [below](#linux-systemd) |
| macOS | The release archive and a launch agent | [below](#macos-launchd) |

Whichever you pick, Pley prints a link with a one-time setup code when it
first starts. Open it, create the administrator account and add your
libraries. Until then, anyone with the code can claim the server.

## What Pley needs

- **One port, TCP 7539**, for the web app, the apps and the
  [Jellyfin-compatible API](../docs/COMPATIBILITY.md).
- **A data folder** for the database, artwork and caches. Change it with
  `--data-dir` or `PLEY_DATA_DIR`; back it up
  ([docs/STORAGE.md](../docs/STORAGE.md)).

  | OS | Default |
  |---|---|
  | Linux | `~/.local/share/pley` (`/var/lib/pley` with the systemd unit) |
  | macOS | `~/Library/Application Support/pley` |
  | Windows | `%LOCALAPPDATA%\pley` (`%ProgramData%\pley` for the service) |
  | Docker | `/config` in the container |

- **FFmpeg and ffprobe.** The Docker image and the Windows installer bring
  their own, and the Synology package uses SynoCommunity's; elsewhere Pley
  takes them from `PATH`, or `--ffmpeg` and `--ffprobe`. The FFmpeg build
  decides what Pley can do with a GPU.

Release archives (Linux, macOS, Windows; x86_64 and arm64) hold `pley` and
`pley-transcoder`, which must stay in the same folder. `pley --help` and
[`pley.example.toml`](../pley.example.toml) list every option. The
`linux-x86_64-static` archive is built against musl: it runs on a Linux
whose glibc is too old for the other one, such as DSM.

## Docker

The image is `ghcr.io/pley-media-server/pley` (amd64 and arm64; `:latest`, or a
version such as `:0.0.1`). It holds Pley, its transcoder, Debian 13's
FFmpeg 7.1 and the GPU drivers for Intel, AMD and NVIDIA.
[docker/README.md](docker/README.md) sets it up with Docker Compose, locked
down, behind a reverse proxy. Without Compose:

```sh
docker run -d --name pley --restart unless-stopped --stop-timeout 30 \
  -p 7539:7539 --shm-size 2g \
  -v /srv/pley:/config -v /srv/media:/media:ro \
  ghcr.io/pley-media-server/pley:latest
docker logs pley    # the setup link
```

- **Intel or AMD GPU:** add `--device /dev/dri`. **NVIDIA:** add
  `--gpus all` (needs the NVIDIA Container Toolkit).
- Pley runs as user and group 1000 (`-e PUID=… -e PGID=…` change them) and
  gives that user the config folder on its first start. Libraries are
  folders under `/media`.
- `docker exec -it pley pley reset-password <user>` sets a lost password.

## Linux (systemd)

From the extracted archive ([systemd/pley.service](systemd/pley.service)):

```sh
sudo useradd --system --user-group --home-dir /var/lib/pley --shell /usr/sbin/nologin pley
sudo install -m755 pley pley-transcoder /usr/local/bin/
sudo cp pley.service /etc/systemd/system/
sudo systemctl enable --now pley
journalctl -u pley    # the setup link, and the log
```

Give the `pley` user read access to your media.

**FFmpeg.** The distribution's works (Debian 13's has NVENC, VAAPI, Quick
Sync and libplacebo). Elsewhere, [jellyfin-ffmpeg](https://repo.jellyfin.org)
is a good choice: it isn't on `PATH`, so set `Environment=PLEY_FFMPEG=…` and
`PLEY_FFPROBE=…` in the unit.

**Hardware transcoding:**

- **Intel and AMD:** install the VAAPI driver (`intel-media-va-driver-non-free`
  or `mesa-va-drivers`) and `mesa-vulkan-drivers` (for HDR tone mapping),
  then uncomment `SupplementaryGroups=render video` in the unit. With several
  GPUs Pley takes the first Intel or AMD one;
  `Environment=PLEY_VAAPI_DEVICE=/dev/dri/renderD129` picks another.
- **NVIDIA:** the proprietary driver is enough. If NVENC works by hand but
  not from the service, load `nvidia-uvm` at boot.

## macOS (launchd)

From the extracted archive ([launchd/pley.plist](launchd/pley.plist)):

```sh
brew install ffmpeg-full
sudo mkdir -p /usr/local/bin && sudo cp pley pley-transcoder /usr/local/bin/
cp pley.plist ~/Library/LaunchAgents/pley.server.plist
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/pley.server.plist
```

Pley runs as you at login and encodes with VideoToolbox. It finds Homebrew's
`ffmpeg-full` by itself (plain `ffmpeg` lacks tone mapping and subtitle
burn-in). Until releases are signed, downloaded binaries may need
`xattr -d com.apple.quarantine pley pley-transcoder`.

## Windows

`pley-server-<version>-windows-<arch>-setup.exe` installs Pley with FFmpeg
and a firewall rule, as a locked-down service or in the notification area:
[windows/README.md](windows/README.md). Or extract the archive, install
FFmpeg (`winget install Gyan.FFmpeg`, whose "full" build has NVENC, QSV, AMF
and Vulkan tone mapping) and run `pley.exe`.

## Synology

`pley-server-<version>-synology-x86_64.spk` installs Pley on a Synology NAS
with an Intel processor (DSM 7.0 or newer) from Package Center's
**Manual Install**, running as a user of its own, with FFmpeg from
SynoCommunity's package and Quick Sync on models that have it:
[synology/README.md](synology/README.md).

## Checking hardware transcoding

Pley logs the encoders it found when it starts (look for `detected`), and
checks again when the GPUs change. **Settings → Transcoder** picks the
encoder (Automatic takes the GPU's), and **Settings → Dashboard** shows each
stream as GPU, hybrid or CPU.

## The tray icon

On a desktop (Windows, macOS, or Linux with a StatusNotifier host such as
KDE, or GNOME with the AppIndicator extension), Pley shows an icon while it
runs. Click it to open the web app; its menu scans the libraries, shows the
logs, turns **Start Pley at login** on or off, and quits. Services,
containers and SSH sessions never show it; `--no-tray` turns it off.

## Firewall and remote access

- **TCP 7539** is the only port that must be reachable. The Plex-compatible
  API answers there too, unless you give it a port of its own (Plex's is
  32400; `--plex-bind 0.0.0.0:32400`, or **Settings → Network**). The
  Jellyfin-compatible API is off until you turn it on in **Settings →
  Network**, where it answers on this port or one of its own (Jellyfin's is
  8096; `--jellyfin-bind 0.0.0.0:8096` on the command line).
- **UDP 7539** lets the apps find servers on the local network. Allow it, or
  type the server's address in the app. A container answers only on the
  host's network (`--network host`, or Unraid's `br0`).
- **From outside your network**, put Pley behind a reverse proxy with TLS
  (Caddy, nginx, Traefik). Docker publishes ports around host firewalls:
  with a proxy on the same computer, publish Pley on `127.0.0.1` only.

## Apps and reverse proxies

Apps sign in on Pley's own page (OAuth 2.1), and Pley names itself after the
address each request came to: if the name it gives differs from the address
an app used, the app refuses the server. So a reverse proxy must:

- pass the original `Host` header, and set `X-Forwarded-Proto` (replacing
  what the client sent);
- pass on only Pley's own names: Pley would name itself after any other
  `Host` too, and prove itself to apps under a name it no longer has.
  Caddy, Traefik and Nginx Proxy Manager answer only the names they are
  given; nginx sends other names to its first server unless another one is
  the `default_server`;
- add the client's address to `X-Forwarded-For`. Pley trusts it only from a
  proxy on the same computer, the local network or a Docker network;
- not buffer responses (live updates are server-sent events, and video
  streams as it is read);
- accept uploads of up to 25 MB (artwork).

**Caddy** does all of it by default:

```caddyfile
pley.example.com {
	reverse_proxy 127.0.0.1:7539
}
```

**nginx:**

```nginx
# Other names never reach Pley.
server {
    listen 443 ssl default_server;
    ssl_reject_handshake on;
}

server {
    listen 443 ssl;
    http2 on;
    server_name pley.example.com;
    # ssl_certificate, ssl_certificate_key ...

    client_max_body_size 25m;

    location / {
        proxy_pass http://127.0.0.1:7539;
        proxy_http_version 1.1;
        proxy_set_header Host $http_host;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_buffering off;
    }
}
```

`$http_host` keeps the port the client used; `$host` drops it. A proxy that
drops the port (Nginx Proxy Manager does by default) works when the public
address uses its scheme's default port (`https://media.example.com`). On
another port (`https://media.example.com:9999`), add the address in
**Settings → Network → Reached through a reverse proxy**, and Pley puts the
port back.

Pley's own apps can always sign in. Apps by other publishers are off until an
administrator turns them on in **Settings → Server → Apps**.

Docker-specific setups (a proxy in Docker, Traefik labels, a proxy on another
computer) are in [docker/README.md](docker/README.md#behind-a-reverse-proxy).
