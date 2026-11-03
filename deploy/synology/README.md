# Pley on Synology

Pley installs on a Synology NAS as a package: Package Center starts and
stops it, the DSM desktop gets a Pley icon, and updates keep your data. It
needs DSM 7.0 or newer on a NAS with an Intel processor (the package is
`x86_64`; ARM models can run the [Docker image](../README.md#docker)
instead, on the CPU). On models with an Intel GPU, such as the DS920+ or the
DS224+, Pley transcodes with Quick Sync.

The package carries no FFmpeg. It runs the one in SynoCommunity's `ffmpeg7`
or `ffmpeg8` package, which is built for DSM and, with SynoCommunity's
SynoCli Video Driver, uses the NAS's GPU. An FFmpeg for other Linux systems
doesn't run on DSM, whose C library is older than theirs.

## Install

1. **Add SynoCommunity.** In Package Center, open **Settings → Package
   Sources → Add**, with the name `SynoCommunity` and the location
   `https://packages.synocommunity.com`. Under **General**, set **Trust
   Level** to **Any publisher**: SynoCommunity's packages and Pley's aren't
   signed by Synology.
2. **Install FFmpeg** from the Community tab: **ffmpeg7** (or **ffmpeg8**).
   For hardware transcoding on an Intel NAS, install **SynoCli Video
   Driver** too.
3. **Install Pley.** Download `pley-server-<version>-synology-x86_64.spk`
   from the [releases page](https://github.com/pley-media-server/pley/releases),
   then in Package Center choose **Manual Install** and pick the file. Keep
   **Run after installation** ticked.
4. **Create your account.** When Pley starts for the first time, Package
   Center shows the setup link, `http://<your NAS>:7539/setup?code=…`. Open
   it and create the administrator account. The link is also in Pley's log,
   `/var/packages/pley/var/logs/pley.log`, until the account exists.
5. **Give Pley your media.** Pley runs as a user of its own, `pley`, which
   DSM keeps out of shared folders until you let it in: in **Control Panel →
   Shared Folder**, edit the folder that holds your media, and under
   **Permissions** choose **System internal user**, then give `pley`
   **Read only**. Do the same for each media folder.
6. **Add your libraries.** Open Pley from the DSM desktop (or
   `http://<your NAS>:7539`), go to **Settings → Libraries → Add**, and pick
   a folder under `/volume1`, such as `/volume1/video/Movies`.
   [Naming your media](../docker/media/README.md) shows how Pley expects
   files to be named.

In the apps, type the NAS's address when they ask for a server. On the same
network, they find it by themselves if DSM's firewall lets Pley through
(below).

## Updating and uninstalling

**To update**, install the new version's `.spk` with **Manual Install**:
Package Center upgrades the package in place and keeps your data (the
database, artwork, settings and caches, in `/var/packages/pley/var`).

**To uninstall**, use Package Center. It asks whether to keep Pley's data
for a later install (the default) or delete it. Your media files are never
touched.

## Hardware transcoding

With SynoCli Video Driver installed, the package asks DSM for the GPU
(`/dev/dri`) and Pley's log says what it found when it starts: look for
`detected`. **Settings → Transcoder** picks the encoder (Automatic takes the
GPU's), and **Settings → Dashboard** shows each stream as GPU, hybrid or
CPU.

If Pley reports only software encoders:

- Check that `/dev/dri` has `card0` and `renderD128` (over SSH), that the
  NAS has an Intel GPU at all (the `+` models with a Celeron or Pentium do;
  the Ryzen and Xeon models don't), and that SynoCli Video Driver is
  installed and running.
- Check that the `pley` user is in the `videodriver` group: `id pley` over
  SSH. DSM adds it when the package is installed; reinstall the package if
  it is missing.

On a DS920+ (Celeron J4125) with SynoCommunity's FFmpeg 7 and SynoCli
Video Driver, Pley finds Quick Sync and VAAPI encoders for H.264 and HEVC
(10-bit included), tone maps HDR on the GPU with libplacebo or OpenCL, and
handles Dolby Vision. That GPU tone maps 4K with libplacebo at 6 to 15
frames a second depending on quality, so a 4K HDR source made SDR runs at
the fastest quality and may still not keep up; a 1080p HDR source is well
within its reach. Detection takes half a minute on that processor, once,
when Pley first starts. The first scan of a large library is slow too:
thumbnails, chapter images and credits detection all run FFmpeg on the CPU.

## Memory

Pley keeps the segments it transcodes in memory when there is room, and on
disk otherwise ([docs/TRANSCODER.md](../../docs/TRANSCODER.md#segments-in-memory)).
A NAS with 4 GB leaves little room beside DSM's own services, so most
streams go to disk, which works, just with more disk activity. More memory
helps a NAS that serves several streams at once.

## Firewall and remote access

The package registers its ports with DSM, so with DSM's firewall on you can
allow **Pley** (TCP 7539 for the web app and the apps, UDP 7539 for the
apps to find the server) and **Pley (Bonjour)** (UDP 5353, for Apple's
apps to find it once Bonjour is on in **Settings → Network**) under
**Control Panel → Security → Firewall → Edit Rules → Select from a list
of built-in applications**. The Plex- or Jellyfin-compatible API on a
port of its own (32400, 8096) needs a rule of its own: **Ports: Custom**,
TCP. From
outside your network, put Pley behind a reverse proxy with TLS, DSM's own
(**Control Panel → Login Portal → Advanced → Reverse Proxy**) or another;
[Apps and reverse proxies](../README.md#apps-and-reverse-proxies) says what
the proxy must pass on for the apps to sign in.

## Where things are

| What | Where |
|---|---|
| Pley's programs | `/var/packages/pley/target/bin/pley`, `pley-transcoder` |
| Data folder | `/var/packages/pley/var` (DSM's `@appdata/pley` on the volume) |
| Log | `/var/packages/pley/var/logs/pley.log`, rotated by Pley |
| Settings file | `/var/packages/pley/var/pley.toml` (optional; [`pley.example.toml`](../../pley.example.toml)) |
| FFmpeg | SynoCommunity's, `/var/packages/ffmpeg7/target/bin` or `ffmpeg8` (the newest installed); `ffmpeg` and `ffprobe` in `pley.toml` pick another |

Over SSH, `sudo -u pley /var/packages/pley/target/bin/pley --data-dir
/var/packages/pley/var reset-password <user>` sets a lost password while
Pley is stopped.

## How the package works

- Pley runs as DSM's package user `pley` (`conf/privilege`), never as root,
  with the data folder DSM gives packages. DSM owns the folder's lifetime:
  kept across upgrades, deleted on uninstall only when the wizard says so
  (`WIZARD_UIFILES/uninstall_uifile`, `scripts/postuninst`).
- `conf/resource` asks DSM for the GPU's device files (`video-driver`) and
  registers the ports (`app/pley.sc`); `app/config` is the desktop icon.
- `scripts/start-stop-status` starts `pley --no-tray` with `PLEY_DATA_DIR`
  and SynoCommunity's FFmpeg in `PLEY_FFMPEG` and `PLEY_FFPROBE`, waits for
  it to listen, and reports the setup link to Package Center the first time.
  `scripts/preinst` refuses to install without an FFmpeg package.
- The programs are a static Linux build (`x86_64-unknown-linux-musl`), so
  DSM's C library doesn't matter: the same archive is published as
  `pley-server-<version>-linux-x86_64-static.tar.gz`.

To build the package yourself: `scripts/build.sh --target
x86_64-unknown-linux-musl --archive` (on a Mac, `PLEY_ZIGBUILD=1` with
[cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild)), then
`scripts/build-spk.sh dist/pley-server-<version>-linux-x86_64-static`.
