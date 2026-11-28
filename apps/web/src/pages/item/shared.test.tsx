import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { useItem } from "@/api/hooks";
import type { ItemDetail } from "@/api/types";
import { makeItem } from "@/test/fixtures";
import { apiError, mockApi, renderWithProviders } from "@/test/render";
import { MyListButton, SpoilersNote } from "./shared";

function detail(overrides: Partial<ItemDetail> = {}): ItemDetail {
  return {
    ...makeItem({ id: 5, kind: "show", title: "Lost" }),
    original_title: null,
    tagline: null,
    genres: [],
    studio: null,
    content_rating: null,
    tmdb_id: null,
    imdb_id: null,
    files: [],
    in_watchlist: false,
    ...overrides,
  };
}

/** The button as the item page has it: on the item from the cache. */
function Page() {
  const item = useItem(5);
  return item.data ? <MyListButton item={item.data} /> : null;
}

describe("MyListButton", () => {
  it("puts the item on My List at once", async () => {
    const user = userEvent.setup();
    let answer: () => void = () => undefined;
    const requests = mockApi({
      "GET /api/items/5": () => detail(),
      "PUT /api/watchlist/5": () => new Promise<undefined>((resolve) => (answer = () => resolve(undefined))),
    });
    renderWithProviders(<Page />);
    await user.click(await screen.findByRole("button", { name: "Add to My List" }));

    expect(screen.getByRole("button", { name: "In My List" })).toBeInTheDocument();
    expect(requests.some((r) => r.method === "PUT" && r.path === "/api/watchlist/5")).toBe(true);
    answer();
  });

  it("takes it off again", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/items/5": () => detail({ in_watchlist: true }),
      "DELETE /api/watchlist/5": () => undefined,
    });
    renderWithProviders(<Page />);
    await user.click(await screen.findByRole("button", { name: "In My List" }));
    await waitFor(() => expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/watchlist/5")).toBe(true));
  });

  it("goes back and says why when the server says no", async () => {
    const user = userEvent.setup();
    mockApi({
      "GET /api/items/5": () => detail(),
      "PUT /api/watchlist/5": () => apiError(400, "bad_request", "only movies and shows can be on my list"),
    });
    renderWithProviders(<Page />);
    await user.click(await screen.findByRole("button", { name: "Add to My List" }));
    expect(await screen.findByText("Only movies and shows can be on my list")).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "Add to My List" })).toBeInTheDocument();
  });
});

describe("SpoilersNote", () => {
  it("says a season's or show's spoilers are hidden, and nothing otherwise", () => {
    const { unmount } = renderWithProviders(<SpoilersNote item={makeItem({ id: 11, kind: "season", title: "Season 1", spoilers_hidden: true })} />);
    expect(screen.getByText("Spoilers hidden: episode titles, descriptions and pictures aren't shown.")).toBeInTheDocument();
    unmount();
    renderWithProviders(<SpoilersNote item={makeItem({ id: 12, kind: "season", title: "Season 1" })} />);
    expect(screen.queryByText(/Spoilers hidden/)).not.toBeInTheDocument();
  });
});
