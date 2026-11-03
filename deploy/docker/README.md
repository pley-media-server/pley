# Pley in Docker

Pley and its transcoder in one container, set up with Docker Compose on a
Linux computer at home, locked down, behind a reverse proxy that handles
HTTPS. Everything is in this folder. On Unraid, use
[the Unraid template](../unraid/README.md) instead.

## Quick start

You need Docker Engine with the Compose plugin (or Docker Desktop).

1. Get Pley and run the setup script:

   ```sh
   git clone https://github.com/pley-media-server/pley.git
   cd pley/deploy/docker
   ./setup.sh
   ```

   On its first run it writes `.env` (Pley runs as you, with hardware
   transcoding on the GPUs Docker can use), builds the image (several
   minutes the first time) and starts Pley.

2. Open the link it prints, which carries a one-time setup code, and create
   the administrator account.

3. Add your libraries in **Settings → Libraries**: folders under `/media`,
   such as `/media/Movies`. Media goes in `media/` here unless `.env` says
   otherwise; [media/README.md](media/README.md) shows how to name it.

Pley is now on `http://127.0.0.1:7539`, on this computer only: point your
reverse proxy at it ([below](#behind-a-reverse-proxy)).

**Without the script** (or on Windows):

```sh
cp .env.example .env        # then edit it: media folder, user, port
docker compose up -d
docker compose logs pley    # the setup link
```

## Settings

All in `.env`; [.env.example](.env.example) explains each one.

| Setting | Default | What |
|---|---|---|
| `PLEY_CONFIG_DIR` | `./config` | Database, artwork and caches |
| `PLEY_MEDIA_DIR` | `./media` | Your media, read-only at `/media` |
| `PLEY_UID`, `PLEY_GID` | `1000` | The user Pley runs as: owns the config folder, reads the media |
| `PLEY_LISTEN` | `127.0.0.1:7539` | Where the web port is published |
| `COMPOSE_FILE` | `compose.yaml` | The add-ons turned on (below) |
| `PLEY_RENDER_GID` | | Intel / AMD: the GPU render node's group |
| `PLEY_RENDER_DEVICE` | `/dev/dri/renderD128` | Intel / AMD: which GPU |
| `PLEY_PROXY_NETWORK` | `proxy` | `compose.proxy.yaml`: the proxy's Docker network |
| `PLEY_PLEX_LISTEN` | `127.0.0.1:32400` | `compose.plex.yaml`: where the Plex-compatible API is published |
| `PLEY_JELLYFIN_LISTEN` | `127.0.0.1:8096` | `compose.jellyfin.yaml`: where the Jellyfin-compatible API is published |
| `PLEY_IMAGE` | `pley:local` | The image; a release is `ghcr.io/pley-media-server/pley:<version>` |
| `PLEY_PULL_POLICY` | `build` | `build` from this checkout, or `missing` for a release |
| `PLEY_SHM_SIZE` | `2gb` | `/dev/shm`, for transcoding in memory |
| `PLEY_LOG` | `info` | Log detail (`debug` to look into a problem) |

**Add-ons** are compose files listed in `COMPOSE_FILE`, separated by `:`
(`;` on Windows), for example
`COMPOSE_FILE=compose.yaml:compose.vaapi.yaml:compose.jellyfin.yaml`:

| File | What |
|---|---|
| `compose.vaapi.yaml` | Intel or AMD hardware transcoding |
| `compose.nvidia.yaml` | NVIDIA hardware transcoding |
| `compose.proxy.yaml` | A reverse proxy in Docker: no published port |
| `compose.plex.yaml` | The [Plex-compatible API](../../docs/COMPATIBILITY.md) on port 32400 (without it, it answers on Pley's port) |
| `compose.jellyfin.yaml` | The [Jellyfin-compatible API](../../docs/COMPATIBILITY.md) on port 8096 (without it, Settings → Network can turn it on, on Pley's port) |

Your own changes go in `compose.override.yaml` next to `compose.yaml` (git
ignores it). Compose reads it by itself unless `COMPOSE_FILE` is set; then
add it to the list.

What changes while Pley runs is in its web app: the TMDB key in **Settings →
Server**; hardware acceleration, tone mapping and transcode limits in
**Settings → Transcoder**.

**More media folders:** `PLEY_MEDIA_DIR` can be a folder holding all your
disks (`/mnt`). Or mount each extra folder at a path of its own, outside
`/media`, in `compose.override.yaml`, always read-only:

```yaml
services:
  pley:
    volumes:
      - /mnt/archive/Movies:/archive/Movies:ro
```

## Hardware transcoding

`setup.sh` turns it on at its first run for the GPUs Docker can use, and on
later runs points out one Pley doesn't use yet.

| GPU | What Pley uses | By hand |
|---|---|---|
| Intel, AMD | VAAPI, Quick Sync (Intel 11th generation and newer), Vulkan tone mapping | Add `compose.vaapi.yaml`, and set `PLEY_RENDER_GID` to `stat -c %g /dev/dri/renderD128`. With several GPUs, `PLEY_RENDER_DEVICE` picks one (`ls -l /dev/dri/by-path` names them) |
| NVIDIA | NVENC, NVDEC, CUDA scaling, Vulkan tone mapping | Install the driver and the [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html), then add `compose.nvidia.yaml` |

- Intel's drivers are in the amd64 image only, and work from the 5th
  generation (Broadwell) on.
- HDR is tone mapped on the GPU when it has Vulkan, else on the CPU.
- arm64 boards such as the Raspberry Pi transcode on the CPU.
- **Docker Desktop on Windows** can't hand Pley the GPU in a way it can use
  for HDR: install [Pley for Windows](../windows/README.md) instead.

To check: the log lists the encoders Pley found
(`docker compose logs pley | grep detected`), and **Settings → Dashboard**
shows each stream as GPU, hybrid or CPU.

## Behind a reverse proxy

Pley serves plain HTTP; the proxy adds HTTPS.
[Apps and reverse proxies](../README.md#apps-and-reverse-proxies) says what
it must pass on, with Caddy and nginx examples. Set Pley up before the proxy
makes it reachable from the internet.

- **A proxy on this computer:** point it at `http://127.0.0.1:7539`.
- **A proxy in Docker** (Caddy, Traefik, Nginx Proxy Manager): add
  `compose.proxy.yaml`. Pley joins the proxy's network, publishes no port,
  and the proxy reaches it at `http://pley:7539`. Create the network once
  (`docker network create proxy`) and attach the proxy to it, or set
  `PLEY_PROXY_NETWORK` to the proxy's own. For Traefik, label Pley in
  `compose.override.yaml`:

  ```yaml
  services:
    pley:
      labels:
        traefik.enable: "true"
        traefik.http.routers.pley.rule: Host(`pley.example.com`)
        traefik.http.routers.pley.entrypoints: websecure
        traefik.http.routers.pley.tls.certresolver: letsencrypt
        traefik.http.services.pley.loadbalancer.server.port: "7539"
  ```

- **A proxy on another computer:** publish Pley on this computer's LAN
  address (`PLEY_LISTEN=192.168.1.10:7539`) and let only the proxy reach it.
  Docker's port rules go around ufw and firewalld, so filter in the
  `DOCKER-USER` chain:
  `iptables -I DOCKER-USER -p tcp --dport 7539 ! -s <proxy address> -j DROP`.

## Everyday use

From this folder:

| To | Run |
|---|---|
| See the log | `docker compose logs -f pley` |
| Update | `git pull && ./setup.sh` |
| Stop, start | `docker compose stop`, `docker compose start` |
| Remove the container (data stays) | `docker compose down` |
| Set a lost password | `docker compose exec pley pley reset-password <user>` |
| Back up | `docker compose stop && tar -czf pley-config-$(date +%F).tar.gz -C config . && docker compose start` |

Pley also keeps its last 3 daily database copies in `config/backups`. A full
backup is the whole config folder: the database alone can't decrypt
two-step sign-in without `secret.key`.

**A published image** instead of building: set
`PLEY_IMAGE=ghcr.io/pley-media-server/pley:<version>` and `PLEY_PULL_POLICY=missing`,
and update with `docker compose pull && docker compose up -d`.

## Other ways to run the image

Started with `user:` as above, Pley runs as that user. NAS systems such as
Synology and TrueNAS start containers as root instead; Pley then runs as
`PUID`:`PGID` (1000 unless set), gives that user the config folder on its
first start, and joins the groups of the GPUs passed in. The image's
[`pley.sh`](pley.sh) does this, and logs what it changed.

```sh
docker run -d --name pley -p 7539:7539 --shm-size 2g \
  -e PUID=1000 -e PGID=1000 \
  -v /srv/pley:/config -v /srv/media:/media:ro \
  --device /dev/dri \
  ghcr.io/pley-media-server/pley:latest
```

For NVIDIA, use `--gpus all` instead of `--device /dev/dri`. For the
Plex-compatible API on Plex's port, add
`-p 32400:32400 -e PLEY_PLEX_BIND=0.0.0.0:32400`; it answers on Pley's port
without them.

## Security

The container starts locked down; nothing here needs changing for Pley to
work.

- **One port, on this computer only** (`127.0.0.1`), so the reverse proxy,
  with its HTTPS and access rules, is the only way in. Network discovery
  (the apps' broadcast and Bonjour) is off, since neither reaches a
  container on Docker's own network; with host networking, remove
  `PLEY_NO_DISCOVERY`, and turn Bonjour on in Settings → Network for
  Apple's devices. The transcoder listens inside the
  container only.
- **Not root, no privileges.** Pley runs as your user, with every Linux
  capability dropped and `no-new-privileges`; the image has no setuid
  programs.
- **Read-only everywhere but its data.** The image is read-only, `/tmp` can't
  run programs, and media is read-only. The config folder, the only writable
  one, is private (`chmod 700`): it holds the database and the key that
  encrypts two-step sign-in secrets.
- **Only the GPU, not the machine.** The Intel/AMD add-on hands over one
  render node; NVIDIA's goes through the toolkit.
- **Bounded.** Processes are capped and Docker's log is rotated. On a machine
  that does other work, also cap memory and CPU in `compose.override.yaml`
  (`mem_limit: 8g`, `cpus: 6`; each transcode takes about 1 GB).
- **Provenance.** Each release's image has build provenance:
  `gh attestation verify oci://ghcr.io/pley-media-server/pley:<version> --repo pley-media-server/pley`.

On Pley's side ([SECURITY.md](../../SECURITY.md)): set Pley up before it can
be reached from the internet, turn on two-step sign-in for administrators,
and keep it up to date (`setup.sh` also fetches newer base images, with
Debian's security fixes).

## Troubleshooting

| What you see | What to do |
|---|---|
| `Permission denied` for `/config` | The config folder isn't `PLEY_UID`'s: `sudo chown -R <PLEY_UID>:<PLEY_GID> config`, or set `PLEY_UID`/`PLEY_GID` to its owner (`ls -ln`) |
| A library stays empty, or "cannot open /media/…" | `PLEY_UID` can't read the media. If a group can (`ls -ln`), put its id in `PLEY_GID` |
| `bind source path does not exist` | A folder in `.env` is missing. Create it: Compose doesn't, as it would belong to root |
| `port is already allocated` | Something else uses 7539: change `PLEY_LISTEN` (`127.0.0.1:7600`) and your proxy |
| Transcodes stay on the CPU | The log's `detected` line lists the encoders found. Intel/AMD: `docker compose exec pley ls -l /dev/dri` should list `renderD128`. NVIDIA: `docker compose exec pley nvidia-smi` should list the GPU |
| Rootless Docker | `PLEY_UID=0` and `PLEY_GID=0` (what `setup.sh` picks): root in the container is you. The host's render group doesn't carry over, so Intel and AMD transcoding stays off |
