import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import type HlsType from "hls.js";
import type { ErrorData } from "hls.js";
import type { Control, ItemSummary, PlaybackMethod, PlaybackMode, PlaybackState, ServerEvent } from "@/api/types";
import { errorMessage } from "@/api/client";
import { onServerEvent } from "@/api/events";
import { invalidateWatchState } from "@/api/hooks";
import { audioCaps, canPlayNativeHls } from "@/lib/caps";
import { loadHls } from "@/lib/hls";
import { playingHere } from "@/lib/playingHere";
import { getStored, writeStorage } from "@/lib/hooks";
import {
  endSession,
  reportProgress,
  startPlayback,
  stopSession,
  stoppedStream,
  type StoppedBy,
} from "@/lib/progress";
import { shuffled } from "@/lib/shuffle";
import { useLatest } from "@/lib/hooks";

export interface AudioPlayerApi {
  queue: ItemSummary[];
  index: number;
  current: ItemSummary | null;
  playing: boolean;
  loading: boolean;
  volume: number;
  muted: boolean;
  shuffle: boolean;
  error: string | null;
  playQueue: (tracks: ItemSummary[], startIndex?: number, opts?: { shuffle?: boolean }) => void;
  toggle: () => void;
  pause: () => void;
  next: () => void;
  prev: () => void;
  jumpTo: (index: number) => void;
  seek: (seconds: number) => void;
  setVolume: (v: number) => void;
  toggleMute: () => void;
  toggleShuffle: () => void;
  close: () => void;
}

export interface AudioTime {
  position: number;
  duration: number;
  buffered: number;
}

const AudioContext = createContext<AudioPlayerApi | null>(null);
const AudioTimeContext = createContext<AudioTime>({ position: 0, duration: 0, buffered: 0 });

const PROGRESS_INTERVAL = 10_000;

/** The remote's commands music obeys. */
const MUSIC_CONTROLS: Control[] = ["play", "pause", "seek", "skip", "next", "previous"];

interface Session {
  sessionId: string;
  itemId: number;
  durationMs: number;
  method: PlaybackMethod;
  /** The stream, asked whether it was stopped on the server. */
  url: string;
  /** Last known playhead (ms); the element reads 0 while it is reset. */
  positionMs: number;
  /** No longer this tab's (the remote lists it again as another device's, if it is still there). */
  leave: () => void;
}

/** Report the final position of `s` and release it on the server. */
function release(s: Session, keepalive = false, positionMs = s.positionMs): Promise<void> {
  s.leave();
  return endSession(s.itemId, { session_id: s.sessionId, duration_ms: s.durationMs }, positionMs, keepalive);
}

