# syntax=docker/dockerfile:1.7
# pley-ffmpeg: FFmpeg 9, as the Windows installer carries (Debian 13's is
# 7.1), and the newer libplacebo it needs, built against Debian 13's
# libraries from the release sources pinned below. The Pley image copies it
# (deploy/docker/Dockerfile); the FFmpeg workflow publishes it
# (README.md).
#
#   docker build -t pley-ffmpeg deploy/ffmpeg

FROM debian:trixie-slim AS build
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
        libfreetype-dev libfribidi-dev libharfbuzz-dev libmp3lame-dev libopus-dev libsvtav1enc-dev libva-dev \
        libvorbis-dev libvpx-dev libwebp-dev libx264-dev libx265-dev libzimg-dev ocl-icd-opencl-dev \
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
RUN curl -fsSL --retry 5 --retry-all-errors --retry-delay 10 -o "ffmpeg-$FFMPEG_VERSION.tar.xz" "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz" \
    && echo "$FFMPEG_SHA256  ffmpeg-$FFMPEG_VERSION.tar.xz" | sha256sum -c - \
    && tar -xJf "ffmpeg-$FFMPEG_VERSION.tar.xz" \
    && if [ "$(dpkg --print-architecture)" = amd64 ]; then \
        set -- --enable-libvpl --enable-ffnvcodec --enable-nvenc --enable-nvdec --enable-cuvid --enable-cuda-llvm; \
    fi \
    && cd "ffmpeg-$FFMPEG_VERSION" \
    && ./configure --prefix=/usr/local --extra-version=pley \
        --disable-debug --disable-doc --disable-ffplay \
        --enable-gpl \
        --enable-libx264 --enable-libx265 --enable-libsvtav1 --enable-libdav1d --enable-libvpx --enable-libwebp \
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
# Pley image installs.
RUN set -eu; \
    ldd /usr/local/bin/ffmpeg /usr/local/bin/ffprobe > /tmp/ldd; \
    if grep 'not found' /tmp/ldd; then exit 1; fi; \
    for lib in $(awk '$2 == "=>" && $3 ~ /^\// { print $3 }' /tmp/ldd | sort -u); do \
        real=$(realpath "$lib"); \
        case "$real" in /usr/local/*) continue ;; esac; \
        owner=$(dpkg -S "$real" 2>/dev/null || dpkg -S "${real#/usr}"); \
        echo "${owner%%:*}" >> /tmp/owners; \
    done; \
    sort -u /tmp/owners > /out/packages; \
    cat /out/packages

# Files only, for the Pley image to copy: /usr, and /packages, the list above.
FROM scratch
LABEL org.opencontainers.image.title="pley-ffmpeg" \
      org.opencontainers.image.description="FFmpeg and libplacebo for the Pley image, built on Debian 13" \
      org.opencontainers.image.source="https://github.com/pley-media-server/pley" \
      org.opencontainers.image.licenses="GPL-2.0-or-later AND LGPL-2.1-or-later"
COPY --from=build /out/ /
