# Pley on Unraid

Pley runs on Unraid 6.12 or newer as a Docker container, from the template
[`pley.xml`](pley.xml). It takes about five minutes, plus a little more to
give it your GPU for hardware transcoding.

## Install

1. **Add the template.** Open Unraid's terminal (the `>_` icon at the top
   right) and paste:

   ```sh
   wget -O /boot/config/plugins/dockerMan/templates-user/my-Pley.xml \
     https://raw.githubusercontent.com/pley-media-server/pley/master/deploy/unraid/pley.xml
   ```

2. **Create the container.** Go to **Docker → Add Container**, and under
   **Template** choose **Pley** (in *User templates*). Set **Media** to the
   share that holds your media, such as `/mnt/user/media`, then click
   **Apply**. Unraid downloads the image and starts Pley.

3. **Create your account.** In the **Docker** tab, click Pley's icon and
   choose **Logs**. Near the top is a line like
   `http://…/setup?code=ABCD-EFGH`: note the code. Then click Pley's icon
   again, choose **WebUI**, and create the administrator account with that
   code.

4. **Add your libraries.** In Pley, go to **Settings → Libraries → Add**
   and pick a folder under `/media`, such as `/media/Movies`.
   [Naming your media](../docker/media/README.md) shows how Pley expects
   files to be named.

Pley is now at `http://<your server>:7539`. In the apps, type that address
when they ask for a server.

### The template's settings

| Setting | Default | What |
|---|---|---|
| Web UI | `7539` | The port of the web app and the apps |
| Config | `/mnt/user/appdata/pley` | Database, artwork and caches: the only folder Pley writes |
| Media | `/mnt/user/media` | Your media, read-only; libraries are folders under `/media` |
| NVIDIA GPU | empty | For NVIDIA transcoding ([below](#nvidia)) |
| PUID, PGID | `99`, `100` | The user and group Pley runs as (*Show more settings*): Unraid's `nobody` and `users` |
| Log level | `info` | `debug` to look into a problem |

More than one media share? Add a path for each: at the bottom of the
template, **Add another Path, Port, Variable, Label or Device**, *Config
Type* **Path**, *Container Path* `/media2`, *Host Path* your share, *Access
Mode* **Read Only**.

## Hardware transcoding

When a device can't play a file as it is, Pley converts it, on the GPU if it
has one: much faster, and much lighter on the CPU. Pley finds the GPU by
itself; you only pass it in.

### Intel (Quick Sync) and AMD

Unraid loads the driver on its own. To check, run `ls /dev/dri` in the
terminal: it should list `renderD128`.

1. In the **Docker** tab, click Pley's icon and choose **Edit**.
2. At the bottom, click **Add another Path, Port, Variable, Label or
   Device**.
3. Set *Config Type* to **Device**, *Name* to `GPU`, and *Value* to
   `/dev/dri`. Click **Add**, then **Apply**.

Intel GPUs from the 5th generation (Broadwell) on work, Arc included; Quick
Sync is used from the 11th generation (Tiger Lake) on, VAAPI before.

### NVIDIA

1. Install the **Nvidia Driver** plugin from **Apps**, and follow what its
   page says (a reboot, the first time). Its page,
   **Settings → Nvidia Driver**, shows your GPU's UUID (`GPU-…`): copy it.
2. In the **Docker** tab, click Pley's icon and choose **Edit**.
3. Paste the UUID in **NVIDIA GPU**.
4. Turn on **Advanced View** (top right), and add `--runtime=nvidia` to
   **Extra Parameters**, after what is there. Click **Apply**.

### Is it working?

Play something Pley has to convert (in the player, the quality menu: a
lower quality). **Settings → Dashboard** shows the stream as *GPU*, *hybrid*
or *CPU*. **Settings → Transcoder** shows the encoders Pley found and lets
you pick one; *Automatic* takes the GPU's.

## Everyday use

- **Update:** in the **Docker** tab, **Check for Updates**, then
  **apply update** beside Pley. Your data stays.
- **Back up** `/mnt/user/appdata/pley`, with Pley stopped (the *Appdata
  Backup* plugin does it on a schedule). It holds the database and the key
  that two-step sign-in needs.
- **Lost password:** click Pley's icon, **Console**, and run
  `pley reset-password <user>`.
- **From outside your home:** put Pley behind a reverse proxy with HTTPS,
  such as Nginx Proxy Manager or SWAG, pointing at `http://<your
  server>:7539`. If the public address has a port of its own, add it in
  **Settings → Network → Reached through a reverse proxy**.
  [Apps and reverse proxies](../README.md#apps-and-reverse-proxies) says
  what the proxy must pass on.
- **Apps finding Pley by themselves** needs the container on your network:
  set *Network Type* to **br0** (Pley gets an address of its own) or
  **Host**. On the default *Bridge*, type the server's address in the apps.

## Troubleshooting

| What you see | What to do |
|---|---|
| The container doesn't start: `unknown or invalid runtime name: nvidia` | Install the Nvidia Driver plugin, or remove `--runtime=nvidia` from Extra Parameters |
| The container doesn't start: `error gathering device information` for `/dev/dri` | This server has no Intel or AMD GPU: remove the `/dev/dri` device |
| A library stays empty | PUID 99 can't read the share. Check the share's permissions (**Tools → New Permissions** resets a share's to Unraid's defaults) |
| The log says `can't use the GPU` | Run the `chmod` command the log gives, in the terminal |
| Transcodes stay on the CPU | Check `ls /dev/dri` (Intel, AMD) or the Nvidia Driver plugin's page, then the steps above. The log lists what Pley found at start: look for `detected` |

## How the container runs

Unraid starts every container as root. Pley's image doesn't stay root: it
gives the config folder to PUID:PGID if needed, joins the groups that may
open the GPU, and runs Pley as that user with every Linux capability
dropped ([`pley.sh`](../docker/pley.sh)). Your media is mounted read-only,
so Pley can change nothing there.
