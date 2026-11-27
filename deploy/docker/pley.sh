#!/bin/sh
# `pley` in the image (/usr/local/bin/pley): runs Pley, /usr/local/lib/pley/pley,
# as an ordinary user whoever starts the container, so `pley health` and
# `docker exec pley pley reset-password` run as that user too.
#
# Started as an ordinary user (Docker Compose's `user:`, `docker run --user`),
# it runs Pley as it is. Started as root, as Unraid, Synology and TrueNAS
# start containers, it runs Pley as PUID:PGID (1000:1000 unless set; the
# Unraid template sets 99:100) instead: on the first start it gives that user
# the config folder, which Docker created as root's; it adds the groups that
# may open the GPUs passed in; and Pley runs with no capabilities, unable to
# gain any. PUID=0 keeps root, for rootless Docker, where root in the
# container is the user who runs Docker.
set -eu

pley=/usr/local/lib/pley/pley

[ "$(id -u)" = 0 ] || exec "$pley" "$@"
uid=${PUID:-1000} gid=${PGID:-1000}
[ "$uid" != 0 ] || exec "$pley" "$@"

say() { printf 'pley: %s\n' "$*" >&2; }

# setuid, setgid and setpcap: to become PUID:PGID and drop every capability.
caps=$(sed -n 's/^CapEff:[[:space:]]*//p' /proc/self/status)
if [ $((0x$caps & 0x1c0)) != $((0x1c0)) ]; then
    say "started as root without the capabilities to become user $uid; start the container as that user (--user $uid:$gid) or with Docker's default capabilities"
    exit 1
fi

if [ "${1:-serve}" = serve ]; then
    # The config folder is the only one Pley writes. Docker creates a missing
    # one as root's; one used before under another PUID is that user's.
    data=${PLEY_DATA_DIR:-/config}
    if [ "$(stat -c %u "$data")" != "$uid" ]; then
        say "giving $data to user $uid, group $gid"
        chown -R "$uid:$gid" "$data" || {
            say "can't give $data to user $uid: change its owner on the host, or set PUID and PGID to its owner"
            exit 1
        }
    fi
    # A render node only root's group may open stays out of reach: joining
    # root's group would give far more than the GPU.
    for node in /dev/dri/renderD*; do
        [ -c "$node" ] || continue
        mode=$(stat -c %a "$node")
        if [ "$(stat -c %g "$node")" = 0 ] && [ $((0$mode & 6)) != 6 ]; then
            say "can't use the GPU $node: only root may open it (mode $mode). On the host, run: chmod 666 $node"
        fi
    done
fi

# The groups of the GPUs passed in (/dev/dri for Intel and AMD, /dev/nvidia*
# for NVIDIA, MPP's and RGA's devices and the DMA heaps for Rockchip), but
# never root's.
groups=
for dev in /dev/dri/* /dev/nvidia* /dev/mpp_service /dev/rga /dev/dma_heap/*; do
    [ -c "$dev" ] || continue
    group=$(stat -c %g "$dev")
    [ "$group" != 0 ] && [ "$group" != "$gid" ] || continue
    case ",$groups," in
        *",$group,"*) ;;
        *) groups=${groups:+$groups,}$group ;;
    esac
done
if [ -n "$groups" ]; then groups=--groups=$groups; else groups=--clear-groups; fi

exec setpriv --reuid="$uid" --regid="$gid" "$groups" \
    --inh-caps=-all --bounding-set=-all --no-new-privs -- "$pley" "$@"
