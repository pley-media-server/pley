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

# ---- FFmpeg -----------------------------------------------------------------
# FFmpeg 9, as the Windows installer carries (Debian 13's is 7.1), and the
# newer libplacebo it needs, built against Debian 13's libraries from
# release sources pinned below. The image keeps the sources and licences
# (LICENSING.md).
FROM debian:trixie-slim AS ffmpeg
ARG FFMPEG_VERSION=9.0.2
ARG FFMPEG_SHA256=8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e
ARG LIBPLACEBO_VERSION=7.360.1
ARG LIBPLACEBO_COMMIT=cee9b076f2c63104ccfd497fa79c39a867293ec4
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential ca-certificates clang curl git meson nasm ninja-build pkg-config xz-utils \
        libfast-float-dev libdovi-dev liblcms2-dev libshaderc-dev libvulkan-dev libxxhash-dev \
        python3-glad python3-jinja2 \
        libass-dev libchromaprint-dev libdav1d-dev libdrm-dev libffmpeg-nvenc-dev libfontconfig-dev \
        libfreetype-dev libfribidi-dev libharfbuzz-dev libmp3lame-dev libopus-dev libva-dev libvorbis-dev \
        libvpx-dev libwebp-dev libx264-dev libx265-dev libzimg-dev ocl-icd-opencl-dev \
    && if [ "$(dpkg --print-architecture)" = amd64 ]; then \
        apt-get install -y --no-install-recommends libvpl-dev; \
    fi \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
# libplacebo is built with the Vulkan headers it pins (a submodule), newer
# than Debian 13's.
RUN git clone --quiet --depth 1 --branch "v$LIBPLACEBO_VERSION" https://code.videolan.org/videolan/libplacebo.git \
    && test "$(git -C libplacebo rev-parse HEAD)" = "$LIBPLACEBO_COMMIT" \
    && git -C libplacebo submodule --quiet update --init --depth 1 3rdparty/Vulkan-Headers \
    && install -d /out/usr/share/doc/libplacebo \
    && install -m644 libplacebo/LICENSE /out/usr/share/doc/libplacebo/ \
    && tar --exclude-vcs -cJf "/out/usr/share/doc/libplacebo/libplacebo-$LIBPLACEBO_VERSION.tar.xz" libplacebo \
    && meson setup libplacebo/build libplacebo --prefix=/usr/local --libdir=lib --buildtype=release \
        -Dvulkan=enabled -Dshaderc=enabled -Dglslang=disabled -Dlcms=enabled -Dlibdovi=enabled \
        -Dxxhash=enabled -Dopengl=disabled -Dd3d11=disabled -Dunwind=disabled -Ddemos=false \
    && ninja -C libplacebo/build install \
    && ldconfig
# Quick Sync (oneVPL) and NVIDIA's are named on amd64, where the image's
# check (.github/workflows/server.yml) expects them; elsewhere FFmpeg takes
# NVIDIA's when it finds them.
RUN curl -fsSL --retry 5 -o "ffmpeg-$FFMPEG_VERSION.tar.xz" "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz" \
    && echo "$FFMPEG_SHA256  ffmpeg-$FFMPEG_VERSION.tar.xz" | sha256sum -c - \
    && tar -xJf "ffmpeg-$FFMPEG_VERSION.tar.xz" \
    && if [ "$(dpkg --print-architecture)" = amd64 ]; then \
        set -- --enable-libvpl --enable-ffnvcodec --enable-nvenc --enable-nvdec --enable-cuvid --enable-cuda-llvm; \
    fi \
    && cd "ffmpeg-$FFMPEG_VERSION" \
    && ./configure --prefix=/usr/local --extra-version=pley \
        --disable-debug --disable-doc --disable-ffplay \
        --enable-gpl \
        --enable-libx264 --enable-libx265 --enable-libdav1d --enable-libvpx --enable-libwebp \
        --enable-libopus --enable-libmp3lame --enable-libvorbis \
        --enable-libass --enable-libfreetype --enable-libfontconfig --enable-libfribidi --enable-libharfbuzz \
        --enable-libplacebo --enable-libzimg --enable-chromaprint \
        --enable-vaapi --enable-libdrm --enable-vulkan --enable-opencl \
        "$@" \
    && make -j"$(nproc)" \
    && make install \
    && install -d /out/usr/local/bin /out/usr/local/lib /out/usr/share/doc/ffmpeg \
    && install -m755 /usr/local/bin/ffmpeg /usr/local/bin/ffprobe /out/usr/local/bin/ \
    && cp -a /usr/local/lib/libplacebo.so.* /out/usr/local/lib/ \
    && install -m644 LICENSE.md COPYING.GPLv2 COPYING.GPLv3 COPYING.LGPLv2.1 COPYING.LGPLv3 /out/usr/share/doc/ffmpeg/ \
    && cp "../ffmpeg-$FFMPEG_VERSION.tar.xz" /out/usr/share/doc/ffmpeg/ \
    && /usr/local/bin/ffmpeg -hide_banner -buildconf > /out/usr/share/doc/ffmpeg/buildconf.txt
# The Debian packages whose libraries FFmpeg and libplacebo link, which the
# runtime image installs.
RUN set -eu; \
    ldd /usr/local/bin/ffmpeg /usr/local/bin/ffprobe > /tmp/ldd; \
    if grep 'not found' /tmp/ldd; then exit 1; fi; \
    for lib in $(awk '$2 == "=>" && $3 ~ /^\// { print $3 }' /tmp/ldd | sort -u); do \
        real=$(realpath "$lib"); \
        case "$real" in /usr/local/*) continue ;; esac; \
        owner=$(dpkg -S "$real" 2>/dev/null || dpkg -S "${real#/usr}"); \
        echo "${owner%%:*}" >> /tmp/owners; \
    done; \
    sort -u /tmp/owners > /packages; \
    cat /packages

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
COPY --from=ffmpeg /out/ /
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
