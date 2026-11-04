import type { ClientCaps, ItemDetail, MediaFile, MediaStream, PlaybackInfo, PlaybackMode, PlaybackRequest } from "../api/types";
import { bestFile, decide, rangeOf, videoOf, type CapsDecision } from "../caps/policy";
import type { QualityPreset } from "./quality";

/**
 * How a playback is asked for: which file, from where, with which tracks,
 * and what goes in the body of POST /api/playback (the Android app's
 * PlaybackPlan). Track indexes go to the server only when it has to act on
 * them: `audio_index` for a track picked, else the server chooses the one
 * this TV plays best, again for each session (a stream has the one it
 * copies); `subtitle_index` only to burn pictures into HLS, and never when
 * the TV draws them from the server's .mks.
 */

/** What starts a playback. */
export interface PlayRequest {
  itemId: number;
  fileId?: number | null;
  startMs?: number | null;
  fromStart?: boolean;
  audioIndex?: number | null;
  subtitleIndex?: number | null;
  queue?: { kind: "playlist" | "collection"; id: number } | null;
}

export function audioStreams(file: MediaFile | null): MediaStream[] {
  return (file?.streams ?? []).filter((s) => s.kind === "audio");
}

export function subtitleStreams(file: MediaFile | null): MediaStream[] {
  return (file?.streams ?? []).filter((s) => s.kind === "subtitle");
}

/** The file to play: the one asked for, else the one that plays best here, else the first. */
export function chooseFile(item: ItemDetail, requested: number | null | undefined, caps: ClientCaps | null): MediaFile | null {
  if (requested != null) {
    const asked = item.files.filter((f) => f.id === requested)[0];
    if (asked) return asked;
  }
  const best = caps ? bestFile(item, caps) : null;
  return best ?? item.files[0] ?? null;
}

/** Where to start: as asked, else from the start when asked or watched, else the resume point. */
export function startMs(item: ItemDetail, req: PlayRequest): number {
  if (req.startMs != null) return Math.max(0, req.startMs);
  if (req.fromStart || item.user_state.watched) return 0;
  return Math.max(0, item.user_state.position_ms);
}

/**
 * The audio to start with: the track asked for; else none, and the server
 * chooses, of the profile's language, the track this TV plays best (as it
 * is rather than converted, its receiver's channels, lossless, Atmos
 * passed through), which the session names (`playingAudio`).
 */
export function initialAudio(file: MediaFile | null, requested: number | null | undefined): MediaStream | null {
  if (requested == null) return null;
  return audioStreams(file).filter((s) => s.index === requested)[0] ?? null;
}

/** The audio a session plays: the server's `audio_index`, else the file's default, else the first. */
export function playingAudio(file: MediaFile | null, info: PlaybackInfo): MediaStream | null {
  const audio = audioStreams(file);
  return audio.filter((s) => s.index === info.audio_index)[0] ?? audio.filter((s) => s.default)[0] ?? audio[0] ?? null;
}

/** The subtitles to start with: as asked, else the profile's choice; `null` is off, whatever the file flags. */
export function initialSubtitle(file: MediaFile | null, requested: number | null | undefined): MediaStream | null {
  const subs = subtitleStreams(file);
  return (requested != null ? subs.filter((s) => s.index === requested)[0] : undefined) ?? subs.filter((s) => s.index === file?.selected_subtitle)[0] ?? null;
}

/**
 * `item` as the server answers once `audio` and `subtitle` (`null`: off)
 * were picked for its file `fileId`. An audio of `null` is Automatic (no
 * track picked); left out (`undefined`), the audio stays as it was.
 */
export function withTracks(item: ItemDetail, fileId: number, audio: number | null | undefined, subtitle: number | null): ItemDetail {
  const tracks = (f: MediaFile): MediaFile =>
    audio === undefined ? { ...f, selected_subtitle: subtitle } : { ...f, selected_audio: audio, audio_picked: audio !== null, selected_subtitle: subtitle };
  return { ...item, files: item.files.map((f) => (f.id === fileId ? tracks(f) : f)) };
}

/** Subtitles shown as pictures (PGS, VobSub, DVB): a stream burns them in unless the TV draws them. */
export function isPicture(s: MediaStream | null | undefined): boolean {
  return !!s && s.text_based === false;
}

export function picturePath(fileId: number, index: number): string {
  return `/api/files/${fileId}/subtitles/${index}.mks`;
}

/**
 * `url` asked for from where the player is (`atMs` of the file): a track the server hasn't copied out of its file yet
 * comes from there first. At the start too: a copy under way from elsewhere answers with the pictures there to a
 * player that doesn't say.
 */
export function pictureUrlAt(url: string, atMs: number): string {
  const at = Math.max(0, Math.round(atMs)) || 0;
  return `${url}${url.indexOf("?") >= 0 ? "&" : "?"}at=${at}`;
}

/**
 * Whether the TV takes `stream`'s sample rate: no higher than the report's
 * `audio_max_sample_rate`, as the server judges it. A rate or a limit not
 * known is taken.
 */
export function ratePlaysHere(stream: MediaStream | null, caps: ClientCaps | null): boolean {
  const most = caps?.audio_max_sample_rate;
  return stream?.sample_rate == null || most == null || stream.sample_rate <= most;
}

/** Whether this TV decodes `stream`'s audio or sends it as it is to the receiver. */
export function audioPlaysHere(stream: MediaStream | null, caps: ClientCaps | null): boolean {
  if (!stream || !caps) return true;
  if (!ratePlaysHere(stream, caps)) return false;
  const codec = stream.codec.toLowerCase();
  return caps.audio_codecs.concat(caps.audio_decode, caps.audio_passthrough).some((c) => c.toLowerCase() === codec);
}

