import { describe, expect, it } from "vitest";
import type { Library } from "@/api/types";
import type { RatingSystems } from "@/api/types";
import {
  AGE_SYSTEM,
  ageBadge,
  ageIn,
  ageLevels,
  birthdayLimit,
  birthdayLimitEnd,
  librariesLabel,
  limitLabel,
  limitMode,
  ratingSystem,
  shownSystem,
  systemChoices,
  toggleId,
} from "./access";

const library = (id: number, name: string): Library => ({
  id,
  name,
  kind: "movies",
  paths: [],
  item_count: 0,
  created_at: 0,
  scanned_at: null,
  scanning: false,
});
const libraries = [library(1, "Movies"), library(2, "Shows")];

const systems: RatingSystems = {
  region: "US",
  systems: [
    {
      country: "US",
      name: "United States",
      ratings: [
        { name: "G", age: 0, kind: "film" },
        { name: "TV-Y", age: 0, kind: "tv" },
        { name: "TV-Y7", age: 7, kind: "tv" },
        { name: "PG", age: 10, kind: "film" },
        { name: "TV-PG", age: 10, kind: "tv" },
        { name: "PG-13", age: 13, kind: "film" },
        { name: "R", age: 17, kind: "film" },
      ],
    },
    {
      country: "CA",
      name: "Canada",
      ratings: [
        { name: "G", age: 0, kind: null },
        { name: "C8", age: 8, kind: "tv" },
        { name: "PG", age: 8, kind: "tv" },
        { name: "PG", age: 10, kind: "film" },
        { name: "14A", age: 14, kind: "film" },
      ],
    },
  ],
};

describe("age limits", () => {
  it("offers a system's ratings by age, saying a name's kind where it has two ages", () => {
    expect(ageLevels(systems, "US")).toEqual([
      { age: 0, rating: "G, TV-Y" },
      { age: 7, rating: "TV-Y7" },
      { age: 10, rating: "PG, TV-PG" },
      { age: 13, rating: "PG-13" },
      { age: 17, rating: "R" },
    ]);
    expect(ageLevels(systems, "CA").map((l) => l.rating)).toEqual(["G", "C8, PG (TV)", "PG (film)", "14A"]);
    expect(ageLevels(systems, AGE_SYSTEM)).toHaveLength(22);
    expect(ageLevels(systems, AGE_SYSTEM)[8]).toEqual({ age: 8, rating: null });
  });

  it("starts on the region's system, and shows a limit no rating has as an age", () => {
    expect(shownSystem(systems, null, null)).toBe("US");
    expect(shownSystem({ ...systems, region: "ZZ" }, null, 13)).toBe(AGE_SYSTEM);
    expect(shownSystem(systems, "CA", 14)).toBe("CA");
    expect(shownSystem(systems, null, 8)).toBe(AGE_SYSTEM);
    expect(shownSystem(systems, "XX", 13)).toBe("US");
    expect(shownSystem(undefined, "US", 13)).toBe(AGE_SYSTEM);
  });

  it("labels limits", () => {
    expect(limitLabel(systems, null, 13)).toBe("PG-13");
    expect(limitLabel(systems, null, 8)).toBe("Age 8");
    expect(limitLabel(systems, null, null)).toBe("No limit");
    expect(limitLabel(systems, null, undefined)).toBe("No limit");
    expect(ageBadge(systems, null, 10)).toBe("Up to PG, TV-PG");
    expect(ageBadge(systems, "CA", 8)).toBe("Up to C8, PG (TV)");
    expect(ageBadge(systems, AGE_SYSTEM, 13)).toBe("Up to age 13");
    expect(ageBadge(systems, null, null)).toBeNull();
  });

  it("keeps a limit at or below the old one when the system changes", () => {
    expect(ageIn(systems, "CA", 13)).toBe(10);
    expect(ageIn(systems, "US", 8)).toBe(7);
    expect(ageIn(systems, AGE_SYSTEM, 8)).toBe(8);
    expect(ageIn(systems, "US", null)).toBeNull();
  });

  it("lists the countries by name", () => {
    expect(systemChoices(systems).map((c) => c.value)).toEqual(["CA", "US"]);
  });

  it("follows a birthday as the server does", () => {
    const november = new Date(2026, 10, 15);
    expect(birthdayLimit("2015-03", new Date(2026, 1, 28))).toBe(10);
    expect(birthdayLimit("2015-03", new Date(2026, 2, 1))).toBe(11);
    expect(birthdayLimit("2026-11", november)).toBe(0);
    expect(birthdayLimit("2005-12", november)).toBe(20);
    expect(birthdayLimit("2005-11", november)).toBeNull();
    expect(birthdayLimit("2015-13", november)).toBeNull();
    expect(birthdayLimitEnd("2015-03")).toBe("March 2036");
    expect(ageBadge(systems, null, 13, "2013-01")).toBe("Up to PG-13 (age 13, by birthday)");
    expect(ageBadge(systems, null, 9, "2017-01")).toBe("Up to age 9 (by birthday)");
  });

  it("tells how a limit is set", () => {
    expect(limitMode(systems, { max_age: null, max_age_system: null })).toBe("none");
    expect(limitMode(systems, { max_age: 13, max_age_system: null })).toBe("rating");
    expect(limitMode(systems, { max_age: 8, max_age_system: null })).toBe("age"); // no US rating is for 8
    expect(limitMode(systems, { max_age: 13, max_age_system: AGE_SYSTEM })).toBe("age");
    expect(limitMode(systems, { max_age: 9, max_age_system: null, birth_month: "2017-01" })).toBe("birthday");
    expect(limitMode(systems, { max_age: null, max_age_system: null, birth_month: "1990-01" })).toBe("birthday");
  });

  it("picks ratings in the stored country, else the region's, else the first", () => {
    expect(ratingSystem(systems, "CA")).toBe("CA");
    expect(ratingSystem(systems, AGE_SYSTEM)).toBe("US");
    expect(ratingSystem({ ...systems, region: "ZZ" }, null)).toBe("US");
    expect(ratingSystem(undefined, null)).toBeNull();
  });
});

describe("librariesLabel", () => {
  it("summarizes access", () => {
    expect(librariesLabel({ all_libraries: true, library_ids: [] }, libraries)).toBe("All libraries");
    expect(librariesLabel({ all_libraries: false, library_ids: [] }, libraries)).toBe("No libraries");
    expect(librariesLabel({ all_libraries: false, library_ids: [2] }, libraries)).toBe("Shows");
    expect(librariesLabel({ all_libraries: false, library_ids: [9] }, libraries)).toBe("1 library");
    expect(librariesLabel({ all_libraries: false, library_ids: [1, 2] }, undefined)).toBe("2 libraries");
  });
});

describe("toggleId", () => {
  it("adds and removes", () => {
    expect(toggleId([1, 2], 3)).toEqual([1, 2, 3]);
    expect(toggleId([1, 2, 3], 2)).toEqual([1, 3]);
  });
});
