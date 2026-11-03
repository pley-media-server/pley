import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { readPictureTrack } from "pley-picture-subtitles/mks";
import { PictureSubtitles } from "pley-picture-subtitles/picture-track";
import { pgsMks as mks } from "@/test/pictures";
import { contain, fetchPictures, hlsLeadMs, hlsShiftMs, loadPictures, LOOKAHEAD_MS, PictureRenderer, PICTURES_AGAIN_MS, pictureUrlAt, planeBox, trackMs } from "./pictures";

const track = (cues: [number, number][]) => PictureSubtitles.of(readPictureTrack(mks(cues)))!;

/** FFmpeg's copy as it is made: its Segment's size isn't written yet. */
const growing = (file: Uint8Array) => {
  const out = file.slice();
  out.set([0x01, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff], 12 + out[11]! + 4);
  return out;
};

/** A box rounded to 1/100 px, for comparing. */
const rounded = (b: { x: number; y: number; width: number; height: number }) =>
  Object.fromEntries(Object.entries(b).map(([k, v]) => [k, Math.round(v * 100) / 100 + 0]));

// ---- timing and layout

describe("trackMs", () => {
  it("is the player's time in ms, a millisecond ahead", () => {
    expect(trackMs(4, 0, 0)).toBe(4000 + LOOKAHEAD_MS);
  });

  it("adds the session's offset and takes away the person's (positive: later)", () => {
    expect(trackMs(4, 250, 0)).toBe(4251);
    expect(trackMs(4, 0, 0.5)).toBe(3501);
    expect(trackMs(4, -80, -1)).toBe(4921);
  });

  it("shows a cue from the frame it starts on, though the frame's time falls a hair before it", () => {
    const subs = track([[4000, 4400]]);
    // A frame's time as the browser gives it, a float a hair short of 4 s.
    const frame = 4 - 1e-9;
    expect(subs.sampleAt(frame * 1000)).toBe(-1);
    expect(subs.sampleAt(trackMs(frame, 0, 0))).toBe(0);
    // And it's gone on the frame its end falls on.
    expect(subs.frameAt(trackMs(4.4 - 1e-9, 0, 0))).toBeNull();
  });
});

describe("hlsLeadMs", () => {
  const frag = (playlistOffset: number | undefined, startPTS?: number) => ({ playlistOffset, elementaryStreams: { video: startPTS === undefined ? undefined : { startPTS } } });

  it("is where the first picture lands against the segment's place in the playlist", () => {
    // A repackaged stream with B-frames: two frames late.
    expect(hlsLeadMs(frag(0, 0.0667))).toBeCloseTo(66.7, 5);
    // A conversion started at 20 s, segment 9.
    expect(hlsLeadMs(frag(18.006, 18.723))).toBeCloseTo(717, 5);
  });

  it("waits for a segment with video, and doesn't trust a lead past a second", () => {
    expect(hlsLeadMs(frag(0))).toBeUndefined();
    expect(hlsLeadMs(frag(undefined, 1))).toBeUndefined();
    expect(hlsLeadMs(frag(18.006, 19.237))).toBeNull();
  });
});

describe("hlsShiftMs", () => {
  it("maps hls.js's time to the file's by its initPTS and the server's timestamp offset", () => {
    // The first picture at 1.4 s in the stream, decoded two frames earlier
    // (hls.js starts there): at 0.0667 s of media time, 0 in the file.
    const initPtsMs = 1400 - 66.7;
    expect(0.0667 * 1000 + hlsShiftMs(1400, initPtsMs, 66.7)).toBeCloseTo(0, 5);
    // A conversion started part-way, whatever its first segment's lead.
    expect(hlsShiftMs(1400, 1400 + 18_006 - 18_723 - 66.7, 717)).toBeCloseTo(-783.7, 5);
  });

  it("takes the measured lead off without the server's offset or hls.js's initPTS", () => {
    expect(hlsShiftMs(undefined, 1333.3, 66.7)).toBeCloseTo(-66.7, 5);
    expect(hlsShiftMs(1400, null, 66.7)).toBeCloseTo(-66.7, 5);
    expect(hlsShiftMs(null, null, null)).toBe(0);
  });
});

