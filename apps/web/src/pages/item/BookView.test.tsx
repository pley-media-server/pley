import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { ItemDetail, ItemSummary } from "@/api/types";
import { AudioPlayerProvider } from "@/audio/AudioPlayer";
import { makeItem } from "@/test/fixtures";
import { mockApi, renderWithProviders, type FakeRequest } from "@/test/render";
import { BookView, joinNames } from "./BookView";

function book(overrides: Partial<ItemDetail> = {}): ItemDetail {
  return {
    ...makeItem({ id: 30, kind: "audiobook", title: "Book One", year: 2006, duration_ms: 3_600_000 }),
    original_title: null,
    tagline: null,
    genres: [],
    studio: null,
    content_rating: null,
    tmdb_id: null,
    imdb_id: null,
    files: [],
    book: { authors: ["Author A"], narrators: ["Narrator B"], series: "Series X", series_index: 1 },
    ...overrides,
  };
}

const unheard = { position_ms: 0, watched: false, play_count: 0 };
function part(id: number, index: number, state = unheard): ItemSummary {
  return makeItem({ id, kind: "part", parent_id: 30, index, title: `Chapter ${index}`, duration_ms: 1_800_000, user_state: state });
}

function serve(parts: ItemSummary[], deck: ItemSummary | null): FakeRequest[] {
  return mockApi({
    "GET /api/items/30/children": () => ({ items: parts }),
    "GET /api/items/30/on-deck": () => ({ item: deck }),
    "POST /api/playback": () => new Response(null, { status: 500 }),
  });
}

function show(item: ItemDetail) {
  return renderWithProviders(
    <AudioPlayerProvider>
      <BookView item={item} />
    </AudioPlayerProvider>,
  );
}

describe("joinNames", () => {
  it("says a list of names as people do", () => {
    expect(joinNames([], "and")).toBe("");
    expect(joinNames(["A"], "and")).toBe("A");
    expect(joinNames(["A", "B"], "and")).toBe("A and B");
    expect(joinNames(["A", "B", "C"], "and")).toBe("A, B and C");
  });
});

describe("a book's page", () => {
  it("says nothing of how much is heard before it's started", async () => {
    serve([part(31, 1)], part(31, 1));
    show(book());
    expect(await screen.findByRole("button", { name: "Play" })).toBeEnabled();
    expect(screen.queryByText(/% listened$/)).not.toBeInTheDocument();
  });

  it("says who wrote and reads it, its series and its parts", async () => {
    serve([part(31, 1), part(32, 2)], part(31, 1));
    show(book());
    expect(screen.getByRole("heading", { name: "Book One" })).toBeInTheDocument();
    expect(screen.getByText("by Author A")).toBeInTheDocument();
    expect(screen.getByText("Read by Narrator B")).toBeInTheDocument();
    expect(screen.getByText("Series X, book 1")).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "Play Chapter 1" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Play Chapter 2" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Play" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "From start" })).not.toBeInTheDocument();
  });

  it("offers to resume a book part way through, and to start over", async () => {
    const left = part(32, 2, { position_ms: 600_000, watched: false, play_count: 0 });
    serve([part(31, 1, { position_ms: 0, watched: true, play_count: 1 }), left], left);
    show(book());
    expect(await screen.findByRole("button", { name: "Resume" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "From start" })).toBeInTheDocument();
    // An hour, of which a part and ten minutes are heard.
    expect(screen.getByText("20m left")).toBeInTheDocument();
    expect(screen.getByText("Up to Chapter 2")).toBeInTheDocument();
    expect(screen.getByLabelText("Finished")).toBeInTheDocument();
    // Two thirds heard, said at the top, rounded down.
    expect(screen.getByText("66% listened")).toBeInTheDocument();
    // Books are listened to, not watched.
    expect(screen.getByRole("button", { name: "Listened" })).toHaveAttribute("title", "Mark as listened");
  });

  it("plays the part chosen", async () => {
    const requests = serve([part(31, 1), part(32, 2)], part(31, 1));
    show(book());
    await userEvent.click(await screen.findByRole("button", { name: "Play Chapter 2" }));
    const start = requests.find((r) => r.method === "POST" && r.path === "/api/playback");
    expect(start?.body).toMatchObject({ item_id: 32, start_ms: 0 });
  });

  it("names a book without a series as an audiobook", async () => {
    serve([part(31, 1)], part(31, 1));
    show(book({ book: { authors: ["Author C"], narrators: [], series: null, series_index: null } }));
    expect(await screen.findByText("Audiobook")).toBeInTheDocument();
    expect(screen.queryByText(/Read by/)).not.toBeInTheDocument();
  });
});
