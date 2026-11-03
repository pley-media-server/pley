import { batch, signal, type Signal } from "@preact/signals";
import { ApiError, errorMessage, isApiError, SessionStoppedError, unreachable, type Api } from "../api/client";
import type {
  ClientCaps,
  ItemDetail,
  ItemKind,
  ItemSummary,
  Marker,
  MediaFile,
  MediaStream,
  PlaybackInfo,
  PlaybackMode,
  PlaybackRequest,
  ServerEvent,
  SubtitleResult,
  SubtitleTrack,
  TranscodeInfo,
  Trickplay,
} from "../api/types";
import { SignedOutError } from "../auth/tokens";
import { Cancel } from "../net/transport";
import { episodeLabel, streamLabel, twoLetterCode } from "../format/format";
import { decide } from "../caps/policy";
import { NOTHING_SHOWN, SubtitlePresenter, type ShownSubtitles } from "../subs/presenter";
import { clampOffset, type OffsetStore } from "../subs/sync";
import { buildDetails, fallbackText, methodText, stoppedText, T, type PlaybackDetails } from "./details";
import type { EngineSource, EngineStatus, PlayerEngine } from "./engine";
import { activeMarker, CreditsGate, markersOf, skipLabel, skipTarget } from "./markers";
import { hasNext, labelOf, nextAfter, startOf, TRICKPLAY_GIVE_UP_MS, TRICKPLAY_RETRY_MS } from "./nextup";
import {
  audioPlaysHere,
  audioStreams,
  chooseFile,
  dvContainerMode,
  dvRetryMode,
  initialAudio,
  initialSubtitle,
  isPicture,
  mimeTypeOf,
  needsIndexes,
  picturePath,
  playbackRequest,
  playingAudio,
  startMs as startOfPlay,
  subtitleStreams,
  willStream,
  type Choices,
  type PlayRequest,
} from "./plan";
import { AutoQuality, FIRST_MINUTE_MS, type AutoMemory, type AutoStep, type LiveStream, type Remembered } from "./autoquality";
import type { NetworkMemory } from "./network";
import { PREPARE_LEAD_MS, warmStream, type StreamFetch } from "./prepare";
import { AUTO, autoLadder, chooseQuality, fileHint, liveFloor, liveLabel, modeOf, namedFor, ORIGINAL, presetById, PRESETS, qualitiesFor, streamLines, type QualityPreset } from "./quality";
import { failureText, Recovery, StruggleMeter, type EngineError, type RecoveryAction } from "./recovery";
import type { Reporter } from "./reporter";
import { SLEEP_OFF, SleepTimer, type SleepChoice, type SleepState } from "./sleep";
import { SPEEDS } from "./speed";

export type Phase =
  | { kind: "preparing" }
  | { kind: "buffering" }
  | { kind: "playing" }
  | { kind: "paused" }
  /** The end, with nothing next: the player leaves. */
  | { kind: "ended" }
  /** `title` replaces "Can't play this item" ("Can't reach your server"). */
  | { kind: "error"; message: string; canRetry: boolean; title?: string };

/** An audio or subtitle track as the menus show it. */
export interface TrackChoice {
  /** The server's stream index; `null` for "Off". */
  index: number | null;
  label: string;
  selected: boolean;
  /** Pictures a stream draws onto the video ("burn-in"). */
  burnIn: boolean;
  /** Subtitles added on the server (found, or a file uploaded on the web). */
  added?: boolean;
}

export interface QualityChoice {
  id: string;
  label: string;
  hint: string | null;
  selected: boolean;
}

export interface ChapterMark {
  title: string | null;
  startMs: number;
  endMs: number;
  imageUrl: string | null;
}

export interface UpNextCard {
  itemId: number;
  title: string;
  heading: string;
  imageUrl: string | null;
  /** Seconds before it plays by itself; `null` when it won't. */
  secondsLeft: number | null;
}

/** What the player screen shows, but the clock and the subtitles, which change often and have their own signals. */
export interface PlayerState {
  phase: Phase;
  itemId: number;
  /** What plays: a movie, an episode, an extra; `null` until its details come. */
  kind: ItemKind | null;
  /** A movie's "Title (Year)", an episode's show. */
  title: string;
  /** An episode's "S1 · E2 · Title", as it reads everywhere. */
  subtitle: string | null;
  audio: TrackChoice[];
  subtitles: TrackChoice[];
  quality: string;
  qualities: QualityChoice[];
  method: string | null;
  note: string | null;
  canPlayOriginal: boolean;
  suggestLowerQuality: boolean;
  skipLabel: string | null;
  markers: Marker[];
  chapters: ChapterMark[];
  trickplay: { fileId: number; info: Trickplay } | null;
  upNext: UpNextCard | null;
  details: PlaybackDetails | null;
  notice: string | null;
  subtitleOffsetMs: number;
  subtitleSync: boolean;
  /** How fast it plays (1: normal), for this title; `speeds`: what the TV's player plays at, empty when only normal. */
  speed: number;
  speeds: readonly number[];
  sleep: SleepState;
  stoppedByServer: boolean;
  /** The server stopped answering mid-playback: the player keeps its place and asks again ("Reconnecting…"). */
  reconnecting: boolean;
  /** Time to the first frame of the session playing, in ms; `null` before it shows. */
  firstFrameMs: number | null;
  /** Times the video stopped to wait for more once it played, seeks aside (Info's Stalls). */
  stalls: number;
  /**
   * It moves to a session made ready beside the last one (a quality, track
   * or Automatic change): its first picture is moments away, so the
   * buffering ring waits longer before it shows.
   */
  switching: boolean;
}

export interface PlayerClock {
  positionMs: number;
  durationMs: number;
  bufferedMs: number;
}

const INITIAL: PlayerState = {
  phase: { kind: "preparing" },
  itemId: 0,
  kind: null,
  title: "",
  subtitle: null,
  audio: [],
  subtitles: [],
  quality: "Original",
  qualities: [],
  method: null,
  note: null,
  canPlayOriginal: false,
  suggestLowerQuality: false,
  skipLabel: null,
  markers: [],
  chapters: [],
  trickplay: null,
  upNext: null,
  details: null,
  notice: null,
  subtitleOffsetMs: 0,
  subtitleSync: false,
  speed: 1,
  speeds: [],
  sleep: SLEEP_OFF,
  stoppedByServer: false,
  reconnecting: false,
  firstFrameMs: null,
  stalls: 0,
  switching: false,
};

/** What a player needs from the app around it. */
export interface PlayerEnv {
  api: Api;
  reporter: Reporter;
  engine: () => PlayerEngine;
  /** The TV's capabilities report; `null` when the probe failed. */
  caps: () => Promise<ClientCaps | null>;
  /** The TV's player can't send headers: its URLs carry a key (docs/API.md, "Keyed URLs"). */
  keyedUrls: boolean;
  /** Containers the TV shows Dolby Vision from, when it says; others are asked for as a remux. */
  dvContainers?: () => string[] | null;
  clientId: string;
  offsets: OffsetStore;
  quality: { remembered(): string | null; remember(id: string): void };
  /** Where Automatic keeps a slow network for the next title (per TV and server). */
  autoMemory?: AutoMemory | null;
  /** What the server knows of this TV's network, asked before Automatic starts and told of its ceilings. */
  network?: NetworkMemory | null;
  autoplayNext: () => boolean;
  /** Subscribes to the server's events; returns the unsubscribe. */
  events: (listener: (e: ServerEvent) => void) => () => void;
  /** Item details the app has already (tap time), to start without waiting. */
  cachedItem?: (id: number) => ItemDetail | null;
  now?: () => number;
}

/** Where a closed or hidden player was, to start again from there. */
export interface SavedPlayback {
  request: PlayRequest;
  quality: string;
  playing: boolean;
}

export const TICK_MS = 500;
/** Buffering this soon after a seek is the seek's, not a stall. */
const SEEK_SETTLE_MS = 2_000;
export const PROGRESS_MS = 10_000;
/** While paused: the server forgets players it hasn't heard from for 90 s. */
export const PAUSED_HEARTBEAT_MS = 30_000;
export const SEEK_REPORT_MS = 1_000;
export const STALL_MS = 12_000;
export const SHORT_END_MS = 60_000;
export const SHORT_PLAY_MS = 5_000;
export const STARVING_MS = 1_000;
export const STRUGGLE_SAMPLE_MS = 2_000;
/**
 * Frames dropped while the TV's player switches a file's audio track (a
 * desktop Chrome drops about 130) aren't the video struggling: for this
 * long after a switch they aren't counted, then counting starts afresh.
 */
export const TRACK_SWITCH_QUIET_MS = 5_000;
export const DETAILS_MS = 2_000;
export const COUNTDOWN_SECONDS = 10;
/** The longest a new session waits for the one before to be ended. */
export const END_WAIT_MS = 2_000;
/** Seeks stop this short of the end, so the end isn't reached by a seek. */
export const SEEK_END_MARGIN_MS = 250;
export const UP_NEXT_IMAGE_WIDTH = 480;
/** The server out of reach mid-playback is asked again this long, as long as it waits for a player it doesn't hear from. */
export const OUTAGE_MS = 90_000;
/** Between its questions: 0.5 s, 1 s, then every 2 s. */
export const OUTAGE_STEPS_MS = [500, 1_000, 2_000];
/** How long each question waits for an answer. */
export const OUTAGE_ASK_MS = 5_000;
/** The engine moving this far past where the server went means it came back by itself. */
export const OUTAGE_MOVED_MS = 1_000;