describe("planeBox", () => {
  const view = { width: 1600, height: 900 };

  it("is the picture's box in the element: letterboxed, pillarboxed", () => {
    expect(rounded(contain(view, 1920, 800))).toEqual({ x: 0, y: 116.67, width: 1600, height: 666.67 });
    expect(rounded(contain({ width: 1000, height: 1000 }, 1920, 1080))).toEqual({ x: 0, y: 218.75, width: 1000, height: 562.5 });
    expect(rounded(contain({ width: 2000, height: 900 }, 1920, 1080))).toEqual({ x: 200, y: 0, width: 1600, height: 900 });
  });

  it("puts a plane of the video's shape over the picture", () => {
    expect(rounded(planeBox({ width: 2000, height: 900 }, { width: 1920, height: 1080 }, { width: 1920, height: 1080 }))).toEqual({
      x: 200,
      y: 0,
      width: 1600,
      height: 900,
    });
    // A 720p transcode of a 1080p film: the plane is the disc's.
    expect(rounded(planeBox(view, { width: 1280, height: 720 }, { width: 1920, height: 1080 }))).toEqual({ x: 0, y: 0, width: 1600, height: 900 });
  });

  it("stretches a DVD's plane over the picture, whose pixels aren't square", () => {
    // 720×576 stored, shown 16:9.
    expect(rounded(planeBox(view, { width: 1024, height: 576 }, { width: 720, height: 576 }))).toEqual({ x: 0, y: 0, width: 1600, height: 900 });
  });

  it("keeps a Blu-ray's plane in its own shape over a film cropped to 2.40:1: its lines go in the bars", () => {
    expect(rounded(planeBox(view, { width: 1920, height: 800 }, { width: 1920, height: 1080 }))).toEqual({ x: 0, y: 0, width: 1600, height: 900 });
    // On a screen as wide as the film, the plane fits inside it, never off it.
    const wide = planeBox({ width: 2400, height: 1000 }, { width: 1920, height: 800 }, { width: 1920, height: 1080 });
    expect(rounded(wide)).toEqual({ x: 311.11, y: 0, width: 1777.78, height: 1000 });
  });

  it("goes by the plane before the video's size is known", () => {
    expect(rounded(planeBox(view, { width: 0, height: 0 }, { width: 1920, height: 1080 }))).toEqual({ x: 0, y: 0, width: 1600, height: 900 });
  });
});

// ---- loading

