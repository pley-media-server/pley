import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { Hub, Library } from "@/api/types";
import { ListMenuProvider } from "@/components/lists/ListMenu";
import { makeItem } from "@/test/fixtures";
import { apiError, mockApi, renderWithProviders, testUser } from "@/test/render";
import { HomePage } from "./HomePage";
import { libraryUrl } from "@/lib/urls";

const movies: Library = {
  id: 1,
  name: "Movies",
  kind: "movies",
  paths: ["/media/movies"],
  item_count: 2,
  created_at: 0,
  scanned_at: 0,
  scanning: false,
};

const hubs: Hub[] = [
  {
    key: "continue_watching",
    title: "Continue Watching",
    library_id: null,
    items: [
      makeItem({ id: 6, title: "Vertigo", duration_ms: 2_640_000, user_state: { position_ms: 600_000, watched: false, play_count: 0 } }),
      makeItem({ id: 7, kind: "episode", title: "Pilot", grandparent_title: "Lost", parent_index: 1, index: 1 }),
    ],
  },
  { key: "recently_added_1", title: "Recently Added in Movies", library_id: 1, items: [makeItem({ id: 8, title: "Jaws", year: 1975 })] },
  { key: "top_rated_1", title: "Top Rated", library_id: 1, items: [] },
];

function serve({ libraries = [movies], home = hubs }: { libraries?: Library[]; home?: Hub[] | Response } = {}) {
  return mockApi({
    "GET /api/libraries": () => libraries,
    "GET /api/hubs/home": () => (home instanceof Response ? home : { hubs: home }),
  });
}

