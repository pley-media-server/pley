# pley-ffmpeg

FFmpeg 9 and libplacebo for the Docker image, built on Debian 13 against
its libraries: `ghcr.io/pley-media-server/pley-ffmpeg:<tag>`. The Pley
image copies them ([../docker/Dockerfile](../docker/Dockerfile)) instead of
compiling them on every build. The image holds files only: `/usr` (the
programs, libplacebo, their licences and sources) and `/packages`, the
Debian packages they link.

Its tag is FFmpeg's version and the [Dockerfile](Dockerfile)'s hash
(`sh tag.sh`), so a changed recipe is a new image.

## Updating FFmpeg or libplacebo

1. Change the versions, hashes or options in the [Dockerfile](Dockerfile).
2. Set `FFMPEG_IMAGE` in [../docker/Dockerfile](../docker/Dockerfile) to the
   new tag (`sh deploy/ffmpeg/tag.sh`). CI fails until they match.
3. Open a pull request. Its Docker image job builds pley-ffmpeg itself and
   tests the Pley image with it.
4. On merge, the FFmpeg workflow (`.github/workflows/ffmpeg.yml`) publishes
   the new tag for amd64 and arm64, with build provenance. Published tags
   are never rebuilt.

GitHub makes the package private when it is first published: make it
public in its settings, so that anyone can build the Pley image.
