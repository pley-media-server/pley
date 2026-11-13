import { useCallback, useEffect, useEffectEvent, useMemo, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import type HlsType from "hls.js";
import {
  ArrowLeft,
  Cast,
  ChevronDown,
  ChevronUp,
  List,
  LoaderCircle,
  Maximize,
  Minimize,
  RefreshCw,
  Star,
  TriangleAlert,
  Tv2,
  Volume1,
  Volume2,
  VolumeX,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { ApiError, errorMessage, isApiError } from "@/api/client";
import { onServerEvent } from "@/api/events";
import type { Control, ServerEvent } from "@/api/types";
import {
  busyConnections,
  CHANNEL_PAGE,
  leaveChannel,
  liveKeys,
  livePlan,
  liveQueries,
  parseCategory,
  playsAsSent,
  programProgress,
  streamUrl,
  useSetFavorite,
  watchChannel,
  type BusyConnection,
  type Channel,
  type LiveCategory,
} from "@/api/livetv";
import { useOptionalAudioPlayer } from "@/audio/AudioPlayer";
import { PlayOn } from "@/components/PlayOn";
import { Slider } from "@/components/Slider";
import { t } from "@/i18n";
import { cn } from "@/lib/cn";
import { canPlayNativeHls } from "@/lib/caps";
import { loadHls } from "@/lib/hls";
import { loadMpegts } from "@/lib/mpegts";
import { playingHere } from "@/lib/playingHere";
import { useDocumentTitle, useLocalStorage } from "@/lib/hooks";
import { clientId } from "@/lib/progress";
import { ChannelLogo, formatSpan, formatTime, liveUrl, setLastWatched, timeLeft, useNow } from "@/pages/livetv/common";
import { ChannelPanel } from "./ChannelPanel";

/** Controls hide after this long without the pointer moving. */
const HIDE_CONTROLS_AFTER = 3500;
/** A stall shows the spinner once it has lasted this long. */
const SPINNER_AFTER = 700;
/** What's on, shown with I, goes after this long. */
const BANNER_FOR = 5000;
/** A stream that broke off waits this long to hear it was taken over before tuning again. */
const TAKEN_OVER_GRACE = 1000;
/** Digits typed for a channel number wait this long for more. */
const NUMBER_WAIT = 1400;
/** The remote's commands Live TV obeys: channel up and down. It doesn't go back in a channel, so no pause or seek. */
const LIVE_CONTROLS: Control[] = ["next", "previous"];

type Trouble = {
  kind: "busy" | "unavailable" | "stopped" | "missing" | "other";
  title: string;
  message: string;
  /** All connections in use: what holds them. */
  connections?: BusyConnection[];
};

function trouble(err: unknown, channel: string): Trouble {
  if (isApiError(err, 409)) {
    return { kind: "busy", title: t("player.busy", { ns: "livetv" }), message: errorMessage(err), connections: busyConnections(err) };
  }
  if (isApiError(err, 410)) return stopped(errorMessage(err));
  if (isApiError(err, 404)) return { kind: "missing", title: t("player.gone", { ns: "livetv" }), message: errorMessage(err) };
  if (isApiError(err, 403)) return { kind: "other", title: t("notOpen", { ns: "livetv" }), message: errorMessage(err) };
  if (isApiError(err, 503)) return { kind: "unavailable", title: t("player.notPlaying", { ns: "livetv", channel }), message: errorMessage(err) };
  return { kind: "other", title: t("player.didntStart", { ns: "livetv", channel }), message: errorMessage(err) };
}

/** The channel stopped being watched on the server (its connection taken over, say): why, as it says. */
function stopped(message: string | null | undefined): Trouble {
  return { kind: "stopped", title: t("player.stopped", { ns: "livetv" }), message: message || t("player.notWatched", { ns: "livetv" }) };
}

/** The stream broke off for good: what hls.js says of it. */
function brokeOff(channel: string, details: string): Trouble {
  return { kind: "other", title: t("player.channelStopped", { ns: "livetv", channel }), message: details };
}

/** Neither hls.js nor the browser itself plays the stream. */
function cantPlay(): Trouble {
  return { kind: "other", title: t("player.cantPlay", { ns: "livetv" }), message: t("player.cantPlayHint", { ns: "livetv" }) };
}

/** An HLS error answered by the server: its status and its own words. */
function hlsTrouble(response: { code?: number; text?: string } | undefined, channel: string): Trouble | null {
  if (!response?.code || response.code < 400) return null;
  let message = "";
  try {
    message = (JSON.parse(response.text ?? "") as { message?: string }).message ?? "";
  } catch {
    // not the server's JSON
  }
  const said = message || t("player.serverAnswered", { ns: "livetv", code: response.code });
  return trouble(new ApiError(response.code, "hls", said), channel);
}

export function LivePlayerPage() {
  const { t } = useTranslation("livetv");
  const { channelId, sourceUid } = useParams();
  const id = Number(channelId);
  const [params] = useSearchParams();
  const category = useMemo(() => parseCategory(params.get("in")), [params]);
  const navigate = useNavigate();
  const qc = useQueryClient();
  const current = useQuery(liveQueries.channel(id));
  const channel = current.data ?? null;
  const zap = useZap(category, id);
  useDocumentTitle(channel ? t("player.documentTitle", { channel: channel.name }) : t("liveTv"));
  // An address without its source's id (or an old one): the channel's own.
  useEffect(() => {
    if (channel && channel.source_uid && channel.source_uid !== sourceUid) {
      void navigate(`${liveUrl(channel, category)}`, { replace: true });
    }
  }, [channel, sourceUid, category, navigate]);

  const tune = useCallback(
    (c: Channel, inCategory: LiveCategory = category) => {
      // What the list knows of it shows at once.
      qc.setQueryData(liveKeys.channel(c.id), c);
      void navigate(liveUrl(c, inCategory), { replace: true });
    },
    [navigate, qc, category],
  );
  /** Channel up or down; false where there is no channel to go to (yet). */
  const step = useCallback(
    (by: 1 | -1) => {
      const next = zap(by);
      if (next) tune(next);
      return next !== null;
    },
    [zap, tune],
  );
  // A channel number: every channel's, as a remote's would.
  const byNumber = useCallback(async (digits: string) => {
    const page = await qc.fetchQuery(liveQueries.page({ kind: "all" }, digits, 0));
    return page.channels.find((c) => c.number === digits) ?? null;
  }, [qc]);

  if (current.isError) {
    const missing = isApiError(current.error, 404);
    return (
      <Blackout>
        <TroubleCard
          trouble={
            missing
              ? { kind: "missing", title: t("player.missing"), message: t("player.missingHint") }
              : { kind: "other", title: t("didntLoad"), message: errorMessage(current.error) }
          }
          onRetry={missing ? undefined : () => void current.refetch()}
          onBack={() => void navigate("/live")}
        />
      </Blackout>
    );
  }
  if (!channel) {
    return (
      <Blackout>
        <LoaderCircle className="size-12 animate-spin text-white/70" />
      </Blackout>
    );
  }
  return (
    <LivePlayer
      channel={channel}
      category={category}
      onTune={tune}
      onStep={step}
      byNumber={byNumber}
    />
  );
}

/**
 * Channel up and down through `category`: the page around the channel
 * watched, and the next or last page at its ends (round to the other end
 * of the list). "Recently watched" keeps the order it had as the player
 * opened: it changes with every channel watched.
 */
function useZap(category: LiveCategory, id: number): (by: 1 | -1) => Channel | null {
  const recent = category.kind === "recent";
  const around = useQuery({ ...liveQueries.around(category, id), enabled: !recent, placeholderData: keepPreviousData });
  const recentPage = useQuery({ ...liveQueries.page(category, "", 0), enabled: recent });
  const [recentOrder, setRecentOrder] = useState<Channel[] | null>(null);
  if (recent && !recentOrder && recentPage.data) setRecentOrder(recentPage.data.channels);

  const page = around.data;
  const at = page ? page.channels.findIndex((c) => c.id === id) : -1;
  const pages = page ? Math.ceil(page.total / CHANNEL_PAGE) : 0;
  const pageNo = page ? Math.floor(page.offset / CHANNEL_PAGE) : 0;
  const edge = page != null && pages > 1;
  const before = useQuery({
    ...liveQueries.page(category, "", (pageNo - 1 + pages) % Math.max(1, pages)),
    enabled: edge && at === 0,
  });
  const after = useQuery({
    ...liveQueries.page(category, "", (pageNo + 1) % Math.max(1, pages)),
    enabled: edge && at === (page?.channels.length ?? 0) - 1,
  });

  return useCallback(
    (by: 1 | -1) => {
      if (recent) {
        const list = recentOrder ?? [];
        if (list.length < 2) return null;
        const i = list.findIndex((c) => c.id === id);
        return list[(Math.max(0, i) + by + list.length) % list.length]!;
      }
      if (!page || page.channels.length === 0) return null;
      // Not in this category (tuned by number, say): its first channel.
      if (at < 0) return page.channels[0]!;
      const j = at + by;
      if (j >= 0 && j < page.channels.length) return page.channels[j]!;
      if (pages <= 1) return page.channels[(j + page.channels.length) % page.channels.length]!;
      const other = by < 0 ? before.data : after.data;
      if (!other?.channels.length) return null;
      return by < 0 ? other.channels[other.channels.length - 1]! : other.channels[0]!;
    },
    [recent, recentOrder, id, page, at, pages, before.data, after.data],
  );
}

function Blackout({ children }: { children: React.ReactNode }) {
  return <div className="fixed inset-0 z-50 flex items-center justify-center bg-black p-4 text-white">{children}</div>;
}

function LivePlayer({
  channel,
  category,
  onTune,
  onStep,
  byNumber,
}: {
  channel: Channel;
  category: LiveCategory;
  onTune: (c: Channel, category?: LiveCategory) => void;
  onStep: (by: 1 | -1) => boolean;
  byNumber: (digits: string) => Promise<Channel | null>;
}) {
  const { t } = useTranslation(["livetv", "common"]);
  const navigate = useNavigate();
  const containerRef = useRef<HTMLDivElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const hlsRef = useRef<HlsType | null>(null);
  // A channel sent as it is plays straight from its stream (mpegts.js).
  const directRef = useRef<{ destroy: () => void } | null>(null);
  // `leave`: no longer this tab's, for the remote.
  const sessionRef = useRef<{ id: string; url: string; leave: () => void } | null>(null);
  const forgetSession = () => {
    sessionRef.current?.leave();
    sessionRef.current = null;
  };
  // Someone took the channel's connection over (`live_taken_over`): why,
  // until it is tuned again. Nothing tunes it again by itself meanwhile.
  const takenOver = useRef<Trouble | null>(null);
  const audioPlayer = useOptionalAudioPlayer();
  const favorite = useSetFavorite();
  const now = useNow(5000);

  const [attempt, setAttempt] = useState(0);
  // The channel whose connection the next try takes over.
  const takeOver = useRef<number | undefined>(undefined);
  const [status, setStatus] = useState<"tuning" | "playing" | "error">("tuning");
  const [problem, setProblem] = useState<Trouble | null>(null);
  const [buffering, setBuffering] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [volume, setVolume] = useLocalStorage<number>("pley.volume", 1);
  const [muted, setMuted] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [panel, setPanel] = useState(false);
  // Watch on…'s menu is open: the controls it hangs from stay.
  const [watchOn, setWatchOn] = useState(false);
  const [showControls, setShowControls] = useState(true);
  /**
   * Tuned from the user's remote, on another device: the tuning screen
   * shows, but not the controls, which nobody here asked for. Over once
   * the channel plays, or someone here moves the mouse, taps or presses a key.
   */
  const [remoteTune, setRemoteTune] = useState(false);
  const [banner, setBanner] = useState(false);
  const [digits, setDigits] = useState("");
  const [notFound, setNotFound] = useState<string | null>(null);
  const hideTimer = useRef<number | undefined>(undefined);

  useEffect(() => setLastWatched(channel.id), [channel.id]);

  useEffect(() => {
    audioPlayer?.pause();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once, as the player opens
  }, []);

  // Tune: start watching, then play the channel's live stream.
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    let cancelled = false;
    let networkRetries = 0;
    let mediaRetries = 0;
    setStatus("tuning");
    setProblem(null);
    setBanner(false);
    takenOver.current = null;
    const fail = (why: Trouble) => {
      if (cancelled) return;
      // Whatever broke as the connection was taken, that's why it stopped.
      setProblem(takenOver.current ?? why);
      setStatus("error");
      hlsRef.current?.destroy();
      hlsRef.current = null;
      directRef.current?.destroy();
      directRef.current = null;
    };
    const start = () => {
      video.play().catch(() => {
        // Autoplay with sound refused: muted, then.
        video.muted = true;
        setMuted(true);
        void video.play().catch(() => undefined);
      });
    };
    const watch = async () => {
      try {
        const taking = takeOver.current;
        takeOver.current = undefined;
        const watching = await watchChannel(channel.id, clientId(), taking, LIVE_CONTROLS);
        if (cancelled) {
          void leaveChannel(watching.session_id);
          return null;
        }
        sessionRef.current?.leave();
        sessionRef.current = { id: watching.session_id, url: watching.url, leave: playingHere(watching.session_id) };
        return watching;
      } catch (err) {
        fail(trouble(err, channel.name));
        return null;
      }
    };
    // A channel sent as it is (H.264 and AAC) plays from its stream as soon
    // as a keyframe comes; any other, or where that can't be, from the
    // transcoder's segments. The stream is read while the server says what
    // the channel is, and plays once it says it's sent as it is.
    const playDirect = async (sessionId: string): Promise<boolean> => {
      const plan = livePlan(sessionId).catch(() => null);
      const mpegts = "MediaSource" in window ? await loadMpegts().catch(() => null) : null;
      if (cancelled || !mpegts?.isSupported()) return false;
      // A device id of its own: reading the stream doesn't end the session asked about.
      const url = new URL(streamUrl(channel.id, `${clientId()}-stream`), window.location.href).href;
      const player = mpegts.createPlayer(
        { type: "mpegts", isLive: true, url },
        {
          enableWorker: true,
          enableStashBuffer: false,
          lazyLoad: false,
          // Never skipped ahead (the picture jumped as a channel started, its
          // first seconds all coming at once): a few seconds behind live is
          // fine. Only far behind (stalls adding up) is it caught up, by
          // playing a little faster.
          liveSync: true,
          liveSyncMaxLatency: 30,
          liveSyncTargetLatency: 10,
          liveSyncPlaybackRate: 1.1,
        },
      );
      let failed = false;
      // Played from its stream, as the server or the stream said it could be.
      let chosen = false;
      player.on(mpegts.Events.ERROR, () => {
        failed = true;
        player.destroy();
        if (directRef.current === player) directRef.current = null;
        // Not chosen yet: the transcoder's segments play once the plan comes, below.
        if (cancelled || !chosen) return;
        // It broke off, playing or before its first picture: the transcoder's
        // segments from now, unless its connection was taken over (that's
        // told as the stream ends, and tuning again would only find every
        // connection in use).
        window.setTimeout(() => {
          if (cancelled || takenOver.current) return;
          void watch().then((w) => w && playHls(w.url));
        }, TAKEN_OVER_GRACE);
      });
      directRef.current = player;
      player.attachMediaElement(video);
      player.load();
      const known = await plan;
      if (cancelled) return true;
      // Let go already (mpegts.js throws when let go twice): it failed, and
      // the transcoder's segments play, or watching ended, which says why.
      if (directRef.current !== player) return !failed;
      // What the server says goes; not known in time, what the stream turned out to be.
      if (known ? !playsAsSent(known) : !hasPlayableTracks(player)) {
        player.destroy();
        directRef.current = null;
        return false;
      }
      chosen = true;
      start();
      return true;
    };
    const playHls = async (url: string) => {
      const Hls = await loadHls().catch(() => null);
      if (cancelled) return;
      if (Hls?.isSupported()) {
        const hls = new Hls({
          liveSyncDurationCount: 3,
          // Behind live is caught up a little faster, never by skipping ahead.
          maxLiveSyncPlaybackRate: 1.1,
          maxBufferLength: 20,
          backBufferLength: 30,
          // The server waits for the channel's first segments before it answers.
          manifestLoadPolicy: {
            default: {
              maxTimeToFirstByteMs: 45_000,
              maxLoadTimeMs: 50_000,
              timeoutRetry: { maxNumRetry: 1, retryDelayMs: 0, maxRetryDelayMs: 0 },
              errorRetry: { maxNumRetry: 1, retryDelayMs: 1000, maxRetryDelayMs: 4000 },
            },
          },
        });
        hlsRef.current = hls;
        hls.on(Hls.Events.MANIFEST_PARSED, start);
        hls.on(Hls.Events.ERROR, (_e, data) => {
          if (!data.fatal) return;
          const answered = hlsTrouble(data.response, channel.name);
          if (answered && (answered.kind !== "other" || data.type === Hls.ErrorTypes.NETWORK_ERROR)) {
            if (answered.kind !== "other" || networkRetries >= 3) return fail(answered);
          }
          if (data.type === Hls.ErrorTypes.NETWORK_ERROR && networkRetries < 4) {
            networkRetries++;
            window.setTimeout(() => hls.startLoad(-1), 1000 * networkRetries);
          } else if (data.type === Hls.ErrorTypes.MEDIA_ERROR && mediaRetries < 2) {
            mediaRetries++;
            hls.recoverMediaError();
          } else {
            fail(answered ?? brokeOff(channel.name, data.details));
          }
        });
        hls.loadSource(url);
        hls.attachMedia(video);
      } else if (canPlayNativeHls(video)) {
        video.src = url;
        start();
      } else {
        fail(cantPlay());
      }
    };
    void (async () => {
      const watching = await watch();
      if (!watching) return;
      if (!(await playDirect(watching.session_id)) && !cancelled) await playHls(watching.url);
    })();
    return () => {
      cancelled = true;
      // The last picture stays while the next channel tunes.
      keepPicture(video);
      hlsRef.current?.destroy();
      hlsRef.current = null;
      directRef.current?.destroy();
      directRef.current = null;
      video.removeAttribute("src");
      video.load();
      if (sessionRef.current) void leaveChannel(sessionRef.current.id);
      forgetSession();
    };
  }, [channel.id, channel.name, attempt]);

  /**
   * Watching ended on the server (its connection taken over, or stopped
   * from another device): it stops here at once, and says why.
   */
  const endHere = (why: Trouble) => {
    takenOver.current = why;
    hlsRef.current?.destroy();
    hlsRef.current = null;
    directRef.current?.destroy();
    directRef.current = null;
    // Already ended on the server: nothing to leave.
    forgetSession();
    // Its stream let go too, the picture kept: the browser's own HLS keeps a
    // source the players above don't, and with it Live TV's "plays on"
    // (onPause) would play the seconds it has buffered. The source goes
    // first, so the pause finds none to play on.
    const video = videoRef.current;
    if (video) {
      keepPicture(video);
      video.removeAttribute("src");
      video.pause();
      video.load();
    }
    setBuffering(false);
    setProblem(why);
    setStatus("error");
  };
  const onEvent = useEffectEvent((ev: ServerEvent) => {
    if (takenOver.current) return;
    switch (ev.type) {
      // Someone else took the channel's connection.
      case "live_taken_over":
        if (ev.channel_id === channel.id) endHere(stopped(ev.message));
        break;
      case "session_stopped":
        if (ev.session_id === sessionRef.current?.id) {
          const what = ev.by_admin ? t("player.stoppedByAdmin") : t("player.stoppedElsewhere");
          endHere(stopped(ev.message ? `${what} ${ev.message}` : what));
        }
        break;
      // The user's remote, on another device: channel up and down, as here.
      // The new channel's watch tells the server, and so the remote.
      case "player_command":
        if (ev.session_id !== sessionRef.current?.id) break;
        if (ev.command !== "next" && ev.command !== "previous") break;
        if (onStep(ev.command === "next" ? 1 : -1)) setRemoteTune(true);
        break;
    }
  });
  useEffect(() => onServerEvent(onEvent), []);

  // Leaving the page (a closed tab) lets the connection go at once.
  useEffect(() => {
    const leave = () => {
      if (sessionRef.current) void leaveChannel(sessionRef.current.id, true);
    };
    window.addEventListener("pagehide", leave);
    return () => window.removeEventListener("pagehide", leave);
  }, []);

  // The video element's state.
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    video.volume = volume;
  }, [volume]);
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    video.muted = muted;
  }, [muted]);
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    // A stall shows the spinner only once it lasts: channels whose stream
    // stutters for a moment don't flicker it.
    let stalled: number | undefined;
    const waiting = () => {
      window.clearTimeout(stalled);
      stalled = window.setTimeout(() => setBuffering(true), SPINNER_AFTER);
    };
    const going = () => {
      window.clearTimeout(stalled);
      stalled = undefined;
      setBuffering(false);
    };
    const onPlaying = () => {
      setStatus("playing");
      setRemoteTune(false);
      going();
      setPlaying(true);
      // The last channel's picture, kept while this one tuned, is done with.
      video.removeAttribute("poster");
    };
    // Live TV doesn't pause: whatever paused it (a media key, say), it plays on.
    const onPause = () => {
      setPlaying(false);
      if (video.src || video.srcObject) window.setTimeout(() => void video.play().catch(() => undefined), 100);
    };
    const handlers: [string, () => void][] = [
      ["playing", onPlaying],
      ["pause", onPause],
      ["play", () => setPlaying(true)],
      ["waiting", waiting],
      ["canplay", going],
      ["timeupdate", () => stalled !== undefined && video.readyState >= 3 && going()],
    ];
    for (const [name, fn] of handlers) video.addEventListener(name, fn);
    return () => {
      window.clearTimeout(stalled);
      for (const [name, fn] of handlers) video.removeEventListener(name, fn);
    };
  }, []);

  // The banner goes after a few seconds.
  useEffect(() => {
    if (!banner || status !== "playing") return;
    const id = window.setTimeout(() => setBanner(false), BANNER_FOR);
    return () => window.clearTimeout(id);
  }, [banner, status]);

  // Controls hide while playing and untouched.
  const poke = useCallback(() => {
    setShowControls(true);
    setRemoteTune(false);
    window.clearTimeout(hideTimer.current);
    hideTimer.current = window.setTimeout(() => setShowControls(false), HIDE_CONTROLS_AFTER);
  }, []);
  useEffect(() => {
    hideTimer.current = window.setTimeout(() => setShowControls(false), HIDE_CONTROLS_AFTER);
    return () => window.clearTimeout(hideTimer.current);
  }, []);
  const controlsVisible =
    showControls || panel || watchOn || status === "error" || (!remoteTune && (!playing || status === "tuning"));

  useEffect(() => {
    const onChange = () => setFullscreen(document.fullscreenElement != null);
    document.addEventListener("fullscreenchange", onChange);
    return () => document.removeEventListener("fullscreenchange", onChange);
  }, []);

  const toggleFullscreen = useCallback(() => {
    if (document.fullscreenElement) void document.exitFullscreen().catch(() => undefined);
    else void containerRef.current?.requestFullscreen?.().catch(() => undefined);
  }, []);
  const goBack = useCallback(() => {
    const idx = (window.history.state as { idx?: number } | null)?.idx ?? 0;
    if (idx > 0) void navigate(-1);
    else void navigate("/live", { replace: true });
  }, [navigate]);

  // A channel number typed: tuned once the digits stop.
  useEffect(() => {
    if (!digits) return;
    let cancelled = false;
    const id = window.setTimeout(() => {
      setDigits("");
      void byNumber(digits)
        .catch(() => null)
        .then((match) => {
          if (cancelled) return;
          if (match) onTune(match, { kind: "all" });
          else setNotFound(digits);
        });
    }, NUMBER_WAIT);
    return () => {
      cancelled = true;
      window.clearTimeout(id);
    };
  }, [digits, byNumber, onTune]);
  useEffect(() => {
    if (!notFound) return;
    const id = window.setTimeout(() => setNotFound(null), 2000);
    return () => window.clearTimeout(id);
  }, [notFound]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const target = e.target as HTMLElement | null;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable)) return;
      poke();
      if (/^[0-9]$/.test(e.key)) {
        setDigits((d) => (d + e.key).slice(0, 5));
        return;
      }
      switch (e.key) {
        // Up is channel up, the next channel, as on a TV's remote.
        case "ArrowUp":
        case "PageUp":
        case "ChannelUp":
          e.preventDefault();
          onStep(1);
          break;
        case "ArrowDown":
        case "PageDown":
        case "ChannelDown":
          e.preventDefault();
          onStep(-1);
          break;
        case "m":
          setMuted((m) => !m);
          break;
        case "f":
          toggleFullscreen();
          break;
        case "g":
        case "c":
          setPanel((p) => !p);
          break;
        case "i":
          setBanner((b) => !b);
          break;
        case "Escape":
          if (panel) setPanel(false);
          else if (!document.fullscreenElement) goBack();
          break;
        case "Backspace":
        case "BrowserBack":
        case "GoBack":
          e.preventDefault();
          if (panel) setPanel(false);
          else goBack();
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [poke, onStep, toggleFullscreen, panel, goBack]);

  const on = channel.now && channel.now.stop > now && channel.now.start <= now ? channel.now : null;
  const next = on ? channel.next : (channel.next ?? channel.now);
  const VolumeIcon = muted || volume === 0 ? VolumeX : volume < 0.5 ? Volume1 : Volume2;
  const button = "flex size-10 items-center justify-center rounded-full transition hover:bg-white/15 disabled:opacity-40 disabled:hover:bg-transparent";

  return (
    <div
      ref={containerRef}
      className={cn("fixed inset-0 z-50 select-none overflow-hidden bg-black text-white", !controlsVisible && "cursor-none")}
      onMouseMove={poke}
      onTouchStart={poke}
    >
      {/* The picture and its controls, beside the channel list when it is open. */}
      <div
        className={cn(
          "absolute inset-y-0 left-0 right-0 overflow-hidden transition-[right] duration-300 ease-out",
          panel && "sm:right-[24rem]",
        )}
      >
        <video
          ref={videoRef}
          className="absolute inset-0 size-full object-contain"
          playsInline
          disablePictureInPicture
          onClick={() => (panel ? setPanel(false) : poke())}
          onDoubleClick={toggleFullscreen}
        />

        {/* Tuning: the channel's logo while it starts. */}
        {status === "tuning" ? (
          <div className="absolute inset-0 flex flex-col items-center justify-center gap-6 bg-black/70 backdrop-blur-sm animate-fade-in">
            <ChannelLogo channel={channel} width={240} className="h-28 w-48 rounded-2xl bg-white/[0.06]" />
            <LoaderCircle className="size-7 animate-spin text-accent" aria-label={t("player.tuning")} />
            <div className="text-center">
              <div className="text-lg font-semibold">
                {channel.number ? <span className="mr-2 tabular-nums text-white/60">{channel.number}</span> : null}
                {channel.name}
              </div>
              <div className="mt-1 text-sm text-white/60">{on ? on.title : t("player.tuningNow")}</div>
            </div>
          </div>
        ) : null}

        {status === "playing" && buffering ? (
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
            <LoaderCircle className="size-12 animate-spin text-white/80 drop-shadow" />
          </div>
        ) : null}

        {status === "error" && problem ? (
          <div className="absolute inset-0 flex items-center justify-center bg-black/85 p-4">
            <TroubleCard
              trouble={problem}
              channel={channel}
              onRetry={problem.kind === "missing" ? undefined : () => setAttempt((a) => a + 1)}
            onTakeOver={(channelId) => {
              takeOver.current = channelId;
              setAttempt((a) => a + 1);
            }}
              onBack={goBack}
              onChannels={() => setPanel(true)}
            />
          </div>
        ) : null}

        {/* A channel number being typed. */}
        {digits || notFound ? (
          <div className="absolute right-6 top-6 z-30 rounded-xl bg-black/85 px-5 py-3 text-4xl font-bold tabular-nums tracking-widest shadow-2xl animate-pop-in">
            {digits || <span className="text-xl tracking-normal text-white/70">{t("player.noChannel", { number: notFound })}</span>}
          </div>
        ) : null}

        {/* Top bar */}
        <div
          className={cn(
            "absolute inset-x-0 top-0 z-10 flex items-start gap-3 bg-linear-to-b from-black/80 via-black/40 to-transparent px-3 pb-16 pt-3 transition-opacity duration-300 sm:px-6 sm:pt-5",
            controlsVisible ? "opacity-100" : "pointer-events-none opacity-0",
          )}
        >
          <button type="button" aria-label={t("common:actions.back")} onClick={goBack} className={cn(button, "shrink-0")}>
            <ArrowLeft className="size-6" />
          </button>
          <ChannelLogo channel={channel} width={80} showName={false} className="mt-0.5 hidden h-10 w-16 shrink-0 rounded-md bg-white/10 sm:flex" />
          <div className="min-w-0 flex-1 pt-0.5">
            <div className="flex items-center gap-2 truncate text-lg font-semibold drop-shadow sm:text-xl">
              {channel.number ? <span className="tabular-nums text-white/60">{channel.number}</span> : null}
              <span className="truncate">{channel.name}</span>
            </div>
            {on ? <div className="truncate text-sm text-white/75">{on.title}</div> : null}
          </div>
          <button
            type="button"
            aria-label={channel.favorite ? t("favourites.remove") : t("favourites.add")}
            aria-pressed={channel.favorite}
            onClick={() => favorite.mutate({ id: channel.id, favorite: !channel.favorite })}
            className={button}
          >
            <Star className={cn("size-5", channel.favorite && "fill-accent text-accent")} />
          </button>
          {/* The channel moves to the device: this one leaves it, for the remote on the device to show. */}
          <PlayOn
            kind="live"
            label={t("watchOn")}
            align="right"
            cast={() => ({ channel_id: channel.id })}
            onOpenChange={setWatchOn}
            onCast={goBack}
            trigger={({ open, toggle, label }) => (
              <button
                type="button"
                aria-label={label}
                title={label}
                aria-haspopup="menu"
                aria-expanded={open}
                onClick={toggle}
                className={cn(button, open && "bg-white/15")}
              >
                <Cast className="size-5" />
              </button>
            )}
          />
          <button type="button" aria-label={t("channels")} title={t("player.channelsKey")} onClick={() => setPanel((p) => !p)} className={cn(button, panel && "bg-white/15")}>
            <List className="size-5" />
          </button>
        </div>

        {/* What's on: shown with I. */}
        <div
          className={cn(
            "pointer-events-none absolute bottom-20 left-3 right-3 z-10 transition-all duration-500 sm:bottom-24 sm:left-6 sm:right-auto sm:w-[30rem]",
            banner && status === "playing" ? "translate-y-0 opacity-100" : "translate-y-3 opacity-0",
          )}
          aria-hidden={!banner}
        >
          <div className="flex gap-4 rounded-xl border border-white/10 bg-black/85 p-4 shadow-2xl">
            <ChannelLogo channel={channel} width={96} className="h-14 w-24 shrink-0 rounded-lg bg-white/[0.07]" />
            <div className="min-w-0 flex-1">
              {on ? (
                <>
                  <div className="truncate font-semibold">{on.title}</div>
                  <div className="mt-0.5 text-xs tabular-nums text-white/60">
                    {formatSpan(on)} · {timeLeft(on, now)}
                  </div>
                  <div className="mt-2 h-1 overflow-hidden rounded-full bg-white/15">
                    <div className="h-full rounded-full bg-accent" style={{ width: `${programProgress(on, now) * 100}%` }} />
                  </div>
                </>
              ) : (
                <div className="pt-1 text-sm text-white/70">{channel.has_guide ? t("nothingNow") : t("player.noGuideHere")}</div>
              )}
              {next ? (
                <div className="mt-2 truncate text-xs text-white/55">
                  {t("player.next")} <span className="tabular-nums">{formatTime(next.start)}</span> · {next.title}
                </div>
              ) : null}
            </div>
          </div>
        </div>

        {/* Bottom bar */}
        <div
          className={cn(
            "absolute inset-x-0 bottom-0 z-10 bg-linear-to-t from-black/90 via-black/55 to-transparent px-3 pb-3 pt-16 transition-opacity duration-300 sm:px-6 sm:pb-5",
            controlsVisible ? "opacity-100" : "pointer-events-none opacity-0",
          )}
        >
          <div className="flex items-center gap-0.5 sm:gap-1.5">
            <div className="group/vol hidden items-center sm:flex">
              <button type="button" aria-label={muted ? t("player.unmute") : t("player.mute")} onClick={() => setMuted((m) => !m)} className={button}>
                <VolumeIcon className="size-5" />
              </button>
              <div className="hidden w-0 overflow-hidden transition-[width] duration-200 group-hover/vol:w-24 group-focus-within/vol:w-24 sm:block">
                <Slider
                  label={t("player.volume")}
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
            <span className="ml-1 flex h-7 shrink-0 items-center gap-1.5 rounded-full bg-danger/90 px-2.5 text-xs font-bold uppercase tracking-wide text-white sm:ml-2">
              <span className="size-1.5 animate-pulse rounded-full bg-white" />
              {t("live")}
            </span>
            {on ? (
              <div className="ml-2 hidden min-w-0 truncate text-sm text-white/70 md:block">
                <span className="tabular-nums">{formatSpan(on)}</span>
              </div>
            ) : null}
            <div className="flex-1" />
            <button type="button" aria-label={t("player.channelUp")} title={t("player.channelUpKey")} onClick={() => onStep(1)} className={button}>
              <ChevronUp className="size-5" />
            </button>
            <button type="button" aria-label={t("player.channelDown")} title={t("player.channelDownKey")} onClick={() => onStep(-1)} className={button}>
              <ChevronDown className="size-5" />
            </button>
            <button
              type="button"
              aria-label={fullscreen ? t("player.exitFullScreen") : t("player.fullScreen")}
              title={t("player.fullScreenKey")}
              onClick={toggleFullscreen}
              className={button}
            >
              {fullscreen ? <Minimize className="size-5" /> : <Maximize className="size-5" />}
            </button>
          </div>
        </div>
      </div>

      <ChannelPanel
        open={panel}
        category={category}
        current={channel.id}
        now={now}
        onClose={() => setPanel(false)}
        onTune={(c, inCategory) => {
          onTune(c, inCategory);
          setPanel(false);
        }}
      />
    </div>
  );
}

