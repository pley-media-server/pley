import { useEffect, useRef, useState, type RefObject } from "react";
import { loadPictures, PictureRenderer, type PictureSubtitles } from "./pictures";

/** What the player hears of a picture track, besides the track itself. */
export interface PictureTrackHooks {
  /** The server can't make picture subtitles' files (404 from an older server, 503 from an older transcoder). */
  unavailable(): void;
  /** The track couldn't be loaded. */
  failed(): void;
  /** Where the player is (ms of the file's time) as a track starts loading: the server reads one it hasn't yet from there first. */
  position(): number;
}

/**
 * The picture track at `url` (the server's `.mks` of the subtitles chosen;
 * `null`: none), as far as it has arrived, asked for again while the
 * server answers with part of it (`X-Pley-Complete: 0`); and whether the
 * server can't make such files, which holds for every track from then on
 * (none is asked for any more). `hooks` hears either problem first, and
 * says where the player is.
 */
export function usePictureTrack(url: string | null, hooks: RefObject<PictureTrackHooks>): { subs: PictureSubtitles | null; unavailable: boolean } {
  const [track, setTrack] = useState<{ url: string; subs: PictureSubtitles } | null>(null);
  const [unavailable, setUnavailable] = useState(false);
  useEffect(() => {
    if (!url || unavailable) return;
    return loadPictures(
      url,
      {
        onTrack: (subs) => setTrack({ url, subs }),
        onUnavailable: () => {
          hooks.current.unavailable();
          setUnavailable(true);
        },
        onFailed: () => hooks.current.failed(),
      },
      hooks.current.position(),
    );
  }, [url, unavailable, hooks]);
  return { subs: !unavailable && track && track.url === url ? track.subs : null, unavailable };
}

/**
 * Picture subtitles (PGS, VobSub, DVB) over the video, drawn by pley from
 * the server's `.mks` of the track (pictures.ts): a canvas laid over the
 * video element, inside the player, so it goes full screen with it.
 * `sessionOffsetMs` is the session's (pictures.ts, `hlsShiftMs`), `offset` the
 * seconds the person moved subtitles by (positive: later); `hold` keeps
 * what shows while the last session's picture stands in for the next one's.
 */
export function PictureSubtitleOverlay({
  videoRef,
  subs,
  sessionOffsetMs,
  offset,
  hold,
}: {
  videoRef: RefObject<HTMLVideoElement | null>;
  subs: PictureSubtitles | null;
  sessionOffsetMs: number;
  offset: number;
  hold: boolean;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const renderer = useRef<PictureRenderer | null>(null);
  useEffect(() => {
    const video = videoRef.current;
    const canvas = canvasRef.current;
    if (!video || !canvas) return;
    const r = new PictureRenderer(video, canvas);
    renderer.current = r;
    return () => {
      r.destroy();
      renderer.current = null;
    };
  }, [videoRef]);
  // After the renderer's own effect, which runs first: each starts it with what it has.
  useEffect(() => renderer.current?.setTrack(subs), [subs]);
  useEffect(() => renderer.current?.setOffsets(sessionOffsetMs, offset), [sessionOffsetMs, offset]);
  useEffect(() => renderer.current?.setHold(hold), [hold]);
  return <canvas ref={canvasRef} aria-hidden className="pointer-events-none absolute inset-0 size-full" data-testid="picture-subtitles" />;
}
