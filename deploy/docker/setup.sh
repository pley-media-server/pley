#!/usr/bin/env sh
# Sets pley up with Docker Compose and starts it:
#   ./deploy/docker/setup.sh
# The first run writes .env: pley runs as the user who runs this (or who ran
# sudo), with hardware transcoding on the GPUs Docker can use. Then it checks
# the config and media folders, builds and starts the container, waits until
# pley answers and prints the link that creates the administrator account.
# Run it again after `git pull` to update pley; .env is kept, and a GPU it
# doesn't use yet is pointed out.
set -eu

cd "$(dirname "$0")"

say() { printf '%s\n' "$*"; }
die() {
    printf 'setup.sh: %s\n' "$*" >&2
    exit 1
}

# A setting as Docker Compose sees it: the environment first, then .env,
# then the default.
setting() {
    eval "value=\${$1-}"
    if [ -z "$value" ] && [ -f .env ]; then
        value=$(sed -n "s/^[[:space:]]*$1=//p" .env | tail -n 1 |
            sed -e 's/[[:space:]]*$//' -e 's/^"\(.*\)"$/\1/' -e "s/^'\(.*\)'\$/\1/")
    fi
    printf '%s' "${value:-$2}"
}

command -v docker >/dev/null 2>&1 || die "Docker isn't installed: https://docs.docker.com/engine/install/"
docker compose version >/dev/null 2>&1 ||
    die "Docker Compose isn't installed (Debian and Ubuntu: the docker-compose-plugin package)"
docker info >/dev/null 2>&1 ||
    die "Docker isn't running, or this user may not use it (add it to the docker group, or run this with sudo)"

case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*) os=windows sep=';' ;;
    Linux) os=linux sep=':' ;;
    *) os=other sep=':' ;;
esac
# Rootless Docker runs containers as this user: root in the container is this
# user outside it, and other users are ones with no files here.
rootless=
if docker info --format '{{.SecurityOptions}}' | grep -q rootless; then rootless=1; fi

