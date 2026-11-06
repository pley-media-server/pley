import { useCallback, useEffect, useEffectEvent, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLocation, useNavigate } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import type HlsType from "hls.js";
import type { ErrorData, FragLoadedData, FragParsedData, InitPTSFoundData, LevelSwitchedData, ManifestParsedData } from "hls.js";
import {
  ArrowLeft,
  CircleStop,
  LoaderCircle,
  Maximize,
  Minimize,
  Pause,
  Play,
  ServerOff,
  SkipForward,
  TriangleAlert,
  Volume1,
  Volume2,
  VolumeX,
} from "lucide-react";
import { errorMessage, isApiError } from "@/api/client";
import { onServerEvent } from "@/api/events";
import { fetchNext, invalidateWatchState, keys, useTrickplay } from "@/api/hooks";
import { deleteSubtitle, trackStream } from "@/api/subtitles";
import type {
  Control,
  ItemDetail,
  ItemSummary,
  MediaFile,
  MediaStream,
  PlaybackInfo,
  PlaybackMode,
  PlaybackState,
  ServerEvent,
  SubtitleTrack,
} from "@/api/types";
import { useOptionalAudioPlayer } from "@/audio/AudioPlayer";
import { useAuth } from "@/auth";
import { Slider } from "@/components/Slider";
import { useToast } from "@/components/Toast";
import { canPlayNativeHls, videoCaps } from "@/lib/caps";
import { deviceFields } from "@/lib/castReceiver";
import { cn } from "@/lib/cn";
import { episodeLabel, fileResolution, formatBitrate, formatClock, streamLabel } from "@/lib/format";
import { loadHls } from "@/lib/hls";
import { playingHere } from "@/lib/playingHere";
import { warmStream } from "./prepare";
import { useDocumentTitle, useLocalStorage } from "@/lib/hooks";
import {
  endSession,
  reportProgress,
  saveStreamChoice,
  startPlayback,
  stopSession,
  stoppedStream,
  type StoppedBy,
} from "@/lib/progress";
import { playUrl } from "@/lib/usePlayItem";
import { nextInQueue, queuePlayUrl, type QueuePosition } from "@/lib/videoQueue";
import { AutoQuality, ORIGINAL_ID, storedMemory, type AutoMemory, type AutoStep } from "./autoQuality";
import { autoLadder, chooseQuality, modeFor, namedFor, ORIGINAL, qualitiesFor, qualityById, qualityLabel } from "./quality";
import { networkStart, reportCeiling } from "./network";
import { ChaptersMenu } from "./ChaptersMenu";
import type { SubtitleSources } from "./FindSubtitles";
import { SeekBar } from "./SeekBar";
import { methodLabel, PlaybackDetails } from "./PlaybackDetails";
import { SettingsPopover } from "./SettingsPopover";
import { speedLabel, stepSpeed } from "./speed";
import { fileMarkers, SkipButton, skipTarget, useSkipMarkers } from "./SkipButton";
import { carriedSleepTimer, sleepChoice, SleepNotice, useSleepTimer } from "./sleepTimer";
import { SubtitleOverlay } from "./SubtitleOverlay";
import { PictureSubtitleOverlay, usePictureTrack, type PictureTrackHooks } from "./PictureSubtitleOverlay";
import { hlsLeadMs, hlsShiftMs, picturePath } from "./pictures";
import {
  clampSubtitleOffset,
  cueAtTop,
  cuesAt,
  DEFAULT_SUBTITLE_STYLE,
  nextCueChange,
  parseVtt,
  storedSubtitleOffset,
  storeSubtitleOffset,
  SUBTITLE_BOTTOM,
  subtitleLift,
  subtitleStyle,
  type SubtitleCue,
  type SubtitleStyle,
} from "./subtitles";
import { QualityNotice, type QualityNoticeText } from "./QualityNotice";
import { UpNext } from "./UpNext";
import { itemUrl } from "@/lib/urls";

const PROGRESS_INTERVAL = 10_000;
/** While paused: the server forgets players it hasn't heard from for 90 s. */
const PAUSED_HEARTBEAT = 30_000;
const HIDE_CONTROLS_AFTER = 3_000;
/**
 * The remote's commands a video obeys: `next` only where something can come
 * next, `audio` where its own menu offers Audio (more than one track), and
 * `subtitles` and `quality` always, as its menu offers Subtitles (a track
 * found or added while it plays can be picked too) and Quality.
 */
function videoControls(next: boolean, audioTracks: number): Control[] {
  const controls: Control[] = ["play", "pause", "seek", "skip"];
  if (next) controls.push("next");
  if (audioTracks > 1) controls.push("audio");
  controls.push("subtitles", "quality");
  return controls;
}

type WebkitVideo = HTMLVideoElement & { webkitEnterFullscreen?: () => void };

/**
 * Full screen is the video element's own player only (the iPhone's
 * Safari): it shows the video alone, so nothing pley draws over it shows.
 */
function videoOnlyFullscreen(): boolean {
  return typeof document.documentElement.requestFullscreen !== "function" && "webkitEnterFullscreen" in HTMLVideoElement.prototype;
}

function rangesOf(tr: TimeRanges): [number, number][] {
  const out: [number, number][] = [];
  for (let i = 0; i < tr.length; i++) out.push([tr.start(i), tr.end(i)]);
  return out;
}

/**
 * Where hls.js starts. A stream's first frame comes a little after 0 (0.08 s
 * for a remux, 0.13 s for a transcode with B-frames): from 0, the playhead
 * would sit before it, on a frozen picture, until hls.js's gap handling
 * moved it on. Just past 0, hls.js seeks to the first frame itself, as the
 * gap is within its tolerance (0.25 s).
 */
function hlsStartPosition(startSec: number): number {
  return startSec > 0 ? startSec : 0.001;
}

/**
 * Of a stream's sizes (hls.js's levels, smallest first, a size listed twice
 * at its lower bitrate first), the one to start at for a player shown
 * `video`'s size: the smallest that fills it (as hls.js's
 * `capLevelToPlayerSize` measures it) at its higher bitrate, no bigger
 * than `first`, the largest the server lists.
 */
export function startSize(
  levels: readonly { width: number; height: number }[],
  video: { clientWidth: number; clientHeight: number },
  first: number,
): number {
  const scale = window.devicePixelRatio || 1;
  const [w, h] = [video.clientWidth * scale, video.clientHeight * scale];
  // Not laid out yet: no size to go by.
  if (!w || !h) return first;
  let fills = levels.findIndex((l) => l.width >= w || l.height >= h);
  if (fills < 0) return first;
  const same = (a: { width: number; height: number }, b: { width: number; height: number }) =>
    a.width === b.width && a.height === b.height;
  while (fills + 1 < levels.length && same(levels[fills + 1]!, levels[fills]!)) fills += 1;
  return Math.min(fills, first);
}

export interface VideoPlayerProps {
  item: ItemDetail;
  startMs: number;
  fileId: number | null;
  audio: number | null;
  sub: number | null;
  /** Playing through a playlist or collection: "next" is its next entry. */
  queue?: QueuePosition | null;
}

/**
 * How long a wait for media lasts before the spinner shows: a seek within
 * what's loaded takes a few dozen milliseconds, and would flash it.
 */
const SPINNER_DELAY_MS = 400;
/** The same while the last session's picture stands in for the next one's (`freezeFrame`). */
const FROZEN_SPINNER_DELAY_MS = 2_500;

/** How long a session prepared alongside the one playing may take to get ready before the player moves to it anyway. */
const PREPARE_MAX_MS = 20_000;
/** How far past where it starts a prepared session's first segment is asked for: where the player will be by then. */
const PREPARE_LEAD_S = 4;

/** Seconds kept of what is buffered when the player grows (`capToPlayer`): the bigger size starts meanwhile. */
const UPSIZE_KEEP_S = 15;

/** How long playback may stand still, wanting to play, before it counts as stalled. */
const STALL_MS = 12_000;
const STALL_CHECK_MS = 2_000;
/** Seconds of playback after which recoveries start over: the trouble is over. */
const RECOVERED_AFTER_S = 5;

/** How often subtitles still being read out of their file are asked for again. */
const SUBTITLES_AGAIN_MS = 3_000;

/** Whether `after` starts with the cues of `before` (subtitles read further). */
function sameStart(before: VTTCue[], after: VTTCue[]): boolean {
  if (after.length < before.length) return false;
  const last = before.length - 1;
  return last < 0 || (after[last]?.startTime === before[last]?.startTime && after[last]?.text === before[last]?.text);
}

/**
 * A copy of `cue` shown `shift` seconds later (positive: later), placed the
 * same; one at the bottom raised `lift` % of the height when asked
 * (`SUBTITLE_POSITIONS`).
 */
function movedCue(cue: VTTCue, shift: number, lift: number): VTTCue {
  const moved = new VTTCue(Math.max(0, cue.startTime + shift), Math.max(0, cue.endTime + shift), cue.text);
  moved.line = cue.line;
  moved.lineAlign = cue.lineAlign;
  moved.position = cue.position;
  moved.positionAlign = cue.positionAlign;
  moved.size = cue.size;
  moved.align = cue.align;
  moved.snapToLines = cue.snapToLines;
  moved.vertical = cue.vertical;
  if (lift > 0 && !cueAtTop(cue)) {
    moved.snapToLines = false;
    moved.line = 100 - SUBTITLE_BOTTOM - lift;
    moved.lineAlign = "end";
  }
  return moved;
}

/** The text track pley gives subtitle cues to, one per video element. */
const pleyTracks = new WeakMap<HTMLVideoElement, TextTrack>();
function pleyTrack(video: HTMLVideoElement): TextTrack {
  let track = pleyTracks.get(video);
  if (!track) {
    track = video.addTextTrack("subtitles", "Pley");
    pleyTracks.set(video, track);
  }
  return track;
}

/** Seconds of media buffered past the playhead. */
function bufferedAhead(v: HTMLMediaElement): number {
  for (let i = 0; i < v.buffered.length; i++) {
    if (v.buffered.start(i) <= v.currentTime + 0.25 && v.currentTime < v.buffered.end(i)) return v.buffered.end(i) - v.currentTime;
  }
  return 0;
}

/** How often Automatic looks at what has loaded (autoQuality.ts). */
const AUTO_SAMPLE_MS = 1_000;

/**
 * Automatic's memory of a slow network: this browser's, for the server it
 * talks to (the page's origin), for 30 minutes.
 */
const autoMemory: AutoMemory = storedMemory(
  {
    get: (key) => window.localStorage.getItem(key),
    set: (key, value) => window.localStorage.setItem(key, value),
    remove: (key) => window.localStorage.removeItem(key),
  },
  "pley.autoQuality",
);

/**
 * Automatic for `file`, not started: it starts at the original or at the
 * network remembered, once the server has said what it knows of this
 * network (`networkStart`). Its ceilings are told to the server too.
 * `afresh`: picked again in the menu, it starts at the original at once,
 * nothing asked.
 */
function newAuto(file: MediaFile, afresh: boolean): AutoQuality {
  const auto = new AutoQuality({
    ladder: autoLadder(file),
    originalKbps: file.bitrate ? file.bitrate / 1000 : null,
    memory: autoMemory,
    onCeiling: reportCeiling,
  });
  if (afresh) auto.start(performance.now(), null);
  return auto;
}