describe("fetchPictures", () => {
  afterEach(() => vi.unstubAllGlobals());
  const answer = (res: Response | Error) =>
    vi.stubGlobal(
      "fetch",
      vi.fn(() => (res instanceof Error ? Promise.reject(res) : Promise.resolve(res))),
    );

  it("reads the track, whole or as far as the server has got", async () => {
    answer(new Response(mks([[1000, 2000]]), { status: 200 }));
    const whole = await fetchPictures("/t.mks", new AbortController().signal);
    expect(whole).toMatchObject({ kind: "track", complete: true });
    expect(whole.kind === "track" && whole.subs.sampleAt(1500)).toBe(0);

    answer(new Response(mks([[1000, 2000]]), { status: 200, headers: { "X-Pley-Complete": "0" } }));
    expect(await fetchPictures("/t.mks", new AbortController().signal)).toMatchObject({ kind: "track", complete: false });
  });

  it("asks with the page's credentials, for Matroska, and past the cache when asking for more", async () => {
    answer(new Response(mks([]), { status: 200 }));
    await fetchPictures("/t.mks", new AbortController().signal, null, true);
    const init = vi.mocked(fetch).mock.calls[0]?.[1];
    expect(init).toMatchObject({ credentials: "same-origin", cache: "no-store", headers: { Accept: "video/x-matroska" } });
  });

  it("says when the server can't make it (an older server or transcoder)", async () => {
    answer(new Response("{}", { status: 404 }));
    expect(await fetchPictures("/t.mks", new AbortController().signal)).toEqual({ kind: "unavailable" });
    answer(new Response("{}", { status: 503 }));
    expect(await fetchPictures("/t.mks", new AbortController().signal)).toEqual({ kind: "unavailable" });
  });

  it("fails on anything else", async () => {
    answer(new Response("{}", { status: 500 }));
    expect(await fetchPictures("/t.mks", new AbortController().signal)).toEqual({ kind: "failed" });
    answer(new TypeError("Failed to fetch"));
    expect(await fetchPictures("/t.mks", new AbortController().signal)).toEqual({ kind: "failed" });
    answer(new Response(new Uint8Array([1, 2, 3, 4]), { status: 200 }));
    expect(await fetchPictures("/t.mks", new AbortController().signal)).toEqual({ kind: "failed" });
  });

  it("waits for more of a file too short to read yet", async () => {
    answer(new Response(mks([]).slice(0, 40), { status: 200, headers: { "X-Pley-Complete": "0" } }));
    expect(await fetchPictures("/t.mks", new AbortController().signal)).toMatchObject({ kind: "early" });
  });

  it("asks for the rest of what it has, and takes the whole file from a server that sends it again", async () => {
    const file = new Uint8Array(mks([[1000, 2000], [5000, 6000]]));
    const have = file.slice(0, file.length - 30);
    answer(new Response(file.slice(have.length), { status: 206, headers: { "Content-Range": `bytes ${have.length}-${file.length - 1}/*`, "X-Pley-Complete": "0" } }));
    const more = await fetchPictures("/t.mks", new AbortController().signal, have, true);
    expect(vi.mocked(fetch).mock.calls[0]?.[1]?.headers).toMatchObject({ Range: `bytes=${have.length}-` });
    expect(more.kind === "track" && [more.bytes.length, more.subs.sampleAt(5500)]).toEqual([file.length, 2]);

    answer(new Response(file, { status: 200, headers: { "X-Pley-Complete": "0" } }));
    const again = await fetchPictures("/t.mks", new AbortController().signal, have, true);
    expect(again.kind === "track" && again.bytes.length).toBe(file.length);

    answer(new Response(null, { status: 416 }));
    expect(await fetchPictures("/t.mks", new AbortController().signal, have, true)).toEqual({ kind: "early", bytes: have });
  });

  it("asks for all of a track once it is whole: what came before may not be its start", async () => {
    const file = new Uint8Array(mks([[1000, 2000], [5000, 6000]]));
    const have = file.slice(0, 50);
    answer(new Response(file.slice(50), { status: 206, headers: { "Content-Range": `bytes 50-${file.length - 1}/${file.length}`, "X-Pley-Complete": "1" } }));
    expect(await fetchPictures("/t.mks", new AbortController().signal, have, true)).toEqual({ kind: "anew" });
    answer(new Response(null, { status: 416, headers: { "X-Pley-Complete": "1" } }));
    expect(await fetchPictures("/t.mks", new AbortController().signal, have, true)).toEqual({ kind: "anew" });
  });

  it("asks for all of another file after one that came whole, keeps the same one, and takes the rest of one still being written", async () => {
    const have = new Uint8Array(mks([[1000, 2000], [5000, 6000]]));
    const fewer = new Uint8Array(mks([[20_000, 21_000]]));
    const none = (length: number) => new Response(null, { status: 416, headers: { "Content-Range": `bytes */${length}`, "X-Pley-Complete": "0" } });
    // The pictures around where the player is now, fewer than those it has:
    // nothing past them, and a length of their own.
    answer(none(fewer.length));
    expect(await fetchPictures("/t.mks", new AbortController().signal, have, true)).toEqual({ kind: "anew" });
    expect(vi.mocked(fetch).mock.calls[0]?.[1]?.headers).toMatchObject({ Range: `bytes=${have.length}-` });
    // The same file again: kept, not asked for again.
    answer(none(have.length));
    expect(await fetchPictures("/t.mks", new AbortController().signal, have, true)).toEqual({ kind: "early", bytes: have });
    // No length (a proxy that drops it): maybe another place's, asked for
    // whole. Of a file still being written, nothing new (above).
    answer(new Response(null, { status: 416, headers: { "X-Pley-Complete": "0" } }));
    expect(await fetchPictures("/t.mks", new AbortController().signal, have, true)).toEqual({ kind: "anew" });

    // More past a file that came whole: another file.
    const one = new Uint8Array(mks([[1000, 2000]]));
    const partial = { status: 206, headers: { "Content-Range": `bytes ${one.length}-${have.length - 1}/*`, "X-Pley-Complete": "0" } };
    answer(new Response(have.slice(one.length), partial));
    expect(await fetchPictures("/t.mks", new AbortController().signal, one, true)).toEqual({ kind: "anew" });

    const file = growing(have);
    answer(new Response(file.slice(one.length), partial));
    const more = await fetchPictures("/t.mks", new AbortController().signal, file.slice(0, one.length), true);
    expect(more.kind === "track" && more.subs.sampleAt(5500)).toBe(2);
  });

  it("keeps nothing of a partial answer without a picture: the next may be another file", async () => {
    answer(new Response(mks([]), { status: 200, headers: { "X-Pley-Complete": "0" } }));
    expect(await fetchPictures("/t.mks", new AbortController().signal)).toEqual({ kind: "early", bytes: null });
  });
});

