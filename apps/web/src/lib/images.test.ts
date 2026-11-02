import { afterEach, describe, expect, it, vi } from "vitest";
import { makeItem } from "@/test/fixtures";
import {
  backdropCandidates,
  bucketFor,
  IMAGE_BUCKETS,
  itemImageUrl,
  placeholderGradient,
  posterCandidates,
  sizedImage,
  thumbCandidates,
} from "./images";

describe("image sizes", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("picks the smallest bucket that covers the width", () => {
    vi.stubGlobal("devicePixelRatio", 1);
    expect(bucketFor(100)).toBe(160);
    expect(bucketFor(160)).toBe(160);
    expect(bucketFor(161)).toBe(240);
    expect(bucketFor(5000)).toBe(Math.max(...IMAGE_BUCKETS));
  });

  it("scales with the pixel ratio, up to 3x", () => {
    vi.stubGlobal("devicePixelRatio", 2);
    expect(bucketFor(200)).toBe(480);
    vi.stubGlobal("devicePixelRatio", 4);
    expect(bucketFor(100)).toBe(320);
  });

  it("adds the width to image urls", () => {
    vi.stubGlobal("devicePixelRatio", 1);
    expect(sizedImage("/api/items/1/images/poster?v=abc", 150)).toBe("/api/items/1/images/poster?v=abc&w=160");
    expect(sizedImage("/api/people/2/photo", 300)).toBe("/api/people/2/photo?w=320");
    expect(sizedImage(null, 300)).toBeNull();
    expect(sizedImage("", 300)).toBeNull();
  });

  it("asks the server for TMDB's pictures at the width drawn, as for any other", () => {
    vi.stubGlobal("devicePixelRatio", 1);
    expect(sizedImage("/api/discover/images/poster/abc.jpg", 150)).toBe("/api/discover/images/poster/abc.jpg?w=160");
  });
});

describe("artwork fallbacks", () => {
  const episode = makeItem({
    id: 30,
    kind: "episode",
    parent_id: 20,
    grandparent_id: 10,
    thumb_url: "/api/items/30/images/thumb?v=1",
  });

  it("builds item image urls", () => {
    expect(itemImageUrl(7, "backdrop")).toBe("/api/items/7/images/backdrop");
  });

  it("falls back from an episode to its season and show", () => {
    expect(posterCandidates(episode)).toEqual(["/api/items/20/images/poster", "/api/items/10/images/poster"]);
    expect(backdropCandidates(episode)).toEqual(["/api/items/10/images/backdrop", "/api/items/20/images/backdrop"]);
    expect(thumbCandidates(episode)).toEqual(["/api/items/30/images/thumb?v=1", "/api/items/10/images/backdrop"]);
  });

  it("uses a movie's own artwork first", () => {
    const movie = makeItem({ poster_url: "/p", backdrop_url: "/b", thumb_url: "/t" });
    expect(posterCandidates(movie)).toEqual(["/p"]);
    expect(backdropCandidates(movie)).toEqual(["/b"]);
    // A movie's thumb is a frame grab: its backdrop comes first.
    expect(thumbCandidates(movie)).toEqual(["/b", "/t"]);
    expect(thumbCandidates(makeItem({ backdrop_url: "/b" }))).toEqual(["/b"]);
  });

  it("gives every other kind its backdrop as a thumb", () => {
    expect(thumbCandidates(makeItem({ kind: "show", backdrop_url: "/b" }))).toEqual(["/b"]);
    expect(thumbCandidates(makeItem({ kind: "show" }))).toEqual([]);
  });
});

describe("placeholderGradient", () => {
  it("is stable for a title and differs between titles", () => {
    expect(placeholderGradient("Jaws")).toBe(placeholderGradient("Jaws"));
    expect(placeholderGradient("Jaws")).not.toBe(placeholderGradient("Vertigo"));
    expect(placeholderGradient("")).toMatch(/^linear-gradient\(135deg, hsl\(0 30% 22%\), hsl\(40 35% 12%\)\)$/);
  });
});