/** The server stopped answering: since when, how often it was asked, and whether the engine gave up too. */
interface Outage {
  since: number;
  attempts: number;
  timer: ReturnType<typeof setTimeout> | null;
  engineFailed: boolean;
  atMs: number;
}

/**
 * The playback of one video, then of what plays after it, on one engine
 * (the Android app's SessionPlaybackController):
 *
 * - **Start.** The item's details and the TV's capabilities come together,
 *   then POST /api/playback, the Dolby Vision guards, and the engine loads
 *   the answer, at once at the resume point.
 * - **Reports.** Progress every 10 s while playing and every 30 s paused,
 *   on pause, and 1 s after the last seek; `stopped` then DELETE at the
 *   end, on closing and before any new session.
 * - **Recovery.** The ladder for errors, 12 s stalls and dropped frames.
 * - **Tracks.** Audio switched by the engine in direct play, a new session
 *   for streams; subtitles always drawn by the TV (the server's WebVTT, or
 *   its .mks of a picture track), burned in only when the server can't
 *   make the .mks. Choices are saved with PUT /api/items/{id}/streams.
 * - **Markers, Up Next, the sleep timer**, and a session the server
 *   stopped: it says so and plays no more.
 */
export class PlaybackController {
  readonly state: Signal<PlayerState> = signal(INITIAL);
  readonly clock: Signal<PlayerClock> = signal({ positionMs: 0, durationMs: 0, bufferedMs: 0 });
  readonly subs: Signal<ShownSubtitles> = signal(NOTHING_SHOWN);

  private readonly env: PlayerEnv;
  private readonly now: () => number;
  private engineRef: PlayerEngine | null = null;

  private current: PlayRequest;
  private item: ItemDetail | null = null;
  private caps: ClientCaps | null = null;
  private file: MediaFile | null = null;
  private audio: MediaStream | null = null;
  /** The audio was picked (asked for, or in the menu), not chosen for this TV by the server: it is saved for next time. */
  private audioPicked = false;
  private subtitle: MediaStream | null = null;
  /** The quality chosen in the menu: Automatic, the original, or one of the conversions. */
  private quality: QualityPreset = ORIGINAL;
  /** Automatic, when chosen: where it plays now, and when it changes. */
  private auto: AutoQuality | null = null;
  private markers: Marker[] = [];
  /** The mode a Dolby Vision guard asked for, for this item's later sessions too. */
  private guardMode: PlaybackMode | null = null;

  private info: PlaybackInfo | null = null;
  private sessionEnded = true;
  /** Bumped by every new start: what an older one finds when it resumes is no longer its own. */
  private generation = 0;
  /** A session being made ready beside the one playing (`begin`'s `prepared`): cancelled when a newer start takes over. */
  private preparing: Cancel | null = null;

  private lastPositionMs = 0;
  private startedAtMs = 0;
  private mediaShown = false;
  /** When a seek of the player's own was asked (its buffering is no stall). */
  private seekedAt = -1e9;
  private wantPlay = true;
  private ended = false;
  private closed = false;
  /** The server stopped the playback: nothing plays again. */
  private stopped = false;
  /** The server has no .mks of picture subtitles: they are burned in. */
  private picturesUnavailable = false;
  private offset = 0;
  /** Something has played: a lost server is waited for from now on, rather than failing at once. */
  private shownOnce = false;
  private outage: Outage | null = null;

  private readonly recovery = new Recovery();
  private readonly struggle = new StruggleMeter();
  private readonly credits = new CreditsGate();
  private readonly presenter: SubtitlePresenter;
  private readonly sleep: SleepTimer;

  private lastAdvanceAt = 0;
  private lastAdvancePos = -1;
  private lastStruggleSample = 0;
  /** Dropped frames aren't counted before this (an audio track switched in place). */
  private framesQuietUntil = 0;
  private sessionStartedAt = 0;
  /** How long the title has played (its picture moving), for Automatic's first minute; and the tick that last saw it move. */
  private titlePlayedMs = 0;
  private movingAt: number | null = null;
  private live: TranscodeInfo | null = null;

  private tickTimer: ReturnType<typeof setInterval> | null = null;
  private progressTimer: ReturnType<typeof setTimeout> | null = null;
  private seekReportTimer: ReturnType<typeof setTimeout> | null = null;
  private countdownTimer: ReturnType<typeof setTimeout> | null = null;
  private detailsTimer: ReturnType<typeof setTimeout> | null = null;
  private trickplayTimer: ReturnType<typeof setTimeout> | null = null;
  private unsubscribe: (() => void) | null = null;

  private next: ItemSummary | null = null;
  private nextDetail: ItemDetail | null = null;
  /** Track choices saved so far (saveStreams). */
  private choicesSaved = 0;
  private nextLookup = 0;
  /** When the screen asked to play: the first frame is timed from it. */
  private pressedAt: number;

  constructor(request: PlayRequest, env: PlayerEnv, pressedAt?: number) {
    this.current = request;
    this.env = env;
    this.now = env.now ?? (() => (typeof performance !== "undefined" ? performance.now() : Date.now()));
    this.pressedAt = pressedAt ?? this.now();
    this.presenter = new SubtitlePresenter(env.api, {
      position: () => this.livePosition(),
      offsetMs: () => this.offset,
      onShown: (shown) => (this.subs.value = shown),
      onFailed: () => this.notice(T.subtitlesFailed),
      onPicturesUnavailable: () => this.picturesFailed(),
    });
    this.sleep = new SleepTimer({
      itemLeftMs: () => this.itemLeftMs(),
      itemKey: () => this.item?.id ?? null,
      onFire: () => this.onSleep(),
      onChange: (s) => this.update({ sleep: s }),
    });
  }

  get engine(): PlayerEngine | null {
    return this.engineRef;
  }

  /**
   * Where the video is now, asked of the engine between the clock's ticks
   * (the controls' time, which changes on each whole second); the clock's
   * position while the engine has nothing to say.
   */
  livePositionMs(): number {
    const engine = this.engineRef;
    const ticked = this.clock.value.positionMs;
    if (!engine || !this.mediaShown || this.ended || this.closed) return ticked;
    const pos = engine.positionMs();
    return isFinite(pos) && pos >= 0 ? pos : ticked;
  }

  // ---------------------------------------------------------------- start

  start(): void {
    this.tickTimer = setInterval(() => this.tick(), TICK_MS);
    this.unsubscribe = this.env.events((e) => {
      if (e.type === "session_stopped" && this.info && e.session_id === this.info.session_id) this.stopByServer(e.message, e.by_admin);
    });
    void this.prepareFirst();
  }

  private async prepareFirst(): Promise<void> {
    const generation = this.nextGeneration();
    try {
      const cached = this.env.cachedItem?.(this.current.itemId) ?? null;
      const [detail, caps, known] = await Promise.all([
        cached ? Promise.resolve(cached) : this.env.api.item(this.current.itemId),
        this.env.caps().catch(() => null),
        this.networkStart(),
      ]);
      if (generation !== this.generation || this.closed) return;
      if (!this.engineRef) {
        this.engineRef = this.env.engine();
        this.engineRef.setListener({
          status: (s) => this.onStatus(s),
          firstFrame: () => this.onFirstFrame(),
          error: (e) => this.onError(e),
          tracks: () => undefined,
        });
      }
      this.update({ speeds: this.engineRef.setSpeed ? SPEEDS : [] });
      this.caps = caps;
      this.load(detail, this.current, known);
      await this.begin(startOfPlay(detail, this.current), generation);
    } catch (e) {
      if (generation === this.generation) this.failWith(e);
    }
  }

  /**
   * What the server knows of this network, for Automatic: asked (and a
   * speed test run, when it says) alongside the item's details, so the
   * press of Play waits for both at once; nothing asked for a quality picked.
   */
  private networkStart(): Promise<Remembered | null | undefined> {
    const network = this.env.network;
    if (!network || chooseQuality(this.env.quality.remembered(), PRESETS).id !== AUTO.id) return Promise.resolve(undefined);
    return network.start().catch(() => undefined);
  }

  /** Takes `detail` as the item playing, with `req`'s choices; `known`: where the server says Automatic may start. */
  private load(detail: ItemDetail, req: PlayRequest, known?: Remembered | null): void {
    this.item = detail;
    this.current = req;
    const chosen = chooseFile(detail, req.fileId, this.caps);
    this.file = chosen;
    this.audio = initialAudio(chosen, req.audioIndex);
    this.audioPicked = this.audio !== null;
    this.subtitle = initialSubtitle(chosen, req.subtitleIndex);
    this.quality = chooseQuality(this.env.quality.remembered(), qualitiesFor(chosen));
    this.titlePlayedMs = 0;
    this.movingAt = null;
    this.startAuto(known);
    this.offset = chosen ? this.env.offsets.get(this.env.api.server, chosen.id) : 0;
    this.markers = chosen ? markersOf(detail, chosen.id) : [];
    const episode = detail.kind === "episode";
    this.update({
      itemId: detail.id,
      kind: detail.kind,
      title: episode ? detail.grandparent_title || detail.title : detail.year != null ? `${detail.title} (${detail.year})` : detail.title,
      subtitle: episode ? [episodeLabel(detail.parent_index, detail.index), detail.title].filter(Boolean).join(" · ") : null,
      markers: this.markers,
      chapters: (chosen?.chapters ?? []).map((c) => ({
        title: c.title,
        startMs: c.start_ms,
        endMs: c.end_ms,
        imageUrl: c.thumb_url ? this.env.api.url(c.thumb_url) : null,
      })),
      trickplay: null,
      subtitleOffsetMs: this.offset,
    });
    this.setClock({ durationMs: chosen?.duration_ms ?? detail.duration_ms ?? 0 });
    this.publishChoices();
    this.loadTrickplay(chosen);
  }

