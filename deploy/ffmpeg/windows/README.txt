pley-ffmpeg for Windows
=======================

ffmpeg.exe and ffprobe.exe, the FFmpeg that Pley's transcoder runs: FFmpeg
built from its release source with patches from jellyfin-ffmpeg that keep
Intel Quick Sync transcodes on the GPU. `ffmpeg -version` gives the version
and the options it was configured with.

Licence
-------

FFmpeg is free software, from https://ffmpeg.org. This build is under the
GNU General Public License, version 3 or later (LICENSE.txt): it is
configured with --enable-gpl and --enable-version3, and has x264, x265 and
other libraries under the GPL built in. It is a separate program from Pley,
which runs it and links none of it.

Source
------

The source folder has what these programs were built from:

  ffmpeg-<version>.tar.xz  FFmpeg's release source, from
                           https://ffmpeg.org/releases
  recipe.tar.xz            the patches applied to it, in the order of their
                           series file, and the script that builds it,
                           Dockerfile.windows. In a folder with them:
                           docker build -f Dockerfile.windows --output out .
  zvbi-<commit>.tar.gz     libzvbi's source, which the script builds too
  configuration.txt        the options FFmpeg was configured with
  libraries.txt            the libraries built in, their versions, and the
                           image they come from

The other libraries come built for Windows from BtbN/FFmpeg-Builds
(https://github.com/BtbN/FFmpeg-Builds), whose scripts.d folder names the
source of each. The patches are jellyfin-ffmpeg's
(https://github.com/jellyfin/jellyfin-ffmpeg), under FFmpeg's licence.

The recipe is kept with Pley's source, in deploy/ffmpeg:
https://github.com/pley-media-server/pley
