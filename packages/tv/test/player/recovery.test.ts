import { describe, expect, it } from "vitest";
import { Recovery, type EngineError } from "../../src/player/recovery";

const audio: EngineError = { kind: "unsupported", audio: true, container: false, detail: "audio" };
const video: EngineError = { kind: "unsupported", audio: false, container: false, detail: "video" };
const container: EngineError = { kind: "unsupported", audio: false, container: true, detail: "container" };

describe("audio the TV fails to play", () => {
  it("is refused at once, in the same mode, once per codec", () => {
    const r = new Recovery();
    expect(r.decide(audio, "direct", "hevc", "EAC3")).toEqual({ kind: "new_session", mode: null, reason: "audio_unplayable" });
    expect(r.refusedAudio).toEqual(["eac3"]);
    expect(r.fallbackMode).toBeNull();
    expect(r.fallbackReason).toBe("audio_unplayable");
    // The file's AC-3, which the server played next, fails too: refused as well.
    expect(r.decide(audio, "remux", "hevc", "ac3")).toEqual({ kind: "new_session", mode: null, reason: "audio_unplayable" });
    expect(r.refusedAudio).toEqual(["eac3", "ac3"]);
    // A codec refused already, AAC, or none known: the ladder as before.
    expect(r.decide(audio, "remux", "hevc", "eac3")).toEqual({ kind: "new_session", mode: "transcode", reason: "unplayable" });
    expect(r.decide(audio, "transcode", "hevc", "aac")).toEqual({ kind: "fail", failure: "unplayable", detail: "audio" });
    expect(r.refusedAudio).toEqual(["eac3", "ac3"]);
    expect(new Recovery().decide(audio, "direct", "h264", "aac")).toEqual({ kind: "new_session", mode: "remux", reason: "unplayable" });
    expect(new Recovery().decide(audio, "direct", "h264")).toEqual({ kind: "new_session", mode: "remux", reason: "unplayable" });
    // A video failure refuses nothing, whatever the audio.
    const v = new Recovery();
    expect(v.decide(video, "direct", "h264", "eac3")).toEqual({ kind: "new_session", mode: "transcode", reason: "unplayable" });
    expect(v.refusedAudio).toEqual([]);
  });

  it("keeps a fallback's mode", () => {
    const r = new Recovery();
    r.decide(container, "direct", "h264");
    expect(r.decide(audio, "remux", "h264", "truehd")).toEqual({ kind: "new_session", mode: "remux", reason: "audio_unplayable" });
    expect(r.fallbackMode).toBe("remux");
  });

  it("is forgotten for a new item, not on Retry or another quality", () => {
    const r = new Recovery();
    r.decide(audio, "direct", "h264", "eac3");
    r.retry();
    r.qualityChanged();
    expect(r.refusedAudio).toEqual(["eac3"]);
    r.newItem();
    expect(r.refusedAudio).toEqual([]);
    expect(r.decide(audio, "direct", "h264", "eac3")).toEqual({ kind: "new_session", mode: null, reason: "audio_unplayable" });
  });
});