  // ---------------------------------------------------------------- sessions

  /** A new start: what an older one finds when it resumes is no longer its own, and a session it was making ready is let go. */
  private nextGeneration(): number {
    this.preparing?.cancel();
    this.preparing = null;
    return ++this.generation;
  }

  /**
   * Starts a server session at `atMs`, ending the one before; an earlier
   * start still running loses. `prepared`, for a change asked for while it
   * plays (a quality, a track, an Automatic step): the new session is made
   * ready while this one plays on (`begin`), when one plays. A recovery
   * starts again at once.
   */
  private restart(atMs: number, prepared = false, away: Outage | null = null): void {
    if (this.stopped || this.closed) return;
    const alongside = prepared && this.canPrepare;
    const generation = this.nextGeneration();
    // Settled, however it went: the stream it plays is watched again.
    const settled = () => {
      if (generation === this.generation) this.preparing = null;
    };
    this.begin(atMs, generation, alongside).then(settled, (e) => {
      if (generation !== this.generation) return;
      settled();
      // Mid-playback, a server or transcoder that doesn't answer is waited for: 90 s in all, however
      // often the server answers in between (`away`, the wait this start ended).
      if (this.shownOnce && outOfReach(e) && !(away && this.now() - away.since >= OUTAGE_MS)) {
        this.lastPositionMs = alongside ? this.livePosition() : atMs;
        this.reachLost(true, away);
      } else this.failWith(e);
    });
  }

  /** A session plays, has shown its picture, and nothing is wrong: the next can be made ready beside it. */
  private get canPrepare(): boolean {
    return !!this.info && !this.sessionEnded && this.mediaShown && !this.ended && !this.outage && !!this.engineRef && this.state.value.phase.kind !== "error";
  }

  /** Playlists and segments as the TV's player asks for them, for `warmStream`. */
  private get streamFetch(): StreamFetch {
    const api = this.env.api;
    return {
      text: (url, cancel, timeoutMs) => api.media("GET", url, cancel, timeoutMs),
      head: (url, cancel, timeoutMs) => api.media("HEAD", url, cancel, timeoutMs).then(() => undefined),
    };
  }

  /**
   * Starts a server session at `atMs` and has the engine play it.
   *
   * `prepared`: the session playing plays on meanwhile. The new one is
   * asked for with `keep_previous` (the server leaves that one to the
   * player to end), its playlist and the segment the TV's player will
   * start with are asked for, so the transcoder makes them (20 s at most),
   * and only then does the session playing end and the engine move to the
   * new one, from where the last one got to: the switch is as short as the
   * TV's player opening a stream whose first segment is made. With no
   * transcode free for two at once (503), the one playing ends first, as
   * for any other start.
   */
  private async begin(atMs: number, generation: number, prepared = false): Promise<void> {
    const detail = this.item;
    if (!detail) return;
    const chosen = this.file;
    if (!chosen) {
      this.fail(T.noFile, false);
      return;
    }
    let alongside = prepared;
    const preparing = alongside ? new Cancel() : null;
    this.preparing = preparing;
    // The session before ends first, unless this one is made ready beside it.
    if (!alongside && !(await this.leave(atMs, generation))) return;

    // The original, chosen or played by Automatic, gets the Dolby Vision guards and the fallbacks; a lower quality is a conversion.
    const playing = this.playingPreset;
    const original = playing.id === ORIGINAL.id;
    if (original && !this.guardMode) this.guardMode = dvContainerMode(chosen, this.caps, this.env.dvContainers?.());
    const mode = original ? this.recovery.fallbackMode ?? this.guardMode ?? modeOf(playing) : modeOf(playing);
    const decision = this.caps ? decide(chosen, this.caps) : null;
    // Automatic audio is chosen again for each session: a stream (a lower
    // quality, say) has the track it copies rather than converting the one
    // the file played as it is.
    const audio = this.audioPicked ? this.audio : null;
    let choices: Choices = {
      file: chosen,
      startMs: atMs,
      audio,
      subtitle: this.subtitle,
      quality: playing,
      mode,
      streaming: willStream(mode, decision, chosen, audio, this.caps),
      sideload: this.sideload,
    };
    const ask = (c: Choices): PlaybackRequest => {
      const req = playbackRequest(detail.id, c, this.env.clientId, this.caps, this.env.keyedUrls);
      if (alongside) req.keep_previous = true;
      return req;
    };
    let sent = ask(choices);
    let info: PlaybackInfo;
    try {
      info = await this.env.api.play(sent);
    } catch (e) {
      // No transcode free for two at once: the one playing ends first, as before.
      if (!alongside || !isApiError(e, 503) || generation !== this.generation || this.closed) throw e;
      alongside = false;
      atMs = this.livePosition();
      if (!(await this.leave(atMs, generation))) return;
      choices = { ...choices, startMs: atMs };
      sent = ask(choices);
      info = await this.env.api.play(sent);
    }
    if (generation !== this.generation || this.closed) {
      // Closed or replaced while the server made it: it never played.
      this.discard(info);
      return;
    }
    const guarded = dvRetryMode(info, chosen, this.caps);
    if (guarded) {
      this.discard(info);
      this.guardMode = guarded;
      choices = { ...choices, mode: guarded, streaming: true };
      sent = ask(choices);
      info = await this.env.api.play(sent);
    }
    if (needsIndexes(info, sent, choices)) {
      this.discard(info);
      choices = { ...choices, streaming: true };
      sent = ask(choices);
      info = await this.env.api.play(sent);
    }
    if (generation !== this.generation || this.closed) {
      this.discard(info);
      return;
    }
    if (this.env.keyedUrls && !isKeyed(info.url)) {
      this.discard(info);
      this.fail("This server is too old to play on this TV: update it.", false);
      return;
    }
    if (alongside && preparing) {
      if (info.hls) await warmStream(this.streamFetch, this.env.api.url(info.url), atMs + PREPARE_LEAD_MS, preparing, this.now);
      // Replaced or closed meanwhile, or the last one ended or failed by itself: this one never plays.
      if (generation !== this.generation || this.closed || this.ended || this.state.value.phase.kind === "error") {
        this.discard(info);
        return;
      }
      // From where the last one got to while this one got ready. The server answered, whatever the last one found.
      atMs = this.livePosition();
      this.endOutage();
      this.endSession(atMs);
      this.reset(atMs, true);
      this.engineRef?.holdPicture?.();
    }
    if (this.preparing === preparing) this.preparing = null;
    this.info = info;
    // No track picked: the one the server chose for this TV plays, and the menu says so.
    this.audio = audio ?? playingAudio(chosen, info);
    this.sessionEnded = false;
    this.live = null;
    if (this.auto) {
      const live = this.autoBegan(this.auto, info, original);
      // A new stream of the same quality (a recovery, a track) goes on at the bitrate the last one had come to.
      if (live && this.auto.liveLowered) this.sendBitrate(this.auto.liveKbps!);
    }
    this.update({
      method: methodText(info),
      note: this.noteText(),
      canPlayOriginal: this.recovery.struggleSwitched && this.recovery.fallbackMode !== null,
    });
    if (info.duration_ms > 0) this.setClock({ durationMs: info.duration_ms });
    this.publishChoices();
    this.armProgress();
    this.refreshDetails(false);
    const engine = this.engineRef;
    if (!engine) return;
    const video = videoStreamOf(chosen);
    const source: EngineSource = {
      url: this.env.api.url(info.url),
      hls: info.hls,
      mime: info.hls ? null : mimeTypeOf(chosen.container),
      startMs: atMs,
      durationHintMs: info.duration_ms > 0 ? info.duration_ms : chosen.duration_ms ?? detail.duration_ms ?? null,
      video: video
        ? {
            codec: info.method === "transcode" ? "unknown" : video.codec,
            width: video.width ?? null,
            height: video.height ?? null,
            frameRate: video.frame_rate ?? null,
            // A conversion's dynamic range is the server's to choose.
            range: info.method === "transcode" ? "sdr" : video.dynamic_range ?? "sdr",
            dvProfile: info.method === "transcode" ? null : video.dv_profile ?? null,
            bitDepth: video.bit_depth ?? 8,
          }
        : null,
      audioCodec: info.audio_converted ? null : this.audio?.codec ?? null,
    };
    engine.prepare(source, this.wantPlay);
    const speed = this.state.value.speed;
    if (speed !== 1) engine.setSpeed?.(speed);
    this.applyTracks();
  }

  /** Reports `stopped` at `positionMs` and deletes the session playing; false when none plays. */
  private endSession(positionMs: number): boolean {
    const info = this.info;
    if (!info || this.sessionEnded) return false;
    this.sessionEnded = true;
    this.clearProgress();
    const itemId = this.item?.id ?? this.current.itemId;
    const api = this.env.api;
    this.env.reporter.send(() => api.progress(itemId, positionMs, info.duration_ms, "stopped", info.session_id));
    this.env.reporter.send(() => api.stopSession(info.session_id));
    return true;
  }

