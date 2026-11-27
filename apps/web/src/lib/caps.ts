import type { ClientCaps } from "@/api/types";

let videoCache: ClientCaps | null = null;
let audioCache: ClientCaps | null = null;

function probe(el: HTMLMediaElement, type: string): boolean {
  try {
    return el.canPlayType(type) !== "";
  } catch {
    return false;
  }
}

/** Subtitles the server turns into WebVTT for the player (as the TVs list them). */
const TEXT_SUBTITLES = ["subrip", "ass", "ssa", "webvtt", "mov_text"];
/** Picture subtitles the player draws itself (player/pictures.ts): PGS, VobSub, DVB. */
export const PICTURE_SUBTITLES = ["hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle"];

type MseCtor = { isTypeSupported(type: string): boolean };

/** Media Source Extensions support (`ManagedMediaSource` on iOS Safari). */
function mse(type: string): boolean {
  const w = window as unknown as { MediaSource?: MseCtor; ManagedMediaSource?: MseCtor };
  try {
    return !!(w.MediaSource?.isTypeSupported(type) || w.ManagedMediaSource?.isTypeSupported(type));
  } catch {
    return false;
  }
}

/** Decodable either by the element itself or through MSE. */
function decodes(el: HTMLMediaElement, ...types: string[]): boolean {
  return types.some((t) => probe(el, t) || mse(t));
}

/** Chrome, Edge, Opera, … play Matroska through `<video src>` even though they don't advertise it. */
function isChromium(): boolean {
  const uaData = (navigator as Navigator & { userAgentData?: { brands?: { brand: string }[] } }).userAgentData;
  if (uaData?.brands?.length) return uaData.brands.some((b) => b.brand === "Chromium");
  return /\b(Chrome|Chromium)\//.test(navigator.userAgent) && !/\bEdge\//.test(navigator.userAgent);
}

function audioCodecs(el: HTMLMediaElement): string[] {
  const codecs: string[] = [];
  if (probe(el, 'audio/mp4; codecs="mp4a.40.2"')) codecs.push("aac");
  if (probe(el, "audio/mpeg") || probe(el, 'audio/mp4; codecs="mp3"')) codecs.push("mp3");
  if (probe(el, 'audio/webm; codecs="opus"') || probe(el, 'audio/ogg; codecs="opus"') || probe(el, 'audio/mp4; codecs="opus"'))
    codecs.push("opus");
  if (probe(el, "audio/flac") || probe(el, 'audio/mp4; codecs="flac"') || probe(el, 'audio/ogg; codecs="flac"'))
    codecs.push("flac");
  if (probe(el, 'audio/ogg; codecs="vorbis"') || probe(el, 'audio/webm; codecs="vorbis"')) codecs.push("vorbis");
  if (probe(el, 'audio/mp4; codecs="ac-3"')) codecs.push("ac3");
  if (probe(el, 'audio/mp4; codecs="ec-3"')) codecs.push("eac3");
  if (probe(el, 'audio/mp4; codecs="alac"')) codecs.push("alac");
  if (probe(el, 'audio/wav; codecs="1"') || probe(el, "audio/wav")) codecs.push("pcm_s16le", "pcm_s24le");
  return codecs;
}

/** What this browser can direct-play in a `<video>` element. */
export function videoCaps(): ClientCaps {
  if (videoCache) return videoCache;
  const v = document.createElement("video");
  const containers: string[] = [];
  if (probe(v, "video/mp4")) containers.push("mp4", "m4v", "mov");
  if (probe(v, "video/webm")) containers.push("webm");
  if (probe(v, "video/x-matroska") || probe(v, "video/matroska") || isChromium()) containers.push("mkv");

  const video: string[] = [];
  if (decodes(v, 'video/mp4; codecs="avc1.640028"', 'video/mp4; codecs="avc1.42E01E"')) video.push("h264");
  if (decodes(v, 'video/mp4; codecs="hvc1.1.6.L120.90"', 'video/mp4; codecs="hev1.1.6.L120.90"')) video.push("hevc");
  if (decodes(v, 'video/webm; codecs="vp9"', 'video/mp4; codecs="vp09.00.10.08"')) video.push("vp9");
  if (decodes(v, 'video/mp4; codecs="av01.0.08M.08"', 'video/webm; codecs="av01.0.08M.08"')) video.push("av1");
  if (probe(v, 'video/webm; codecs="vp8"')) video.push("vp8");

  // HDR needs a 10-bit decoder for an HDR-capable codec, and an HDR display
  // or desktop Chromium, which tone-maps HDR10 and HLG video for SDR displays
  // itself (closer to the original than the transcoder's tone mapping).
  const tenBit = decodes(
    v,
    'video/mp4; codecs="hvc1.2.4.L153.B0"', // HEVC Main 10
    'video/mp4; codecs="vp09.02.51.10.01.09.16.09.00"', // VP9 profile 2, BT.2020 PQ
    'video/mp4; codecs="av01.0.13M.10"', // AV1 Main 10-bit
  );
  const hdrDisplay = typeof window.matchMedia === "function" && window.matchMedia("(dynamic-range: high)").matches;
  const dolbyVision = decodes(v, 'video/mp4; codecs="dvh1.05.06"', 'video/mp4; codecs="dvhe.05.06"');

  videoCache = {
    containers,
    video_codecs: video,
    audio_codecs: audioCodecs(v),
    hdr: tenBit && (hdrDisplay || (isChromium() && !/\bAndroid\b/.test(navigator.userAgent))),
    dolby_vision: dolbyVision,
    // hls.js, and a browser's own HLS, move between a transcode's sizes as the connection allows.
    hls: { variants: true },
    // Text subtitles come as WebVTT from the server; pictures (PGS, VobSub,
    // DVB) from their own `.mks`, drawn over the video (player/pictures.ts),
    // whatever the stream. These two say so without `caps_version: 2`,
    // which would switch every other rule of the server's to the Android
    // app's: they change none of its decisions, as the player asks for no
    // `subtitle_index` for the pictures it draws (docs/API.md).
    subtitle_codecs: [...TEXT_SUBTITLES, ...PICTURE_SUBTITLES],
    picture_subtitles_sideload: true,
  };
  return videoCache;
}