export function VideoPlayer({ item, startMs, fileId, audio: initialAudio, sub: initialSub, queue }: VideoPlayerProps) {
  const { t } = useTranslation(["player", "common"]);
  const navigate = useNavigate();
  const qc = useQueryClient();
  const toast = useToast();
  const audioPlayer = useOptionalAudioPlayer();

  const containerRef = useRef<HTMLDivElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const hlsRef = useRef<HlsType | null>(null);
  const sessionRef = useRef<PlaybackInfo | null>(null);
  const startMsRef = useRef(startMs);
  /** Last known playhead (s). Kept while a session (re)starts and the element sits at 0. */
  const positionRef = useRef(startMs / 1000);
  /** Mode used instead of the preference after direct play failed to decode (this player only). */
  const fallbackModeRef = useRef<PlaybackMode | null>(null);
  /**
   * Recoveries tried: `network`/`media` per session; `recreate` (new
   * sessions) and `stuck` (new sessions for media that arrives but doesn't
   * play) across them. All start over once playback moves on again.
   */
  const recoveries = useRef({ network: 0, media: 0, recreate: 0, stuck: 0 });
  /** Where playback was when it last (re)started showing media: moving on from there resets `recoveries`. */
  const playedFrom = useRef<number | null>(null);
  /**
   * The element shows the session's media (not the reset element's 0 while
   * a session starts or hls.js reattaches): only then is its time the playhead.
   */
  const mediaShown = useRef(false);
  /** POST /api/playback of the previous run, which ends its own session if that run was cancelled. */
  const pendingStartRef = useRef<Promise<void>>(Promise.resolve());
  /** A new session or a seek: waits for data don't count for Automatic until it plays again. */
  const autoSettling = useRef(true);
  /** What Automatic last said over the picture. */
  const [qualityNotice, setQualityNotice] = useState<QualityNoticeText | null>(null);
  const say = (text: string) => setQualityNotice((old) => ({ text, n: (old?.n ?? 0) + 1 }));
  /** "Quality lowered to 8 Mbps 1080p". */
  const smootherText = (label: string) => t("video.qualityLowered", { quality: label });
  /** A remote's skip, by the offset it asked for: "+30 s", "−10 s". */
  const skipText = (offsetMs: number) => {
    const seconds = Math.round(Math.abs(offsetMs) / 1000);
    return offsetMs < 0 ? t("video.skippedBack", { seconds }) : t("video.skippedAhead", { seconds });
  };
  /** Other subtitles, as C and the remote say them: "Subtitles: English (SRT)", "Subtitles off". */
  const subtitlesText = (track: MediaStream | null) =>
    track ? t("video.subtitlesShown", { track: streamLabel(track) }) : t("video.subtitlesOff");
  /** Automatic's original fell back to a conversion: the next session says at what. */
  const announceFallback = useRef(false);
  /** The lines of the size a stream in several sizes plays at now (hls.js picks it). */
  const [levelLines, setLevelLines] = useState<number | null>(null);
  /** The next session is a quality, audio or Automatic change: it is prepared while this one plays. */
  const prepareNext = useRef(false);
  /** The last picture of the session left, shown until the next one shows its own. */
  const freezeRef = useRef<HTMLCanvasElement>(null);
  const [frozen, setFrozen] = useState(false);
  const frozenNow = useRef(false);
  const unfreeze = () => {
    frozenNow.current = false;
    setFrozen(false);
  };
  const freezeFrame = () => {
    const c = freezeRef.current;
    const v = videoRef.current;
    if (!c || !v || !v.videoWidth) return;
    c.width = v.videoWidth;
    c.height = v.videoHeight;
    try {
      c.getContext("2d")?.drawImage(v, 0, 0, c.width, c.height);
      frozenNow.current = true;
      setFrozen(true);
    } catch {
      // Nothing to show: the element shows black meanwhile, as before.
    }
  };

  const file = item.files.find((f) => f.id === fileId) ?? item.files[0] ?? null;
  const audioStreams = useMemo(() => file?.streams.filter((s) => s.kind === "audio") ?? [], [file]);
  // Subtitles added while it plays (found, or a file uploaded) are listed at
  // once, and those deleted go, before the item says so.
  const [addedTracks, setAddedTracks] = useState<SubtitleTrack[]>([]);
  const [deletedTracks, setDeletedTracks] = useState<number[]>([]);
  /** Tracks added from this player: whoever added them may delete them. */
  const [addedHere, setAddedHere] = useState<number[]>([]);
  const subStreams = useMemo(() => {
    const own = file?.streams.filter((s) => s.kind === "subtitle" && !deletedTracks.includes(s.index)) ?? [];
    return own.concat(addedTracks.filter((t) => !own.some((s) => s.index === t.index)).map(trackStream));
  }, [file, addedTracks, deletedTracks]);

  // Explicit tracks from the URL win; otherwise the remembered choice for the show / movie.
  // The audio is left to the server unless picked: of the chosen language, it plays the
  // track this browser plays best, and the session says which (`info.audio_index`).
  const trickplay = useTrickplay(file?.id);
  const [audioIndex, setAudioIndex] = useState<number | null>(initialAudio ?? null);
  const [subIndex, setSubIndex] = useState<number | null>(initialSub ?? file?.selected_subtitle ?? null);
  // Only a quality picked is stored: until one is, Automatic plays.
  const [quality, setQuality] = useLocalStorage<string>("pley.quality", "auto");
  // One menu: the original plays the best way the browser can, Automatic
  // plays it unless it doesn't play well, every other quality is a
  // transcode. A remembered quality this file doesn't need (an 8 Mbps cap
  // on a 5 Mbps file) plays the original.
  const qualities = useMemo(() => qualitiesFor(file), [file]);
  const selectedQuality = chooseQuality(quality, qualities);
  // Automatic, when chosen: one for each file (Up Next's starts where the
  // network last allowed), and a fresh one when picked again.
  const [autoRun, setAutoRun] = useState(0);
  const [, setAutoStepped] = useState(0);
  const auto = useMemo(
    () => (selectedQuality.id === "auto" && file ? newAuto(file, autoRun > 0) : null),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [file?.id, selectedQuality.id, autoRun],
  );
  // Automatic starts once the server has said what it knows of this network
  // (asked as the player's page loaded, alongside the item), after a speed
  // test when it asks for one; hls.js loads meanwhile.
  const autoReady = !auto || auto.started;
  useEffect(() => {
    if (!auto || auto.started) return;
    let gone = false;
    loadHls().catch(() => undefined);
    void networkStart().then((known) => {
      if (gone || auto.started) return;
      auto.start(performance.now(), known);
      setAutoStepped((n) => n + 1);
    });
    return () => {
      gone = true;
    };
  }, [auto]);
  const playingQuality = auto ? namedFor(file, qualityById(auto.playing)) : selectedQuality;
  const mode = modeFor(playingQuality);
  const [loadToken, setLoadToken] = useState(0);

  const selectedSub = subStreams.find((s) => s.index === subIndex) ?? null;
  /**
   * Picture subtitles (PGS, VobSub, DVB) are drawn here, over the video,
   * from their own file (`.mks`), asked for as soon as they are chosen (the
   * server starts reading it out of the video then) and again while the
   * answer is part of it; choosing another starts no new session. They are
   * burned into the video, as before, only when the server can't make such
   * files: from then on, for every picture track.
   */
  const pictureHooks = useRef<PictureTrackHooks>({
    unavailable: () => undefined,
    failed: () => undefined,
    position: () => Math.round(positionRef.current * 1000),
  });
  const pictureChosen = selectedSub && selectedSub.text_based === false ? selectedSub : null;
  const pictures = usePictureTrack(pictureChosen && file ? picturePath(file.id, pictureChosen.index) : null, pictureHooks);
  // A track still being read is asked for again from where the player went.
  const picturesSought = useEffectEvent(() => pictures.sought());
  const burnIndex = pictureChosen && pictures.unavailable ? pictureChosen.index : null;
  const drawnPictureIndex = pictureChosen && !pictures.unavailable ? pictureChosen.index : null;
  const shownTextIndex = selectedSub && selectedSub.text_based !== false ? selectedSub.index : null;

  const [info, setInfo] = useState<PlaybackInfo | null>(null);
  /**
   * How hls.js's time relates to the file's in a session, for picture
   * subtitles (timed as the file is; pictures.ts, `hlsShiftMs`): its
   * `initPTS` (ms), and how far its first picture runs ahead of its place
   * in the playlist (`hlsLeadMs`), for a server that doesn't say
   * `pts_offset_ms`.
   */
  const [hlsTiming, setHlsTiming] = useState<{ session: string; initPtsMs: number | null; leadMs: number | null } | null>(null);
  const [fellBack, setFellBack] = useState(false);
  /** The fallback to a transcode came from a stall rather than an error. */
  const [stalled, setStalled] = useState(false);
  /** When playback last moved (and where to), and when media data last arrived (see `onStall`). */
  const lastAdvance = useRef({ at: 0, time: -1 });
  const lastData = useRef(0);
  const [loading, setLoading] = useState(true);
  /** Waiting for media long enough to say so (see `SPINNER_DELAY_MS`). */
  const [buffering, setBuffering] = useState(false);
  const bufferingTimer = useRef<number | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  /** The error is the server answering nothing: "Can't reach your server", as the apps say. */
  const [unreachable, setUnreachable] = useState(false);
  const failUnreachable = () => {
    setUnreachable(true);
    // What to check when the server answers nothing (the Android apps say the same).
    setError(t("video.reachHint"));
  };
  const [needsGesture, setNeedsGesture] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [current, setCurrent] = useState(startMs / 1000);
  const [mediaDuration, setMediaDuration] = useState(0);
  const [buffered, setBuffered] = useState<[number, number][]>([]);
  const [volume, setVolume] = useLocalStorage<number>("pley.volume", 1);
  const [muted, setMuted] = useLocalStorage<boolean>("pley.muted", false);
  const [fullscreen, setFullscreen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [methodHover, setMethodHover] = useState(false);
  // How subtitles look, chosen on each device (as Plex keeps it per player).
  const [storedSubtitleStyle, setSubtitleStyle] = useLocalStorage<SubtitleStyle>("pley.subtitleStyle", DEFAULT_SUBTITLE_STYLE);
  const subStyle = subtitleStyle(storedSubtitleStyle);
  /** The subtitles showing now (drawn by SubtitleOverlay). */
  const [cues, setCues] = useState<SubtitleCue[]>([]);
  /** The video in the iPhone's own fullscreen player, which only draws subtitles itself. */
  const [nativeFullscreen, setNativeFullscreen] = useState(false);
  const [chaptersOpen, setChaptersOpen] = useState(false);
  const [active, setActive] = useState(true);
  const [hoveringControls, setHoveringControls] = useState(false);
  /**
   * Paused from the user's remote, on another device: the controls don't
   * come up for it as they do for a pause here, since nobody here asked
   * for them. Over once it plays again, or someone here moves the mouse,
   * taps or presses a key.
   */
  const [remotePaused, setRemotePaused] = useState(false);
  const [ended, setEnded] = useState(false);
  const [upNext, setUpNext] = useState<ItemSummary | null>(null);
  /** Ended on the server: nothing plays any more, and it says why. */
  const [stopped, setStopped] = useState<StoppedBy | null>(null);
  const stoppedRef = useRef(false);
  // Subtitles moved earlier or later (text, and pictures drawn here), remembered for each profile and file on this device.
  const me = useAuth();
  const userId = me.id;
  const [offsetChoice, setOffsetChoice] = useState(() => ({
    file: file?.id ?? null,
    offset: storedSubtitleOffset(userId, file?.id),
  }));
  const subtitleOffset =
    offsetChoice.file === (file?.id ?? null) ? offsetChoice.offset : storedSubtitleOffset(userId, file?.id);

  const duration = (info?.duration_ms ?? item.duration_ms ?? 0) / 1000 || mediaDuration;
  const isEpisode = item.kind === "episode";
  // The next episode, or the queue's next entry.
  const hasNext = isEpisode || !!queue;
  const findNext = () => (queue ? nextInQueue(qc, queue) : fetchNext(qc, item.id));
  const nextUrl = (next: ItemSummary) => {
    const at = next.user_state.watched ? 0 : next.user_state.position_ms;
    return queue ? queuePlayUrl(queue.queue, queue.index + 1, next.id, at) : playUrl(next.id, { startMs: at });
  };
  // Bonus material belongs to its movie or show.
  const home = itemUrl(item.kind === "extra" && item.parent_id ? item.parent_id : item.id);
  const endUrl = queue?.queue.returnTo ?? home;
  const title = isEpisode ? (item.grandparent_title ?? item.title) : item.year ? `${item.title} (${item.year})` : item.title;
  const subtitle = isEpisode ? [episodeLabel(item), item.title].filter(Boolean).join(" · ") : null;
  useDocumentTitle(isEpisode ? `${title} ${episodeLabel(item)}` : item.title);

  // The sleep timer starts off, but goes on to the next episode while it runs.
  const location = useLocation();
  const [carriedSleep] = useState(() => carriedSleepTimer(location.state));
  const sleep = useSleepTimer({
    initial: carriedSleep,
    mediaLeft: duration > 0 ? duration - current : Number.POSITIVE_INFINITY,
    onSleep: () => {
      videoRef.current?.pause();
      // Asleep: the next episode doesn't start on its own either.
      setUpNext(null);
    },
  });
  const sleepEndLabel = isEpisode
    ? t("video.sleepEnd.episode")
    : item.kind === "movie"
      ? t("video.sleepEnd.movie")
      : t("video.sleepEnd.video");

  const destroyHls = () => {
    hlsRef.current?.destroy();
    hlsRef.current = null;
  };

  /** The playhead in seconds: the element's time once it shows the session's media, else the last known one. */
  const livePosition = () => {
    const v = videoRef.current;
    return v && sessionRef.current && mediaShown.current && v.readyState >= HTMLMediaElement.HAVE_CURRENT_DATA
      ? v.currentTime
      : positionRef.current;
  };
  const currentMs = () => Math.round(livePosition() * 1000);

  const restartAt = useCallback((seconds: number) => {
    startMsRef.current = Math.max(0, Math.round(seconds * 1000));
    setLoadToken((t) => t + 1);
  }, []);

  /** Set once mounted: a progress report that answers 410 stops playback. */
  const progressGone = useRef<(sessionId: string) => void>(() => undefined);
  /**
   * What session `s` plays, which each of its reports says for the user's
   * remotes: its audio track, and the subtitles shown now (-1: none).
   */
  const shownSubtitleIndex = selectedSub?.index ?? -1;
  const shownSubtitle = useRef(shownSubtitleIndex);
  useLayoutEffect(() => {
    shownSubtitle.current = shownSubtitleIndex;
  }, [shownSubtitleIndex]);
  // …and its Quality menu's pick.
  const pickedQuality = useRef(selectedQuality.id);
  useLayoutEffect(() => {
    pickedQuality.current = selectedQuality.id;
  }, [selectedQuality.id]);
  const tracksOf = useCallback(
    (s: PlaybackInfo) => ({
      audio_index: s.audio_index ?? undefined,
      subtitle_index: shownSubtitle.current,
      quality: pickedQuality.current,
    }),
    [],
  );
  const report = useCallback(
    (state: PlaybackState, keepalive = false) => {
      const s = sessionRef.current;
      if (!s) return;
      void reportProgress(
        item.id,
        {
          position_ms: Math.round(positionRef.current * 1000),
          duration_ms: s.duration_ms || undefined,
          state,
          session_id: s.session_id,
          ...tracksOf(s),
        },
        keepalive,
        () => progressGone.current(s.session_id),
      );
    },
    [item.id, tracksOf],
  );

  const goBack = useCallback(() => {
    const idx = (window.history.state as { idx?: number } | null)?.idx ?? 0;
    if (idx > 0) void navigate(-1);
    else void navigate(home, { replace: true });
  }, [navigate, home]);

  /**
   * The server ended this session (an administrator, or the user elsewhere):
   * let go of the stream without starting another, and say so. Called again
   * with more to say (the event after a 410), it only adds that.
   */
  const stopPlayback = useEffectEvent((by: StoppedBy) => {
    setStopped((old) => ({ message: by.message ?? old?.message ?? null, byAdmin: by.byAdmin ?? old?.byAdmin ?? null }));
    if (stoppedRef.current) return;
    stoppedRef.current = true;
    // Nothing to release: the server has ended it.
    sessionRef.current = null;
    destroyHls();
    const v = videoRef.current;
    if (v) {
      v.pause();
      v.removeAttribute("src");
      v.load();
    }
    window.clearTimeout(bufferingTimer.current);
    bufferingTimer.current = undefined;
    setBuffering(false);
    setLoading(false);
    unfreeze();
    setPlaying(false);
    setUpNext(null);
    sleep.clear();
    if (document.fullscreenElement) void document.exitFullscreen().catch(() => undefined);
    invalidateWatchState(qc);
  });

  const onEvent = useEffectEvent((ev: ServerEvent) => {
    if (ev.type !== "session_stopped") return;
    const ours = sessionRef.current?.session_id ?? info?.session_id;
    if (ev.session_id === ours) stopPlayback({ message: ev.message, byAdmin: ev.by_admin });
  });
  useEffect(() => onServerEvent(onEvent), []);

  // The remote leaves this tab's own session out.
  const sessionHere = stopped ? null : (info?.session_id ?? null);
  useEffect(() => (sessionHere ? playingHere(sessionHere) : undefined), [sessionHere]);

  /** A progress report answered 410: stopped on the server, heard without the event stream. */
  const onProgressGone = useEffectEvent((sessionId: string) => {
    if (sessionRef.current?.session_id === sessionId) stopPlayback({ message: null, byAdmin: null });
  });
  useEffect(() => {
    progressGone.current = onProgressGone;
    return () => {
      progressGone.current = () => undefined;
    };
  }, []);

  /** The stream answered 410: stopped on the server. Its message comes from asking the stream once more. */
  const streamGone = useEffectEvent((url: string) => {
    stopPlayback({ message: null, byAdmin: null });
    void stoppedStream(url).then((by) => {
      if (by) stopPlayback(by);
    });
  });

  // Pause any music when a video starts.
  useEffect(() => {
    audioPlayer?.pause();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** A session of Automatic's began: what it measured of the one before goes. */
  const autoBegan = useEffectEvent((pinfo: PlaybackInfo) => {
    const announce = announceFallback.current;
    announceFallback.current = false;
    if (!auto) return;
    auto.began();
    // The original converted here (a fallback): Automatic steps down from what it plays at.
    const converted = auto.playing === ORIGINAL_ID && pinfo.auto_quality ? namedFor(file, qualityById(pinfo.auto_quality)) : null;
    if (converted?.maxBitrate) {
      auto.setOriginalKbps(converted.maxBitrate);
      if (announce) say(smootherText(converted.label));
    }
  });

  /** A segment of `bytes` took `ms` to load: the network's speed, for Automatic. */
  const autoLoaded = useEffectEvent((bytes: number, ms: number) => {
    auto?.loaded(performance.now(), bytes, ms);
  });

  /**
   * Automatic plays no size bigger than the player shows. hls.js's own
   * `capLevelToPlayerSize` empties all it buffered ahead when the player
   * grows (going full screen), and the transcoder then starts the bigger
   * size from cold, with a wait. Here what is buffered past the next
   * `UPSIZE_KEEP_S` goes, fetched again at the bigger size while those
   * seconds play.
   */
  const capToPlayer = useEffectEvent(() => {
    const hls = hlsRef.current;
    const v = videoRef.current;
    if (!hls || !v || !hls.levels.length) return;
    const cap = auto ? startSize(hls.levels, v, hls.levels.length - 1) : -1;
    const before = hls.autoLevelCapping;
    if (cap === before) return;
    hls.autoLevelCapping = cap;
    const bigger = before >= 0 && (cap < 0 || cap > before);
    if (bigger && bufferedAhead(v) > UPSIZE_KEEP_S + 5) {
      const Hls = hls.constructor as typeof HlsType;
      hls.trigger(Hls.Events.BUFFER_FLUSHING, { startOffset: v.currentTime + UPSIZE_KEEP_S, endOffset: Number.POSITIVE_INFINITY, type: null });
    }
  });
  useEffect(() => {
    const v = videoRef.current;
    if (!v || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => capToPlayer());
    observer.observe(v);
    return () => observer.disconnect();
  }, []);

  // ---------------------------------------------------------------------
  // Session lifecycle: (re)start whenever the stream parameters change.

  // ---------------------------------------------------------------------
  useEffect(() => {
    const video = videoRef.current;
    // Stopped on the server: nothing starts again here. Automatic waits to know where it starts.
    if (!video || !file || stoppedRef.current || !autoReady) return;
    let cancelled = false;
    const q = playingQuality;
    let startSec = startMsRef.current / 1000;
    const requestMode = fallbackModeRef.current ?? mode;
    // A quality, audio or Automatic change while it plays (`prepareNext`):
    // the new session is made ready while this one plays on, and the
    // player moves to it on this one's last picture, without a spinner.
    let prepared = prepareNext.current && sessionRef.current !== null && mediaShown.current && !error;
    prepareNext.current = false;
    const previousStart = pendingStartRef.current;

    /**
     * Lets go of the session playing, reporting the last real position (not
     * the reset element's 0), and resets the element for one starting at
     * `at`. The next session doesn't wait for that: the server ends this
     * tab's sessions itself before it starts another (and before it checks
     * its transcode limit), unless it was prepared alongside. It does wait
     * for the previous run's start, which ends its own session if that run
     * was cancelled.
     */
    const release = (at: number) => {
      recoveries.current.network = 0;
      recoveries.current.media = 0;
      playedFrom.current = null;
      mediaShown.current = false;
      autoSettling.current = true;
      setLevelLines(null);
      // A new session gets the full grace before it counts as stalled.
      lastAdvance.current = { at: performance.now(), time: -1 };
      lastData.current = performance.now();
      if (!prepared) {
        setLoading(true);
        unfreeze();
      }
      setError(null);
      setUnreachable(false);
      setNeedsGesture(false);
      setEnded(false);
      window.clearTimeout(bufferingTimer.current);
      bufferingTimer.current = undefined;
      setBuffering(false);
      const prev = sessionRef.current;
      sessionRef.current = null;
      if (prev) void endSession(item.id, prev, positionRef.current * 1000, false, tracksOf(prev));
      destroyHls();
      video.pause();
      video.removeAttribute("src");
      video.load();
      positionRef.current = at;
      setCurrent(at);
    };
    if (!prepared) release(startSec);

    const seekOnLoad = (seconds: number) => {
      if (seconds <= 0) return;
      const onMeta = () => {
        video.currentTime = seconds;
      };
      video.addEventListener("loadedmetadata", onMeta, { once: true });
    };

    // Most plays end up on hls.js: fetch it while the server prepares the stream.
    loadHls().catch(() => undefined);

    /** A prepared session's stream, asked for as the player will ask once it moves there. */
    const warm = (pinfo: PlaybackInfo): Promise<void> => {
      if (!pinfo.hls) return Promise.resolve();
      const abort = new AbortController();
      const timer = window.setTimeout(() => abort.abort(), PREPARE_MAX_MS);
      const pick = (sizes: { width: number; height: number }[], listed: number) => (auto ? startSize(sizes, video, listed) : listed);
      return warmStream(pinfo.url, startSec + PREPARE_LEAD_S, pick, abort.signal)
        .catch(() => undefined)
        .finally(() => window.clearTimeout(timer));
    };

    // The new session is recorded as soon as it plays, so the next run (or
    // unmount) ends it; a run cancelled before that ends it here instead.
    const acquired = (async (): Promise<PlaybackInfo | null> => {
      await previousStart;
      if (cancelled) return null;
      const start = (alongside: boolean) =>
        startPlayback({
          item_id: item.id,
          file_id: file.id,
          audio_index: audioIndex,
          subtitle_index: burnIndex,
          max_bitrate: q.maxBitrate,
          max_height: q.maxHeight,
          start_ms: Math.round(startSec * 1000),
          mode: requestMode,
          caps: videoCaps(),
          controls: videoControls(hasNext, audioStreams.length),
          ...deviceFields(),
          ...(alongside ? { keep_previous: true } : {}),
        });
      let pinfo: PlaybackInfo;
      try {
        pinfo = await start(prepared);
      } catch (err) {
        // No transcode free for two at once: the last one ends first, as before.
        if (!prepared || !isApiError(err, 503) || cancelled) throw err;
        prepared = false;
        startSec = livePosition();
        release(startSec);
        pinfo = await start(false);
      }
      if (prepared && !cancelled && !stoppedRef.current) await warm(pinfo);
      if (cancelled || stoppedRef.current) {
        await stopSession(pinfo.session_id);
        return null;
      }
      if (prepared) {
        // Where the last one got to while this one got ready.
        freezeFrame();
        startSec = livePosition();
        release(startSec);
      }
      sessionRef.current = pinfo;
      return pinfo;
    })();
    pendingStartRef.current = acquired.then(
      () => undefined,
      () => undefined,
    );

    void (async () => {
      try {
        const pinfo = await acquired;
        if (!pinfo || cancelled) return;
        setInfo(pinfo);
        setFellBack(requestMode !== mode);
        autoBegan(pinfo);

        if (pinfo.hls) {
          const hasMse = typeof window.MediaSource !== "undefined" || "ManagedMediaSource" in window;
          if (!hasMse && canPlayNativeHls(video)) {
            video.src = pinfo.url;
            seekOnLoad(startSec);
          } else {
            const Hls = await loadHls();
            if (cancelled) return;
            if (!Hls.isSupported()) {
              if (!canPlayNativeHls(video)) throw new Error(t("video.errors.noHls"));
              video.src = pinfo.url;
              seekOnLoad(startSec);
            } else {
              const hls = new Hls({
                startPosition: hlsStartPosition(startSec),
                // Started once the playlist is read, at the size chosen then.
                autoStartLoad: false,
                // The first segment is asked for as soon as the playlist is
                // in, not once the element's MediaSource has opened (~0.1 s later).
                startFragPrefetch: true,
                // A stream in several sizes (`pinfo.adaptive`): Automatic
                // takes none bigger than the player shows (`capToPlayer`); a
                // quality picked plays at its size whenever the connection allows.
                capLevelToPlayerSize: false,
                // Up a size only with room to spare, so it doesn't swing
                // between two on a network near the line (docs/QUALITY.md).
                abrBandWidthUpFactor: 0.5,
                maxBufferLength: 30,
                maxMaxBufferLength: 120,
                backBufferLength: 60,
                enableWorker: true,
                enableCEA708Captions: false,
                fragLoadingMaxRetry: 6,
                manifestLoadingMaxRetry: 2,
                levelLoadingMaxRetry: 2,
              });
              hlsRef.current = hls;
              hls.on(Hls.Events.ERROR, (_evt, data: ErrorData) => {
                if (!data.fatal || cancelled) return;
                const r = recoveries.current;
                if (data.type === Hls.ErrorTypes.NETWORK_ERROR) {
                  const code = data.response?.code;
                  if (code === 410) {
                    // Stopped on the server: another session would only be stopped again.
                    streamGone(pinfo.url);
                    return;
                  }
                  const playlist =
                    data.details === Hls.ErrorDetails.MANIFEST_LOAD_ERROR ||
                    data.details === Hls.ErrorDetails.MANIFEST_LOAD_TIMEOUT ||
                    data.details === Hls.ErrorDetails.MANIFEST_PARSING_ERROR;
                  if (code === 404 || playlist) {
                    // The session expired (idle timeout, server restart) or its
                    // playlist can't be loaded: only a new session helps.
                    if (r.recreate < 3) {
                      r.recreate += 1;
                      restartAt(livePosition());
                      return;
                    }
                  } else if (r.network < 4) {
                    r.network += 1;
                    // Without a position hls.js would restart from the beginning.
                    hls.startLoad(livePosition());
                    return;
                  }
                } else if (data.type === Hls.ErrorTypes.MEDIA_ERROR) {
                  if (r.media < 2) {
                    r.media += 1;
                    hls.recoverMediaError();
                    return;
                  }
                }
                // No answer at all, after the retries: the server is out of reach.
                if (data.type === Hls.ErrorTypes.NETWORK_ERROR && !data.response?.code) failUnreachable();
                else setError(t("video.errors.failed", { details: data.details }));
              });
              hls.on(Hls.Events.MANIFEST_PARSED, (_evt, data: ManifestParsedData) => {
                // The size to start at, rather than hls.js's guess from a
                // bandwidth it hasn't measured: the first the server lists
                // (the one it started making), or for Automatic the smallest
                // that fills the player, as it then keeps to.
                hls.startLevel = auto ? startSize(data.levels, video, data.firstLevel) : data.firstLevel;
                capToPlayer();
                hls.startLoad(hlsStartPosition(startSec));
              });
              hls.on(Hls.Events.LEVEL_SWITCHED, (_evt, data: LevelSwitchedData) => {
                const level = hls.levels[data.level];
                // Lines as a 16:9 picture's: a 1920×800 film plays at 1080p.
                if (level && pinfo.adaptive) setLevelLines(Math.max(level.height, Math.round((level.width * 9) / 16)));
              });
              const timing = (change: { initPtsMs?: number; leadMs?: number }) =>
                setHlsTiming((t) => ({
                  ...(t?.session === pinfo.session_id ? t : { initPtsMs: null, leadMs: null }),
                  ...change,
                  session: pinfo.session_id,
                }));
              hls.on(Hls.Events.INIT_PTS_FOUND, (_evt, data: InitPTSFoundData) => {
                // The main stream's (hls.js is loaded lazily: its enum isn't at hand).
                if ((data.id as string) === "main" && data.timescale > 0) timing({ initPtsMs: (data.initPTS / data.timescale) * 1000 });
              });
              let measured = false;
              hls.on(Hls.Events.FRAG_PARSED, (_evt, data: FragParsedData) => {
                if (measured || data.part) return;
                const ahead = hlsLeadMs(data.frag);
                if (ahead === undefined) return;
                measured = true;
                if (ahead !== null) timing({ leadMs: ahead });
              });
              hls.on(Hls.Events.FRAG_LOADED, (_evt, data: FragLoadedData) => {
                // Media that arrives isn't media that plays: budgets start over
                // when playback moves on (`onTime`).
                lastData.current = performance.now();
                // How fast the network brought it, for Automatic: from its
                // first byte, as the server sends a segment once it is made
                // (the wait before is the transcoder's, not the network's).
                const stats = data?.frag?.stats;
                if (stats) autoLoaded(stats.loaded, stats.loading.end - Math.max(stats.loading.start, stats.loading.first));
              });
              hls.loadSource(pinfo.url);
              hls.attachMedia(video);
            }
          }
        } else {
          video.src = pinfo.url;
          seekOnLoad(startSec);
        }

        try {
          await video.play();
        } catch (err) {
          if (!cancelled && err instanceof DOMException && err.name === "NotAllowedError") setNeedsGesture(true);
        }
      } catch (err) {
        if (cancelled) return;
        if (isApiError(err, 0)) failUnreachable();
        else setError(errorMessage(err));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [item.id, file?.id, audioIndex, burnIndex, playingQuality.id, loadToken, autoReady]);

  // Final teardown on unmount: report "stopped" and release the transcode.
  useEffect(() => {
    const video = videoRef.current;
    return () => {
      const s = sessionRef.current;
      sessionRef.current = null;
      hlsRef.current?.destroy();
      hlsRef.current = null;
      // A detached element keeps loading (a direct play's range request):
      // it would compete with whatever plays next.
      if (video) {
        video.pause();
        video.removeAttribute("src");
        video.load();
      }
      const done = s ? endSession(item.id, s, positionRef.current * 1000, true, tracksOf(s)) : Promise.resolve();
      void done.then(() => invalidateWatchState(qc));
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Tab closed / navigated away: best-effort stop with keepalive requests (in
  // parallel: nothing can wait for a response while the page unloads).
  // Restored from the back/forward cache: start again where we were.
  useEffect(() => {
    const onHide = () => {
      const s = sessionRef.current;
      if (!s) return;
      sessionRef.current = null;
      void endSession(item.id, s, positionRef.current * 1000, true, tracksOf(s));
    };
    const onShow = (e: PageTransitionEvent) => {
      if (e.persisted && !sessionRef.current) restartAt(positionRef.current);
    };
    window.addEventListener("pagehide", onHide);
    window.addEventListener("pageshow", onShow);
    return () => {
      window.removeEventListener("pagehide", onHide);
      window.removeEventListener("pageshow", onShow);
    };
  }, [item.id, restartAt, tracksOf]);

  // ---------------------------------------------------------------------
  // Media element events.
  // ---------------------------------------------------------------------
  const onMediaEnded = useEffectEvent(() => {
    setEnded(true);
    setPlaying(false);
    const s = sessionRef.current;
    const durMs = s?.duration_ms || Math.round(duration * 1000);
    positionRef.current = durMs / 1000;
    const done = s
      ? reportProgress(item.id, {
          position_ms: durMs,
          duration_ms: durMs || undefined,
          state: "stopped",
          session_id: s.session_id,
          ...tracksOf(s),
        })
      : Promise.resolve();
    void done.then(() => invalidateWatchState(qc));
    if (!hasNext) {
      void done.then(() => navigate(endUrl, { replace: true }));
      return;
    }
    if (sleep.atEnd) {
      // The sleep timer was for this one: nothing plays next.
      sleep.clear();
      return;
    }
    findNext()
      .then((next) => {
        if (next) setUpNext(next);
        else void navigate(endUrl, { replace: true });
      })
      .catch(() => navigate(endUrl, { replace: true }));
  });

  /** `at`: where playback was when the element failed. */
  const recoverFromMediaError = useEffectEvent((s: PlaybackInfo, code: number, message: string, at: number) => {
    const undecodable = code === MediaError.MEDIA_ERR_DECODE || code === MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED;
    if (s.method === "direct" && undecodable && !fallbackModeRef.current) {
      // The browser claimed support but can't decode this file: transcode it instead.
      fallbackModeRef.current = "transcode";
      auto?.fellBack(performance.now());
      restartAt(at);
      return;
    }
    if (code === MediaError.MEDIA_ERR_NETWORK && recoveries.current.recreate < 3) {
      // Server restart or connection drop: the same method will most likely work again.
      recoveries.current.recreate += 1;
      restartAt(at);
      return;
    }
    if (code === MediaError.MEDIA_ERR_NETWORK) failUnreachable();
    else setError(message || t("video.errors.cantPlay"));
  });

  const onMediaError = useEffectEvent(() => {
    const v = videoRef.current;
    if (!v || !v.error || hlsRef.current) return; // hls.js reports its own errors
    const s = sessionRef.current;
    if (!s) return; // the element was reset for the next session
    const code = v.error.code;
    const message = v.error.message;
    const at = livePosition();
    // A stream stopped on the server answers 410 (the element doesn't say
    // which status it got): ask before starting another session.
    void stoppedStream(s.url).then((by) => {
      if (sessionRef.current !== s) return; // moved on meanwhile
      if (by) stopPlayback(by);
      else recoverFromMediaError(s, code, message, at);
    });
  });

  // A stall without an error: the browser waits for data, or for a decoder
  // that can't keep up, and its spinner would stay for good. When playback
  // hasn't moved for STALL_MS although it should, recover as from an error:
  // starving, load again (then start a new session); with data that doesn't
  // play, transcode a direct play or reset the HLS decoder (then a new session).
  const onStall = useEffectEvent(() => {
    const v = videoRef.current;
    const s = sessionRef.current;
    const now = performance.now();
    if (!v || !s || error || needsGesture || ended || v.paused || v.currentTime !== lastAdvance.current.time) {
      // Moving, or not meant to be (paused, failed, finished).
      lastAdvance.current = { at: now, time: v?.currentTime ?? -1 };
      return;
    }
    if (now - lastAdvance.current.at < STALL_MS) return;
    // The next check waits a full STALL_MS again.
    lastAdvance.current = { at: now, time: v.currentTime };
    const starving = now - lastData.current >= STALL_MS && bufferedAhead(v) < 1;
    const r = recoveries.current;
    const hls = hlsRef.current;
    if (!starving && s.method === "direct" && !fallbackModeRef.current) {
      // The browser said it plays this file, and doesn't keep up: transcode it.
      // Automatic counts it as its own change, and says what it plays then.
      fallbackModeRef.current = "transcode";
      setStalled(true);
      if (auto) {
        auto.fellBack(now);
        announceFallback.current = true;
      }
      restartAt(livePosition());
      return;
    }
    if (!starving && hls && r.media < 2) {
      r.media += 1;
      hls.recoverMediaError();
      return;
    }
    if (!starving) {
      // Media that arrives and doesn't play: one new session, in case the
      // last one went wrong; the same again would only loop.
      if (r.stuck < 1) {
        r.stuck += 1;
        restartAt(livePosition());
        return;
      }
      setError(t("video.errors.sameSpot"));
      return;
    }
    if (starving && hls && r.network < 4) {
      r.network += 1;
      hls.startLoad(livePosition());
      return;
    }
    if (r.recreate < 3) {
      r.recreate += 1;
      restartAt(livePosition());
      return;
    }
    setError(t("video.errors.stalled"));
  });
  useEffect(() => {
    const timer = window.setInterval(onStall, STALL_CHECK_MS);
    return () => window.clearInterval(timer);
  }, []);

  // ---------------------------------------------------------------------
  // Automatic (docs/QUALITY.md): it watches the video while it plays (not
  // paused, seeking, settling after a seek or a new session, or out of
  // sight) and starts again at the quality it decides on.
  // ---------------------------------------------------------------------
  const applyAutoStep = (step: AutoStep | null) => {
    if (!step) return;
    startMsRef.current = currentMs();
    prepareNext.current = true;
    // The stall watch starts over: the new session is on its way.
    lastAdvance.current = { at: performance.now(), time: -1 };
    setAutoStepped((n) => n + 1);
    if (step.why) say(smootherText(namedFor(file, qualityById(step.to)).label));
  };

  const watchQuality = useEffectEvent(() => {
    const v = videoRef.current;
    if (!auto || !v) return;
    const now = performance.now();
    if (!sessionRef.current || !mediaShown.current || autoSettling.current || v.paused || v.seeking || v.ended || document.hidden || error || stoppedRef.current) {
      auto.interrupted();
      return;
    }
    // A stream in several sizes: hls.js (or the browser) moves between them
    // as the network allows, without a new session. Automatic only steps
    // back up from it, after calm minutes.
    const sizes = sessionRef.current.adaptive;
    if (!sizes) auto.sample(now, v.currentTime, v.currentTime + bufferedAhead(v));
    applyAutoStep(auto.check(now, sizes));
  });

  const autoEvent = useEffectEvent((kind: "waiting" | "playing" | "seeking" | "pause" | "hidden") => {
    const v = videoRef.current;
    if (!auto || !v) return;
    const now = performance.now();
    if (kind === "waiting") {
      // The picture stops for want of data while it plays: not a seek's wait, nor a new session's.
      if (!autoSettling.current && !v.seeking && !v.paused && mediaShown.current && !document.hidden) auto.waiting(now, bufferedAhead(v));
    } else if (kind === "playing") {
      autoSettling.current = false;
      auto.resumed(now);
    } else {
      if (kind === "seeking") autoSettling.current = true;
      auto.interrupted();
    }
  });

  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    const timer = window.setInterval(watchQuality, AUTO_SAMPLE_MS);
    const onWaiting = () => autoEvent("waiting");
    const onPlaying = () => autoEvent("playing");
    const onSeeking = () => autoEvent("seeking");
    const onPause = () => autoEvent("pause");
    const onVisibility = () => {
      if (document.hidden) autoEvent("hidden");
    };
    v.addEventListener("waiting", onWaiting);
    v.addEventListener("playing", onPlaying);
    v.addEventListener("seeking", onSeeking);
    v.addEventListener("pause", onPause);
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      window.clearInterval(timer);
      v.removeEventListener("waiting", onWaiting);
      v.removeEventListener("playing", onPlaying);
      v.removeEventListener("seeking", onSeeking);
      v.removeEventListener("pause", onPause);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, []);

  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    const onPlay = () => {
      setPlaying(true);
      setRemotePaused(false);
      setNeedsGesture(false);
      setEnded(false);
    };
    const onPause = () => {
      setPlaying(false);
      if (!v.ended) report("paused");
    };
    // While a session (re)starts the element is reset to 0 before it seeks to
    // the start position: only trust its time once it shows the session's media.
    const showsMedia = () => !!sessionRef.current && v.readyState >= HTMLMediaElement.HAVE_CURRENT_DATA;
    const onTime = () => {
      if (!showsMedia()) return;
      mediaShown.current = true;
      positionRef.current = v.currentTime;
      setCurrent(v.currentTime);
      const from = playedFrom.current;
      if (from === null || v.currentTime < from) {
        playedFrom.current = v.currentTime;
      } else if (v.currentTime - from >= RECOVERED_AFTER_S) {
        // Playing on again: the next incident gets every recovery again.
        Object.assign(recoveries.current, { network: 0, media: 0, recreate: 0, stuck: 0 });
        playedFrom.current = v.currentTime;
      }
    };
    const onProgress = () => {
      lastData.current = performance.now();
      setBuffered(rangesOf(v.buffered));
    };
    const onDuration = () => setMediaDuration(Number.isFinite(v.duration) ? v.duration : 0);
    // Browsers say they wait on every seek, even within what's loaded: the
    // spinner only comes for a wait that lasts, so seeks don't flash it.
    const startWaiting = () => {
      if (bufferingTimer.current !== undefined) return;
      bufferingTimer.current = window.setTimeout(() => setBuffering(true), frozenNow.current ? FROZEN_SPINNER_DELAY_MS : SPINNER_DELAY_MS);
    };
    const stopWaiting = () => {
      window.clearTimeout(bufferingTimer.current);
      bufferingTimer.current = undefined;
      setBuffering(false);
    };
    const onWaiting = startWaiting;
    const onPlaying = () => {
      stopWaiting();
      unfreeze();
    };
    const onCanPlay = stopWaiting;
    const onLoadedData = () => unfreeze();
    const onSeeked = () => {
      // The frame at the new time can arrive before anything to play on
      // from it (a transcode starting there): playing again ends the wait.
      if (v.paused || v.readyState >= HTMLMediaElement.HAVE_FUTURE_DATA) stopWaiting();
      // hls.js's own seeks while it (re)attaches are not the playhead.
      if (!sessionRef.current || !mediaShown.current) return;
      positionRef.current = v.currentTime;
      report(v.paused ? "paused" : "playing");
      picturesSought();
    };
    const onEnded = () => {
      stopWaiting();
      onMediaEnded();
    };
    const onError = () => onMediaError();
    // Wherever a seek comes from, its target is where playback is meant to
    // be, also while it loads (a restart after a stall resumes there).
    const onSeeking = () => {
      if (sessionRef.current && mediaShown.current) positionRef.current = v.currentTime;
      // Progress counts from where it lands.
      playedFrom.current = null;
      // Paused, a seek far away waits too (browsers only say so when playing).
      startWaiting();
    };
    // Reset (a new session, hls.js reattaching): its time means nothing until it shows media again.
    const onEmptied = () => {
      mediaShown.current = false;
      playedFrom.current = null;
    };

    v.addEventListener("play", onPlay);
    v.addEventListener("pause", onPause);
    v.addEventListener("timeupdate", onTime);
    v.addEventListener("progress", onProgress);
    v.addEventListener("durationchange", onDuration);
    v.addEventListener("waiting", onWaiting);
    v.addEventListener("playing", onPlaying);
    v.addEventListener("canplay", onCanPlay);
    v.addEventListener("loadeddata", onLoadedData);
    v.addEventListener("seeking", onSeeking);
    v.addEventListener("emptied", onEmptied);
    v.addEventListener("seeked", onSeeked);
    v.addEventListener("ended", onEnded);
    v.addEventListener("error", onError);
    return () => {
      v.removeEventListener("play", onPlay);
      v.removeEventListener("pause", onPause);
      v.removeEventListener("timeupdate", onTime);
      v.removeEventListener("progress", onProgress);
      v.removeEventListener("durationchange", onDuration);
      v.removeEventListener("waiting", onWaiting);
      v.removeEventListener("playing", onPlaying);
      v.removeEventListener("canplay", onCanPlay);
      v.removeEventListener("loadeddata", onLoadedData);
      v.removeEventListener("seeking", onSeeking);
      v.removeEventListener("emptied", onEmptied);
      v.removeEventListener("seeked", onSeeked);
      v.removeEventListener("ended", onEnded);
      v.removeEventListener("error", onError);
      window.clearTimeout(bufferingTimer.current);
      bufferingTimer.current = undefined;
    };
  }, [report]);

  // Progress while playing; while paused, now and then, so the server knows
  // the player is still there (it keeps the session in "Now playing" and
  // its transcode going; a closed tab drops out within a minute or two).
  useEffect(() => {
    if (ended || error) return;
    const timer = window.setInterval(
      () => report(playing ? "playing" : "paused"),
      playing ? PROGRESS_INTERVAL : PAUSED_HEARTBEAT,
    );
    return () => window.clearInterval(timer);
  }, [playing, report, ended, error]);

  // Speed: this title's only. The element starts each session at its
  // default rate, so that changes too.
  const [speed, setSpeed] = useState(1);
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    v.defaultPlaybackRate = speed;
    v.playbackRate = speed;
  }, [speed]);
  const stepPlaybackSpeed = (by: 1 | -1) => {
    const next = stepSpeed(speed, by);
    setSpeed(next);
    toast.info(t("video.speed", { speed: speedLabel(next) }));
  };

  // Volume.
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    v.volume = Math.max(0, Math.min(1, volume));
    v.muted = muted;
  }, [volume, muted]);

  // Text subtitles: pley fetches the WebVTT itself and gives its cues to a
  // track of its own, which times them; pley draws the lines showing now
  // (SubtitleOverlay), except in the iPhone's own fullscreen player, where
  // the browser has to. (A <track src> would hold playback until its file
  // arrives, and pulling subtitles out of a big file can take minutes.)
  const shownTextUrl =
    [...addedTracks, ...(info?.subtitles ?? [])].find((s) => s.index === shownTextIndex && s.text_based)?.url ?? null;
  const [loadedCues, setLoadedCues] = useState<{ url: string; cues: VTTCue[] } | null>(null);
  const subtitlesFailed = useEffectEvent(() => toast.error(t("video.subtitlesNotLoaded")));
  // Subtitles show up by themselves once loaded (nothing is said meanwhile).
  // Embedded ones come as far as the server has read them out of the file
  // (`X-Pley-Complete: 0`); the player asks again until it has them all.
  useEffect(() => {
    if (!shownTextUrl || typeof VTTCue === "undefined") return;
    const abort = new AbortController();
    let again: number | undefined;
    const load = () =>
      fetch(shownTextUrl, { credentials: "same-origin", signal: abort.signal })
        .then(async (r) => {
          if (!r.ok) throw new Error(`HTTP ${r.status}`);
          const complete = r.headers.get("x-pley-complete") !== "0";
          setLoadedCues({ url: shownTextUrl, cues: parseVtt(await r.text(), VTTCue) });
          if (!complete) again = window.setTimeout(() => void load(), SUBTITLES_AGAIN_MS);
        })
        .catch(() => {
          if (!abort.signal.aborted) subtitlesFailed();
        });
    void load();
    return () => {
      abort.abort();
      window.clearTimeout(again);
    };
  }, [shownTextUrl]);
  const shownCues = loadedCues && loadedCues.url === shownTextUrl ? loadedCues.cues : null;
  /** The cues on the track: more of the same subtitles only add the new ones. */
  const trackCues = useRef<{ track: TextTrack | null; cues: VTTCue[] | null; shift: number; lift: number }>({
    track: null,
    cues: null,
    shift: 0,
    lift: 0,
  });
  const subtitlePosition = subStyle.position;
  useEffect(() => {
    const v = videoRef.current;
    if (!v || typeof v.addTextTrack !== "function") return;
    // One track for the element's life: tracks added by script can't be removed.
    const track = pleyTrack(v);
    // In native fullscreen the browser draws the track's cues at their own
    // times and places, so moved or raised subtitles go on it moved or raised.
    const shift = nativeFullscreen ? subtitleOffset : 0;
    const lift = nativeFullscreen ? subtitleLift(subtitlePosition) : 0;
    const before = trackCues.current;
    const more =
      before.track === track &&
      before.shift === shift &&
      before.lift === lift &&
      !!before.cues &&
      !!shownCues &&
      sameStart(before.cues, shownCues);
    if (!more) for (const cue of Array.from(track.cues ?? [])) track.removeCue(cue);
    for (const cue of (shownCues ?? []).slice(more ? (before.cues?.length ?? 0) : 0)) {
      track.addCue(shift || lift ? movedCue(cue, shift, lift) : cue);
    }
    trackCues.current = { track, cues: shownCues, shift, lift };
    track.mode = shownCues?.length ? (nativeFullscreen ? "showing" : "hidden") : "disabled";
    // The lines at the playhead, worked out here: after a seek the track's
    // own active cues stay as they were until the video plays again (seconds
    // while it loads), and a line ending meanwhile never showed. Moved
    // subtitles (`subtitleOffset`, positive: later) show what the file has
    // that much earlier.
    let change: number | undefined;
    const update = () => {
      window.clearTimeout(change);
      const at = v.currentTime - subtitleOffset;
      const shown = track.mode === "hidden" && shownCues;
      const next = shown ? cuesAt(shownCues, at) : [];
      setCues((now) => (now.length === next.length && now.every((c, i) => c === next[i]) ? now : next));
      // The track's `cuechange` comes at the file's times: moved, the next
      // line is timed here (`timeupdate` alone comes only every ~250 ms).
      if (shown && subtitleOffset !== 0 && !v.paused) {
        const nextChange = nextCueChange(shownCues, at);
        if (nextChange !== null)
          change = window.setTimeout(update, ((nextChange - at) * 1000) / (v.playbackRate || 1) + 5);
      }
    };
    track.addEventListener("cuechange", update);
    v.addEventListener("seeking", update);
    v.addEventListener("timeupdate", update);
    v.addEventListener("pause", update);
    update();
    return () => {
      window.clearTimeout(change);
      track.removeEventListener("cuechange", update);
      v.removeEventListener("seeking", update);
      v.removeEventListener("timeupdate", update);
      v.removeEventListener("pause", update);
      setCues([]);
    };
  }, [shownCues, nativeFullscreen, subtitleOffset, subtitlePosition]);

  // A server that can't make picture subtitles' files (404, 503) has them
  // burned in instead: a new session where it plays, prepared while this one plays on.
  const picturesUnavailable = useEffectEvent(() => {
    startMsRef.current = currentMs();
    prepareNext.current = true;
  });
  const picturesFrom = useEffectEvent(() => currentMs());
  useEffect(() => {
    pictureHooks.current = { unavailable: picturesUnavailable, failed: subtitlesFailed, position: picturesFrom };
  }, []);


  const changeSubtitleOffset = (offset: number) => {
    const value = clampSubtitleOffset(offset);
    setOffsetChoice({ file: file?.id ?? null, offset: value });
    storeSubtitleOffset(userId, file?.id, value);
  };
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    const begin = () => setNativeFullscreen(true);
    const end = () => setNativeFullscreen(false);
    v.addEventListener("webkitbeginfullscreen", begin);
    v.addEventListener("webkitendfullscreen", end);
    return () => {
      v.removeEventListener("webkitbeginfullscreen", begin);
      v.removeEventListener("webkitendfullscreen", end);
    };
  }, []);

  // Fullscreen state.
  useEffect(() => {
    const onChange = () => setFullscreen(!!document.fullscreenElement);
    document.addEventListener("fullscreenchange", onChange);
    return () => document.removeEventListener("fullscreenchange", onChange);
  }, []);

  // ---------------------------------------------------------------------
  // Controls.
  // ---------------------------------------------------------------------
  const hideTimer = useRef<number | undefined>(undefined);
  const poke = useCallback(() => {
    setActive(true);
    // From here a remote's pause is a pause like any other: the controls stay.
    setRemotePaused(false);
    window.clearTimeout(hideTimer.current);
    hideTimer.current = window.setTimeout(() => setActive(false), HIDE_CONTROLS_AFTER);
  }, []);
  // Controls show at start and hide after a while (`active` starts true).
  useEffect(() => {
    hideTimer.current = window.setTimeout(() => setActive(false), HIDE_CONTROLS_AFTER);
    return () => window.clearTimeout(hideTimer.current);
  }, []);

  const showControls =
    active ||
    hoveringControls ||
    (!playing && !remotePaused) ||
    settingsOpen ||
    chaptersOpen ||
    loading ||
    !!error ||
    needsGesture ||
    ended ||
    !!stopped;
  const spinning = (loading || buffering) && !error && !needsGesture && !stopped;

  const togglePlay = () => {
    const v = videoRef.current;
    if (!v || error || stopped) return;
    if (v.paused || v.ended) void v.play().catch(() => setNeedsGesture(true));
    else v.pause();
  };

  /** Where it seeks to (s), no further than the end; null without a video. */
  const seekTo = (seconds: number): number | null => {
    const v = videoRef.current;
    if (!v) return null;
    const max = duration > 0 ? Math.max(0, duration - 0.25) : Number.POSITIVE_INFINITY;
    const t = Math.max(0, Math.min(seconds, max));
    v.currentTime = t;
    positionRef.current = t;
    setCurrent(t);
    if (ended) setEnded(false);
    if (upNext) setUpNext(null);
    return t;
  };
  const seekBy = (delta: number) => seekTo((videoRef.current?.currentTime ?? current) + delta);

  const changeVolume = (delta: number) => {
    const next = Math.max(0, Math.min(1, (muted ? 0 : volume) + delta));
    setVolume(next);
    setMuted(next === 0);
  };
  const toggleMute = () => {
    if (muted && volume === 0) setVolume(0.5);
    setMuted(!muted);
  };

  // On the iPhone, picture subtitles drawn here can't show in the video's
  // own full screen: the player, which fills the window, stays as it is.
  const fullscreenOffered = !(drawnPictureIndex !== null && videoOnlyFullscreen());
  const toggleFullscreen = () => {
    const el = containerRef.current;
    if (!el || (!fullscreenOffered && !document.fullscreenElement)) return;
    if (document.fullscreenElement) void document.exitFullscreen().catch(() => undefined);
    else if (el.requestFullscreen) void el.requestFullscreen().catch(() => undefined);
    else {
      // iOS Safari only has fullscreen on the video element itself.
      const video: WebkitVideo | null = videoRef.current;
      video?.webkitEnterFullscreen?.();
    }
  };

  /**
   * Store the choice so the next episode (or the next viewing) starts with the same tracks.
   * Audio left out (`undefined`) keeps what was picked before: the server's choice for this
   * browser isn't saved, another device may play another track of that language better.
   */
  const rememberTracks = (audio: number | null | undefined, sub: number | null) => {
    void saveStreamChoice(item.id, { file_id: file?.id ?? null, audio_index: audio, subtitle_index: sub }).then(() => {
      void qc.invalidateQueries({ queryKey: ["item"], predicate: (q) => q.queryKey[1] !== item.id });
      // This one is asked again once it shows again (its page, or played
      // again), not while it plays.
      void qc.invalidateQueries({ queryKey: keys.item(item.id), refetchType: "none" });
    });
  };
  /** Another track: a new session, made ready while this one plays on. False: it plays already. */
  const changeAudio = (index: number): boolean => {
    const playing = audioIndex ?? info?.audio_index ?? audioStreams.find((s) => s.default)?.index ?? audioStreams[0]?.index;
    if (index === playing) return false;
    startMsRef.current = currentMs();
    prepareNext.current = true;
    setAudioIndex(index);
    rememberTracks(index, subIndex);
    return true;
  };
  /**
   * A quality picked in the menu, kept for the next titles. Automatic
   * picked again (or anew) starts afresh at the original. The session starts
   * again when it asks for something else, or leaves a fallback behind.
   */
  const changeQuality = (id: string) => {
    if (id === selectedQuality.id && !(auto && auto.playing !== ORIGINAL_ID)) return;
    startMsRef.current = currentMs();
    prepareNext.current = true;
    const fellBackBefore = fallbackModeRef.current !== null;
    fallbackModeRef.current = null;
    setStalled(false);
    if (id === "auto") {
      autoMemory.set(null);
      reportCeiling(null);
      setAutoRun((n) => n + 1);
    }
    setQuality(id);
    if (fellBackBefore) setLoadToken((t) => t + 1);
  };
  const changeSubtitle = (index: number | null) => {
    const s = subStreams.find((x) => x.index === index);
    const nextBurn = s && s.text_based === false && pictures.unavailable ? s.index : null;
    if (nextBurn !== burnIndex) {
      startMsRef.current = currentMs();
      prepareNext.current = true;
    }
    setSubIndex(index);
    rememberTracks(audioIndex ?? undefined, index);
  };
  /** A track added on the server (found, or a file uploaded): listed, and shown. */
  const subtitleAdded = (track: SubtitleTrack) => {
    setAddedTracks((list) => [...list.filter((t) => t.index !== track.index), track]);
    setDeletedTracks((list) => list.filter((i) => i !== track.index));
    setAddedHere((list) => [...list, track.index]);
    changeSubtitle(track.index);
    toast.success(t("video.subtitlesAdded", { track: streamLabel(trackStream(track)) }));
    // The item's page lists it the next time it shows.
    void qc.invalidateQueries({ queryKey: keys.item(item.id), refetchType: "none" });
  };
  const canDeleteSubtitle = (s: MediaStream) =>
    me.is_admin ||
    addedHere.includes(s.index) ||
    [...addedTracks, ...(info?.subtitles ?? [])].some((t) => t.index === s.index && t.deletable);
  const removeSubtitle = (index: number) => {
    deleteSubtitle(item.id, index).then(
      () => {
        setDeletedTracks((list) => [...list, index]);
        setAddedTracks((list) => list.filter((t) => t.index !== index));
        if (subIndex === index) changeSubtitle(null);
        toast.info(t("video.subtitlesDeleted"));
        void qc.invalidateQueries({ queryKey: keys.item(item.id), refetchType: "none" });
      },
      (err: unknown) => toast.error(t("video.subtitlesNotDeleted", { error: errorMessage(err) })),
    );
  };
  const subtitleSources: SubtitleSources | null = file
    ? { itemId: item.id, fileId: file.id, language: me.subtitle_language, isAdmin: me.is_admin, onAdded: subtitleAdded }
    : null;

  const cycleSubtitles = () => {
    const texts = subStreams.filter((s) => s.text_based !== false);
    if (!texts.length) return;
    const pos = texts.findIndex((s) => s.index === shownTextIndex);
    const next = pos + 1 < texts.length ? texts[pos + 1] : null;
    changeSubtitle(next ? next.index : null);
    toast.info(subtitlesText(next ?? null));
  };

  /** `remote`: asked for from another device, which is told nothing here when nothing comes next. */
  const playNext = async (remote = false) => {
    try {
      const next = upNext ?? (await findNext());
      if (!next) {
        if (!remote) toast.info(queue ? t("video.lastInQueue") : t("video.lastEpisode"));
        return;
      }
      // A running sleep timer goes on with the next one.
      const timer = sleep.timer && "endsAt" in sleep.timer ? sleep.timer : null;
      void navigate(nextUrl(next), { replace: true, state: timer ? { sleep: timer } : null });
    } catch (err) {
      toast.error(errorMessage(err));
    }
  };

  // The user's remote, on another device: this player acts on commands for
  // the session it plays now, and reports at once, so the remote sees it.
  // What a command changes is said in a brief notice; the controls stay as
  // they are, for whoever watches here didn't ask for them.
  const onCommand = useEffectEvent((ev: ServerEvent) => {
    if (ev.type !== "player_command") return;
    const v = videoRef.current;
    const s = sessionRef.current;
    if (!v || !s || ev.session_id !== s.session_id || error || stopped) return;
    switch (ev.command) {
      case "play":
        if (v.paused || v.ended) {
          void v.play().then(
            () => {
              report("playing");
              toast.info(t("video.remotePlaying"));
            },
            // A browser that wants a click first: its Play shows, as it must.
            () => setNeedsGesture(true),
          );
        } else {
          // Playing already: said again, so a remote a little behind settles.
          report("playing");
        }
        break;
      case "pause":
        // Its pause event reports; paused already, it is said again.
        if (!v.paused) {
          setRemotePaused(true);
          v.pause();
          toast.info(t("video.remotePaused"));
        } else {
          report("paused");
        }
        break;
      case "seek":
      case "skip": {
        const ms = ev.command === "seek" ? ev.position_ms : ev.offset_ms;
        if (ms === null) break;
        const at = seekTo(ev.command === "seek" ? ms / 1000 : livePosition() + ms / 1000);
        // Not when the seek lands, which a conversion starting there can take a while to.
        report(v.paused ? "paused" : "playing");
        if (ev.command === "seek") toast.info(t("video.jumpedTo", { time: formatClock(at) }));
        else if (ms !== 0) toast.info(skipText(ms));
        break;
      }
      case "next":
        if (hasNext) void playNext(true);
        break;
      case "previous":
        // Videos have none.
        break;
      // As its own menu picks them; what plays already is said again at once.
      // A new audio track's session says it as it starts, and other
      // subtitles are said as they show (below).
      case "audio": {
        if (ev.stream_index === null) break;
        const index = ev.stream_index;
        if (!changeAudio(index)) {
          report(v.paused ? "paused" : "playing");
          break;
        }
        const track = audioStreams.find((x) => x.index === index);
        if (track) toast.info(t("video.audioShown", { track: streamLabel(track) }));
        break;
      }
      case "subtitles": {
        if (ev.stream_index === null) break;
        const index = ev.stream_index < 0 ? null : ev.stream_index;
        if (index === (selectedSub?.index ?? null)) {
          report(v.paused ? "paused" : "playing");
          break;
        }
        changeSubtitle(index);
        // A track it doesn't list shows none: "Subtitles off", where some showed.
        const track = subStreams.find((x) => x.index === index) ?? null;
        if (track || selectedSub) toast.info(subtitlesText(track));
        break;
      }
      // As its Quality menu picks it: the stream starts again at that
      // quality, whose session says it as it starts.
      case "quality": {
        const picked = qualities.find((q) => q.id === ev.quality);
        if (!picked) break;
        if (picked.id === selectedQuality.id) {
          report(v.paused ? "paused" : "playing");
          break;
        }
        changeQuality(picked.id);
        toast.info(t("video.qualityShown", { quality: qualityLabel(picked) }));
        break;
      }
    }
  });
  useEffect(() => onServerEvent(onCommand), []);

  // Other subtitles or another quality, wherever they were picked (the menu,
  // C, a remote), are said at once, for the user's remotes to show.
  const qualityShown = selectedQuality.id;
  useEffect(() => {
    const v = videoRef.current;
    if (v) report(v.paused ? "paused" : "playing");
  }, [shownSubtitleIndex, qualityShown, report]);

  // Skip Intro / Skip Credits; an episode's credits bring up Up Next.
  const markers = useMemo(() => fileMarkers(item, info?.file_id ?? file?.id), [item, info?.file_id, file?.id]);
  const marker = useSkipMarkers({
    markers,
    seconds: current,
    enabled: !!info && !loading && !error && !ended,
    onCredits: () => {
      if (!hasNext || upNext || sleep.atEnd) return;
      findNext()
        .then((next) => {
          if (next) setUpNext(next);
        })
        .catch(() => undefined);
    },
  });
  const skipMarker = () => {
    if (marker) seekTo(Math.min(skipTarget(marker), duration));
  };

  const retry = () => {
    setError(null);
    setUnreachable(false);
    recoveries.current.recreate = 0;
    recoveries.current.stuck = 0;
    // Without a session (its start failed) the element knows nothing: use the intended start.
    restartAt(sessionRef.current ? livePosition() : startMsRef.current / 1000);
  };

  // Keyboard shortcuts.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey) return;
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName;
      if (tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA") return;
      switch (e.key) {
        case " ":
          if (tag === "BUTTON") return; // let the focused button handle it
          e.preventDefault();
          togglePlay();
          break;
        case "k":
        case "K":
          togglePlay();
          break;
        case "ArrowLeft":
        case "j":
        case "J":
          e.preventDefault();
          seekBy(-10);
          break;
        case "ArrowRight":
        case "l":
        case "L":
          e.preventDefault();
          seekBy(10);
          break;
        case "ArrowUp":
          e.preventDefault();
          changeVolume(0.05);
          break;
        case "ArrowDown":
          e.preventDefault();
          changeVolume(-0.05);
          break;
        case "m":
        case "M":
          toggleMute();
          break;
        case "f":
        case "F":
          toggleFullscreen();
          break;
        case "c":
        case "C":
          cycleSubtitles();
          break;
        case ">":
        case "<":
          stepPlaybackSpeed(e.key === ">" ? 1 : -1);
          break;
        case "s":
        case "S":
          if (!marker) return;
          skipMarker();
          break;
        case "Escape":
          if (settingsOpen) return;
          if (!document.fullscreenElement) goBack();
          break;
        default:
          return;
      }
      poke();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  });

  // Surface clicks: mouse click toggles play (double-click = fullscreen); taps toggle the controls.
  const clickTimer = useRef<number | undefined>(undefined);
  const pointerType = useRef("mouse");
  const onSurfaceClick = () => {
    if (pointerType.current !== "mouse") {
      if (active && playing) setActive(false);
      else poke();
      return;
    }
    window.clearTimeout(clickTimer.current);
    clickTimer.current = window.setTimeout(togglePlay, 220);
    poke();
  };
  const onSurfaceDoubleClick = () => {
    window.clearTimeout(clickTimer.current);
    toggleFullscreen();
  };

  const VolumeIcon = muted || volume === 0 ? VolumeX : volume < 0.5 ? Volume1 : Volume2;
  // After a failed direct play the server only knows that a transcode was requested.
  const fallbackNote = fellBack ? (stalled ? t("video.originalNotSmooth") : t("video.originalFailed")) : null;
  // What Automatic plays ("Original", "8 Mbps 1080p"; the original converted here by its quality), and why when lower.
  const converted = info?.auto_quality && playingQuality.id === ORIGINAL_ID ? namedFor(file, qualityById(info.auto_quality)) : null;
  const autoNow = !auto
    ? null
    : info?.adaptive && levelLines
      ? `${levelLines}p`
      : playingQuality.id === ORIGINAL_ID
        ? converted?.maxBitrate
          ? converted.label
          : qualityLabel(ORIGINAL)
        : qualityLabel(playingQuality);
  // Why Automatic plays below the original, as the details say.
  const autoWhy = auto?.lowered && auto.playing !== ORIGINAL_ID ? t(`video.lowered.${auto.lowered}`) : null;
  const note = fallbackNote ?? autoWhy;

  return (
    <div
      ref={containerRef}
      className={cn("fixed inset-0 z-50 select-none overflow-hidden bg-black text-white", !showControls && "cursor-none")}
      onPointerMove={(e) => {
        if (e.pointerType === "mouse") poke();
      }}
      // Tabbing to a control, or a key pressed on one, shows them: hidden
      // controls can still be focused and pressed.
      onFocusCapture={poke}
      onKeyDownCapture={poke}
    >
      <video
        ref={videoRef}
        className="absolute inset-0 size-full bg-black object-contain"
        playsInline
        preload="auto"
        data-subtitle-font={subStyle.font}
        onPointerDown={(e) => {
          pointerType.current = e.pointerType;
        }}
        onClick={onSurfaceClick}
        onDoubleClick={onSurfaceDoubleClick}
      >
      </video>
      <canvas ref={freezeRef} aria-hidden className={cn("pointer-events-none absolute inset-0 size-full object-contain", !frozen && "hidden")} />
      {drawnPictureIndex !== null ? (
        <PictureSubtitleOverlay
          videoRef={videoRef}
          subs={pictures.subs}
          sessionOffsetMs={
            hlsTiming && hlsTiming.session === info?.session_id
              ? hlsShiftMs(info.pts_offset_ms, hlsTiming.initPtsMs, hlsTiming.leadMs)
              : 0
          }
          offset={subtitleOffset}
          hold={frozen}
        />
      ) : null}
      <SubtitleOverlay cues={cues} lifted={showControls} style={subStyle} />

      <div
        aria-hidden
        className={cn(
          "pointer-events-none absolute inset-0 flex items-center justify-center transition-opacity duration-300",
          spinning ? "opacity-100" : "opacity-0",
        )}
      >
        <LoaderCircle className={cn("size-14 text-white/80", spinning && "animate-spin")} />
      </div>

      {needsGesture && !error ? (
        <button
          type="button"
          aria-label={t("common:actions.play")}
          onClick={togglePlay}
          className="absolute left-1/2 top-1/2 z-20 flex size-20 -translate-x-1/2 -translate-y-1/2 items-center justify-center rounded-full bg-accent text-accent-fg shadow-2xl transition hover:scale-105"
        >
          <Play className="ml-1 size-9 fill-current" />
        </button>
      ) : null}

      {stopped ? (
        <div
          role="alert"
          className="absolute inset-0 z-30 flex flex-col items-center justify-center gap-4 bg-black/85 p-6 text-center"
        >
          <CircleStop className="size-12 text-accent" />
          <div className="max-w-lg">
            <div className="text-lg font-semibold">
              {stopped.byAdmin === false ? t("video.stoppedElsewhere") : t("video.stoppedByAdmin")}
            </div>
            {stopped.message ? (
              <p className="mt-2 whitespace-pre-line break-words text-sm text-white/80">{stopped.message}</p>
            ) : null}
          </div>
          <button
            type="button"
            onClick={goBack}
            className="rounded-md bg-white/10 px-5 py-2 text-sm font-semibold hover:bg-white/20"
          >
            {t("common:actions.back")}
          </button>
        </div>
      ) : error ? (
        <div role="alert" className="absolute inset-0 z-30 flex flex-col items-center justify-center gap-4 bg-black/85 p-6 text-center">
          {unreachable ? <ServerOff className="size-12 text-accent" /> : <TriangleAlert className="size-12 text-accent" />}
          <div className="max-w-lg">
            <div className="text-lg font-semibold">{unreachable ? t("video.unreachable") : t("video.errorTitle")}</div>
            <p className="mt-1 text-sm text-white/70">{error}</p>
          </div>
          <div className="flex gap-2">
            <button type="button" onClick={retry} className="rounded-md bg-accent px-5 py-2 text-sm font-semibold text-accent-fg hover:bg-accent-hover">
              {t("common:actions.retry")}
            </button>
            <button type="button" onClick={goBack} className="rounded-md bg-white/10 px-5 py-2 text-sm font-semibold hover:bg-white/20">
              {t("common:actions.back")}
            </button>
          </div>
        </div>
      ) : null}

      {/* Top bar */}
      <div
        className={cn(
          "absolute inset-x-0 top-0 z-10 flex items-start gap-3 bg-linear-to-b from-black/80 via-black/40 to-transparent px-3 pb-16 pt-3 transition-opacity duration-300 sm:px-6 sm:pt-5",
          showControls ? "opacity-100" : "pointer-events-none opacity-0",
        )}
        onMouseEnter={() => setHoveringControls(true)}
        onMouseLeave={() => setHoveringControls(false)}
      >
        <button
          type="button"
          aria-label={t("common:actions.back")}
          onClick={goBack}
          className="flex size-10 shrink-0 items-center justify-center rounded-full transition hover:bg-white/15"
        >
          <ArrowLeft className="size-6" />
        </button>
        <div className="min-w-0 pt-1">
          <div className="truncate text-lg font-semibold drop-shadow sm:text-xl">{title}</div>
          {subtitle ? <div className="truncate text-sm text-white/75">{subtitle}</div> : null}
        </div>
      </div>

      {/* Bottom bar */}
      <div
        className={cn(
          "absolute inset-x-0 bottom-0 z-10 bg-linear-to-t from-black/90 via-black/55 to-transparent px-3 pb-3 pt-20 transition-opacity duration-300 sm:px-6 sm:pb-5",
          showControls ? "opacity-100" : "pointer-events-none opacity-0",
        )}
        onMouseEnter={() => setHoveringControls(true)}
        onMouseLeave={() => setHoveringControls(false)}
      >
        <SeekBar
          current={current}
          duration={duration}
          buffered={buffered}
          onSeek={seekTo}
          previews={file && trickplay.data ? { fileId: file.id, trickplay: trickplay.data } : null}
          chapters={file?.chapters ?? []}
        />
        <div className="mt-1 flex items-center gap-0.5 sm:gap-1.5">
          <button
            type="button"
            aria-label={playing ? t("controls.pause") : t("common:actions.play")}
            onClick={togglePlay}
            className="flex size-11 items-center justify-center rounded-full transition hover:bg-white/15"
          >
            {playing ? <Pause className="size-6 fill-current" /> : <Play className="ml-0.5 size-6 fill-current" />}
          </button>
          {/* A phone's own buttons set the volume: the row keeps its room for the rest. */}
          <div className="group/vol hidden items-center sm:flex">
            <button
              type="button"
              aria-label={muted ? t("controls.unmute") : t("controls.mute")}
              onClick={toggleMute}
              className="flex size-10 items-center justify-center rounded-full transition hover:bg-white/15"
            >
              <VolumeIcon className="size-5" />
            </button>
            <div className="hidden w-0 overflow-hidden transition-[width] duration-200 group-hover/vol:w-24 group-focus-within/vol:w-24 sm:block">
              <Slider
                label={t("controls.volume")}
                value={muted ? 0 : volume}
                max={1}
                onPreview={(v) => {
                  setVolume(v);
                  setMuted(v === 0);
                }}
                className="ml-1 w-20"
              />
            </div>
          </div>
          <div className="ml-1 shrink-0 whitespace-nowrap text-sm tabular-nums text-white/85 sm:ml-2">
            {formatClock(current)} <span className="hidden text-white/45 sm:inline">/ {formatClock(duration)}</span>
          </div>

          <div className="flex-1" />

          {info ? (
            // Hovering (or focusing) the badge shows how it plays, as the menu's Playback details do.
            <div
              className="relative mr-1 hidden md:block"
              onMouseEnter={() => setMethodHover(true)}
              onMouseLeave={() => setMethodHover(false)}
              onFocus={() => setMethodHover(true)}
              onBlur={() => setMethodHover(false)}
            >
              <span
                tabIndex={0}
                aria-label={[methodLabel(info), note ?? info.reason].filter(Boolean).join(". ")}
                className={cn(
                  "cursor-default rounded px-2 py-0.5 text-xs font-semibold outline-none focus-visible:ring-2 focus-visible:ring-white/60",
                  info.method === "direct" ? "bg-success/20 text-success" : info.method === "remux" ? "bg-info/20 text-info" : "bg-accent/20 text-accent",
                )}
              >
                {methodLabel(info)}
              </span>
              {methodHover && !settingsOpen ? (
                <div
                  role="tooltip"
                  className="absolute bottom-full right-0 mb-3 w-80 animate-pop-in rounded-xl border border-white/10 bg-black/90 py-2 text-white shadow-2xl"
                >
                  <PlaybackDetails
                    info={info}
                    file={file}
                    audioIndex={audioIndex}
                    note={note}
                    positionMs={current * 1000}
                    open
                  />
                </div>
              ) : null}
            </div>
          ) : null}
          {hasNext ? (
            <button
              type="button"
              aria-label={queue ? t("common:actions.next") : t("video.nextEpisode")}
              title={queue ? t("common:actions.next") : t("video.nextEpisode")}
              onClick={() => void playNext()}
              className="flex size-10 items-center justify-center rounded-full transition hover:bg-white/15"
            >
              <SkipForward className="size-5 fill-current" />
            </button>
          ) : null}
          {file && (file.chapters?.length ?? 0) > 1 ? (
            <ChaptersMenu
              chapters={file.chapters}
              current={current}
              open={chaptersOpen}
              setOpen={setChaptersOpen}
              onSeek={seekTo}
            />
          ) : null}
          <SettingsPopover
            open={settingsOpen}
            setOpen={setSettingsOpen}
            details={
              info ? (
                <PlaybackDetails
                  info={info}
                  file={file}
                  audioIndex={audioIndex}
                  note={note}
                  positionMs={current * 1000}
                  open={settingsOpen}
                />
              ) : undefined
            }
            qualities={qualities}
            quality={selectedQuality.id}
            originalHint={[fileResolution(file), formatBitrate(file?.bitrate)].filter(Boolean).join(" · ") || null}
            autoNow={autoNow}
            autoWhy={autoWhy}

            onQuality={changeQuality}
            audioStreams={audioStreams}
            audioIndex={audioIndex ?? info?.audio_index ?? null}
            onAudio={changeAudio}
            subtitleStreams={subStreams}
            subtitleIndex={subIndex}
            onSubtitle={changeSubtitle}
            subtitleStyle={subStyle}
            onSubtitleStyle={setSubtitleStyle}
            subtitleOffset={subtitleOffset}
            onSubtitleOffset={changeSubtitleOffset}
            picturesMovable={drawnPictureIndex !== null}
            subtitleSources={subtitleSources}
            canDeleteSubtitle={canDeleteSubtitle}
            onDeleteSubtitle={removeSubtitle}
            onSubtitleFileRefused={(message) => toast.error(message)}
            speed={speed}
            onSpeed={setSpeed}
            sleep={sleepChoice(sleep.timer)}
            sleepLeft={sleep.timer && "endsAt" in sleep.timer ? sleep.secondsLeft : null}
            sleepEndLabel={sleepEndLabel}
            onSleep={sleep.choose}
          />
          {fullscreenOffered || fullscreen ? (
            <button
              type="button"
              aria-label={fullscreen ? t("video.exitFullscreen") : t("video.fullscreen")}
              title={t("video.fullscreenShortcut")}
              onClick={toggleFullscreen}
              className="flex size-10 items-center justify-center rounded-full transition hover:bg-white/15"
            >
              {fullscreen ? <Minimize className="size-5" /> : <Maximize className="size-5" />}
            </button>
          ) : null}
        </div>
      </div>

      {marker && !upNext && !ended ? <SkipButton marker={marker} onSkip={skipMarker} lifted={active || !playing} /> : null}

      {sleep.notice !== null && !stopped && !error && !ended ? (
        <SleepNotice seconds={sleep.notice} onKeepWatching={sleep.keepWatching} lifted={active || !playing} />
      ) : null}

      {!stopped && !error ? <QualityNotice notice={qualityNotice} /> : null}

      {upNext ? (
        <UpNext

          next={upNext}
          paused={!playing && !ended}
          onPlay={() => void playNext()}
          onCancel={() => {
            setUpNext(null);
          }}
        />
      ) : null}
    </div>
  );
}