  /**
   * Ends the session playing ("stopped", then DELETE, waited for unless the
   * server is slow to say so) and gets ready for one at `atMs`; false when
   * a newer start took over meanwhile.
   */
  private async leave(atMs: number, generation: number): Promise<boolean> {
    if (this.endSession(atMs)) await this.env.reporter.flush(END_WAIT_MS);
    if (generation !== this.generation || this.closed) return false;
    this.reset(atMs, false);
    return true;
  }

  /** What is watched of a session starts over, for one at `atMs`; `switching` to one made ready beside the last. */
  private reset(atMs: number, switching: boolean): void {
    this.lastPositionMs = atMs;
    this.startedAtMs = atMs;
    this.mediaShown = false;
    this.ended = false;
    this.struggle.reset();
    this.recovery.onJump();
    this.lastAdvanceAt = this.now();
    this.lastAdvancePos = -1;
    this.sessionStartedAt = this.now();
    this.update({ phase: { kind: "preparing" }, skipLabel: null, firstFrameMs: null, switching });
    this.setClock({ positionMs: atMs });
  }

  /** Deletes a session that never played: no `stopped`, which would move the resume point. */
  private discard(info: PlaybackInfo): void {
    const api = this.env.api;
    this.env.reporter.send(() => api.stopSession(info.session_id));
  }

  /** Subtitles go where they're drawn: the server's WebVTT for text, its .mks for pictures, nothing when burned in. */
  private applyTracks(): void {
    const info = this.info;
    const engine = this.engineRef;
    if (!info || !engine) return;
    if (!info.hls && this.audio) {
      // Another track than the one the file starts with: the switch may drop frames.
      if (engine.selectAudio(audioStreams(this.file).indexOf(this.audio), this.audio.language) && !this.audio.default) this.quietFrames();
    }
    const sub = this.subtitle;
    if (!sub) {
      this.presenter.hide();
      return;
    }
    if (!isPicture(sub)) {
      const path = info.subtitles.filter((t) => t.index === sub.index)[0]?.url ?? `/api/files/${info.file_id}/subtitles/${sub.index}.vtt`;
      this.presenter.showText(path);
    } else if (!info.burn_subtitles && this.sideload) {
      this.presenter.showPictures(picturePath(info.file_id, sub.index));
    } else {
      this.presenter.hide();
    }
  }

  /** An audio track switched in place: the frames it drops aren't counted, and counting starts afresh after. */
  private quietFrames(): void {
    this.framesQuietUntil = this.now() + TRACK_SWITCH_QUIET_MS;
    this.struggle.reset();
  }

  /** Picture subtitles are drawn here unless the server can't make them. */
  private get sideload(): boolean {
    return !this.picturesUnavailable && !!this.caps?.picture_subtitles_sideload;
  }

  /** The server can't make the .mks (an older server or transcoder): picture subtitles are burned in from now on. */
  private picturesFailed(): void {
    if (this.closed || this.stopped) return;
    this.picturesUnavailable = true;
    this.publishChoices();
    if (isPicture(this.subtitle) && this.info && !this.info.burn_subtitles) this.restart(this.livePosition(), true);
  }

  // ---------------------------------------------------------------- engine events

  private onStatus(status: EngineStatus): void {
    if (!this.info || this.closed) return;
    switch (status) {
      case "buffering":
        if (this.ended) break;
        // A stall: it played, and no seek of its own asked it to wait.
        if (this.state.value.phase.kind === "playing" && this.now() - this.seekedAt > SEEK_SETTLE_MS) {
          this.update({ stalls: this.state.value.stalls + 1 });
          this.auto?.waiting(this.now());
        }
        this.phase({ kind: "buffering" });
        break;
      case "ready":
        this.mediaShown = true;
        this.shownOnce = true;
        if (this.state.value.switching) this.update({ switching: false });
        this.auto?.resumed(this.now());
        if (!this.outage) this.phase(this.engineRef?.playing() ? { kind: "playing" } : { kind: "paused" });
        break;
      case "ended":
        if (this.endedShort()) this.onEndedShort();
        else this.onEnded();
        break;
      default:
        break;
    }
  }

  private onFirstFrame(): void {
    if (this.closed || !this.info) return;
    this.mediaShown = true;
    this.shownOnce = true;
    if (this.state.value.switching) this.update({ switching: false });
    if (this.state.value.firstFrameMs === null) {
      // From the press for the first session; from the session's start after that.
      const from = this.pressedAt || this.sessionStartedAt;
      this.pressedAt = 0;
      this.update({ firstFrameMs: Math.round(this.now() - from) });
    }
    this.presenter.refresh();
  }

  private onError(error: EngineError): void {
    const info = this.info;
    if (!info || this.closed || this.stopped) return;
    if (error.kind === "stopped") {
      this.stopByServer(error.message, true);
      return;
    }
    // A native player can't say why a request failed: the session's own answer tells a stop from the rest.
    if (error.kind === "http" || error.kind === "network" || error.kind === "playlist") {
      if (this.outage) {
        // Reconnecting already: the engine gave up too, so a new session starts once the server answers.
        this.outage.engineFailed = true;
        return;
      }
      const sessionId = info.session_id;
      this.env.api.sessionStatus(sessionId, undefined, OUTAGE_ASK_MS).then(
        () => this.recover(error, info),
        (e: unknown) => {
          if (this.info?.session_id !== sessionId) return;
          if (e instanceof SessionStoppedError) this.stopByServer(e.note, true);
          else if (outOfReach(e)) this.reachLost(true);
          else if (e instanceof ApiError && e.status === 404) this.recover({ kind: "http", status: 404, detail: e.message }, info);
          else this.recover(error, info);
        },
      );
      return;
    }
    this.recover(error, info);
  }

  private recover(error: EngineError, info: PlaybackInfo): void {
    if (this.info !== info || this.closed || this.stopped) return;
    this.act(this.recovery.decide(error, info.method, videoStreamOf(this.file)?.codec ?? null), false);
  }

  private act(action: RecoveryAction, starving: boolean): void {
    switch (action.kind) {
      case "new_session":
        // The player's own fallback is a change too: Automatic waits before it changes again.
        if (action.reason) this.auto?.fellBack(this.now());
        if (action.reason === "not_smooth" && this.recovery.struggleSwitched) this.update({ canPlayOriginal: true });
        this.restart(this.livePosition());
        break;
      case "check_server": {
        const info = this.info;
        if (!info) return;
        this.env.api.sessionStatus(info.session_id, undefined, OUTAGE_ASK_MS).then(
          (status) => {
            if (this.info === info) this.act(this.recovery.afterServerCheck(status.transcode?.speed ?? null, starving), starving);
          },
          (e: unknown) => {
            if (this.info !== info) return;
            // A stall with the server gone isn't the video's fault: wait for the server rather than convert.
            if (outOfReach(e)) this.reachLost(false);
            else if (e instanceof SessionStoppedError) this.stopByServer(e.note, true);
            else this.act(this.recovery.afterServerCheck(null, starving), starving);
          },
        );
        break;
      }
      case "suggest_lower_quality":
        this.update({ suggestLowerQuality: true, note: T.slowServer });
        break;
      case "fail":
        this.fail(failureText(action.failure, action.detail), action.failure !== "unplayable", action.failure === "network" ? T.offlineTitle : undefined);
        break;
    }
  }

  /** The media ended moments after where it started, a minute or more before its end: not the end. */
  private endedShort(): boolean {
    if (this.ended || !this.engineRef) return false;
    const expected = (this.info?.duration_ms || 0) > 0 ? this.info!.duration_ms : this.file?.duration_ms ?? 0;
    if (!expected) return false;
    const reached = this.engineRef.positionMs();
    return reached + SHORT_END_MS < expected && reached < this.startedAtMs + SHORT_PLAY_MS;
  }

  private onEndedShort(): void {
    const info = this.info;
    if (!info) return;
    this.mediaShown = false;
    this.lastPositionMs = this.startedAtMs;
    this.setClock({ positionMs: this.startedAtMs });
    this.update({ skipLabel: null });
    this.act(this.recovery.onEndedShort(info.method), false);
  }

  private onEnded(): void {
    if (this.ended) return;
    this.ended = true;
    const sleeping = this.sleep.itemEnded(this.item?.id ?? null);
    const engineDuration = this.engineRef?.durationMs() ?? 0;
    const duration = (this.info?.duration_ms || 0) > 0 ? this.info!.duration_ms : engineDuration > 0 ? engineDuration : this.lastPositionMs;
    this.lastPositionMs = duration;
    this.endSession(duration);
    this.setClock({ positionMs: duration });
    this.update({ skipLabel: null });
    const detail = this.item;
    if (!detail || !hasNext(detail.kind, this.current.queue)) {
      this.phase({ kind: "ended" });
      return;
    }
    this.phase({ kind: "paused" });
    const known = this.next;
    if (known) {
      this.showUpNext(known, !sleeping && this.countdownTimer === null);
      return;
    }
    this.lookUpNext((found) => {
      if (!found) this.phase({ kind: "ended" });
      else this.showUpNext(found, !sleeping);
    });
  }

  // ---------------------------------------------------------------- the clock