/**
 * What plays of a channel's own stream: each video codec up to a bit depth,
 * and the sound. Codec names are ffprobe's.
 */
export interface StreamCaps {
  video: { codec: string; max_bit_depth: number }[];
  audio_codecs: string[];
}

/**
 * The video mpegts.js reads from MPEG-TS: what it writes each as once read,
 * and how MSE is asked for it at 8 and at 10 bits (H.264 High and High 10,
 * HEVC Main and Main 10), as a decoder for one doesn't say the other is
 * there. MPEG-2 video it doesn't read, nor does a browser play it; AV1 it
 * reads only as a draft carries it, which FFmpeg, and so the server's plan,
 * doesn't name.
 */
const STREAM_VIDEO: [codec: string, written: string, eight: string, ten: string][] = [
  ["h264", "avc1", "avc1.640028", "avc1.6E0028"],
  ["hevc", "hvc1", "hvc1.1.6.L120.90", "hvc1.2.4.L153.B0"],
];

const mseVideo = (codec: string) => mse(`video/mp4; codecs="${codec}"`);
const mseAac = () => mse('audio/mp4; codecs="mp4a.40.2"');
/** MPEG audio goes to MSE as it is, but in MP4 on Firefox (as mpegts.js chooses). */
const mseMpegAudio = () => mse(/firefox/i.test(navigator.userAgent) ? 'audio/mp4; codecs="mp3"' : "audio/mpeg");

/**
 * The sound mpegts.js reads from MPEG-TS: what it writes each as once read
 * (AAC in ADTS and in LATM alike, MP2 as MP3), and whether this browser
 * plays it. No type asks for MP2: Chromium's MPEG audio decoder plays it,
 * unasked as it plays Matroska. E-AC-3, which mpegts.js reads too, is left
 * out: mpegts.js 1.8 doesn't count it as the stream's sound, so the picture
 * starts without it, and a browser takes no track added once one plays.
 */
const STREAM_AUDIO: [codec: string, written: string, plays: () => boolean][] = [
  ["aac", "mp4a", mseAac],
  ["aac_latm", "mp4a", mseAac],
  ["ac3", "ac-3", () => mse('audio/mp4; codecs="ac-3"')],
  ["mp3", "mp3", mseMpegAudio],
  ["mp2", "mp3", () => isChromium() && mseMpegAudio()],
  ["opus", "opus", () => mse('audio/mp4; codecs="opus"')],
];

/**
 * What this browser plays of a live channel's MPEG-TS. mpegts.js reads it
 * and hands it to Media Source Extensions, so MSE is what's asked, not the
 * element as for a file.
 */
export function streamCaps(): StreamCaps {
  return {
    video: STREAM_VIDEO.filter(([, , eight]) => mseVideo(eight)).map(([codec, , , ten]) => ({
      codec,
      max_bit_depth: mseVideo(ten) ? 10 : 8,
    })),
    audio_codecs: STREAM_AUDIO.filter(([, , plays]) => plays()).map(([codec]) => codec),
  };
}

/**
 * Whether this browser plays a track mpegts.js read from a stream, by what
 * it wrote it as. Video by its whole codec ("hvc1.2.1.L153.B0"): the
 * stream's own profile and level, which the server's plan doesn't give.
 * Sound by its name: `said` is the server's for it ("mp2"), which tells
 * apart what mpegts.js writes alike; unknown, each it may be has to play.
 */
export function playsStreamTrack(kind: "video" | "audio", written: string, said?: string | null): boolean {
  if (kind === "video") return STREAM_VIDEO.some(([, prefix]) => written.startsWith(prefix)) && mseVideo(written);
  const may = STREAM_AUDIO.filter(([, prefix]) => written.startsWith(prefix));
  const is = may.filter(([codec]) => codec === said);
  return may.length > 0 && (is.length ? is : may).every(([, , plays]) => plays());
}

/** What this browser can direct-play in an `<audio>` element. */
export function audioCaps(): ClientCaps {
  if (audioCache) return audioCache;
  const a = document.createElement("audio");
  const containers: string[] = [];
  if (probe(a, "audio/mpeg")) containers.push("mp3");
  if (probe(a, "audio/mp4")) containers.push("m4a", "mp4", "m4b");
  if (probe(a, "audio/flac")) containers.push("flac");
  if (probe(a, "audio/ogg")) containers.push("ogg", "oga", "opus");
  if (probe(a, "audio/wav")) containers.push("wav");
  if (probe(a, "audio/webm")) containers.push("webm");
  audioCache = { containers, video_codecs: [], audio_codecs: audioCodecs(a) };
  return audioCache;
}

export function canPlayNativeHls(el: HTMLMediaElement): boolean {
  return probe(el, "application/vnd.apple.mpegurl");
}
