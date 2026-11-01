import { afterEach, describe, expect, it, vi } from "vitest";
import { categoryKey, parseCategory, type LiveGroup } from "@/api/livetv";
import { categoryName, groupSections } from "./CategoryRail";
import { channelName, formatGuideDay, liveUrl, splitRegion } from "./common";

const group = (name: string, region: string | null, label: string, count = 1): LiveGroup => ({ name, region, label, count });

describe("groupSections", () => {
  it("gathers regions in alphabetical order, the rest after in the provider's order", () => {
    const { regions, others } = groupSections([
      group("UK| News", "UK", "News", 3),
      group("24/7 Films", null, "24/7 Films"),
      group("FR| Cinéma", "FR", "Cinéma"),
      group("UK| Sport", "UK", "Sport", 4),
      group("FR| Info", "FR", "Info"),
      group("DE| Sport", "DE", "Sport"),
    ]);
    expect(regions.map((r) => `${r.region} ${r.count}`)).toEqual(["FR 2", "UK 7"]);
    expect(others.map((g) => g.name)).toEqual(["24/7 Films", "DE| Sport"]);
  });

  it("leaves a lineup with fewer than two regions as it is", () => {
    const groups = [group("News", null, "News"), group("UK| Sport", "UK", "Sport"), group("UK| News", "UK", "News")];
    expect(groupSections(groups)).toEqual({ regions: [], others: groups });
  });
});

describe("categories", () => {
  it("go to and from their keys", () => {
    for (const c of [{ kind: "all" }, { kind: "favorites" }, { kind: "recent" }, { kind: "group", group: "UK| Sport" }] as const) {
      expect(parseCategory(categoryKey(c))).toEqual(c);
    }
    expect(parseCategory(null)).toEqual({ kind: "all" });
  });

  it("are named by their region and label", () => {
    const groups = [group("UK| Sport", "UK", "Sport")];
    expect(categoryName({ kind: "group", group: "UK| Sport" }, groups)).toBe("UK · Sport");
    expect(categoryName({ kind: "group", group: "Gone" }, groups)).toBe("Gone");
    expect(categoryName({ kind: "favorites" }, groups)).toBe("Favourites");
  });

  it("leave channel names without the region their group says", () => {
    const sky = { name: "UK| SKY SPORTS F1 HD", group: "UK| SPORTS" };
    expect(channelName(sky, { kind: "group", group: "UK| SPORTS" })).toBe("SKY SPORTS F1 HD");
    expect(channelName(sky, { kind: "all" })).toBe("UK| SKY SPORTS F1 HD");
    expect(channelName({ name: "UK|SKY SPORTS ACTION", group: "UK| SPORTS" }, { kind: "group", group: "x" })).toBe("SKY SPORTS ACTION");
    expect(channelName({ name: "FR| TF1", group: "UK| SPORTS" }, { kind: "group", group: "x" })).toBe("FR| TF1");
    expect(splitRegion("Live: Sport")).toBeNull();
    expect(splitRegion("[DE] Sport")).toEqual(["DE", "Sport"]);
  });

  it("travel with a channel to the player", () => {
    const seven = { id: 7, source_uid: "a1b2c3" };
    expect(liveUrl(seven)).toBe("/live/a1b2c3/7");
    expect(liveUrl(seven, { kind: "all" })).toBe("/live/a1b2c3/7");
    expect(liveUrl(seven, { kind: "group", group: "UK| Sport" })).toBe("/live/a1b2c3/7?in=group%3AUK%7C%20Sport");
  });
});

describe("formatGuideDay", () => {
  afterEach(() => vi.unstubAllEnvs());

  // Times on local dates, not now ± 24 hours: a day is 23 or 25 hours when
  // daylight saving starts or ends. Every day of a year covers a zone's changes.
  it.each(["the browser's zone", "America/New_York", "Europe/London", "Australia/Sydney"])("names today, tomorrow and yesterday by date in %s", (zone) => {
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
});