# ---- GPUs ------------------------------------------------------------------
# On an engine that runs on this computer (not in Docker Desktop's VM): the
# first render node of an Intel or AMD GPU (not NVIDIA's, nor WSL's Direct3D
# bridge). One in group 0 would make pley a member of root's group, and
# rootless Docker can't hand the host's render group over (README.md).
render_node='' render_driver='' render_skipped=''
if [ "$os" = linux ] && ! docker info --format '{{.OperatingSystem}}' | grep -q 'Docker Desktop'; then
    for node in /dev/dri/renderD*; do
        [ -c "$node" ] || continue
        driver=$(basename "$(readlink -f "/sys/class/drm/${node##*/}/device/driver" 2>/dev/null)")
        case $driver in
            i915 | xe | amdgpu | radeon)
                render_node=$node render_driver=$driver
                break
                ;;
        esac
    done
fi
if [ -n "$render_node" ]; then
    if [ -n "$rootless" ]; then
        render_skipped="rootless Docker can't give Pley the host's render group"
    elif [ "$(stat -c %g "$render_node")" = 0 ]; then
        render_skipped="$render_node belongs to root's group"
    fi
fi
nvidia=
if docker info --format '{{json .Runtimes}}' | grep -q '"nvidia"'; then nvidia=1; fi
# Rockchip's video engines, on Rockchip's own kernel, which has MPP's device:
# compose.rockchip.yaml hands over it, RGA's, the DMA heaps and /dev/dri.
rockchip='' rockchip_skipped=''
if [ "$os" = linux ] && [ -c /dev/mpp_service ]; then
    rockchip=1
    for dev in /dev/rga /dev/dma_heap /dev/dri; do
        [ -e "$dev" ] || rockchip_skipped="this system has no $dev"
    done
    if [ -n "$rootless" ]; then
        rockchip_skipped="rootless Docker can't give Pley the host's video group"
    elif [ "$(stat -c %g /dev/mpp_service)" = 0 ]; then
        rockchip_skipped="/dev/mpp_service belongs to root's group"
    fi
fi

# ---- .env, on the first run ------------------------------------------------
if [ ! -f .env ]; then
    # Docker Desktop (macOS, Windows) maps file owners by itself; on Linux pley
    # runs as whoever owns the folders: this user, or the one who ran sudo.
    uid=1000 gid=1000
    if [ -n "$rootless" ]; then
        uid=0 gid=0
    elif [ "$os" = linux ]; then
        uid=${SUDO_UID:-$(id -u)} gid=${SUDO_GID:-$(id -g)}
        [ "$uid" != 0 ] || uid=1000 gid=1000
    fi
    set -- -e "s|^PLEY_UID=.*|PLEY_UID=$uid|" -e "s|^PLEY_GID=.*|PLEY_GID=$gid|"

    files=compose.yaml
    if [ -n "$render_node" ] && [ -z "$render_skipped" ]; then
        files="$files${sep}compose.vaapi.yaml"
        set -- "$@" -e "s|^# PLEY_RENDER_GID=.*|PLEY_RENDER_GID=$(stat -c %g "$render_node")|"
        [ "$render_node" = /dev/dri/renderD128 ] ||
            set -- "$@" -e "s|^# PLEY_RENDER_DEVICE=.*|PLEY_RENDER_DEVICE=$render_node|"
        say "Found an Intel or AMD GPU ($render_driver, $render_node): hardware transcoding is on (compose.vaapi.yaml)."
    elif [ -n "$render_node" ]; then
        say "Found an Intel or AMD GPU ($render_driver, $render_node), left off: $render_skipped (README.md, Hardware transcoding)."
    fi
    if [ -n "$nvidia" ]; then
        files="$files${sep}compose.nvidia.yaml"
        say "Found the NVIDIA Container Toolkit: NVIDIA hardware transcoding is on (compose.nvidia.yaml)."
    fi
    if [ -n "$rockchip" ] && [ -z "$rockchip_skipped" ]; then
        files="$files${sep}compose.rockchip.yaml"
        set -- "$@" -e "s|^# PLEY_ROCKCHIP_GID=.*|PLEY_ROCKCHIP_GID=$(stat -c %g /dev/mpp_service)|"
        say "Found Rockchip's video engines (/dev/mpp_service): hardware transcoding is on (compose.rockchip.yaml)."
    elif [ -n "$rockchip" ]; then
        say "Found Rockchip's video engines (/dev/mpp_service), left off: $rockchip_skipped (README.md, Hardware transcoding)."
    fi
    [ "$files" = compose.yaml ] || set -- "$@" -e "s|^# COMPOSE_FILE=.*|COMPOSE_FILE=$files|"

    sed "$@" .env.example >.env.new
    mv .env.new .env
    [ -z "${SUDO_UID-}" ] || chown "$SUDO_UID:$SUDO_GID" .env
    say "Wrote .env (Pley runs as user $uid, group $gid); edit it to change folders or ports."
else
    # A GPU added since: say how to use it, but leave .env as its owner made it.
    files=$(setting COMPOSE_FILE compose.yaml)
    case $files in
        *compose.vaapi.yaml*) ;;
        *)
            [ -z "$render_node" ] || [ -n "$render_skipped" ] ||
                say "Pley doesn't use the Intel or AMD GPU ($render_driver, $render_node): add compose.vaapi.yaml to COMPOSE_FILE and PLEY_RENDER_GID=$(stat -c %g "$render_node") to .env."
            ;;
    esac
    case $files in
        *compose.nvidia.yaml*) ;;
        *) [ -z "$nvidia" ] || say "Docker has the NVIDIA Container Toolkit, which Pley doesn't use: add compose.nvidia.yaml to COMPOSE_FILE in .env." ;;
    esac
    case $files in
        *compose.rockchip.yaml*) ;;
        *)
            [ -z "$rockchip" ] || [ -n "$rockchip_skipped" ] ||
                say "Pley doesn't use Rockchip's video engines: add compose.rockchip.yaml to COMPOSE_FILE and PLEY_ROCKCHIP_GID=$(stat -c %g /dev/mpp_service) to .env."
            ;;
    esac
fi

# ---- folders ---------------------------------------------------------------
config_dir=$(setting PLEY_CONFIG_DIR ./config)
media_dir=$(setting PLEY_MEDIA_DIR ./media)
run_uid=$(setting PLEY_UID 1000)
run_gid=$(setting PLEY_GID 1000)
[ "$run_uid" != 0 ] || [ -n "$rootless" ] || die "PLEY_UID is 0: Pley must not run as root"

