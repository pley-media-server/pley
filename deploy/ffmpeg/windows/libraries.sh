#!/bin/sh
# The libraries FFmpeg's programs are linked with, each with the version its
# pkg-config file gives, and the headers of NVIDIA's and AMD's encoders:
# source\libraries.txt of the Windows build (Dockerfile.windows). Run in
# FFmpeg's build folder, after configure.
set -eu
lib="$FFBUILD_PREFIX/lib"
# What FFmpeg links that the image built, not Windows' or the compiler's.
linked=$(sed -n 's/^EXTRALIBS-[a-z]*=//p' ffbuild/config.mak | tr ' ' '\n' | sed -n 's/^-l:\{0,1\}//p' | sort -u |
    while read -r name; do
        if [ -e "$lib/lib$name.a" ] || [ -e "$lib/$name" ]; then echo "$name"; fi
    done)
named=""
{
    for pc in "$lib"/pkgconfig/*.pc; do
        # A library's own name is the first one its pkg-config file links.
        own=$(sed -n 's/^Libs:[^-]*\(-L[^ ]* *\)*-l:\{0,1\}\([^ ]*\).*/\2/p' "$pc" | head -1)
        if [ -n "$own" ] && echo "$linked" | grep -qxF -- "$own"; then
            echo "$(sed -n 's/^Name: *//p' "$pc" | sed 's/ *$//') $(sed -n 's/^Version: *//p' "$pc")"
            named="$named $own"
        fi
    done
    # Those without a pkg-config file, by name alone.
    for name in $linked; do
        case " $named " in *" $name "*) ;; *) echo "$name" ;; esac
    done
    echo "nv-codec-headers $(pkg-config --modversion ffnvcodec)"
    amf="$FFBUILD_PREFIX/include/AMF/core/Version.h"
    echo "AMF headers $(sed -n 's/^#define AMF_VERSION_MAJOR *//p' "$amf").$(sed -n 's/^#define AMF_VERSION_MINOR *//p' "$amf").$(sed -n 's/^#define AMF_VERSION_RELEASE *//p' "$amf")"
} | sort -fu
