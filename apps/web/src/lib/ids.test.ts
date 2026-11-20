import { describe, expect, it } from "vitest";
import { decodeId, encodeId } from "./ids";

describe("short ids", () => {
  it("round-trip every kind of id", () => {
    const ids = [0, 1, 2, 3, 9, 10, 99, 1234, 65_535, 65_536, 1_000_000, 123_456_789, 0x7fffffff, 0xffffffff];
    for (let id = 1; id < 5000; id += 7) ids.push(id);
    for (const id of ids) {
      const short = encodeId(id);
      expect(short).toMatch(/^[bcdfghjkmnpqrstvwxz][bcdfghjkmnpqrstvwxz2-9]{6}$/);
      expect(decodeId(short)).toBe(id);
    }
  });

  it("don't look like counting", () => {
    const shorts = [1, 2, 3, 4].map(encodeId);
    expect(new Set(shorts).size).toBe(4);
    // Neighbors share no position.
    for (let i = 0; i < 7; i++) expect(new Set(shorts.map((s) => s.charAt(i))).size).toBeGreaterThan(1);
  });

  it("are unique", () => {
    const seen = new Set<string>();
    for (let id = 1; id <= 20_000; id++) seen.add(encodeId(id));
    expect(seen.size).toBe(20_000);
  });

  it("still read plain numbers from older links, in any case", () => {
    expect(decodeId("42")).toBe(42);
    expect(decodeId(encodeId(42).toUpperCase())).toBe(42);
  });

  it("refuse anything else", () => {
    for (const bad of [undefined, null, "", "0", "-1", "abc", "kx7bq2", "kx7bq2mm", "2x7bq2m", "kx7bq1m", "ka7bq2m", "1e5", "12345678901234567"]) {
      expect(decodeId(bad)).toBeNull();
    }
    // Some 7-character spellings are past 32 bits.
    expect(decodeId("z999999")).toBeNull();
  });

  it("leave ids that don't fit 32 bits as numbers", () => {
    expect(encodeId(2 ** 33)).toBe(String(2 ** 33));
    expect(decodeId(String(2 ** 33))).toBe(2 ** 33);
  });
});
