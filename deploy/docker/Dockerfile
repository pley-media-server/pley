# syntax=docker/dockerfile:1.7
# pley media server and its transcoder, on Debian 13 with FFmpeg 9:
# NVIDIA (NVENC, CUDA scaling), Intel (Quick Sync, VAAPI) and AMD (VAAPI)
# transcoding, and HDR tone mapping on the GPU (libplacebo over Vulkan,
# OpenCL) or the CPU (zscale).
#
#   docker build -f deploy/docker/Dockerfile -t pley .
#   docker run -d -p 127.0.0.1:7539:7539 -v ./config:/config -v /srv/media:/media:ro pley
#
# deploy/docker/README.md sets it up with Docker Compose, hardened, and
# covers hardware transcoding; deploy/unraid has the Unraid template.

# FFmpeg and libplacebo, built by deploy/ffmpeg. The tag is
# deploy/ffmpeg/tag.sh's, which CI checks.
ARG FFMPEG_IMAGE=ghcr.io/pley-media-server/pley-ffmpeg:9.0.2-294bcb128bd3
FROM ${FFMPEG_IMAGE} AS ffmpeg

# ---- web app ----------------------------------------------------------------
FROM oven/bun:1.4.4-slim AS web
# The commit apps show beside the version (scripts/commit.mjs): the build
# context has no .git.
ARG PLEY_COMMIT=unknown
ENV PLEY_COMMIT=$PLEY_COMMIT
WORKDIR /src
COPY package.json bun.lock bunfig.toml ./
COPY scripts/commit.mjs scripts/commit.d.mts scripts/
COPY apps/web/package.json apps/web/
COPY packages/picture-subtitles/package.json packages/picture-subtitles/
RUN bun install --frozen-lockfile
COPY packages/picture-subtitles packages/picture-subtitles
COPY apps/web apps/web
RUN bun run build

# ---- Rust binaries ----------------------------------------------------------
FROM rust:1-trixie AS build
ARG PLEY_COMMIT=unknown
ENV PLEY_COMMIT=$PLEY_COMMIT
WORKDIR /src
COPY Cargo.toml Cargo.lock rustfmt.toml ./
COPY apps/server apps/server
COPY apps/transcoder apps/transcoder
COPY packages packages
COPY --from=web /src/apps/web/dist apps/web/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p pley-server -p pley-transcoder \
    && install -Dm755 target/release/pley target/release/pley-transcoder -t /out/bin

# ---- runtime ----------------------------------------------------------------
FROM debian:trixie-slim
ARG PLEY_COMMIT=unknown
LABEL org.opencontainers.image.title="Pley" \
      org.opencontainers.image.description="Pley media server, with its transcoder and FFmpeg" \
      org.opencontainers.image.source="https://github.com/pley-media-server/pley" \
      org.opencontainers.image.documentation="https://github.com/pley-media-server/pley/blob/master/deploy/docker/README.md" \
      org.opencontainers.image.licenses="AGPL-3.0-only" \
      org.opencontainers.image.revision=$PLEY_COMMIT
# Mesa brings VAAPI for AMD and Vulkan for AMD and Intel. Intel's own VAAPI
# driver and the runtime FFmpeg's Quick Sync encoders load (oneVPL) exist for
# amd64 only. Intel's is the driver from Debian's non-free section: the free
# one lacks the shader encoder 6th to 10th generation GPUs (Skylake to Comet
# Lake, many a home server's) need for H.264 at a target bitrate unless the
# host loads their HuC firmware, which it doesn't by default, so they would
# transcode on the CPU. NVIDIA's container toolkit brings its driver's
# libraries; the OpenCL loader learns of NVIDIA's from the file below. No
# program keeps a setuid or setgid bit: nothing in the image can gain
# privileges, even when it is run without no-new-privileges.
#
# FFmpeg, libplacebo, and the Debian packages with the libraries they link.
COPY --from=ffmpeg /usr/ /usr/
RUN --mount=type=bind,from=ffmpeg,source=/packages,target=/tmp/ffmpeg-packages \
    apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates tini $(cat /tmp/ffmpeg-packages) \
        libvulkan1 mesa-vulkan-drivers mesa-va-drivers \
    && if [ "$(dpkg --print-architecture)" = amd64 ]; then \
        sed -i 's/^Components: main$/Components: main non-free/' /etc/apt/sources.list.d/debian.sources \
        && apt-get update \
        && apt-get install -y --no-install-recommends intel-media-va-driver-non-free libmfx-gen1.2; \
    fi \
    && rm -rf /var/lib/apt/lists/* \
    && ldconfig \
    && ffmpeg -hide_banner -version >/dev/null \
    && mkdir -p /etc/OpenCL/vendors && echo libnvidia-opencl.so.1 > /etc/OpenCL/vendors/nvidia.icd \
    && find / -xdev -perm /6000 -type f -exec chmod a-s {} + \
    && useradd --uid 1000 --user-group --no-create-home --home-dir /config --shell /usr/sbin/nologin pley \
    && mkdir -p /config /media && chown pley:pley /config && chmod 700 /config

# Pley and its transcoder side by side (it starts the one beside it), and
# `pley` the script that runs Pley as an ordinary user however the container
# is started (pley.sh).
COPY --from=build /out/bin/ /usr/local/lib/pley/
COPY --chmod=755 deploy/docker/pley.sh /usr/local/bin/pley
# Pley's licence. FFmpeg's and libplacebo's, with their sources, are in
# /usr/share/doc/ffmpeg and /usr/share/doc/libplacebo; Debian's packages keep
# theirs in /usr/share/doc/<package>/copyright (LICENSING.md).
COPY LICENSE /usr/share/doc/pley/LICENSE.txt
COPY LICENSING.md /usr/share/doc/pley/

# HOME is /config for any user the container runs as (`--user`), so the GPU
# drivers' shader caches and fontconfig's land in the one writable folder.
# Started as root (no `--user`: Unraid, Synology, TrueNAS), Pley runs as
# PUID:PGID (pley.sh); started as a user, as that user.
ENV PLEY_DATA_DIR=/config \
    PLEY_BIND=0.0.0.0:7539 \
    HOME=/config \
    PUID=1000 \
    PGID=1000 \
    NVIDIA_DRIVER_CAPABILITIES=compute,video,utility,graphics
VOLUME ["/config"]
# The web app and its API, and the compatibility APIs when they answer on
# Pley's port (the Plex-compatible one does by default).
EXPOSE 7539
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --start-interval=2s \
    CMD ["pley", "health"]
ENTRYPOINT ["tini", "--", "pley"]
