# syntax=docker/dockerfile:1.7
# pley-ffmpeg: FFmpeg 9 (Debian 13's is 7.1) with the patches in patches/,
# and the newer libplacebo it needs, built against Debian 13's libraries
# from the release sources pinned below. On arm64 it also has Rockchip's
# video engines (the patches in rockchip/, and the MPP and RGA libraries
# pinned below). The Pley image copies it (deploy/docker/Dockerfile); the
# FFmpeg workflow publishes it (README.md). Dockerfile.windows builds the
# same FFmpeg for Windows.
#
#   docker build -t pley-ffmpeg deploy/ffmpeg

FROM debian:trixie-slim AS build
ARG FFMPEG_VERSION=9.0.2
ARG FFMPEG_SHA256=8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e
ARG LIBPLACEBO_VERSION=7.360.1
ARG LIBPLACEBO_COMMIT=cee9b076f2c63104ccfd497fa79c39a867293ec4
# Rockchip's MPP and RGA libraries, from jellyfin-ffmpeg's mirror of them
# at the commits its 8.1.3-2 release builds (builder/scripts.d/50-rkmpp.sh
# and 50-rkrga.sh there): Rockchip publishes no release sources of its own.
ARG RKMPP_COMMIT=848b885f95587994a53216e435f650041a894e31
ARG RKRGA_COMMIT=1d330cc28551943bed3380261a5a9c6fbd58ff53
# How many compilers run at once: all the processors, unless set.
ARG JOBS
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
    && if [ "$(dpkg --print-architecture)" = arm64 ]; then \
        apt-get install -y --no-install-recommends cmake; \
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
    && ninja -C libplacebo/build ${JOBS:+-j "$JOBS"} install \
    && ldconfig \
    && rm -rf libplacebo
# Rockchip's MPP (its decoders and encoders) and RGA (its scaler), on arm64
# only: shared libraries the image carries beside FFmpeg, as jellyfin-ffmpeg's
# packages do, with their licenses and the sources they are built from.
RUN if [ "$(dpkg --print-architecture)" = arm64 ]; then \
        set -e; \
        fetch() { \
            git init -q "$1" \
            && git -C "$1" fetch -q --depth 1 https://github.com/nyanmisaka/rk-mirrors.git "$2" \
            && git -C "$1" checkout -q FETCH_HEAD \
            && test "$(git -C "$1" rev-parse HEAD)" = "$2"; \
        }; \
        fetch rkmpp "$RKMPP_COMMIT"; \
        fetch rkrga "$RKRGA_COMMIT"; \
        install -d /out/usr/share/doc/rockchip-mpp /out/usr/share/doc/librga; \
        cp -r rkmpp/LICENSES /out/usr/share/doc/rockchip-mpp/; \
        install -m644 rkrga/COPYING /out/usr/share/doc/librga/; \
        tar --exclude-vcs -cJf "/out/usr/share/doc/rockchip-mpp/rockchip-mpp-$RKMPP_COMMIT.tar.xz" rkmpp; \
        tar --exclude-vcs -cJf "/out/usr/share/doc/librga/librga-$RKRGA_COMMIT.tar.xz" rkrga; \
        cmake -S rkmpp -B rkmpp/build -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/usr/local \
            -DCMAKE_INSTALL_LIBDIR=lib -DBUILD_SHARED_LIBS=ON -DBUILD_TEST=OFF; \
        ninja -C rkmpp/build ${JOBS:+-j "$JOBS"} install; \
        meson setup rkrga/build rkrga --prefix=/usr/local --libdir=lib --buildtype=release \
            --default-library=shared -Dcpp_args=-fpermissive -Dlibdrm=false -Dlibrga_demo=false; \
        ninja -C rkrga/build ${JOBS:+-j "$JOBS"} install; \
        ldconfig; \
        rm -rf rkmpp rkrga; \
    fi
