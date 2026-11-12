# pley-ffmpeg

FFmpeg 9 and libplacebo for the Docker image: FFmpeg's release source with
the [patches](patches) from
[jellyfin-ffmpeg](https://github.com/jellyfin/jellyfin-ffmpeg) that keep an
Intel GPU's transcodes on the GPU, built on Debian 13 against its
libraries: `ghcr.io/pley-media-server/pley-ffmpeg:<tag>`. The Pley image
copies them ([../docker/Dockerfile](../docker/Dockerfile)) instead of
compiling them on every build. The image holds files only: `/usr` (the
programs, libplacebo, their licences and sources, the patches among them)
and `/packages`, the Debian packages they link. `ffmpeg -version` names it
`9.0.2-pley`.

## The patches

FFmpeg as released decodes, scales and encodes on an Intel GPU, but HDR
leaves it on the way: Quick Sync's 10-bit frames can't be handed to OpenCL,
so they are copied to memory to be tone mapped, and its `tonemap_opencl`
has neither BT.2390 nor Dolby Vision. jellyfin-ffmpeg's patches close that
gap. With them a 4K HDR10 or Dolby Vision film is decoded, scaled, tone
mapped and encoded without a frame leaving the GPU:

```
vpp_qsv=w=1920:h=1080:format=p010le,hwmap=derive_device=opencl:mode=read,
tonemap_opencl=tonemap=bt2390:format=nv12:p=bt709:t=bt709:m=bt709:r=tv,
hwmap=derive_device=qsv:mode=write:reverse=1,format=qsv
```

- On Windows, Quick Sync and OpenCL are both made from one D3D11 device
  (`d3d11va=dx:,vendor_id=0x8086`, `qsv=qs@dx`, `opencl=ocl@dx`).
- On Linux they are made from one VAAPI device (`vaapi=va:<render node>`,
  `qsv=qs@va`, `opencl=ocl@va`), which needs Intel's OpenCL runtime. Debian
  13 has no package of it, so the Pley image is without: there Quick Sync
  tone maps with `tonemap_vaapi`.
- Dolby Vision's reshaping needs FFmpeg's own decoder, which reads it
  (`-hwaccel d3d11va` or `vaapi`, then `hwmap=derive_device=qsv`): Quick
  Sync's decoder (`-hwaccel qsv`) passes none on, and profile 5 then has
  nothing to be tone mapped by.

[patches/series](patches/series) lists them in the order they apply. Each
keeps jellyfin-ffmpeg's number and name, and says at its top what it does
and what changed when it was moved from FFmpeg 8.1 to 9.0.2.

| Patches | What they bring |
|---|---|
| 0007 | `tonemap_opencl` with BT.2390, the stream's own peak, Dolby Vision (`apply_dovi`) and dithering |
| 0009, 0010, 0011, 0012, 0040, 0072 | Quick Sync's and D3D11VA's frames, 10-bit ones too, shared with OpenCL and back to the encoder without a copy |
| 0013, 0030 | D3D11 on an Intel GPU: the adapter picked by `vendor=`, surfaces laid out as Quick Sync's |
| 0017, 0019, 0020, 0036, 0059, 0064, 0071, 0099, 0105 | Quick Sync: `vpp_qsv` tone maps HDR without HDR10 metadata and takes `passthrough`, the encoders take frames from OpenCL and leave low-power mode when the GPU refuses it, old Intel drivers start |
| 0033 | HEVC 4:2:2 and 4:4:4 decoded with D3D11VA on an Intel GPU |
| 0070, 0103 | VAAPI: `tonemap_vaapi` without mastering display metadata, 64-bit timestamps in `deinterlace_vaapi` |
| 0085 | libplacebo's SDR output isn't tagged with HDR10's metadata |
| 0058, 0067, 0068, 0098, 0106 | `ffmpeg` itself: picture subtitles burned in without the same picture uploaded for every frame, filter graphs that end in hardware frames, no stall or crash at a stream's end or change |

The rest of jellyfin-ffmpeg's series is left out: what is for other GPUs
(NVIDIA's CUDA filters, AMD's AMF, Apple's VideoToolbox, Rockchip),
filters, decoders and options Pley doesn't use, what FFmpeg 9.0.2 already
has, and the patches that only change a default (the Quick Sync encoders'
B-frames, VAAPI's scaler and its encoders' depth): what Pley wants there,
its transcoder asks for.

## Building

```
docker build -t pley-ffmpeg deploy/ffmpeg
```

It downloads FFmpeg's release source, checks it against the hash in the
Dockerfile, and stops at a patch that doesn't apply exactly. `--build-arg
JOBS=8` limits the compilers run at once.

## Updating FFmpeg, libplacebo or the patches

The image's tag is FFmpeg's version and the hash of its recipe, the
[Dockerfile](Dockerfile) and the patches (`sh tag.sh`), so a changed
recipe is a new image.

1. Change the versions, hashes or options in the Dockerfile, or the
   patches. A patch for a new FFmpeg version is made to apply exactly
   (`patch -p1 -F0`), and its header says what changed.
2. Set `FFMPEG_IMAGE` in [../docker/Dockerfile](../docker/Dockerfile) to the
   new tag (`sh deploy/ffmpeg/tag.sh`). CI fails until they match.
3. Open a pull request. Its Docker image job builds pley-ffmpeg itself and
   tests the Pley image with it.
4. On merge, the FFmpeg workflow (`.github/workflows/ffmpeg.yml`) publishes
   the new tag for amd64 and arm64, with build provenance. Published tags
   are never rebuilt.

GitHub makes the package private when it is first published: make it
public in its settings, so that anyone can build the Pley image.
