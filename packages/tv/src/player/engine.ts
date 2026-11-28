import type { DynamicRange } from "../api/types";
import type { EngineError } from "./recovery";

/**
 * The TV's own player, as the session drives it: Samsung's AVPlay, which
 * apps/tizen implements it with. Everything above it (the server session,
 * recovery, subtitles, Up Next) is in this package.
 */

export type EngineStatus = "idle" | "buffering" | "ready" | "ended";

/** The MIME type HLS goes by on TVs. */
export const HLS_MIME = "application/vnd.apple.mpegurl";

/** What to play, and what is known of it before it opens. */
export interface EngineSource {
  /** Absolute; carries the session's key when the player can't send headers. */
  url: string;
  hls: boolean;
  /** The container's MIME type for a file played as it is. */
  mime: string | null;
  startMs: number;
  durationHintMs: number | null;
  video: {
    codec: string;
    width: number | null;
    height: number | null;
    frameRate: number | null;
    range: DynamicRange;
    dvProfile: number | null;
    bitDepth: number;
  } | null;
  /** The audio stream's codec, when played as it is (passthrough or decode). */
  audioCodec: string | null;
  /** A live channel's HLS: it starts at the live edge, whatever `startMs` says. */
  live?: boolean;
  /**
   * The audio played is the one the file starts with (its default track,
   * or none): a player that can't be told which track to play can take it.
   */
  audioDefault?: boolean;
}

/** An audio track as the engine knows it, in the file's order. */
export interface EngineTrack {
  language: string | null;
  codec: string | null;
  channels: number | null;
}

/** What the engine can say about what it plays, for the details panel. Every field is optional. */
export interface EngineStats {
  decoder?: string;
  hardware?: boolean;
  width?: number;
  height?: number;
  frameRate?: number;
  bitDepth?: number;
  codec?: string;
  codecString?: string;
  /** "HDR10", "Dolby Vision", "HLG", "SDR"… as the engine reports it. */
  hdr?: string;
  /** Bits per second of the stream as it arrives. */
  bitrate?: number;
  bandwidth?: number;
  audioCodec?: string;
  audioChannels?: number;
  /** "Passed through as E-AC-3", "Decoded to PCM"… */
  audioOutput?: string;
  dropped?: number;
  frames?: number;
  bufferedAheadMs?: number;
  /** Lines the engine adds as it is ("Label: value"), when it has more to say. */
  extra?: string[];
}

export interface EngineListener {
  status(status: EngineStatus): void;
  /** The first picture of a source shows. */
  firstFrame(): void;
  error(error: EngineError): void;
  tracks(): void;
  /**
   * A seek to `targetMs` (or the start at `source.startMs`) didn't get
   * there: the player refused it, or said it was made and plays on from
   * elsewhere. It plays on where it is; the session decides what next.
   * `slow`: it was still loading when the player gave up on it, which a
   * slow network does too: not a refusal to remember for the kind of file.
   */
  seekFailed?(targetMs: number, slow?: boolean): void;
  /**
   * The engine started the source again on another of the TV's players,
   * before a first picture (Samsung's <video> couldn't open the file, so
   * AVPlay does): the time waited so far isn't the new player's.
   */
  restarted?(): void;
}

export interface PlayerEngine {
  /** "AVPlay". */
  readonly name: string;
  setListener(listener: EngineListener): void;
  /** Opens `source` and starts at `source.startMs`; plays at once when `play` is true. */
  prepare(source: EngineSource, play: boolean): void;
  /**
   * The next `prepare` moves to a stream made ready beside this one (a
   * quality or track change): the picture showing now stays on screen
   * until the new source shows its own, where the engine can keep it.
   * AVPlay can't (it draws on a plane the page can't read), so it has none.
   */
  holdPicture?(): void;
  play(): void;
  pause(): void;
  seekTo(ms: number): void;
  /** Stops and closes the source; the engine can prepare another. */
  stop(): void;
  /** Gone for good (the player screen closes). */
  release(): void;
  positionMs(): number;
  /** 0 while unknown. */
  durationMs(): number;
  bufferedMs(): number;
  /** False when `bufferedMs` only repeats the position (AVPlay doesn't say how far it has loaded). */
  readonly knowsBuffer?: boolean;
  /**
   * Where the picture goes, in the 960×540 canvas's px (the picture kept
   * whole inside it), or the whole screen (`null`). An engine on a plane of
   * its own (AVPlay) is told; one in the page follows its host's size.
   */
  setArea?(area: { left: number; top: number; width: number; height: number } | null): void;
  /** How fast the network brings the stream, kbps, when the engine measures it; `null` when it can't say. */
  bandwidthKbps?(): number | null;
  /**
   * Plays at `rate` times normal speed, with its sound, from now and for
   * the sources it opens after; false when it can't. An engine without it
   * plays at normal speed only.
   */
  setSpeed?(rate: number): boolean;

  /**
   * A seek it is still making, in a stream the server makes: the time it
   * waits is the server's, not a stall (bounded by the session). Already
   * true when it says "buffering" for the seek.
   */
  seekPending?(): boolean;
  /**
   * It plays a stream started part-way from the stream's first segment,
   * and seeks to the place once it plays (Samsung's AVPlay): the session
   * has the server start making it there too, rather than at the place,
   * where the server would stop for the first segment and start again.
   */
  readonly hlsFromFirstSegment?: boolean;
  /**
   * Which of the TV's players plays now, where the engine has more than one
   * (Samsung's "avplay" and "video"); what each refuses is remembered apart.
   * `null` or absent: the engine is one player.
   */
  activePlayer?(): string | null;
  /**
   * A seek in a file played as it is that the player playing refused: the
   * same address plays on the TV's other player at `targetMs` (Samsung's
   * <video>, which fetches from there by HTTP range), with the session
   * left as it is. False when no other player takes the file.
   */
  switchForSeek?(targetMs: number): boolean;
  status(): EngineStatus;
  playing(): boolean;
  audioTracks(): EngineTrack[];
  /**
   * Switches to the audio track at `ordinal` among the file's audio
   * streams, checked against `language`; false when it can't.
   */
  selectAudio(ordinal: number, language: string | null): boolean;
  /** Dropped and total frames so far, when the engine counts them. */
  frameCounts(): { dropped: number; total: number } | null;
  stats(): EngineStats | null;
}