describe("loadPictures", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  function server(answers: (() => Response)[]) {
    let n = 0;
    const fetchFn = vi.fn(() => Promise.resolve((answers[Math.min(n++, answers.length - 1)] ?? (() => new Response(null, { status: 500 })))()));
    vi.stubGlobal("fetch", fetchFn);
    return fetchFn;
  }
  const hooks = () => ({ onTrack: vi.fn<(s: PictureSubtitles) => void>(), onUnavailable: vi.fn(), onFailed: vi.fn() });
  const partial = (cues: [number, number][]) => () => new Response(mks(cues), { status: 200, headers: { "X-Pley-Complete": "0" } });
  const copying = (cues: [number, number][]) => () => new Response(growing(new Uint8Array(mks(cues))), { status: 200, headers: { "X-Pley-Complete": "0" } });
  const whole = (cues: [number, number][]) => () => new Response(mks(cues), { status: 200 });
  const ranges = (fetchFn: ReturnType<typeof server>) =>
    fetchFn.mock.calls.map((c) => ((c as unknown[])[1] as RequestInit).headers as Record<string, string>).map((h) => h.Range ?? null);

  it("asks again while the answer is partial, and stops once it is whole", async () => {
    const fetchFn = server([copying([[1000, 2000]]), copying([[1000, 2000]]), whole([[1000, 2000], [5000, 6000]])]);
    const h = hooks();
    loadPictures("/t.mks", h);
    await vi.advanceTimersByTimeAsync(0);
    expect(h.onTrack).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(PICTURES_AGAIN_MS);
    await vi.advanceTimersByTimeAsync(PICTURES_AGAIN_MS);
    expect(h.onTrack).toHaveBeenCalledTimes(3);
    expect(h.onTrack.mock.calls[2]?.[0].sampleAt(5500)).toBe(2);
    await vi.advanceTimersByTimeAsync(PICTURES_AGAIN_MS * 3);
    // The rest of FFmpeg's copy, then the whole file once more now that it is finished.
    expect(fetchFn).toHaveBeenCalledTimes(4);
    const asked = ranges(fetchFn);
    expect(asked[0]).toBeNull();
    expect(asked[1]).toMatch(/^bytes=\d+-$/);
    expect(asked[3]).toBeNull();
    expect(h.onUnavailable).not.toHaveBeenCalled();
    expect(h.onFailed).not.toHaveBeenCalled();
  });

  it("takes fewer pictures for where the player is now in place of those it has, and downloads the same ones once", async () => {
    const files = [new Uint8Array(mks([[1000, 2000], [5000, 6000]])), new Uint8Array(mks([[20_000, 21_000]]))];
    let n = 0;
    // As the server answers a Range: nothing past a file no longer than it, and its length.
    const fetchFn = vi.fn((_url: string, init?: RequestInit) => {
      const body = files[Math.min(n++, files.length - 1)]!;
      const from = /^bytes=(\d+)-$/.exec((init?.headers as Record<string, string>).Range ?? "");
      const headers = { "X-Pley-Complete": "0" };
      if (from && Number(from[1]) >= body.length) {
        return Promise.resolve(new Response(null, { status: 416, headers: { ...headers, "Content-Range": `bytes */${body.length}` } }));
      }
      return Promise.resolve(new Response(body, { status: 200, headers }));
    });
    vi.stubGlobal("fetch", fetchFn);
    const h = hooks();
    loadPictures("/w.mks", h);
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(PICTURES_AGAIN_MS);
    expect(h.onTrack).toHaveBeenCalledTimes(2);
    expect(h.onTrack.mock.calls[1]?.[0].sampleAt(20_500)).toBe(0);
    // The same pictures again: nothing past them, and their length.
    await vi.advanceTimersByTimeAsync(PICTURES_AGAIN_MS);
    expect(fetchFn).toHaveBeenCalledTimes(4);
    expect(h.onTrack).toHaveBeenCalledTimes(2);
  });

  it("tells once when the server can't make it, and asks no more", async () => {
    const fetchFn = server([() => new Response("{}", { status: 404 })]);
    const h = hooks();
    loadPictures("/t.mks", h);
    await vi.advanceTimersByTimeAsync(PICTURES_AGAIN_MS * 3);
    expect(h.onUnavailable).toHaveBeenCalledTimes(1);
    expect(fetchFn).toHaveBeenCalledTimes(1);
  });

  it("asks from where the player is each time, so the server reads the pictures there first", async () => {
    const fetchFn = server([partial([[1000, 2000]]), partial([[1000, 2000]]), whole([[1000, 2000]])]);
    const urls = () => fetchFn.mock.calls.map((c) => (c as unknown[])[0]);
    let at = 600_000.4;
    loadPictures("/at.mks", hooks(), () => at);
    await vi.advanceTimersByTimeAsync(0);
    at = 603_000;
    await vi.advanceTimersByTimeAsync(PICTURES_AGAIN_MS * 3);
    // The last, all of it once it is whole.
    expect(urls()).toEqual(["/at.mks?at=600000", "/at.mks?at=603000", "/at.mks?at=603000", "/at.mks?at=603000"]);
    // Had whole: asked for as it was, which the browser's cache keeps.
    fetchFn.mockClear();
    loadPictures("/at.mks", hooks(), () => 900_000);
    await vi.advanceTimersByTimeAsync(0);
    expect(urls()).toEqual(["/at.mks"]);
    // At the start too: a copy under way from elsewhere would answer with the pictures there.
    expect(pictureUrlAt("/t.mks", 0)).toBe("/t.mks?at=0");
    expect(pictureUrlAt("/t.mks", Number.NaN)).toBe("/t.mks?at=0");
    expect(pictureUrlAt("/t.mks?k=1", 5000)).toBe("/t.mks?k=1&at=5000");
  });

  it("asks again at once from where a seek went, and takes no answer from before it", async () => {
    let answered = null as ((r: Response) => void) | null;
    const fetchFn = vi.fn((url: string) =>
      url.endsWith("at=0") ? new Promise<Response>((resolve) => (answered = resolve)) : Promise.resolve(partial([[20_000, 21_000]])()),
    );
    vi.stubGlobal("fetch", fetchFn);
    const h = hooks();
    let at = 0;
    const loading = loadPictures("/s.mks", h, () => at);
    await vi.advanceTimersByTimeAsync(0);
    at = 200_000;
    loading.sought();
    await vi.advanceTimersByTimeAsync(0);
    expect(fetchFn.mock.calls.map((c) => c[0])).toEqual(["/s.mks?at=0", "/s.mks?at=200000"]);
    // The ask from the start, still under way, is dropped: it would hold a
    // connection the video needs while the server reads there.
    const signals = () => fetchFn.mock.calls.map((c) => ((c as unknown[])[1] as RequestInit).signal?.aborted);
    expect(signals()).toEqual([true, false]);
    expect(h.onTrack).toHaveBeenCalledTimes(1);
    expect(h.onTrack.mock.calls[0]?.[0].sampleAt(20_500)).toBe(0);
    // The answer from the start comes late: not what shows.
    answered!(partial([[1000, 2000]])());
    await vi.advanceTimersByTimeAsync(0);
    expect(h.onTrack).toHaveBeenCalledTimes(1);
    // A seek once it is whole asks for nothing.
    fetchFn.mockImplementation(() => Promise.resolve(whole([[1000, 2000]])()));
    loading.sought();
    await vi.advanceTimersByTimeAsync(0);
    const asked = fetchFn.mock.calls.length;
    loading.sought();
    await vi.advanceTimersByTimeAsync(PICTURES_AGAIN_MS * 2);
    expect(fetchFn).toHaveBeenCalledTimes(asked);
  });

  it("stops when told, a request under way included", async () => {
    const fetchFn = server([partial([[1000, 2000]])]);
    const h = hooks();
    loadPictures("/t.mks", h).stop();
    await vi.advanceTimersByTimeAsync(PICTURES_AGAIN_MS * 3);
    expect(h.onTrack).not.toHaveBeenCalled();
    expect(fetchFn).toHaveBeenCalledTimes(1);
    expect(((vi.mocked(fetchFn).mock.calls[0] as unknown[] | undefined)?.[1] as RequestInit).signal?.aborted).toBe(true);
  });
});

