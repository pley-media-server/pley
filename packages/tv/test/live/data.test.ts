import { afterEach, describe, expect, it, vi } from "vitest";
import type { LiveChannel, LiveProgram } from "../../src/api/types";
import {
  categoryKey,
  categoryName,
  cellAt,
  channelByNumber,
  channelName,
  chunkStarts,
  formatGuideDay,
  GUIDE_CHUNK,
  browseQuery,
  groupSections,
  groupsMatching,
  guideCells,
  mergeGuide,
  nowAndNext,
  parseCategory,
  splitRegion,
  timeLeft,
} from "../../src/live/data";

function program(channel: number, start: number, stop: number, title = `P${start}`): LiveProgram {
  return {
    channel_id: channel,
    start,
    stop,
    title,
    subtitle: null,
    description: null,
    categories: [],
    image: null,
    season: null,
    episode: null,
    is_new: false,
    is_live: false,
    rating: null,
  };
}

function channel(id: number, over: Partial<LiveChannel> = {}): LiveChannel {
  return { id, source_id: 1, name: `Ch ${id}`, number: String(100 + id), group: "News", logo: null, favorite: false, has_guide: true, now: null, next: null, ...over };
}

describe("what's on", () => {
  it("is the channel's now while it airs, then its next once the list is a minute old", () => {
    const c = channel(1, { now: program(1, 0, 600), next: program(1, 600, 1200) });
    expect(nowAndNext(c, 300).on?.start).toBe(0);
    expect(nowAndNext(c, 300).next?.start).toBe(600);
    // The list was asked for before 600: its next is on now, and nothing is known after it.
    expect(nowAndNext(c, 700)).toEqual({ on: c.next, next: null });
    // Nothing on yet: what comes first is next.
    expect(nowAndNext(channel(2, { now: program(2, 900, 1200) }), 300)).toEqual({ on: null, next: program(2, 900, 1200) });
  });

  it("says how long is left as the web does", () => {
    expect(timeLeft({ stop: 600 }, 0)).toBe("10 min left");
    expect(timeLeft({ stop: 3900 }, 0)).toBe("1 h 5 min left");
    expect(timeLeft({ stop: 7200 }, 0)).toBe("2 h left");
    expect(timeLeft({ stop: 10 }, 0)).toBe("1 min left");
  });
});

describe("days", () => {
  afterEach(() => vi.unstubAllEnvs());

  // Times on local dates, not now ± 24 hours: a day is 23 or 25 hours when
  // daylight saving starts or ends. Every day of a year covers a zone's changes.
  it.each(["the device's zone", "America/New_York", "Europe/London", "Australia/Sydney"])("are today, tomorrow and yesterday by date in %s", (zone) => {
    if (zone.includes("/")) vi.stubEnv("TZ", zone);
    const today = new Date();
    const at = (days: number, hour: number, minute: number) =>
      new Date(today.getFullYear(), today.getMonth(), today.getDate() + days, hour, minute).getTime() / 1000;
    for (let days = 0; days < 366; days++) {
      for (const now of [at(days, 0, 30), at(days, 23, 30)]) {
        expect(formatGuideDay(at(days, 0, 1), now)).toBe("Today");
        expect(formatGuideDay(at(days, 23, 59), now)).toBe("Today");
        expect(formatGuideDay(at(days + 1, 0, 1), now)).toBe("Tomorrow");
        expect(formatGuideDay(at(days + 1, 23, 59), now)).toBe("Tomorrow");
        expect(formatGuideDay(at(days - 1, 0, 1), now)).toBe("Yesterday");
        expect(formatGuideDay(at(days - 1, 23, 59), now)).toBe("Yesterday");
        expect(formatGuideDay(at(days + 2, 0, 1), now)).not.toMatch(/^(Today|Tomorrow)$/);
        expect(formatGuideDay(at(days - 2, 23, 59), now)).not.toMatch(/^(Today|Yesterday)$/);
      }
    }
  });

  it("are written as the web writes them past that", () => {
    const now = new Date(2026, 9, 27, 20).getTime() / 1000;
    expect(formatGuideDay(new Date(2026, 9, 30, 0, 1).getTime() / 1000, now)).toBe("Fri 30 Oct");
  });
});