export function AudioPlayerProvider({ children }: { children: ReactNode }) {
  const qc = useQueryClient();
  const audioRef = useRef<HTMLAudioElement>(null);
  const hlsRef = useRef<HlsType | null>(null);
  const sessionRef = useRef<Session | null>(null);
  /** Where the next (re)start of the current track begins (ms). */
  const startAtRef = useRef(0);
  /** Mode used after direct play of the current track failed to decode. */
  const fallbackRef = useRef<PlaybackMode | null>(null);
  /** `network`/`media` count per session; `recreate` (new sessions) is reset once media loads again. */
  const recoveries = useRef({ network: 0, media: 0, recreate: 0 });
  /** POST /api/playback of the previous run, which ends its own session if that run was cancelled. */
  const pendingStartRef = useRef<Promise<void>>(Promise.resolve());

  const [queue, setQueue] = useState<ItemSummary[]>([]);
  const [original, setOriginal] = useState<ItemSummary[]>([]);
  const [index, setIndex] = useState(0);
  // Bumped whenever playback of the current index must (re)start.
  const [playToken, setPlayToken] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [shuffle, setShuffle] = useState(false);
  const [volume, setVolumeState] = useState(() => getStored("pley.audio.volume", 1));
  const [muted, setMuted] = useState(() => getStored("pley.audio.muted", false));
  const [time, setTime] = useState<AudioTime>({ position: 0, duration: 0, buffered: 0 });

  const queueRef = useLatest(queue);
  const indexRef = useLatest(index);

  const current = queue[index] ?? null;

  const destroyHls = () => {
    hlsRef.current?.destroy();
    hlsRef.current = null;
  };

  /** End the active session (report + release) without touching the element. */
  const endCurrent = useCallback((keepalive = false) => {
    const s = sessionRef.current;
    if (!s) return;
    sessionRef.current = null;
    void release(s, keepalive);
  }, []);

  const goTo = useCallback((i: number) => {
    startAtRef.current = 0;
    fallbackRef.current = null;
    recoveries.current.recreate = 0;
    setIndex(i);
    setPlayToken((t) => t + 1);
  }, []);

  /** Start the current track again with a new session (expired session, decode fallback). */
  const restartAt = useCallback((ms: number) => {
    startAtRef.current = Math.max(0, Math.round(ms));
    setPlayToken((t) => t + 1);
  }, []);

  /**
   * The server ended this session (an administrator, or the user elsewhere):
   * let go of the stream without starting another, and say so.
   */
  const stopPlayback = useCallback(
    (by: StoppedBy) => {
      // Nothing to release: the server has ended it.
      sessionRef.current?.leave();
      sessionRef.current = null;
      hlsRef.current?.destroy();
      hlsRef.current = null;
      const audio = audioRef.current;
      if (audio) {
        audio.pause();
        audio.removeAttribute("src");
        audio.load();
      }
      setPlaying(false);
      setLoading(false);
      const what =
        by.byAdmin === false
          ? "Playback was stopped from another device."
          : "Playback was stopped by the server's administrator.";
      setError(by.message ? `${what} ${by.message}` : what);
      invalidateWatchState(qc);
    },
    [qc],
  );

  useEffect(
    () =>
      onServerEvent((ev: ServerEvent) => {
        if (ev.type === "session_stopped" && ev.session_id === sessionRef.current?.sessionId) {
          stopPlayback({ message: ev.message, byAdmin: ev.by_admin });
        }
      }),
    [stopPlayback],
  );

  /** The stream failed: stopped on the server, or start a new session if not (up to 3 times). */
  const recreateUnlessStopped = useCallback(
    (s: Session) => {
      void stoppedStream(s.url).then((by) => {
        if (sessionRef.current !== s) return; // moved on meanwhile
        if (by) stopPlayback(by);
        else if (recoveries.current.recreate < 3) {
          recoveries.current.recreate += 1;
          restartAt(s.positionMs);
        } else setError("Playback failed");
      });
    },
    [restartAt, stopPlayback],
  );

  // Start playback whenever the current track (or token) changes.
  useEffect(() => {
    const audio = audioRef.current;
    if (!audio || !current) return;
    let cancelled = false;
    const itemId = current.id;
    const startMs = startAtRef.current;
    const mode = fallbackRef.current ?? undefined;
    recoveries.current.network = 0;
    recoveries.current.media = 0;

    // Release the previous session before asking for the next one (the
    // transcode limit), even if this run gets cancelled.
    const prev = sessionRef.current;
    sessionRef.current = null;
    const released = prev ? release(prev) : Promise.resolve();
    const previousStart = pendingStartRef.current;
    destroyHls();
    audio.removeAttribute("src");
    audio.load();
    setLoading(true);
    setError(null);
    setTime({ position: startMs / 1000, duration: (current.duration_ms ?? 0) / 1000, buffered: 0 });

    // Recorded as soon as it exists so the next run (or unmount) ends it; a
    // run cancelled before that ends it here.
    const acquired = (async () => {
      await Promise.all([released, previousStart]);
      if (cancelled) return null;
      const info = await startPlayback({ item_id: itemId, caps: audioCaps(), start_ms: startMs, mode, controls: MUSIC_CONTROLS });
      if (cancelled) {
        await stopSession(info.session_id);
        return null;
      }
      sessionRef.current = {
        sessionId: info.session_id,
        itemId,
        durationMs: info.duration_ms,
        method: info.method,
        url: info.url,
        positionMs: startMs,
        leave: playingHere(info.session_id),
      };
      return info;
    })();
    pendingStartRef.current = acquired.then(
      () => undefined,
      () => undefined,
    );

    const position = () => (sessionRef.current?.positionMs ?? startMs) / 1000;

    void (async () => {
      try {
        const info = await acquired;
        if (!info || cancelled) return;
        setTime((t) => ({ ...t, duration: info.duration_ms / 1000 || t.duration }));
        if (info.hls && !canPlayNativeHls(audio)) {
          const Hls = await loadHls();
          if (cancelled) return;
          if (!Hls.isSupported()) throw new Error("This browser can't play HLS streams");
          const hls = new Hls({ enableWorker: true, maxBufferLength: 60, startPosition: startMs / 1000 });
          hlsRef.current = hls;
          hls.on(Hls.Events.ERROR, (_e, data: ErrorData) => {
            if (!data.fatal || cancelled) return;
            const r = recoveries.current;
            if (data.type === Hls.ErrorTypes.NETWORK_ERROR) {
              const code = data.response?.code;
              if (code === 410) {
                // Stopped on the server: another session would only be stopped again.
                void stoppedStream(info.url).then((by) => {
                  if (!cancelled) stopPlayback(by ?? { message: null, byAdmin: null });
                });
                return;
              }
              const playlist =
                data.details === Hls.ErrorDetails.MANIFEST_LOAD_ERROR ||
                data.details === Hls.ErrorDetails.MANIFEST_LOAD_TIMEOUT ||
                data.details === Hls.ErrorDetails.MANIFEST_PARSING_ERROR;
              if (code === 404 || playlist) {
                // The session expired (e.g. paused for a long time): start a new one here.
                if (r.recreate < 3) {
                  r.recreate += 1;
                  restartAt(position() * 1000);
                  return;
                }
              } else if (r.network < 4) {
                r.network += 1;
                hls.startLoad(position());
                return;
              }
            } else if (data.type === Hls.ErrorTypes.MEDIA_ERROR && r.media < 2) {
              r.media += 1;
              hls.recoverMediaError();
              return;
            }
            setError("Playback failed");
          });
          hls.on(Hls.Events.FRAG_LOADED, () => {
            recoveries.current.recreate = 0;
            recoveries.current.network = 0;
          });
          hls.loadSource(info.url);
          hls.attachMedia(audio);
        } else {
          audio.src = info.url;
          if (startMs > 0) {
            const seek = () => {
              audio.currentTime = startMs / 1000;
            };
            audio.addEventListener("loadedmetadata", seek, { once: true });
          }
        }
        await audio.play().catch(() => {
          // Autoplay may be blocked until the user interacts; state stays paused.
        });
      } catch (err) {
        if (cancelled) return;
        setError(errorMessage(err));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
    // `current` is captured by id on purpose: re-renders with the same track must not restart it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current?.id, playToken, restartAt]);

  // Media element events.
  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;
    // The element reads 0 while a session (re)starts: only trust it once it has media.
    const hasMedia = () => !!sessionRef.current && audio.readyState >= HTMLMediaElement.HAVE_CURRENT_DATA;
    const onPlay = () => setPlaying(true);
    const onPause = () => {
      setPlaying(false);
      const s = sessionRef.current;
      if (s && !audio.ended) {
        void reportProgress(s.itemId, {
          position_ms: s.positionMs,
          duration_ms: s.durationMs || undefined,
          state: "paused",
          session_id: s.sessionId,
        });
      }
    };
    const onTime = () => {
      if (!hasMedia()) return;
      const s = sessionRef.current;
      if (s) s.positionMs = Math.round(audio.currentTime * 1000);
      let buffered = 0;
      const b = audio.buffered;
      if (b.length) buffered = b.end(b.length - 1);
      setTime((t) => ({
        position: audio.currentTime,
        duration: Number.isFinite(audio.duration) && audio.duration > 0 ? audio.duration : t.duration,
        buffered,
      }));
    };
    const onPlaying = () => {
      // hls.js resets the budget per loaded fragment; direct play once it plays again.
      if (!hlsRef.current) recoveries.current.recreate = 0;
    };
    const onEnded = () => {
      const s = sessionRef.current;
      if (s) {
        sessionRef.current = null;
        const dur = s.durationMs || Math.round(audio.duration * 1000) || 0;
        void release(s, false, dur).then(() => invalidateWatchState(qc));
      }
      const i = indexRef.current;
      if (i + 1 < queueRef.current.length) goTo(i + 1);
      else setPlaying(false);
    };
    const onError = () => {
      // hls.js reports its own errors; a reset element has no src.
      if (hlsRef.current || !audio.getAttribute("src")) return;
      const s = sessionRef.current;
      const code = audio.error?.code;
      if (audio.error?.message.includes("MediaSink")) {
        // Firefox without a sound output (none there, or it went away)
        // says it can't decode, and plays on in silence: another session
        // or a transcode wouldn't help.
        setError("No sound output: check the speakers or headphones");
        return;
      }
      const undecodable = code === MediaError.MEDIA_ERR_DECODE || code === MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED;
      if (s?.method === "direct" && undecodable && !fallbackRef.current) {
        // The browser claimed support but can't decode this file: transcode it instead.
        fallbackRef.current = "transcode";
        restartAt(s.positionMs);
        return;
      }
      if (s && code === MediaError.MEDIA_ERR_NETWORK) {
        // A stream stopped on the server answers 410 (the element doesn't say
        // which status it got): ask before starting another session.
        recreateUnlessStopped(s);
        return;
      }
      setError("Playback failed");
    };
    audio.addEventListener("play", onPlay);
    audio.addEventListener("pause", onPause);
    audio.addEventListener("playing", onPlaying);
    audio.addEventListener("timeupdate", onTime);
    audio.addEventListener("progress", onTime);
    audio.addEventListener("durationchange", onTime);
    audio.addEventListener("ended", onEnded);
    audio.addEventListener("error", onError);
    return () => {
      audio.removeEventListener("play", onPlay);
      audio.removeEventListener("pause", onPause);
      audio.removeEventListener("playing", onPlaying);
      audio.removeEventListener("timeupdate", onTime);
      audio.removeEventListener("progress", onTime);
      audio.removeEventListener("durationchange", onTime);
      audio.removeEventListener("ended", onEnded);
      audio.removeEventListener("error", onError);
    };
  }, [qc, goTo, restartAt, recreateUnlessStopped, indexRef, queueRef]);

  // Periodic progress while playing. One answered 410 was stopped on the
  // server, heard without the event stream (for a direct play, the only
  // request that can tell).
  useEffect(() => {
    if (!playing) return;
    const timer = window.setInterval(() => {
      const s = sessionRef.current;
      if (!s) return;
      void reportProgress(
        s.itemId,
        {
          position_ms: s.positionMs,
          duration_ms: s.durationMs || undefined,
          state: "playing",
          session_id: s.sessionId,
        },
        false,
        () => {
          if (sessionRef.current === s) stopPlayback({ message: null, byAdmin: null });
        },
      );
    }, PROGRESS_INTERVAL);
    return () => window.clearInterval(timer);
  }, [playing, stopPlayback]);

  // Volume.
  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;
    audio.volume = volume;
    audio.muted = muted;
  }, [volume, muted]);

  // Report "stopped" when the tab goes away (keepalive requests, sent in parallel).
  useEffect(() => {
    const onHide = () => endCurrent(true);
    window.addEventListener("pagehide", onHide);
    return () => window.removeEventListener("pagehide", onHide);
  }, [endCurrent]);

  // Provider unmounted (logout, 401): stop loading and end the session.
  useEffect(() => {
    return () => {
      hlsRef.current?.destroy();
      hlsRef.current = null;
      endCurrent(true);
    };
  }, [endCurrent]);

  const playQueue = useCallback(
    (tracks: ItemSummary[], startIndex = 0, opts?: { shuffle?: boolean }) => {
      if (tracks.length === 0) return;
      const start = Math.min(Math.max(startIndex, 0), tracks.length - 1);
      setOriginal(tracks);
      if (opts?.shuffle) {
        setQueue(shuffled(tracks));
        setShuffle(true);
        goTo(0);
      } else {
        setQueue(tracks);
        setShuffle(false);
        goTo(start);
      }
    },
    [goTo],
  );

  const toggle = useCallback(() => {
    const audio = audioRef.current;
    if (!audio || !queueRef.current.length) return;
    if (audio.paused) void audio.play().catch(() => undefined);
    else audio.pause();
  }, [queueRef]);

  const pause = useCallback(() => {
    audioRef.current?.pause();
  }, []);

  const next = useCallback(() => {
    const i = indexRef.current;
    if (i + 1 < queueRef.current.length) goTo(i + 1);
  }, [goTo, indexRef, queueRef]);

  const seek = useCallback((seconds: number) => {
    const audio = audioRef.current;
    if (!audio) return;
    const target = Math.max(0, seconds);
    audio.currentTime = target;
    if (sessionRef.current) sessionRef.current.positionMs = Math.round(target * 1000);
    setTime((t) => ({ ...t, position: target }));
  }, []);

  /** The track before, or this one from its start a few seconds in (true: it started over). */
  const prev = useCallback((): boolean => {
    const audio = audioRef.current;
    const i = indexRef.current;
    if ((audio && audio.currentTime > 3) || i === 0) {
      seek(0);
      return true;
    }
    goTo(i - 1);
    return false;
  }, [goTo, seek, indexRef]);

  const jumpTo = useCallback(
    (i: number) => {
      if (i < 0 || i >= queueRef.current.length) return;
      goTo(i);
    },
    [goTo, queueRef],
  );

  // The user's remote, on another device: the queue acts on commands for
  // the track it plays now, and reports at once, so the remote sees it. A
  // new track reports as it starts.
  useEffect(() => {
    const reportNow = (s: Session, state: PlaybackState) => {
      if (sessionRef.current !== s) return;
      void reportProgress(s.itemId, {
        position_ms: s.positionMs,
        duration_ms: s.durationMs || undefined,
        state,
        session_id: s.sessionId,
      });
    };
    return onServerEvent((ev: ServerEvent) => {
      const audio = audioRef.current;
      const s = sessionRef.current;
      if (ev.type !== "player_command" || !audio || !s || ev.session_id !== s.sessionId) return;
      const state = () => (audio.paused ? "paused" : "playing");
      switch (ev.command) {
        case "play":
          if (audio.paused) {
            void audio.play().then(
              () => reportNow(s, "playing"),
              () => undefined,
            );
          } else {
            // Playing already: said again, so a remote a little behind settles.
            reportNow(s, "playing");
          }
          break;
        case "pause":
          // Its pause event reports; paused already, it is said again.
          if (!audio.paused) audio.pause();
          else reportNow(s, "paused");
          break;
        case "seek":
        case "skip": {
          const ms = ev.command === "seek" ? ev.position_ms : ev.offset_ms === null ? null : s.positionMs + ev.offset_ms;
          if (ms === null) break;
          const end = s.durationMs || (Number.isFinite(audio.duration) ? audio.duration * 1000 : Number.POSITIVE_INFINITY);
          seek(Math.min(Math.max(0, ms), end) / 1000);
          reportNow(s, state());
          break;
        }
        case "next":
          next();
          break;
        case "previous":
          if (prev()) reportNow(s, state());
          break;
      }
    });
  }, [next, prev, seek]);

  const setVolume = useCallback((v: number) => {
    const clamped = Math.max(0, Math.min(1, v));
    setVolumeState(clamped);
    writeStorage("pley.audio.volume", clamped);
    if (clamped > 0) {
      setMuted(false);
      writeStorage("pley.audio.muted", false);
    }
  }, []);

  const toggleMute = useCallback(() => {
    setMuted((m) => !m);
  }, []);

  useEffect(() => {
    writeStorage("pley.audio.muted", muted);
  }, [muted]);

  // Reordering the queue keeps the current track playing (no token bump).
  const toggleShuffle = useCallback(() => {
    const q = queueRef.current;
    const i = indexRef.current;
    const cur = q[i];
    if (!cur) return;
    if (!shuffle) {
      const rest = shuffled(q.filter((_, j) => j !== i));
      setQueue([cur, ...rest]);
      setIndex(0);
      setShuffle(true);
    } else {
      const base = original.length ? original : q;
      const j = base.findIndex((t) => t.id === cur.id);
      setQueue(base);
      setIndex(j >= 0 ? j : 0);
      setShuffle(false);
    }
  }, [original, shuffle, indexRef, queueRef]);

  const close = useCallback(() => {
    const audio = audioRef.current;
    endCurrent();
    destroyHls();
    if (audio) {
      audio.pause();
      audio.removeAttribute("src");
      audio.load();
    }
    setQueue([]);
    setOriginal([]);
    setIndex(0);
    setPlaying(false);
    setError(null);
    invalidateWatchState(qc);
  }, [endCurrent, qc]);

  const api = useMemo<AudioPlayerApi>(
    () => ({
      queue,
      index,
      current,
      playing,
      loading,
      volume,
      muted,
      shuffle,
      error,
      playQueue,
      toggle,
      pause,
      next,
      prev,
      jumpTo,
      seek,
      setVolume,
      toggleMute,
      toggleShuffle,
      close,
    }),
    [
      queue,
      index,
      current,
      playing,
      loading,
      volume,
      muted,
      shuffle,
      error,
      playQueue,
      toggle,
      pause,
      next,
      prev,
      jumpTo,
      seek,
      setVolume,
      toggleMute,
      toggleShuffle,
      close,
    ],
  );

  return (
    <AudioContext.Provider value={api}>
      <AudioTimeContext.Provider value={time}>
        {children}
        <audio ref={audioRef} preload="auto" className="hidden" />
      </AudioTimeContext.Provider>
    </AudioContext.Provider>
  );
}

export function useAudioPlayer(): AudioPlayerApi {
  const ctx = useContext(AudioContext);
  if (!ctx) throw new Error("useAudioPlayer must be used inside <AudioPlayerProvider>");
  return ctx;
}

/** Same as useAudioPlayer but returns null outside the provider. */
export function useOptionalAudioPlayer(): AudioPlayerApi | null {
  return useContext(AudioContext);
}

export function useAudioTime(): AudioTime {
  return useContext(AudioTimeContext);
}
