import { describe, expect, it } from "vitest";
import type { LiveChannel, LiveChannelPage } from "../../src/api/types";
import { GuideStore } from "../../src/live/guide";
import { ChannelPages, type LiveBrowser } from "../../src/live/pages";

function channel(id: number): LiveChannel {
  return { id, source_id: 1, name: `Ch ${id}`, number: String(id), group: "G", logo: null, favorite: false, has_guide: true, now: null, next: null };
}

/** A server with `total` channels, ids 1 to `total`, and what it was asked. */
function server(total: number) {
  const asked: string[] = [];
  const api: LiveBrowser = {
    liveBrowse: (q) => {
      asked.push(q.around !== undefined ? `${q.category} around ${q.around}` : `${q.category}${q.group ? ` ${q.group}` : ""} @${q.offset ?? 0}`);
      const limit = q.limit ?? 100;
      const offset = q.around !== undefined ? Math.floor((q.around - 1) / limit) * limit : q.offset ?? 0;
      const channels: LiveChannel[] = [];
      for (let i = offset; i < Math.min(total, offset + limit); i++) channels.push(channel(i + 1));
      return Promise.resolve<LiveChannelPage>({ total, offset, channels });
    },
  };
  return { api, asked };
}

const settle = () => new Promise((r) => setTimeout(r, 0));

describe("a category's channels", () => {
  it("come a page at a time as they show, the first saying how many", async () => {
    const { api, asked } = server(57_176);
    const pages = new ChannelPages(api, { kind: "group", group: "UK| Sport" });
    pages.want(0, 12);
    await settle();
    expect(asked).toEqual(["group UK| Sport @0"]);
    expect(pages.total.value).toBe(57_176);
    expect(pages.at(11)?.id).toBe(12);
    expect(pages.at(150)).toBeUndefined();
    pages.want(190, 210);
    await settle();
    // The first page is fresh; the second and third come.
    expect(asked).toEqual(["group UK| Sport @0", "group UK| Sport @100", "group UK| Sport @200"]);
    expect(pages.at(205)?.id).toBe(206);
  });

  it("know where a channel is by the page around it", async () => {
    const { api, asked } = server(57_176);
    const pages = new ChannelPages(api, { kind: "all" });
    expect(await pages.indexOf(30_042)).toBe(30_041);
    expect(asked).toEqual(["all around 30042"]);
    // Known now: not asked again.
    expect(await pages.indexOf(30_050)).toBe(30_049);
    expect(asked).toHaveLength(1);
  });

  it("go round their ends for channel up and down", async () => {
    const { api } = server(250);
    const pages = new ChannelPages(api, { kind: "favorites" }, { frozen: true });
    expect((await pages.neighbor(250, 1))?.id).toBe(1);
    expect((await pages.neighbor(1, -1))?.id).toBe(250);
    expect((await pages.neighbor(100, 1))?.id).toBe(101);
    expect((await pages.neighbor(42, -1))?.id).toBe(41);
  });

  it("show a favorite marked at once", async () => {
    const { api } = server(10);
    const pages = new ChannelPages(api, { kind: "all" });
    pages.want(0, 10);
    await settle();
    pages.patch(3, { favorite: true });
    expect(pages.at(2)?.favorite).toBe(true);
  });
});

describe("the guide of the rows in view", () => {
  it("asks only for the channels shown, and only once a chunk", async () => {
    const asked: string[] = [];
    const store = new GuideStore(
      { liveGuide: (from, _to, ids) => (asked.push(`${from}: ${ids.join(",")}`), Promise.resolve({ from, to: from, programs: {} })) },
      0,
    );
    store.want(0, 3600, [1, 2, 3]);
    store.want(0, 3600, [2, 3, 4]);
    expect(asked).toEqual(["0: 1,2,3", "0: 4"]);
    await settle();
    expect(store.answered(1, 60)).toBe(true);
    expect(store.answered(9, 60)).toBe(false);
    store.want(0, 3600, []);
    expect(asked).toHaveLength(2);
  });
});