if [ ! -d "$config_dir" ]; then
    mkdir -p "$config_dir" ||
        die "can't create $config_dir (PLEY_CONFIG_DIR): create it, owned by $run_uid:$run_gid"
    [ "$(id -u)" != 0 ] || chown "$run_uid:$run_gid" "$config_dir"
fi
if [ "$os" = linux ]; then
    # The user pley's files belong to outside the container.
    host_uid=$run_uid host_gid=$run_gid
    [ -z "$rootless" ] || host_uid=$(id -u) host_gid=$(id -g)
    owner=$(ls -nd "$config_dir" | awk '{print $3}')
    # Run as root on a new checkout, the folder is git's, empty: pley's now.
    if [ "$owner" != "$host_uid" ] && [ "$(id -u)" = 0 ] &&
        [ -z "$(find "$config_dir" -mindepth 1 -maxdepth 1 ! -name .gitignore | head -n 1)" ]; then
        chown "$host_uid:$host_gid" "$config_dir" && owner=$host_uid
    fi
    if [ "$owner" = "$host_uid" ]; then
        # The database and the key in it are for pley alone.
        chmod 700 "$config_dir" 2>/dev/null || true
    else
        say "Warning: $config_dir belongs to user $owner but Pley runs as $host_uid, so it may not be able to"
        say "write there. Fix it with: sudo chown -R $host_uid:$host_gid $config_dir"
    fi
fi
[ -d "$media_dir" ] || die "$media_dir (PLEY_MEDIA_DIR in .env) isn't a folder"

# ---- start -----------------------------------------------------------------
# The commit the apps show beside the version, for the build below and the
# one `up` runs (pull_policy "build"), which then finds every layer built.
PLEY_COMMIT=$(git rev-parse --short=9 HEAD 2>/dev/null || echo unknown)
export PLEY_COMMIT
# Newer base images bring Debian's security fixes along with pley's changes.
case $(setting PLEY_PULL_POLICY build) in
    build)
        say "Building Pley (the first time takes several minutes)..."
        docker compose build --pull pley
        ;;
    never) ;;
    *) docker compose pull pley ;;
esac
docker compose up -d

cid=$(docker compose ps -q pley)
[ -n "$cid" ] || die "the Pley container didn't start: docker compose logs pley"
tries=0
while :; do
    state=$(docker inspect --format '{{.State.Status}} {{.RestartCount}} {{if .State.Health}}{{.State.Health.Status}}{{end}}' "$cid")
    # Restarts while it starts mean pley stops right away.
    case $state in
        "running "*" healthy") break ;;
        "running 0 "*) ;;
        *) tries=90 ;;
    esac
    tries=$((tries + 1))
    if [ "$tries" -gt 90 ]; then
        docker logs --tail 30 "$cid" >&2 || true
        die "Pley didn't start (container: $state). Its log is above; the folders' owners are a common cause."
    fi
    sleep 2
done

# ---- how to reach it -------------------------------------------------------
addr=$(docker compose port pley 7539 2>/dev/null | head -n 1 || true)
case $addr in
    0.0.0.0:* | "[::]:"*) addr="localhost:${addr##*:}" ;;
esac
started=$(docker inspect --format '{{.State.StartedAt}}' "$cid")
code=$(docker logs --since "$started" "$cid" 2>&1 |
    sed -n 's/.*setup[?#]code=\([A-Z0-9-]*\).*/\1/p' | tail -n 1)

say ""
if [ -n "$code" ]; then
    say "Pley is running. Create the administrator account with setup code $code"
    say "(a new code is made each time Pley starts until then):"
    [ -z "$addr" ] || say "  http://$addr/setup?code=$code"
    say "  https://<your domain>/setup?code=$code   through your reverse proxy"
    say ""
    say "Then add libraries from folders under /media, such as /media/Movies."
else
    say "Pley is running${addr:+ at http://$addr}."
fi
if [ -n "$addr" ]; then
    say "Point your reverse proxy at http://$addr (README.md has Caddy, nginx and Traefik examples)."
else
    say "Your reverse proxy reaches it at http://pley:7539 on its Docker network."
fi