  private tick(): void {
    const engine = this.engineRef;
    const info = this.info;
    if (!engine || !info || this.closed || this.sessionEnded) return;
    const now = this.now();
    const status = engine.status();
    const moving = this.mediaShown && status === "ready" && engine.playing() && !this.ended;
    if (moving && this.movingAt !== null) this.titlePlayedMs += Math.min(now - this.movingAt, 2 * TICK_MS);
    this.movingAt = moving ? now : null;
    if (this.mediaShown && !this.ended) {
      const pos = engine.positionMs();
      this.lastPositionMs = pos;
      if (moving) this.recovery.onPosition(pos);
      const marker = this.state.value.phase.kind === "error" ? null : activeMarker(this.markers, pos);
      const engineDuration = engine.durationMs();
      this.setClock({
        positionMs: pos,
        bufferedMs: engine.bufferedMs(),
        durationMs: info.duration_ms > 0 ? info.duration_ms : engineDuration > 0 ? engineDuration : this.clock.value.durationMs,
      });
      const label = marker ? skipLabel(marker) : null;
      if (label !== this.state.value.skipLabel) this.update({ skipLabel: label });
      if (marker && this.credits.enter(marker)) this.onCredits();
    }
    // Reconnecting: the stall ladder and the frame counts wait; the engine moving on by itself ends it.
    const outage = this.outage;
    if (outage) {
      if (moving && engine.positionMs() > outage.atMs + OUTAGE_MOVED_MS) {
        this.endOutage();
        this.phase({ kind: "playing" });
      }
      return;
    }
    // A session made ready beside this one: nothing more is decided about the stream it leaves.
    if (this.preparing) {
      this.lastAdvanceAt = now;
      return;
    }
    if (this.watchQuality(now, engine, moving)) return;
    // A stall: meant to play, not moving.
    const stalling = this.wantPlay && !this.ended && this.state.value.phase.kind !== "error" && status !== "idle";
    const pos = this.mediaShown ? engine.positionMs() : -1;
    if (!stalling || pos !== this.lastAdvancePos) {
      this.lastAdvanceAt = now;
      this.lastAdvancePos = pos;
    } else if (now - this.lastAdvanceAt >= STALL_MS) {
      this.lastAdvanceAt = now;
      const starving = status === "buffering" && engine.bufferedMs() - engine.positionMs() < STARVING_MS;
      this.onStalled(info, starving);
      return;
    }
    // Dropped frames, sampled every 2 s of playback.
    if (moving && now >= this.framesQuietUntil && now - this.lastStruggleSample >= STRUGGLE_SAMPLE_MS) {
      this.lastStruggleSample = now;
      const counts = engine.frameCounts();
      if (counts && this.struggle.sample(now, counts.dropped, counts.total)) {
        this.struggle.reset();
        const auto = this.auto;
        if (auto && auto.patient && this.titlePlayedMs >= FIRST_MINUTE_MS) {
          // Automatic on a stream that can't change as it plays: dropped frames move it in the
          // title's first minute only, a rough guess that this TV can't keep up with the file;
          // later the picture plays on, and the next title starts lower (docs/QUALITY.md).
          auto.struggled();
        } else {
          // The original falls back to a conversion once, as ever; a conversion that drops frames is Automatic's to lower.
          const action = this.recovery.onStruggling(info.method);
          if (action) this.act(action, false);
          else if (auto) {
            auto.decoding();
            this.applyAuto(auto.check(now));
          }
        }
      }
    }
  }

  /**
   * A 12 s stall. The TV's own player says nothing when the server goes,
   * so the server is asked first: one that doesn't answer is waited for
   * (the video isn't at fault, and a conversion couldn't start anyway);
   * else the recovery ladder decides.
   */
  private onStalled(info: PlaybackInfo, starving: boolean): void {
    this.env.api.sessionStatus(info.session_id, undefined, OUTAGE_ASK_MS).then(
      () => {
        if (this.info === info && !this.outage && !this.closed) this.act(this.recovery.onStall(info.method, starving), starving);
      },
      (e: unknown) => {
        if (this.info !== info || this.outage || this.closed) return;
        if (outOfReach(e)) this.reachLost(false);
        else if (e instanceof SessionStoppedError) this.stopByServer(e.note, true);
        else this.act(this.recovery.onStall(info.method, starving), starving);
      },
    );
  }

  /** Progress every 10 s while playing and every 30 s paused, so the server keeps the session and its transcode. */
  private armProgress(): void {
    this.clearProgress();
    if (!this.info || this.sessionEnded) return;
    this.progressTimer = setTimeout(
      () => {
        this.progressTimer = null;
        // Not while reconnecting: unanswered reports would queue up behind each other.
        if (!this.ended && !this.outage && this.state.value.phase.kind !== "error") this.report(this.wantPlay ? "playing" : "paused");
        this.armProgress();
      },
      this.wantPlay ? PROGRESS_MS : PAUSED_HEARTBEAT_MS,
    );
  }

  private clearProgress(): void {
    if (this.progressTimer !== null) clearTimeout(this.progressTimer);
    this.progressTimer = null;
  }

  private report(state: "playing" | "paused"): void {
    const info = this.info;
    if (!info || this.sessionEnded) return;
    const itemId = this.item?.id ?? this.current.itemId;
    const position = this.livePosition();
    const api = this.env.api;
    this.env.reporter.send(() => api.progress(itemId, position, info.duration_ms, state, info.session_id));
  }

  private livePosition(): number {
    const engine = this.engineRef;
    return this.mediaShown && engine && this.info && !this.sessionEnded && !this.ended ? engine.positionMs() : this.lastPositionMs;
  }

  /** How long the item playing has left, for the sleep timer; `null` while unknown. */
  private itemLeftMs(): number | null {
    if (!this.info || !this.mediaShown || this.ended) return null;
    const duration = this.clock.value.durationMs;
    return duration > 0 ? duration - this.livePosition() : null;
  }

  /** The sleep timer ended: it pauses, and what plays next doesn't start by itself. */
  private onSleep(): void {
    this.stopCountdown();
    const card = this.state.value.upNext;
    if (card) this.update({ upNext: { ...card, secondsLeft: null } });
    this.engineRef?.pause();
    this.setWantPlay(false);
  }

  /**
   * The server ended the session: the engine stops, nothing more is asked
   * of the server (only `stopped`, for the resume point), and the screen
   * says why, without Try again, then leaves.
   */
  private stopByServer(message: string | null, byAdmin: boolean): void {
    if (this.closed || this.stopped) return;
    this.stopped = true;
    this.nextGeneration();
    const position = this.livePosition();
    this.lastPositionMs = position;
    const info = this.info;
    if (info && !this.sessionEnded) {
      this.sessionEnded = true;
      this.clearProgress();
      const itemId = this.item?.id ?? this.current.itemId;
      const api = this.env.api;
      this.env.reporter.send(() => api.progress(itemId, position, info.duration_ms, "stopped", info.session_id));
    }
    if (this.seekReportTimer !== null) clearTimeout(this.seekReportTimer);
    this.nextLookup++;
    this.hideUpNext();
    this.presenter.hide();
    this.sleep.cancel();
    this.engineRef?.stop();
    this.update({ phase: { kind: "error", message: stoppedText(message, byAdmin), canRetry: false }, skipLabel: null, stoppedByServer: true });
  }

  // ---------------------------------------------------------------- commands

  play(): void {
    if (this.closed || this.stopped) return;
    if (this.ended) {
      this.restart(0);
      return;
    }
    this.setWantPlay(true);
    this.engineRef?.play();
  }

  pause(): void {
    if (this.closed) return;
    this.engineRef?.pause();
    this.setWantPlay(false);
  }

  /** Play or pause, from what the player means to do now. */
  get wantsToPlay(): boolean {
    return this.wantPlay;
  }

  private setWantPlay(play: boolean): void {
    const was = this.wantPlay;
    this.wantPlay = play;
    if (play) this.sleep.resumed();
    if (!this.info || was === play) return;
    if (!play) {
      this.struggle.reset();
      this.auto?.interrupted();
      if (!this.ended) this.report("paused");
      if (this.engineRef?.status() === "ready") this.phase({ kind: "paused" });
    } else if (this.engineRef?.status() === "ready") {
      this.phase({ kind: "playing" });
    }
    this.armProgress();
  }

  seekTo(positionMs: number): void {
    const engine = this.engineRef;
    if (!engine || this.closed || this.stopped) return;
    const duration = this.clock.value.durationMs;
    const target = duration > 0 ? Math.max(0, Math.min(positionMs, Math.max(0, duration - SEEK_END_MARGIN_MS))) : Math.max(0, positionMs);
    this.hideUpNext();
    if (this.ended || !this.info || this.sessionEnded) {
      this.lastPositionMs = target;
      this.setClock({ positionMs: target });
      this.restart(target);
      return;
    }
    engine.seekTo(target);
    this.seekedAt = this.now();
    this.lastPositionMs = target;
    this.startedAtMs = target;
    this.recovery.onJump();
    this.struggle.reset();
    this.auto?.interrupted();
    this.lastAdvanceAt = this.now();
    this.setClock({ positionMs: target });
    this.presenter.sought(target);
    if (this.seekReportTimer !== null) clearTimeout(this.seekReportTimer);
    this.seekReportTimer = setTimeout(() => {
      this.seekReportTimer = null;
      this.report(this.wantPlay ? "playing" : "paused");
    }, SEEK_REPORT_MS);
  }

  selectAudio(index: number): void {
    const chosen = audioStreams(this.file).filter((s) => s.index === index)[0];
    if (!chosen || chosen.index === this.audio?.index) return;
    this.audio = chosen;
    this.audioPicked = true;
    this.saveStreams();
    this.publishChoices();
    const info = this.info;
    if (!info) return;
    const engine = this.engineRef;
    if (!info.hls && engine && audioPlaysHere(chosen, this.caps) && engine.selectAudio(audioStreams(this.file).indexOf(chosen), chosen.language)) {
      this.quietFrames();
      return;
    }
    this.restart(this.livePosition(), true);
  }