// ---- drawing

/** A 2D context that records what is drawn where. */
function fakeContext() {
  return {
    drawn: [] as [number, number, number, number][],
    clears: 0,
    imageSmoothingEnabled: false,
    imageSmoothingQuality: "low",
    clearRect() {
      this.clears++;
      this.drawn.length = 0;
    },
    drawImage(_image: unknown, x: number, y: number, w: number, h: number) {
      this.drawn.push([x, y, w, h]);
    },
    createImageData: (w: number, h: number) => ({ data: new Uint8ClampedArray(w * h * 4), width: w, height: h }),
    putImageData() {},
  };
}

/** A video element that presents frames on demand (`present`), with `requestVideoFrameCallback` or without. */
function fakeVideo({ frames = true, width = 1600, height = 900 } = {}) {
  const video = document.createElement("video");
  let callbacks: ((now: number, meta: VideoFrameCallbackMetadata) => void)[] = [];
  Object.defineProperties(video, {
    clientWidth: { value: width, configurable: true },
    clientHeight: { value: height, configurable: true },
    videoWidth: { value: 1920, configurable: true },
    videoHeight: { value: 1080, configurable: true },
    readyState: { value: 4, writable: true, configurable: true },
    paused: { value: false, writable: true, configurable: true },
    seeking: { value: false, writable: true, configurable: true },
    currentTime: { value: 0, writable: true, configurable: true },
  });
  if (frames) {
    Object.assign(video, {
      requestVideoFrameCallback: (cb: (now: number, meta: VideoFrameCallbackMetadata) => void) => {
        callbacks.push(cb);
        return callbacks.length;
      },
      cancelVideoFrameCallback: () => {
        callbacks = [];
      },
    });
  } else {
    Object.assign(video, { requestVideoFrameCallback: undefined });
  }
  const present = (mediaTime: number) => {
    const now = callbacks;
    callbacks = [];
    for (const cb of now) cb(0, { mediaTime } as VideoFrameCallbackMetadata);
  };
  return { video, present };
}