describe("HomePage", () => {
  it("shows the hubs that have something in them", async () => {
    serve();
    renderWithProviders(<HomePage />);

    const continueRow = (await screen.findByRole("heading", { name: "Continue Watching" })).closest("section") as HTMLElement;
    // Movies say how much is left; episodes which one is next.
    expect(within(continueRow).getByText("34m left")).toBeInTheDocument();
    expect(within(continueRow).getByText("S1 · E1 · Pilot")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Recently Added in Movies" })).toHaveAttribute(
      "href",
      libraryUrl(1, "tab=library&sort=added&order=desc"),
    );
    expect(screen.queryByRole("heading", { name: "Top Rated" })).not.toBeInTheDocument();
  });

  it("asks an administrator to add a first library", async () => {
    serve({ libraries: [] });
    renderWithProviders(<HomePage />);

    expect(await screen.findByText("No libraries yet")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Add a library" })).toHaveAttribute("href", "/settings/libraries?add=1");
  });

  it("tells everyone else to ask the owner", async () => {
    serve({ libraries: [] });
    renderWithProviders(<HomePage />, { user: { ...testUser, is_admin: false } });

    expect(await screen.findByText("Ask the server owner to add a library.")).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Add a library" })).not.toBeInTheDocument();
  });

  it("says when the libraries are still being scanned", async () => {
    serve({ libraries: [{ ...movies, scanning: true }], home: [] });
    renderWithProviders(<HomePage />);
    expect(await screen.findByText("Scanning your libraries…")).toBeInTheDocument();
  });

  it("has My List when the server sends it", async () => {
    serve({ home: [...hubs, { key: "my_list", title: "My List", library_id: null, items: [makeItem({ id: 9, title: "Rashomon" })] }] });
    renderWithProviders(<HomePage />);
    const row = (await screen.findByRole("heading", { name: "My List" })).closest("section") as HTMLElement;
    expect(within(row).getAllByRole("link", { name: "Rashomon" })).not.toHaveLength(0);
  });

  it("takes a card off Continue Watching at once", async () => {
    const user = userEvent.setup();
    let answer: (res: Response | undefined) => void = () => undefined;
    const requests = mockApi({
      "GET /api/libraries": () => [movies],
      "GET /api/hubs/home": () => ({ hubs }),
      "GET /api/watchlist": () => ({ items: [] }),
      "DELETE /api/continue-watching/6": () => new Promise<Response | undefined>((resolve) => (answer = resolve)),
    });
    renderWithProviders(
      <ListMenuProvider>
        <HomePage />
      </ListMenuProvider>,
    );
    const row = (await screen.findByRole("heading", { name: "Continue Watching" })).closest("section") as HTMLElement;
    fireEvent.contextMenu(within(row).getAllByRole("link", { name: "Vertigo" })[0] as HTMLElement);
    await user.click(screen.getByRole("menuitem", { name: "Remove from Continue Watching" }));

    // Gone before the server answers.
    expect(within(row).queryAllByRole("link", { name: "Vertigo" })).toHaveLength(0);
    expect(within(row).getByText("S1 · E1 · Pilot")).toBeInTheDocument();
    expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/continue-watching/6")).toBe(true);
    answer(undefined);
  });

  it("puts the card back when the server says no", async () => {
    const user = userEvent.setup();
    mockApi({
      "GET /api/libraries": () => [movies],
      "GET /api/hubs/home": () => ({ hubs }),
      "GET /api/watchlist": () => ({ items: [] }),
      "DELETE /api/continue-watching/6": () => apiError(500, "internal", "database is locked"),
    });
    renderWithProviders(
      <ListMenuProvider>
        <HomePage />
      </ListMenuProvider>,
    );
    const row = (await screen.findByRole("heading", { name: "Continue Watching" })).closest("section") as HTMLElement;
    fireEvent.contextMenu(within(row).getAllByRole("link", { name: "Vertigo" })[0] as HTMLElement);
    await user.click(screen.getByRole("menuitem", { name: "Remove from Continue Watching" }));

    expect(await screen.findByText("Database is locked")).toBeInTheDocument();
    expect(await within(row).findAllByRole("link", { name: "Vertigo" })).not.toHaveLength(0);
  });

  it("only offers that in Continue Watching", async () => {
    mockApi({
      "GET /api/libraries": () => [movies],
      "GET /api/hubs/home": () => ({ hubs }),
      "GET /api/watchlist": () => ({ items: [] }),
    });
    renderWithProviders(
      <ListMenuProvider>
        <HomePage />
      </ListMenuProvider>,
    );
    fireEvent.contextMenu((await screen.findAllByRole("link", { name: "Jaws" }))[0] as HTMLElement);
    expect(await screen.findByRole("menuitem", { name: "Add to My List" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Remove from Continue Watching" })).not.toBeInTheDocument();
  });

  it("puts a card on My List, or takes it off, from its menu", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/libraries": () => [movies],
      "GET /api/hubs/home": () => ({ hubs }),
      "GET /api/watchlist": () => ({ items: [makeItem({ id: 8, title: "Jaws" })] }),
      "DELETE /api/watchlist/8": () => undefined,
      "PUT /api/watchlist/6": () => undefined,
    });
    renderWithProviders(
      <ListMenuProvider>
        <HomePage />
      </ListMenuProvider>,
    );
    fireEvent.contextMenu((await screen.findAllByRole("link", { name: "Jaws" }))[0] as HTMLElement);
    await user.click(await screen.findByRole("menuitem", { name: "Remove from My List" }));
    await waitFor(() => expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/watchlist/8")).toBe(true));

    fireEvent.contextMenu(screen.getAllByRole("link", { name: "Vertigo" })[0] as HTMLElement);
    await user.click(await screen.findByRole("menuitem", { name: "Add to My List" }));
    await waitFor(() => expect(requests.some((r) => r.method === "PUT" && r.path === "/api/watchlist/6")).toBe(true));
  });

  it("retries after an error", async () => {
    const user = userEvent.setup();
    let fail = true;
    mockApi({
      "GET /api/libraries": () => [movies],
      "GET /api/hubs/home": () => (fail ? apiError(500, "internal", "database is locked") : { hubs }),
    });
    renderWithProviders(<HomePage />);

    expect(await screen.findByText("Database is locked")).toBeInTheDocument();
    fail = false;
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("heading", { name: "Continue Watching" })).toBeInTheDocument();
  });
});