/** What mpegts.js found in the stream is H.264 and AAC (or either alone), which the browser plays. */
function hasPlayableTracks(player: { mediaInfo?: unknown }): boolean {
  const info = player.mediaInfo as { videoCodec?: string; audioCodec?: string; hasVideo?: boolean; hasAudio?: boolean } | undefined;
  if (!info) return false;
  const video = !info.hasVideo || (info.videoCodec ?? "").startsWith("avc");
  const audio = !info.hasAudio || (info.audioCodec ?? "").startsWith("mp4a");
  return video && audio && (info.hasVideo || info.hasAudio) === true;
}

/** Keeps what the video shows as its poster: the picture stays while the next channel tunes. */
function keepPicture(video: HTMLVideoElement) {
  if (!video.videoWidth || video.readyState < 2) return;
  try {
    const canvas = document.createElement("canvas");
    const scale = Math.min(1, 960 / video.videoWidth);
    canvas.width = Math.round(video.videoWidth * scale);
    canvas.height = Math.round(video.videoHeight * scale);
    canvas.getContext("2d")?.drawImage(video, 0, 0, canvas.width, canvas.height);
    video.poster = canvas.toDataURL("image/jpeg", 0.75);
  } catch {
    // No picture to keep: black while it tunes.
  }
}

/** Who holds a connection: "You, on another device", their names, or how many. */
function holders(c: BusyConnection): string {
  if (c.yours) return t("player.you", { ns: "livetv" });
  if (c.watchers.length) return c.watchers.join(", ");
  return c.people === 1 ? t("player.someoneElse", { ns: "livetv" }) : t("player.people", { ns: "livetv", count: c.people });
}