  selectSubtitle(index: number | null): void {
    const chosen = index === null ? null : subtitleStreams(this.file).filter((s) => s.index === index)[0];
    if (chosen === undefined) return;
    if ((chosen?.index ?? null) === (this.subtitle?.index ?? null)) return;
    this.subtitle = chosen;
    this.saveStreams();
    this.publishChoices();
    const info = this.info;
    if (!info) return;
    // Pictures the TV can't draw are burned in: that is another session.
    const burn = isPicture(chosen) && !this.sideload;
    if (burn !== info.burn_subtitles || (burn && info.subtitle_index !== chosen?.index)) this.restart(this.livePosition(), true);
    else this.applyTracks();
  }

  /** Subtitle files the server finds for what plays, in `language` (ISO 639-2; the profile's own when `null`). */
  findSubtitles(language: string | null, cancel?: Cancel): Promise<SubtitleResult[]> {
    const item = this.item;
    const file = this.file;
    if (!item || !file) return Promise.resolve([]);
    return this.env.api.searchSubtitles(item.id, file.id, language ? [twoLetterCode(language)] : null, cancel);
  }

  /** Adds `result` on the server, for everyone; then it is listed with the file's and shows. */
  addFoundSubtitle(result: SubtitleResult): Promise<void> {
    const item = this.item;
    const file = this.file;
    if (!item || !file) return Promise.reject(new Error(T.noFile));
    return this.env.api.addSubtitle(item.id, file.id, result.id).then((track) => this.subtitleAdded(track));
  }

  /** A track added on the server: listed with the file's own (and the session's, for its address), and shown. */
  private subtitleAdded(track: SubtitleTrack): void {
    const file = this.file;
    const item = this.item;
    if (this.closed || this.stopped || !file || !item) return;
    const stream: MediaStream = {
      index: track.index,
      kind: "subtitle",
      codec: track.codec,
      language: track.language,
      title: track.title,
      default: false,
      forced: track.forced,
      external: true,
      text_based: track.text_based,
      added: true,
      deletable: track.deletable,
    };
    const updated: MediaFile = { ...file, streams: file.streams.filter((s) => !(s.kind === "subtitle" && s.index === track.index)).concat([stream]) };
    this.file = updated;
    this.item = { ...item, files: item.files.map((f) => (f.id === updated.id ? updated : f)) };
    const info = this.info;
    if (info) this.info = { ...info, subtitles: info.subtitles.filter((t) => t.index !== track.index).concat([track]) };
    this.notice(T.subtitlesAdded(streamLabel(stream)));
    if (this.subtitle?.index === track.index) this.publishChoices();
    else this.selectSubtitle(track.index);
  }

  setSubtitleOffset(ms: number): void {
    if (this.closed) return;
    const moved = clampOffset(ms);
    if (moved === this.offset) return;
    this.offset = moved;
    this.update({ subtitleOffsetMs: moved });
    if (this.file) this.env.offsets.set(this.env.api.server, this.file.id, moved);
    this.presenter.refresh();
  }

  /** Plays at `rate` times normal speed, until the next title; only at the speeds the TV's player plays. */
  setSpeed(rate: number): void {
    const engine = this.engineRef;
    if (this.closed || !engine?.setSpeed || this.state.value.speeds.indexOf(rate) < 0) return;
    if (engine.setSpeed(rate)) this.update({ speed: rate });
  }

  setSleep(choice: SleepChoice): void {
    if (this.closed || this.stopped) return;
    this.sleep.set(choice);
  }

  keepWatching(): void {
    if (this.closed || this.stopped) return;
    this.sleep.extend();
    // What plays next was held for the timer: its countdown goes on.
    const known = this.next;
    if (known && this.state.value.upNext && this.countdownTimer === null && !this.ended) this.showUpNext(known, true);
  }

  /**
   * A quality picked in the menu, kept for the next titles. Automatic
   * picked again while it plays lower starts afresh at the original. The
   * session starts again only when it would ask for something else.
   */
  selectQuality(id: string): void {
    const chosen = qualitiesFor(this.file).filter((p) => p.id === presetById(id)?.id)[0];
    if (!chosen) return;
    if (chosen.id === this.quality.id && !(this.auto && (this.auto.playing !== ORIGINAL.id || this.auto.liveLowered))) return;
    const before = this.playingPreset.id;
    const fellBack = this.recovery.fallbackMode !== null;
    // A stream Automatic had sent on at less than its own bitrate.
    const eased = this.auto && this.auto.liveLowered ? this.auto.liveTop : null;
    this.quality = chosen;
    this.recovery.qualityChanged();
    if (chosen.id === AUTO.id) {
      this.env.autoMemory?.set(null);
      this.env.network?.report(null);
    }
    // Automatic picked again starts afresh at the original, nothing asked.
    this.startAuto(null);
    this.update({ suggestLowerQuality: false, note: this.noteText(), canPlayOriginal: false });
    this.env.quality.remember(chosen.id);
    if (this.item && (this.playingPreset.id !== before || fellBack)) {
      this.publishChoices();
      this.restart(this.livePosition(), true);
      return;
    }
    // The same stream plays on: Automatic watches it afresh, and it goes back to its own bitrate.
    const info = this.info;
    if (this.auto && info && !this.sessionEnded) this.autoBegan(this.auto, info, this.playingPreset.id === ORIGINAL.id);
    this.publishChoices();
    if (eased !== null) this.sendBitrate(eased);
  }

  skip(): void {
    const marker = activeMarker(this.markers, this.livePosition());
    if (!marker) return;
    const duration = this.clock.value.durationMs;
    this.seekTo(duration > 0 ? Math.min(skipTarget(marker), duration) : skipTarget(marker));
  }

  playNext(): void {
    if (this.closed || this.stopped) return;
    const known = this.next;
    if (known) {
      this.switchTo(known);
      return;
    }
    this.lookUpNext((found) => {
      if (!found) this.notice(this.current.queue ? T.lastInQueue : T.lastEpisode);
      else this.switchTo(found);
    });
  }

  cancelUpNext(): void {
    this.hideUpNext();
    if (this.ended) this.phase({ kind: "ended" });
  }

  playOriginal(): void {
    if (this.recovery.fallbackMode === null) return;
    this.recovery.playOriginal();
    this.update({ canPlayOriginal: false, note: this.noteText() });
    this.restart(this.livePosition(), true);
  }

  setDetailsOpen(open: boolean): void {
    if (this.detailsTimer !== null) clearTimeout(this.detailsTimer);
    this.detailsTimer = null;
    if (!open) {
      this.update({ details: null });
      return;
    }
    const loop = () => {
      const info = this.info;
      const done = () => {
        this.refreshDetails(true);
        this.detailsTimer = setTimeout(loop, DETAILS_MS);
      };
      if (info && info.hls) {
        this.env.api.sessionStatus(info.session_id).then(
          (s) => {
            if (this.detailsTimer === null && this.state.value.details === null && !open) return;
            this.live = s.transcode;
            done();
          },
          () => done(),
        );
      } else {
        done();
      }
    };
    this.detailsTimer = setTimeout(loop, 0);
  }

  private refreshDetails(force: boolean): void {
    if (!force && this.state.value.details === null) return;
    const info = this.info;
    if (!info) return;
    this.update({
      details: buildDetails({
        info,
        file: this.file,
        audioIndex: this.audio?.index ?? null,
        note: this.noteText(),
        positionMs: this.livePosition(),
        live: this.live,
        subtitle: this.subtitle,
        sideload: this.sideload,
      }),
    });
  }

  dismissNotice(): void {
    this.update({ notice: null });
  }

  retry(): void {
    if (this.closed || this.stopped) return;
    this.endOutage();
    this.recovery.retry();
    this.update({ phase: { kind: "preparing" } });
    if (!this.item) void this.prepareFirst();
    else this.restart(this.lastPositionMs);
  }

  /** Where the player is, to start again there after the app comes back. */
  saveState(): SavedPlayback {
    return {
      request: {
        ...this.current,
        itemId: this.item?.id ?? this.current.itemId,
        fileId: this.file?.id ?? this.current.fileId ?? null,
        startMs: this.livePosition(),
        fromStart: false,
        audioIndex: this.audioPicked ? this.audio?.index ?? null : null,
        subtitleIndex: this.subtitle?.index ?? null,
      },
      quality: this.quality.id,
      playing: this.wantPlay,
    };
  }

  close(): void {
    if (this.closed) return;
    const position = this.livePosition();
    this.closed = true;
    this.nextGeneration();
    this.endOutage();
    this.endSession(position);
    this.sleep.cancel();
    this.presenter.dispose();
    this.stopCountdown();
    for (const t of [this.seekReportTimer, this.detailsTimer, this.trickplayTimer]) if (t !== null) clearTimeout(t);
    if (this.tickTimer !== null) clearInterval(this.tickTimer);
    this.unsubscribe?.();
    this.engineRef?.release();
    this.engineRef = null;
  }

  // ---------------------------------------------------------------- Up Next

  private onCredits(): void {
    const detail = this.item;
    if (!detail || !hasNext(detail.kind, this.current.queue) || this.state.value.upNext) return;
    this.lookUpNext((found) => {
      if (found && !this.closed) this.showUpNext(found, true);
    });
  }

