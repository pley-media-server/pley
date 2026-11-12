#!/bin/sh
# The pley-ffmpeg image's tag: FFmpeg's version and the hash of its recipe,
# the Dockerfile and the patches it applies in their order, so a changed
# recipe is always a new image.
set -eu
cd "$(dirname "$0")"
version=$(sed -n 's/^ARG FFMPEG_VERSION=//p' Dockerfile)
hash=$({
    cat Dockerfile patches/series
    while read -r patch; do cat "patches/$patch"; done < patches/series
} | sha256sum | cut -c1-12)
echo "$version-$hash"