# The patches are applied in the order of their series file, and the build
# stops at one that doesn't apply exactly. Quick Sync (oneVPL) and NVIDIA's
# are named on amd64, where the image's check (.github/workflows/server.yml)
# expects them; elsewhere FFmpeg takes NVIDIA's when it finds them. There
# too, FFmpeg must have found how Quick Sync's frames are shared with
# OpenCL, which it leaves out silently otherwise. On arm64 Rockchip's
# patches follow Pley's, and MPP and RGA, under Apache-2.0, make that
# FFmpeg GPLv3 (--enable-version3). Its Vulkan headers must not know
# internally synchronised queues: FFmpeg asks for them where they do and
# the GPU's driver has them (NVIDIA's), and its libplacebo filter then
# refuses a libplacebo before 7.365, without a word.
COPY patches/ patches/
COPY rockchip/ rockchip/
RUN curl -fsSL --retry 5 --retry-all-errors --retry-delay 10 -o "ffmpeg-$FFMPEG_VERSION.tar.xz" "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz" \
    && echo "$FFMPEG_SHA256  ffmpeg-$FFMPEG_VERSION.tar.xz" | sha256sum -c - \
    && tar -xJf "ffmpeg-$FFMPEG_VERSION.tar.xz" \
    && arch=$(dpkg --print-architecture) \
    && if [ "$arch" = amd64 ]; then \
        set -- --enable-libvpl --enable-ffnvcodec --enable-nvenc --enable-nvdec --enable-cuvid --enable-cuda-llvm; \
    elif [ "$arch" = arm64 ]; then \
        set -- --enable-version3 --enable-rkmpp --enable-rkrga; \
    fi \
    && cd "ffmpeg-$FFMPEG_VERSION" \
    && while read -r patch; do \
        patch -p1 --no-backup-if-mismatch -F0 -i "../patches/$patch" || exit 1; \
    done < ../patches/series \
    && if [ "$arch" = arm64 ]; then \
        while read -r patch; do \
            patch -p1 --no-backup-if-mismatch -F0 -i "../rockchip/$patch" || exit 1; \
        done < ../rockchip/series; \
    fi \
    && ./configure --prefix=/usr/local --extra-version=pley \
        --disable-debug --disable-doc --disable-ffplay \
        --enable-gpl \
        --enable-libx264 --enable-libx265 --enable-libsvtav1 --enable-libdav1d --enable-libvpx --enable-libwebp \
        --enable-libopus --enable-libmp3lame --enable-libvorbis \
        --enable-libass --enable-libfreetype --enable-libfontconfig --enable-libfribidi --enable-libharfbuzz \
        --enable-libplacebo --enable-libzimg --enable-chromaprint \
        --enable-vaapi --enable-libdrm --enable-vulkan --enable-opencl \
        "$@" \
    && if [ "$(dpkg --print-architecture)" = amd64 ]; then \
        grep -qx '#define HAVE_OPENCL_VAAPI_INTEL_MEDIA 1' config.h; \
    fi \
    && ! grep -q VK_KHR_internally_synchronized_queues /usr/include/vulkan/vulkan_core.h \
    && make -j"${JOBS:-$(nproc)}" \
    && make install \
    && install -d /out/usr/local/bin /out/usr/local/lib /out/usr/share/doc/ffmpeg \
    && install -m755 /usr/local/bin/ffmpeg /usr/local/bin/ffprobe /out/usr/local/bin/ \
    && cp -a /usr/local/lib/libplacebo.so.* /out/usr/local/lib/ \
    && install -m644 LICENSE.md COPYING.GPLv2 COPYING.GPLv3 COPYING.LGPLv2.1 COPYING.LGPLv3 /out/usr/share/doc/ffmpeg/ \
    && cp -r "../ffmpeg-$FFMPEG_VERSION.tar.xz" ../patches /out/usr/share/doc/ffmpeg/ \
    && if [ "$arch" = arm64 ]; then \
        cp -a /usr/local/lib/librockchip_mpp.so.* /usr/local/lib/librga.so.* /out/usr/local/lib/ \
        && cp -r ../rockchip /out/usr/share/doc/ffmpeg/; \
    fi \
    && /usr/local/bin/ffmpeg -hide_banner -buildconf > /out/usr/share/doc/ffmpeg/buildconf.txt \
    && cd .. \
    && rm -rf "ffmpeg-$FFMPEG_VERSION" "ffmpeg-$FFMPEG_VERSION.tar.xz" patches rockchip /usr/local/include /usr/local/lib/*.a
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
# FFmpeg is GPLv3 on arm64, with Rockchip's MPP (Apache-2.0 and MIT) and
# RGA (Apache-2.0) beside it. TARGETARCH, the platform built for, is
# BuildKit's own: declared here without a value, it would be empty.
FROM scratch AS out-amd64
LABEL org.opencontainers.image.licenses="GPL-2.0-or-later AND LGPL-2.1-or-later"
FROM scratch AS out-arm64
LABEL org.opencontainers.image.licenses="GPL-3.0-or-later AND LGPL-2.1-or-later AND Apache-2.0 AND MIT"
FROM out-${TARGETARCH}
LABEL org.opencontainers.image.title="pley-ffmpeg" \
      org.opencontainers.image.description="FFmpeg and libplacebo for the Pley image, built on Debian 13" \
      org.opencontainers.image.source="https://github.com/pley-media-server/pley"
COPY --from=build /out/ /