  /** Finds what plays next (one lookup at a time), then calls `then` with it. */
  private lookUpNext(then: (found: ItemSummary | null) => void): void {
    const detail = this.item;
    if (!detail) {
      then(null);
      return;
    }
    const lookup = ++this.nextLookup;
    nextAfter(this.env.api, detail.id, detail.kind, this.current.queue).then(
      (found) => {
        if (lookup !== this.nextLookup || this.closed) return;
        if (found) this.next = found;
        then(found);
      },
      () => {
        if (lookup === this.nextLookup && !this.closed) then(null);
      },
    );
  }

  private showUpNext(found: ItemSummary, restartCountdown: boolean): void {
    this.next = found;
    if (this.nextDetail?.id !== found.id) {
      const choices = this.choicesSaved;
      this.env.api.item(found.id).then(
        (d) => {
          // Tracks picked meanwhile change what it starts with: asked again then.
          if (choices === this.choicesSaved) this.nextDetail = d;
        },
        () => undefined,
      );
    }
    const image = found.thumb_url ?? found.backdrop_url ?? null;
    const card: UpNextCard = {
      itemId: found.id,
      title: found.title,
      heading: T.upNext(labelOf(found)),
      imageUrl: image ? this.env.api.url(withWidth(image, UP_NEXT_IMAGE_WIDTH)) : null,
      secondsLeft: null,
    };
    this.update({ upNext: card });
    if (!restartCountdown) return;
    this.stopCountdown();
    if (!this.env.autoplayNext() || this.sleep.endsWith(this.item?.id ?? null)) return;
    let left = COUNTDOWN_SECONDS;
    const step = () => {
      const current = this.state.value.upNext;
      if (!current || current.itemId !== found.id) return;
      if (left <= 0) {
        this.countdownTimer = null;
        this.switchTo(found);
        return;
      }
      this.update({ upNext: { ...current, secondsLeft: left } });
      left--;
      this.countdownTimer = setTimeout(step, 1000);
    };
    step();
  }

  private stopCountdown(): void {
    if (this.countdownTimer !== null) clearTimeout(this.countdownTimer);
    this.countdownTimer = null;
  }

  private hideUpNext(): void {
    this.stopCountdown();
    if (this.state.value.upNext) this.update({ upNext: null });
  }

  /** Plays `target` on the same engine: the session before ends, the next one starts where `target` resumes. */
  private switchTo(target: ItemSummary): void {
    this.hideUpNext();
    this.nextLookup++;
    const position = this.livePosition();
    const endedBefore = this.endSession(position);
    this.next = null;
    this.recovery.newItem();
    this.credits.reset();
    this.guardMode = null;
    this.presenter.hide();
    this.ended = false;
    this.mediaShown = false;
    const req: PlayRequest = {
      ...this.current,
      itemId: target.id,
      fileId: null,
      startMs: startOf(target),
      fromStart: false,
      audioIndex: null,
      subtitleIndex: null,
    };
    this.current = req;
    const sleep = this.state.value.sleep;
    const title = this.state.value.title;
    // The next title plays at normal speed.
    if (this.state.value.speed !== 1) this.engineRef?.setSpeed?.(1);
    this.state.value = { ...INITIAL, itemId: target.id, title, sleep, speeds: this.state.value.speeds };
    this.setClock({ positionMs: req.startMs ?? 0, bufferedMs: 0 });
    const generation = this.nextGeneration();
    const prefetched = this.nextDetail?.id === target.id ? this.nextDetail : null;
    const known = this.networkStart();
    (async () => {
      if (endedBefore) await this.env.reporter.flush(END_WAIT_MS);
      const detail = prefetched ?? (await this.env.api.item(target.id));
      const ceiling = await known;
      if (generation !== this.generation || this.closed) return;
      this.load(detail, req, ceiling);
      await this.begin(req.startMs ?? 0, generation);
    })().catch((e: unknown) => {
      if (generation === this.generation) this.failWith(e);
    });
  }

  // ---------------------------------------------------------------- seek previews

  private loadTrickplay(file: MediaFile | null): void {
    if (this.trickplayTimer !== null) clearTimeout(this.trickplayTimer);
    this.trickplayTimer = null;
    if (!file) return;
    const started = this.now();
    const ask = () => {
      this.env.api.trickplay(file.id).then(
        (info) => {
          if (this.file?.id !== file.id || this.closed) return;
          if (info.count > 0) this.update({ trickplay: { fileId: file.id, info } });
        },
        () => {
          if (this.file?.id !== file.id || this.closed) return;
          if (this.now() - started < TRICKPLAY_GIVE_UP_MS) this.trickplayTimer = setTimeout(ask, TRICKPLAY_RETRY_MS);
        },
      );
    };
    ask();
  }

  // ---------------------------------------------------------------- Automatic

  /** What plays: Automatic's quality of the moment, or the one chosen. */
  private get playingPreset(): QualityPreset {
    return this.auto ? namedFor(this.file, presetById(this.auto.playing) ?? ORIGINAL) : this.quality;
  }

  /**
   * Automatic, when chosen: a new one for the title, starting where the
   * server says this network allows (`known`), else where the TV last found
   * it did (Up Next too). Its ceilings are told to the server.
   */
  private startAuto(known?: Remembered | null): void {
    this.auto = null;
    if (this.quality.id !== AUTO.id) return;
    const file = this.file;
    const network = this.env.network;
    this.auto = new AutoQuality({
      ladder: autoLadder(file),
      originalKbps: file?.bitrate ? file.bitrate / 1000 : null,
      memory: this.env.autoMemory ?? null,
      onCeiling: network ? (kbps) => network.report(kbps) : undefined,
      // Samsung's AVPlay and LG's pipeline go black between streams (docs/QUALITY.md).
      blanks: true,
    });
    this.auto.start(this.now(), known);
  }

  /** The quality a converted stream was asked at: the one Automatic plays, or the server's own for the original; `null` for none. */
  private streamPreset(info: PlaybackInfo): QualityPreset | null {
    const playing = this.playingPreset;
    const preset = playing.id === ORIGINAL.id ? presetById(info.auto_quality) : playing;
    return preset && preset.maxBitrateKbps !== null ? namedFor(this.file, preset) : null;
  }

  /**
   * Automatic watches the session `info` afresh: a stream whose bitrate
   * changes as it is sent (returned), or the `original` converted (a
   * fallback, a file the TV can't play as it is), which it steps down from
   * what it plays at and moves for any wait of 2 s.
   */
  private autoBegan(auto: AutoQuality, info: PlaybackInfo, original: boolean): LiveStream | null {
    const live = this.liveStream(info);
    auto.began(live);
    const converted = original && info.auto_quality ? presetById(info.auto_quality) : null;
    if (converted) auto.setOriginalKbps(converted.maxBitrateKbps);
    return live;
  }

  /**
   * A converted stream whose bitrate changes as it is sent
   * (`live_bitrate`): its own bitrate, and the least its size keeps (8 Mbps
   * for 4K, 3 for 1080p); `null` for any other.
   */
  private liveStream(info: PlaybackInfo): LiveStream | null {
    if (!info.live_bitrate || info.method !== "transcode") return null;
    const preset = this.streamPreset(info);
    if (!preset) return null;
    const top = preset.maxBitrateKbps!;
    return { top, floor: Math.min(top, liveFloor(streamLines(this.file, preset))) };
  }

  /**
   * The stream playing goes on at `kbps` from the segments the TV's player
   * hasn't fetched yet (`PUT /api/playback/{id}/bitrate`): the same stream,
   * so the picture plays on. Automatic hears what the server made of it.
   */
  private sendBitrate(kbps: number): void {
    const info = this.info;
    if (!info || this.sessionEnded) return;
    const auto = this.auto;
    const told = () => {
      this.update({ note: this.noteText() });
      this.publishChoices();
    };
    this.env.api.setBitrate(info.session_id, kbps).then(
      (answer) => {
        if (this.info !== info || this.closed || !auto || auto !== this.auto) return;
        auto.liveAt(answer.bitrate, kbps);
        told();
      },
      (e: unknown) => {
        if (this.info !== info || this.closed || !auto || auto !== this.auto) return;
        // A stream that can't change (an older transcoder, a stream no longer converted) isn't asked again.
        auto.liveRefused(isApiError(e, 400));
        told();
      },
    );
  }

  /**
   * Tells Automatic what the engine shows while it is meant to play (not
   * paused, not settling after a seek), and acts on what it decides; true
   * when a new session starts.
   */
  private watchQuality(now: number, engine: PlayerEngine, moving: boolean): boolean {
    const auto = this.auto;
    if (!auto || !this.wantPlay || !this.mediaShown || this.ended || this.state.value.phase.kind === "error") return false;
    if (now - this.seekedAt <= SEEK_SETTLE_MS) return false;
    // A stream in several sizes: the TV's player moves between them as its
    // connection allows, without a new session (dropped frames still lower it).
    if (this.info?.adaptive) return false;
    if (moving) {
      auto.sample(now, engine.positionMs() / 1000, engine.knowsBuffer === false ? null : engine.bufferedMs() / 1000);
      const kbps = engine.bandwidthKbps ? engine.bandwidthKbps() : null;
      if (kbps) auto.bandwidth(now, kbps);
    }
    return this.applyAuto(auto.check(now));
  }

  /**
   * Acts on Automatic's step: the stream playing goes on at its new
   * bitrate, or a new one starts at its new quality. A step down says so,
   * once.
   */
  private applyAuto(step: AutoStep | null): boolean {
    if (!step || this.closed || this.stopped) return false;
    const stream = step.bitrate !== undefined && this.info ? this.streamPreset(this.info) : null;
    const to = stream ? liveLabel(this.file, stream, step.bitrate!) : namedFor(this.file, presetById(step.to) ?? ORIGINAL).label;
    this.update({ suggestLowerQuality: false, note: this.noteText() });
    this.publishChoices();
    if (step.why) this.notice(T.smoother(to));
    this.struggle.reset();
    if (step.bitrate !== undefined) this.sendBitrate(step.bitrate);
    else this.restart(this.livePosition(), true);
    return true;
  }

