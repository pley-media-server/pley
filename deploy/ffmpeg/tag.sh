#!/bin/sh
# The pley-ffmpeg image's tag: FFmpeg's version and the hash of its recipe,
# the Dockerfile and the patches it applies in their order, so a changed
# recipe is always a new image. `tag.sh windows` gives the Windows build's
# (Dockerfile.windows, the patches and windows/).
set -eu
# windows/'s files in the same order whatever the shell's language.
export LC_ALL=C
cd "$(dirname "$0")"
dockerfile=Dockerfile
[ "${1:-}" = windows ] && dockerfile=Dockerfile.windows
version=$(sed -n 's/^ARG FFMPEG_VERSION=//p' "$dockerfile")
hash=$({
    cat "$dockerfile" patches/series
    while read -r patch; do cat "patches/$patch"; done < patches/series
    if [ "$dockerfile" = Dockerfile.windows ]; then
        for f in windows/*; do cat "$f"; done
    fi
} | sha256sum | cut -c1-12)
echo "$version-$hash"