function setup(opts: Parameters<typeof fakeVideo>[0] = {}) {
  const { video, present } = fakeVideo(opts);
  const canvas = document.createElement("canvas");
  const g = fakeContext();
  vi.spyOn(canvas, "getContext").mockReturnValue(g as unknown as CanvasRenderingContext2D);
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(g as unknown as CanvasRenderingContext2D);
  const renderer = new PictureRenderer(video, canvas);
  return { video, canvas, g, present, renderer };
}

describe("PictureRenderer", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("backs the canvas with the element's device pixels", () => {
    vi.stubGlobal("devicePixelRatio", 2);
    const { canvas } = setup();
    expect([canvas.width, canvas.height]).toEqual([3200, 1800]);
  });

  it("backs the canvas afresh when the pixel ratio changes unannounced", () => {
    const { canvas, present, renderer } = setup();
    renderer.setTrack(track([[0, 9000]]));
    vi.stubGlobal("devicePixelRatio", 2);
    present(1);
    expect([canvas.width, canvas.height]).toEqual([3200, 1800]);
  });

  it("draws the cue of the frame presented, by that frame's own time, not the element's", () => {
    const { video, g, present, renderer } = setup();
    renderer.setTrack(track([[4000, 4400]]));
    expect(g.drawn).toEqual([]);
    // The element's time runs ahead of the frame on screen.
    Object.assign(video, { currentTime: 4.02 });
    video.dispatchEvent(new Event("timeupdate"));
    expect(g.drawn).toEqual([]);
    present(3.9667);
    expect(g.drawn).toEqual([]);
    present(4.0);
    // 1920×1080 plane over a 1600×900 element: the object at (100, 900), 4×2.
    expect(g.drawn).toEqual([[83, 750, 4, 2]]);
    present(4.4);
    expect(g.drawn).toEqual([]);
  });

  it("decodes once, and draws again only when what shows changes", () => {
    const { g, present, renderer } = setup();
    renderer.setTrack(track([[4000, 4400]]));
    present(4.0);
    const clears = g.clears;
    present(4.033);
    present(4.067);
    expect(g.clears).toBe(clears);
  });

  it("lays the plane over the picture of a letterboxed element, at device pixels", () => {
    vi.stubGlobal("devicePixelRatio", 2);
    const { g, present, renderer } = setup({ width: 1000, height: 1000 });
    renderer.setTrack(track([[0, 4000]]));
    present(1);
    // A 1000×562.5 picture 218.75 px down, at 2 device pixels a pixel.
    const s = (1000 / 1920) * 2;
    expect(g.drawn).toEqual([[Math.round(100 * s), Math.round(437.5 + 900 * s), Math.round(104 * s) - Math.round(100 * s), Math.round(437.5 + 902 * s) - Math.round(437.5 + 900 * s)]]);
  });

  it("moves with the session's and the person's offsets", () => {
    const { video, g, renderer } = setup();
    renderer.setTrack(track([[4000, 4400]]));
    Object.assign(video, { paused: true, currentTime: 4.5 });
    video.dispatchEvent(new Event("pause"));
    expect(g.drawn).toEqual([]);
    // Shown 0.5 s later: what the file has at 4.0 shows at 4.5.
    renderer.setOffsets(0, 0.5);
    expect(g.drawn).toHaveLength(1);
    // The session's time is 300 ms behind the file's.
    renderer.setOffsets(300, 0.5);
    expect(g.drawn).toHaveLength(1);
    renderer.setOffsets(150, 0);
    expect(g.drawn).toEqual([]);
  });

  it("shows what a seek lands on while paused, from the element's time", () => {
    const { video, g, renderer } = setup();
    renderer.setTrack(track([[4000, 4400], [7000, 7400]]));
    Object.assign(video, { paused: true, seeking: true, currentTime: 7.1 });
    video.dispatchEvent(new Event("seeking"));
    expect(g.drawn).toHaveLength(1);
    Object.assign(video, { seeking: false, currentTime: 8 });
    video.dispatchEvent(new Event("seeked"));
    expect(g.drawn).toEqual([]);
  });

  it("goes by the frame on screen when paused, wherever in it the element's time falls", () => {
    const { video, g, present, renderer } = setup();
    renderer.setTrack(track([[3500, 3900]]));
    Object.assign(video, { paused: true, currentTime: 3.8999 });
    present(3.8667);
    expect(g.drawn).toHaveLength(1);
    // The seek's time a hair before the next frame, past the cue's end with the look-ahead.
    Object.assign(video, { currentTime: 3.9 });
    video.dispatchEvent(new Event("seeked"));
    expect(g.drawn).toHaveLength(1);
    // A jump forgets that frame.
    Object.assign(video, { seeking: true, currentTime: 3.95 });
    video.dispatchEvent(new Event("seeking"));
    expect(g.drawn).toEqual([]);
  });

  it("clears for a reset element, unless held for the next session", () => {
    const { video, g, present, renderer } = setup();
    renderer.setTrack(track([[0, 9000]]));
    present(1);
    expect(g.drawn).toHaveLength(1);
    renderer.setHold(true);
    Object.assign(video, { readyState: 0, currentTime: 0 });
    video.dispatchEvent(new Event("emptied"));
    expect(g.drawn).toHaveLength(1);
    renderer.setHold(false);
    expect(g.drawn).toEqual([]);
  });

  it("follows animation frames where frames don't say their time", () => {
    const raf: FrameRequestCallback[] = [];
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => raf.push(cb));
    vi.stubGlobal("cancelAnimationFrame", () => undefined);
    const { video, g, renderer } = setup({ frames: false });
    renderer.setTrack(track([[4000, 4400]]));
    video.dispatchEvent(new Event("playing"));
    Object.assign(video, { currentTime: 4.1 });
    raf.splice(0).forEach((cb) => cb(0));
    expect(g.drawn).toHaveLength(1);
    expect(raf).toHaveLength(1);
  });

  it("draws nothing once destroyed", () => {
    const { g, present, renderer } = setup();
    renderer.setTrack(track([[0, 9000]]));
    renderer.destroy();
    present(1);
    expect(g.drawn).toEqual([]);
  });
});