  /** Why it doesn't play the original: the player's fallback, or Automatic's step down. */
  private noteText(): string | null {
    if (this.recovery.fallbackReason) return fallbackText(this.recovery.fallbackReason);
    const auto = this.auto;
    return auto && auto.lowered && (auto.playing !== ORIGINAL.id || auto.liveLowered) ? T.lowered[auto.lowered] : null;
  }

  // ---------------------------------------------------------------- state

  private publishChoices(): void {

    const file = this.file;
    const info = this.info;
    const audio = audioStreams(file).map((s) => ({ index: s.index, label: streamLabel(s), selected: s.index === this.audio?.index, burnIn: false }));
    const subs = subtitleStreams(file);
    const off: TrackChoice = { index: null, label: T.subtitlesOff, selected: !this.subtitle, burnIn: false };
    const subtitles: TrackChoice[] = subs.length
      ? [off].concat(
          subs.map((s) => ({ index: s.index, label: streamLabel(s), selected: s.index === this.subtitle?.index, burnIn: isPicture(s) && !this.sideload, added: !!s.added })),
        )
      : [];
    const sub = this.subtitle;
    // Text moves always; pictures when drawn here from the server's .mks.
    const syncable = !!sub && (!isPicture(sub) || (this.sideload && info?.burn_subtitles !== true));
    const playing = this.playingPreset;
    // The original, converted here: at the closest quality, which Automatic says it plays.
    const convertedId = info?.auto_quality && playing.id === ORIGINAL.id ? info.auto_quality : null;
    const known = convertedId ? presetById(convertedId) : null;
    const converted = known ? namedFor(file, known) : null;
    const convertedLabel = converted && converted.maxBitrateKbps !== null ? converted.label : null;
    const hint = fileHint(file) || null;
    const auto = this.auto;
    // A stream Automatic sends on below its own bitrate: "4 Mbps 1080p".
    const stream = auto && auto.liveLowered && info ? this.streamPreset(info) : null;
    const eased = stream ? liveLabel(file, stream, auto!.liveKbps!) : null;
    const autoNow = eased ?? (playing.id === ORIGINAL.id ? convertedLabel ?? ORIGINAL.label : playing.label);
    const qualities: QualityChoice[] = qualitiesFor(file).map((p) => ({
      id: p.id,
      label: p.label,
      hint:
        p.id === AUTO.id
          ? auto
            ? T.autoNow(autoNow, auto.lowered && (playing.id !== ORIGINAL.id || eased) ? T.lowered[auto.lowered] : null)
            : T.autoHint
          : p.id === ORIGINAL.id
            ? hint
            : null,
      selected: p.id === this.quality.id,
    }));
    this.update({
      audio,
      subtitles,
      qualities,
      quality: auto ? T.automatic(autoNow) : this.quality.label,
      subtitleSync: syncable,
      subtitleOffsetMs: this.offset,
    });
  }

  /**
   * PUT /api/items/{id}/streams: the next episode, or the next viewing,
   * starts with the same tracks. Audio the server chose for this TV isn't
   * saved: another device may play another track of that language better.
   */
  private saveStreams(): void {
    const itemId = this.item?.id;
    if (itemId == null) return;
    const api = this.env.api;
    const fileId = this.file?.id ?? null;
    const audio = this.audioPicked ? this.audio?.index ?? null : undefined;
    const subtitle = this.subtitle?.index ?? null;
    this.env.reporter.send(() => api.setStreams(itemId, fileId, audio, subtitle));
    // The next episode's details, asked for before, name the tracks it
    // started with then: it is asked for again, after this choice is saved.
    this.choicesSaved++;
    this.nextDetail = null;
  }

  private update(patch: Partial<PlayerState>): void {
    this.state.value = { ...this.state.value, ...patch };
  }

  private setClock(patch: Partial<PlayerClock>): void {
    const c = this.clock.value;
    const next = { ...c, ...patch };
    if (next.positionMs !== c.positionMs || next.durationMs !== c.durationMs || next.bufferedMs !== c.bufferedMs) this.clock.value = next;
  }

  private phase(phase: Phase): void {
    if (this.state.value.phase.kind === "error") return;
    const was = this.state.value.phase;
    if (was.kind === phase.kind) return;
    batch(() => {
      this.update({ phase });
      if (phase.kind === "playing") this.presenter.resume();
    });
  }

  private notice(text: string): void {
    this.update({ notice: text });
  }

  private fail(message: string, canRetry: boolean, title?: string): void {
    this.update({ phase: { kind: "error", message, canRetry, title }, skipLabel: null });
  }

  private failWith(e: unknown): void {
    if (e instanceof SessionStoppedError) {
      this.stopByServer(e.note, true);
      return;
    }
    if (unreachable(e)) {
      this.fail(T.offlineBody, true, T.offlineTitle);
      return;
    }
    const canRetry = !(e instanceof SignedOutError) && !(e instanceof ApiError && (e.status === 403 || e.status === 404 || e.status === 401));
    this.fail(errorMessage(e), canRetry);
  }

  // ---------------------------------------------------------------- the server out of reach

  /**
   * The server stopped answering mid-playback, or its transcoder did (the
   * Android app's MediaLoading and ReconnectWatcher): the player keeps its
   * place, says "Reconnecting…" and asks again after 0.5 s, 1 s, then every
   * 2 s, for 90 s, as long as the server waits for a player it doesn't hear
   * from. No recovery is spent on it, and no stall makes it convert the
   * video. `away`: the wait before, which a new session ended too soon, goes on.
   */
  private reachLost(engineFailed: boolean, away: Outage | null = null): void {
    if (this.closed || this.stopped) return;
    if (this.outage) {
      if (engineFailed) this.outage.engineFailed = true;
      return;
    }
    this.outage = { since: away ? away.since : this.now(), attempts: away ? away.attempts : 0, timer: null, engineFailed, atMs: this.lastPositionMs };
    this.update({ reconnecting: true, skipLabel: null });
    if (this.state.value.phase.kind !== "error") this.phase({ kind: "buffering" });
    this.askAgainLater(this.outage);
  }

  private askAgainLater(outage: Outage): void {
    const wait = OUTAGE_STEPS_MS[Math.min(outage.attempts, OUTAGE_STEPS_MS.length - 1)]!;
    outage.attempts++;
    outage.timer = setTimeout(() => {
      outage.timer = null;
      if (this.outage === outage) this.askAgain(outage);
    }, wait);
  }

  private askAgain(outage: Outage): void {
    const info = this.info;
    const api = this.env.api;
    const asked = info && !this.sessionEnded ? api.sessionStatus(info.session_id, undefined, OUTAGE_ASK_MS) : api.reachable(OUTAGE_ASK_MS);
    asked.then(
      () => this.reachBack(outage, false),
      (e: unknown) => {
        if (this.outage !== outage || this.closed) return;
        if (e instanceof SessionStoppedError) {
          this.endOutage();
          this.stopByServer(e.note, true);
        } else if (!outOfReach(e)) {
          // It answers again, but the session went (404) or failed: a new one.
          this.reachBack(outage, true);
        } else if (this.now() - outage.since >= OUTAGE_MS) {
          this.endOutage();
          this.fail(T.offlineBody, true, T.offlineTitle);
        } else {
          this.askAgainLater(outage);
        }
      },
    );
  }

  /** The server answers again: the engine carries on if it came back by itself, else a new session starts at the place. */
  private reachBack(outage: Outage, sessionGone: boolean): void {
    if (this.outage !== outage || this.closed || this.stopped) return;
    const at = this.lastPositionMs;
    this.endOutage();
    const engine = this.engineRef;
    if (!sessionGone && !outage.engineFailed && engine && engine.playing() && engine.status() === "ready") {
      this.phase({ kind: "playing" });
      return;
    }
    this.restart(at, false, outage);
  }

  private endOutage(): void {
    const outage = this.outage;
    if (!outage) return;
    if (outage.timer !== null) clearTimeout(outage.timer);
    this.outage = null;
    // The stall clock starts over.
    this.lastAdvanceAt = this.now();
    this.update({ reconnecting: false });
  }
}

/**
 * Nothing answers, or the server says it can't play just now (`503
 * unavailable`: its transcoder restarts, say). Pages show the server's
 * words for such a 503; mid-playback it is waited out like the server.
 */
function outOfReach(e: unknown): boolean {
  return unreachable(e) || (isApiError(e, 503) && e.code === "unavailable");
}

/** Keyed playback URLs live under `/api/media/` (docs/API.md, "Keyed URLs"). */
export function isKeyed(url: string): boolean {
  return /^(https?:\/\/[^/]+)?\/api\/media\/[^/]+\//.test(url);
}

function videoStreamOf(file: MediaFile | null): MediaStream | null {
  const videos = (file?.streams ?? []).filter((s) => s.kind === "video");
  return videos.filter((s) => s.default)[0] ?? videos[0] ?? null;
}

/** An image address asked for at `width` pixels (the server buckets it). */
export function withWidth(url: string, width: number): string {
  return url + (url.indexOf("?") >= 0 ? "&" : "?") + `w=${width}`;
}
