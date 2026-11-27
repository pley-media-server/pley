import { describe, expect, it } from "vitest";
import { audioKind, SEEK_MEMORY_MS, SEEK_REFUSALS_TRUSTED, seekKind, storedAudioSeekMemory, storedSeekMemory } from "../../src/player/seekmemory";

function store() {
  const m = new Map<string, string>();
  return { get: (k: string) => m.get(k) ?? null, set: (k: string, v: string) => void m.set(k, v) };
}

describe("files this TV's player can't seek in", () => {
  it("knows a kind once refused on enough different items", () => {
    const memory = storedSeekMemory(store(), "k", () => 0);
    for (let id = 1; id < SEEK_REFUSALS_TRUSTED; id++) memory.refused("mkv/hevc", id);
    memory.refused("mkv/hevc", 1);
    expect(memory.known("mkv/hevc")).toBe(false);
    memory.refused("mkv/hevc", 99);
    expect(memory.known("mkv/hevc")).toBe(true);
    expect(memory.known("mp4/hevc")).toBe(false);
  });

  it("tries seeks in such files again after a while (a firmware update, files remade)", () => {
    let now = 0;
    const s = store();
    const memory = storedSeekMemory(s, "k", () => now);
    memory.refused("mkv/hevc", 1);
    memory.refused("mkv/hevc", 2);
    now = SEEK_MEMORY_MS - 1;
    expect(memory.known("mkv/hevc")).toBe(true);
    now = SEEK_MEMORY_MS;
    expect(memory.known("mkv/hevc")).toBe(false);
    // Learned again from new refusals.
    memory.refused("mkv/hevc", 3);
    expect(memory.known("mkv/hevc")).toBe(false);
    memory.refused("mkv/hevc", 4);
    expect(memory.known("mkv/hevc")).toBe(true);
  });

  it("forgets what storage can't read", () => {
    const s = store();
    s.set("k", "not json");
    expect(storedSeekMemory(s, "k").known("mkv/hevc")).toBe(false);
  });

  it("names a file by its container and video codec", () => {
    expect(seekKind({ container: "matroska,webm", video_codec: "HEVC" } as never)).toBe("mkv/hevc");
    expect(seekKind({ container: null, video_codec: "hevc" } as never)).toBeNull();
  });
});

describe("kinds of file the TV's <video> seeks in where AVPlay refuses", () => {
  it("knows a kind once <video> made a seek AVPlay refused there, and forgets it after a while", () => {
    let now = 0;
    const memory = storedSeekMemory(store(), "k", () => now);
    expect(memory.seeksViaElement("mkv/hevc")).toBe(false);
    memory.seekedViaElement("mkv/hevc", 1);
    expect(memory.seeksViaElement("mkv/hevc")).toBe(true);
    expect(memory.seeksViaElement("mp4/hevc")).toBe(false);
    // Kept apart from the refusals.
    expect(memory.known("mkv/hevc")).toBe(false);
    now = SEEK_MEMORY_MS;
    expect(memory.seeksViaElement("mkv/hevc")).toBe(false);
  });

  it("survives refusals recorded after it", () => {
    const memory = storedSeekMemory(store(), "k", () => 0);
    memory.seekedViaElement("mkv/hevc", 1);
    memory.refused("mkv/hevc", 2);
    memory.refused("mkv/hevc", 3);
    expect(memory.seeksViaElement("mkv/hevc")).toBe(true);
    expect(memory.known("mkv/hevc")).toBe(true);
  });
});

describe("how music reaches a place in each kind of audio file on this TV", () => {
  it("names a file's kind by its address's extension", () => {
    expect(audioKind("http://nas:7539/api/media/pley_mk_1/file.FLAC")).toBe("flac");
    expect(audioKind("/api/media/k/file.mp3#t=12.5")).toBe("mp3");
    expect(audioKind("/api/media/k/file")).toBe(null);
  });

  it("starts at a plain move until a later step is learned, keeps it 30 days, and forgets it once a plain move works", () => {
    let now = 0;
    const s = store();
    const memory = storedAudioSeekMemory(s, "k", () => now);
    expect(memory.step("flac")).toBe(1);
    memory.landed("flac", 3);
    memory.landed("ogg", 2);
    expect([memory.step("flac"), memory.step("ogg"), memory.step("mp3")]).toEqual([3, 2, 1]);
    now = SEEK_MEMORY_MS;
    expect(memory.step("flac")).toBe(1);
    now = 0;
    memory.landed("flac", 1);
    expect([memory.step("flac"), memory.step("ogg")]).toEqual([1, 2]);
  });

  it("tries every step when storage fails or holds something else", () => {
    const failing = { get: () => { throw new Error("blocked"); }, set: () => { throw new Error("full"); } };
    const memory = storedAudioSeekMemory(failing, "k");
    expect(() => memory.landed("flac", 3)).not.toThrow();
    expect(memory.step("flac")).toBe(1);
    const s = store();
    s.set("k", JSON.stringify({ "audio:flac": { step: 7, at: Date.now() } }));
    expect(storedAudioSeekMemory(s, "k").step("flac")).toBe(1);
  });
});