describe("categories", () => {
  const group = (name: string, region: string | null, label: string, count: number) => ({ name, region, label, count });
  const groups = [group("UK| Sport", "UK", "Sport", 40), group("News", null, "News", 2), group("UK| News", "UK", "News", 10), group("FR| Films", "FR", "Films", 7)];

  it("round-trip through their keys, a search's words included", () => {
    for (const c of [{ kind: "all" }, { kind: "favorites" }, { kind: "recent" }, { kind: "group", group: "UK| Sport" }, { kind: "search", q: "bbc one" }] as const)
      expect(parseCategory(categoryKey(c))).toEqual(c);
    expect(parseCategory(null)).toEqual({ kind: "all" });
    expect(parseCategory("search:")).toEqual({ kind: "all" });
  });

  it("are asked for as browse wants them: a search among every channel", () => {
    expect(browseQuery({ kind: "search", q: "bbc" })).toEqual({ category: "all", q: "bbc" });
    expect(browseQuery({ kind: "group", group: "News" })).toEqual({ category: "group", group: "News" });
    expect(browseQuery({ kind: "recent" })).toEqual({ category: "recent" });
  });

  it("list the regions in alphabetical order, then the other groups in the provider's order", () => {
    const more = groups.concat([group("FR| News", "FR", "News", 3), group("DE| Sport", "DE", "Sport", 4)]);
    const { regions, others } = groupSections(more);
    expect(regions.map((r) => `${r.region} (${r.groups.length}, ${r.count})`)).toEqual(["FR (2, 10)", "UK (2, 50)"]);
    // A region of one group is one of the rest.
    expect(others.map((g) => g.name)).toEqual(["News", "DE| Sport"]);
    // Fewer than two regions: one plain list.
    expect(groupSections(groups)).toEqual({ regions: [], others: groups });
  });

  it("are found by name for a search's links", () => {
    expect(groupsMatching(groups, "news", 5).map((g) => g.name)).toEqual(["News", "UK| News"]);
    expect(groupsMatching(groups, " ", 5)).toEqual([]);
  });

  it("are named as the web names them", () => {
    expect(categoryName({ kind: "group", group: "UK| Sport" }, groups)).toBe("UK · Sport");
    expect(categoryName({ kind: "group", group: "News" }, groups)).toBe("News");
    expect(categoryName({ kind: "recent" }, groups)).toBe("Recently watched");
    expect(categoryName({ kind: "group", group: "Gone" }, null)).toBe("Gone");
  });

  it("leave out of a channel's name, in a group, the region the group already says", () => {
    const sport = { kind: "group", group: "UK| Sport" } as const;
    expect(channelName({ name: "UK| Sky Sports 1", group: "UK| Sport" }, sport)).toBe("Sky Sports 1");
    expect(channelName({ name: "[UK] Sky Sports 1", group: "UK| Sport" }, sport)).toBe("Sky Sports 1");
    // Another region, or not in a group: the name as it is.
    expect(channelName({ name: "AR| Sport 01", group: "BEE| Sport" }, { kind: "group", group: "BEE| Sport" })).toBe("AR| Sport 01");
    expect(channelName({ name: "UK| Sky Sports 1", group: "UK| Sport" }, { kind: "all" })).toBe("UK| Sky Sports 1");
    expect(splitRegion("UK: Sport")).toEqual(["UK", "Sport"]);
    expect(splitRegion("Sport")).toBeNull();
  });
});

describe("changing channels", () => {
  it("finds a number typed, with or without its leading zeros", () => {
    const numbered = [channel(1, { number: "007" }), channel(2, { number: "101" }), channel(3, { number: "1.1" })];
    expect(channelByNumber(numbered, "101")?.id).toBe(2);
    expect(channelByNumber(numbered, "7")?.id).toBe(1);
    expect(channelByNumber(numbered, "11")).toBeNull();
  });
});

describe("the guide", () => {
  it("asks for the chunks covering what shows, within the two days it spans", () => {
    expect(chunkStarts(0, 3600, 0)).toEqual([0]);
    expect(chunkStarts(GUIDE_CHUNK - 60, GUIDE_CHUNK + 60, 0)).toEqual([0, GUIDE_CHUNK]);
    expect(chunkStarts(-3600, 60, 0)).toEqual([0]);
    expect(chunkStarts(47 * 3600, 60 * 3600, 0)).toEqual([7 * GUIDE_CHUNK]);
  });

  it("merges chunks each programme once, in order", () => {
    const into = new Map<number, LiveProgram[]>();
    mergeGuide(into, { from: 0, to: 100, programs: { "1": [program(1, 60, 120), program(1, 0, 60)] } });
    mergeGuide(into, { from: 100, to: 200, programs: { "1": [program(1, 60, 120), program(1, 120, 300)], "2": [program(2, 0, 300)] } });
    expect(into.get(1)!.map((p) => p.start)).toEqual([0, 60, 120]);
    expect(into.get(2)!.map((p) => p.start)).toEqual([0]);
  });

  it("fills what the guide says nothing of, so the remote doesn't jump over it", () => {
    const cells = guideCells([program(1, 0, 1800), program(1, 3600, 5400), program(1, 5430, 7200)], 900, 9000);
    expect(cells.map((c) => [c.key, c.start, c.stop])).toEqual([
      ["0", 900, 1800],
      ["gap/1800", 1800, 3600],
      ["3600", 3600, 5400],
      // 30 s is no gap of its own.
      ["5430", 5430, 7200],
      ["gap/7200", 7200, 9000],
    ]);
    expect(cellAt(cells, 1000)).toBe(0);
    expect(cellAt(cells, 3600)).toBe(2);
    expect(cellAt(cells, 99_999)).toBe(4);
    // A channel without a guide is one cell.
    expect(guideCells(undefined, 0, 3600)).toEqual([{ key: "gap/0", start: 0, stop: 3600, program: null }]);
  });
});

