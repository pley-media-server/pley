import { describe, expect, it } from "vitest";
import { Cancel } from "../../src/net/transport";
import { segmentAt, variantsOf, warmStream, type StreamFetch } from "../../src/player/prepare";

const BASE = "http://nas:7539/api/media/pley_mk_KEY/master.m3u8";

describe("a stream's playlists", () => {
  it("list its sizes in their order, against the playlist's address", () => {
    const master = ["#EXTM3U", "#EXT-X-STREAM-INF:BANDWIDTH=8000000,RESOLUTION=1920x1080", "v0/index.m3u8", "", "#EXT-X-STREAM-INF:BANDWIDTH=4000000", "v1/index.m3u8"].join("\r\n");
    expect(variantsOf(master, BASE)).toEqual(["http://nas:7539/api/media/pley_mk_KEY/v0/index.m3u8", "http://nas:7539/api/media/pley_mk_KEY/v1/index.m3u8"]);
    expect(variantsOf("#EXTM3U\n#EXTINF:4,\n0.ts", BASE)).toEqual([]);
  });

  it("name the segment holding a time, the last for one past the end", () => {
    const media = ["#EXTM3U", "#EXTINF:4.000,", "0.ts", "#EXTINF:4.000,", "1.ts", "#EXTINF:2.5,", "2.ts", "#EXT-X-ENDLIST"].join("\n");
    expect(segmentAt(media, BASE, 0)).toBe("http://nas:7539/api/media/pley_mk_KEY/0.ts");
    expect(segmentAt(media, BASE, 4_000)).toBe("http://nas:7539/api/media/pley_mk_KEY/1.ts");
    expect(segmentAt(media, BASE, 9_000)).toBe("http://nas:7539/api/media/pley_mk_KEY/2.ts");
    expect(segmentAt(media, BASE, 60_000)).toBe("http://nas:7539/api/media/pley_mk_KEY/2.ts");
    expect(segmentAt("#EXTM3U", BASE, 0)).toBeNull();
  });
});

describe("warming a stream", () => {
  it("asks for a single-size stream's segment directly, and is done when anything fails", async () => {
    const asked: string[] = [];
    const fetch: StreamFetch = {
      text: async (url) => {
        asked.push(`GET ${url}`);
        return "#EXTM3U\n#EXTINF:6,\n0.ts\n#EXTINF:6,\n1.ts";
      },
      head: async (url) => {
        asked.push(`HEAD ${url}`);
        throw new Error("404");
      },
    };
    await warmStream(fetch, "http://nas:7539/api/media/k/index.m3u8", 7_000, new Cancel());
    expect(asked).toEqual(["GET http://nas:7539/api/media/k/index.m3u8", "HEAD http://nas:7539/api/media/k/1.ts"]);
  });

  it("is done at once when canceled, whatever still runs", async () => {
    const cancel = new Cancel();
    const fetch: StreamFetch = { text: () => new Promise(() => undefined), head: () => new Promise(() => undefined) };
    const done = warmStream(fetch, BASE, 0, cancel);
    cancel.cancel();
    await expect(done).resolves.toBeUndefined();
  });
});
