import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Collection, HomeRow } from "@/api/lists";
import type { Library } from "@/api/types";
import { mockApi, renderWithProviders } from "@/test/render";
import { HomeRowsModal, RecommendedRowsModal, recommendedRows } from "./RowsModal";

function collection(id: number, title: string, recommended: number | null): Collection {
  return {
    id,
    library_id: 1,
    title,
    summary: null,
    automatic: false,
    sort: "release",
    smart_filter: null,
    category: { id: 3, key: "other", title: "Other" },
    builtin: null,
    home_position: null,
    recommended_position: recommended,
    item_count: 3,
    poster_url: "",
    backdrop_url: "",
    created_at: 0,
    updated_at: 0,
  };
}

const library = (id: number, name: string): Library => ({
  id,
  name,
  kind: "movies",
  paths: [],
  item_count: 0,
  created_at: 0,
  scanned_at: 0,
  scanning: false,
});

describe("recommendedRows", () => {
  it("keeps the collections on Recommended, in their order", () => {
    const rows = recommendedRows([collection(1, "A", 2), collection(2, "B", null), collection(3, "C", 0)]);
    expect(rows.map((c) => c.id)).toEqual([3, 1]);
  });
});

describe("RecommendedRowsModal", () => {
  it("moves, takes off and adds rows", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/libraries/1/collections": () => ({
        collections: [collection(1, "Recently Added", 0), collection(2, "Top Rated", 1), collection(3, "Favorites", null)],
      }),
      "PUT /api/libraries/1/rows/order": () => undefined,
      "PATCH /api/collections/2": () => collection(2, "Top Rated", null),
      "PATCH /api/collections/3": () => collection(3, "Favorites", 2),
    });
    renderWithProviders(<RecommendedRowsModal open onClose={vi.fn()} libraryId={1} />);

    const list = await screen.findByRole("list", { name: "Recommended rows" });
    expect(within(list).getAllByRole("listitem").map((r) => r.textContent)).toEqual(["Recently Added", "Top Rated"]);

    await user.click(screen.getByRole("button", { name: "Move Top Rated up" }));
    await waitFor(() =>
      expect(requests.find((r) => r.method === "PUT")?.body).toEqual({ collection_ids: [2, 1] }),
    );

    await user.click(screen.getByRole("button", { name: "Take Top Rated off" }));
    await waitFor(() => expect(requests.find((r) => r.path === "/api/collections/2")?.body).toEqual({ on_recommended: false }));

    await user.selectOptions(screen.getByLabelText("Add a row"), "3");
    await user.click(screen.getByRole("button", { name: "Add a row" }));
    await waitFor(() => expect(requests.find((r) => r.path === "/api/collections/3")?.body).toEqual({ on_recommended: true }));
  });

  it("keeps the focus on the rows moved or taken off from the keyboard, and says where they went", async () => {
    const user = userEvent.setup();
    let collections = [collection(1, "Recently Added", 0), collection(2, "Top Rated", 1), collection(3, "Favorites", 2)];
    mockApi({
      "GET /api/libraries/1/collections": () => ({ collections }),
      "PUT /api/libraries/1/rows/order": (req) => {
        const ids = (req.body as { collection_ids: number[] }).collection_ids;
        collections = collections.map((c) => ({ ...c, recommended_position: ids.indexOf(c.id) }));
        return undefined;
      },
      "PATCH /api/collections/1": () => {
        collections = collections.map((c) => (c.id === 1 ? { ...c, recommended_position: null } : c));
        return { ...collections[0], items: [] };
      },
    });
    renderWithProviders(<RecommendedRowsModal open onClose={vi.fn()} libraryId={1} />);
    const list = await screen.findByRole("list", { name: "Recommended rows" });

    // At the top, Move up stops working: the focus goes to Move down.
    await user.click(screen.getByRole("button", { name: "Move Top Rated up" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Move Top Rated down" })).toHaveFocus());
    expect(screen.getByRole("status")).toHaveTextContent("Top Rated moved to position 1 of 3");

    // Taken off, a row hands the focus to the one that takes its place.
    await user.click(screen.getByRole("button", { name: "Take Recently Added off" }));
    await waitFor(() => expect(within(list).queryByText("Recently Added")).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Take Favorites off" })).toHaveFocus();
  });

  it("says when Pley's collections are all there to restore", async () => {
    const user = userEvent.setup();
    let restored = 0;
    mockApi({
      "GET /api/libraries/1/collections": () => ({ collections: [collection(1, "Recently Added", 0)] }),
      "POST /api/libraries/1/collections/defaults": () => ({ restored }),
    });
    renderWithProviders(<RecommendedRowsModal open onClose={vi.fn()} libraryId={1} />);
    await screen.findByRole("list", { name: "Recommended rows" });

    await user.click(screen.getByRole("button", { name: "Restore Pley's" }));
    expect(await screen.findByText("Nothing to restore: Pley's collections are all here")).toBeInTheDocument();
    restored = 2;
    await user.click(screen.getByRole("button", { name: "Restore Pley's" }));
    expect(await screen.findByText("2 collections restored")).toBeInTheDocument();
  });
});

describe("HomeRowsModal", () => {
  const rows: HomeRow[] = [
    { key: "recently_added:1", title: "Recently Added in Movies", collection_ids: [1], library_ids: [1] },
    { key: "collections:4,9", title: "Christmas", collection_ids: [4, 9], library_ids: [1, 2] },
    { key: "recently_added:3", title: "Recently Added in Music", collection_ids: [], library_ids: [3] },
  ];

  it("orders Home by the rows' keys and takes a row off with all its collections", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/home/rows": () => ({ rows }),
      "GET /api/libraries": () => [library(1, "Movies"), library(2, "Kids"), { ...library(3, "Music"), kind: "music" }],
      "PUT /api/home/rows/order": () => undefined,
      "PATCH /api/collections/4": () => ({ ...collection(4, "Christmas", null), items: [] }),
      "PATCH /api/collections/9": () => ({ ...collection(9, "Christmas", null), library_id: 2, items: [] }),
    });
    renderWithProviders(<HomeRowsModal open onClose={vi.fn()} />);

    expect(await screen.findByText("Movies, Kids")).toBeInTheDocument();
    // "Recently Added in Movies" names its library already.
    const list = screen.getByRole("list");
    expect(within(list).getAllByRole("listitem")[0]).toHaveTextContent(/^Recently Added in Movies$/);
    await user.click(screen.getByRole("button", { name: "Move Christmas up" }));
    await waitFor(() =>
      expect(requests.find((r) => r.method === "PUT")?.body).toEqual({
        rows: ["collections:4,9", "recently_added:1", "recently_added:3"],
      }),
    );

    await user.click(screen.getByRole("button", { name: "Take Christmas off" }));
    await waitFor(() =>
      expect(requests.filter((r) => r.method === "PATCH").map((r) => [r.path, r.body])).toEqual([
        ["/api/collections/4", { on_home: false }],
        ["/api/collections/9", { on_home: false }],
      ]),
    );
    // A music library's Recently Added is no collection: it stays.
    expect(screen.getByRole("button", { name: "Take Recently Added in Music off" })).toBeDisabled();
  });
});
