#!/bin/sh
# The pley-ffmpeg image's tag: FFmpeg's version and the Dockerfile's hash, so
# a changed recipe is always a new image.
set -eu
dockerfile="$(dirname "$0")/Dockerfile"
version=$(sed -n 's/^ARG FFMPEG_VERSION=//p' "$dockerfile")
hash=$(sha256sum "$dockerfile" | cut -c1-12)
echo "$version-$hash"