function TroubleCard({
  trouble,
  channel,
  onRetry,
  onTakeOver,
  onBack,
  onChannels,
}: {
  trouble: Trouble;
  channel?: Channel;
  onRetry?: () => void;
  onTakeOver?: (channelId: number) => void;
  onBack: () => void;
  onChannels?: () => void;
}) {
  const { t } = useTranslation(["livetv", "common"]);
  const Icon = trouble.kind === "busy" ? Tv2 : TriangleAlert;
  const connections = trouble.connections ?? [];
  const anyToTake = connections.some((c) => c.can_take_over);
  return (
    <div className="w-full max-w-md rounded-2xl border border-white/10 bg-surface/95 p-6 text-center shadow-2xl animate-pop-in" role="alert">
      {channel ? (
        <ChannelLogo channel={channel} width={120} className="mx-auto mb-4 h-14 w-24 rounded-lg bg-white/[0.06]" />
      ) : (
        <Icon className={cn("mx-auto mb-4 size-10", trouble.kind === "busy" ? "text-accent" : "text-danger")} />
      )}
      <h2 className="text-lg font-semibold">{trouble.title}</h2>
      {connections.length ? (
        <>
          <p className="mt-2 text-sm text-muted [text-wrap:pretty]">
            {t("player.connectionsWatching", { count: connections.length })}
          </p>
          <ul className="mt-4 space-y-2 text-left">
            {connections.map((c) => (
              <li key={c.channel_id} className="flex items-center gap-3 rounded-lg border border-white/10 bg-white/[0.04] px-3 py-2.5">
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-medium">{c.channel_name}</div>
                  <div className="truncate text-xs text-muted">{holders(c)}</div>
                </div>
                {c.can_take_over && onTakeOver ? (
                  <button
                    type="button"
                    onClick={() => onTakeOver(c.channel_id)}
                    className="h-8 shrink-0 rounded-md bg-accent px-3 text-xs font-semibold text-accent-fg hover:bg-accent-hover"
                  >
                    {t("player.watchHere")}
                  </button>
                ) : null}
              </li>
            ))}
          </ul>
          {!anyToTake ? (
            <p className="mt-3 text-xs text-subtle [text-wrap:pretty]">
              {t("player.joinHint")}
            </p>
          ) : null}
        </>
      ) : trouble.message ? (
        <p className="mt-2 text-sm text-muted [text-wrap:pretty]">{trouble.message}</p>
      ) : null}
      <div className="mt-6 flex flex-wrap justify-center gap-2">
        {onRetry ? (
          <button type="button" onClick={onRetry} className="flex h-10 items-center gap-2 rounded-md bg-accent px-4 text-sm font-medium text-accent-fg hover:bg-accent-hover">
            <RefreshCw className="size-4" /> {trouble.kind === "stopped" ? t("player.watchAgain") : t("common:actions.tryAgain")}
          </button>
        ) : null}
        {onChannels ? (
          <button type="button" onClick={onChannels} className="flex h-10 items-center gap-2 rounded-md bg-surface-3 px-4 text-sm font-medium hover:bg-border">
            <List className="size-4" /> {t("player.otherChannels")}
          </button>
        ) : null}
        <button type="button" onClick={onBack} className="flex h-10 items-center rounded-md px-4 text-sm font-medium text-muted hover:bg-white/5 hover:text-fg">
          {t("player.backToGuide")}
        </button>
      </div>
    </div>
  );
}