/** Whether this TV opens `file`'s container as it is. */
export function containerPlaysHere(file: MediaFile | null, caps: ClientCaps | null): boolean {
  const container = (file?.container ?? "").toLowerCase();
  if (!container || !caps) return true;
  const list = caps.containers.map((c) => c.toLowerCase());
  return list.indexOf(container) >= 0 || (container === "mkv" && list.indexOf("matroska") >= 0) || (container === "matroska" && list.indexOf("mkv") >= 0);
}

/** Whether the session will surely stream (HLS), before asking. */
export function willStream(mode: PlaybackMode, decision: CapsDecision | null, file: MediaFile | null, audio: MediaStream | null, caps: ClientCaps | null): boolean {
  return (
    mode === "remux" || mode === "transcode" || (decision !== null && decision.kind !== "direct") || !containerPlaysHere(file, caps) || !audioPlaysHere(audio, caps)
  );
}

export interface Choices {
  file: MediaFile;
  startMs: number;
  audio: MediaStream | null;
  subtitle: MediaStream | null;
  quality: QualityPreset;
  mode: PlaybackMode;
  /** The session will stream (HLS): the server needs the track indexes. */
  streaming: boolean;
  /** Pictures are drawn by the TV from the server's .mks: none to burn in. */
  sideload: boolean;
}

/** The body of POST /api/playback for `choices`. */
export function playbackRequest(itemId: number, c: Choices, clientId: string, caps: ClientCaps | null, keyed: boolean): PlaybackRequest {
  const req: PlaybackRequest = {
    item_id: itemId,
    file_id: c.file.id,
    start_ms: Math.round(c.startMs),
    mode: c.mode,
    client_id: clientId,
    caps: caps ?? emptyCaps(),
  };
  if (c.audio) req.audio_index = c.audio.index;
  if (c.subtitle && c.streaming && isPicture(c.subtitle) && !c.sideload) req.subtitle_index = c.subtitle.index;
  if (c.quality.maxBitrateKbps != null) req.max_bitrate = c.quality.maxBitrateKbps;
  if (c.quality.maxHeight != null) req.max_height = c.quality.maxHeight;
  if (keyed) req.keyed_urls = true;
  return req;
}

/** A report for when the TV couldn't be probed: the version 1 fields, empty. */
export function emptyCaps(): ClientCaps {
  return {
    containers: [],
    video_codecs: [],
    audio_codecs: [],
    hdr: false,
    dolby_vision: false,
  } as unknown as ClientCaps;
}

/**
 * Whether `info`'s session must be asked for again with the subtitle
 * index: it streams, though it wasn't expected to, and the subtitles
 * chosen are pictures to burn in.
 */
export function needsIndexes(info: PlaybackInfo, sent: PlaybackRequest, c: Choices): boolean {
  return info.hls && isPicture(c.subtitle) && sent.subtitle_index == null && !c.sideload;
}

/** The container's MIME type, for players that can't tell from the address. */
export function mimeTypeOf(container: string | null | undefined): string | null {
  switch ((container ?? "").toLowerCase()) {
    case "mkv":
    case "matroska":
    case "mka":
      return "video/x-matroska";
    case "mp4":
    case "m4v":
    case "mov":
      return "video/mp4";
    case "webm":
      return "video/webm";
    case "avi":
      return "video/x-msvideo";
    case "ts":
    case "m2ts":
    case "mpegts":
      return "video/mp2t";
    default:
      return null;
  }
}

/**
 * The Dolby Vision guards (the Android app's DvGuards): a session that
 * would show wrong colours is asked for again another way. A remux of
 * profile 5 loses its signalling in MPEG-TS on a TV that doesn't read the
 * master playlist's Dolby Vision codecs: transcode it. Direct play of
 * profile 7 on a TV that doesn't decode it: its HDR10 base layer.
 */
export function dvRetryMode(info: PlaybackInfo, file: MediaFile | null, caps: ClientCaps | null): PlaybackMode | null {
  const video = videoOf(file);
  if (!video || rangeOf(video) !== "dolby_vision") return null;
  if (video.dv_profile === 5) {
    return info.method === "remux" && !(caps?.hls.master_playlist && caps.hdr_types.indexOf("dolby_vision") >= 0) ? "transcode" : null;
  }
  if (video.dv_profile === 7) {
    const decodesP7 = (caps?.dolby_vision_profiles ?? []).some((p) => p.profile === 7);
    if (info.method !== "direct" || decodesP7) return null;
    return (caps?.hdr_types ?? []).indexOf("hdr10") >= 0 || caps?.hdr ? "remux" : "transcode";
  }
  return null;
}

/**
 * A Dolby Vision file in a container the TV shows Dolby Vision only from
 * others (LG before webOS 25: not Matroska): played as it is, it would
 * show its base layer, so it is asked for as a remux, which keeps it.
 */
export function dvContainerMode(file: MediaFile | null, caps: ClientCaps | null, dvContainers: string[] | null | undefined): PlaybackMode | null {
  if (!file || !caps || !dvContainers) return null;
  const video = videoOf(file);
  if (!video || rangeOf(video) !== "dolby_vision" || caps.hdr_types.indexOf("dolby_vision") < 0) return null;
  if (!caps.dolby_vision_profiles.some((p) => p.profile === video.dv_profile)) return null;
  const container = (file.container ?? "").toLowerCase().replace("matroska", "mkv").split(",")[0]!;
  return container && dvContainers.indexOf(container) < 0 ? "remux" : null;
}

export { decide };
