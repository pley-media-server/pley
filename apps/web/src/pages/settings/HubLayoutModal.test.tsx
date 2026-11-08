import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { HubLayoutRow, HubPlace, Library } from "@/api/types";
import { mockApi, renderWithProviders, testUser } from "@/test/render";
import { HubLayoutModal } from "./HubLayoutModal";

const movies: Library = { id: 1, name: "Movies", kind: "movies", paths: ["/media/movies"], item_count: 3, created_at: 0, scanned_at: null, scanning: false };

const builtin = (hub: string, title: string, home = false): HubLayoutRow => ({ hub, title, on_library: true, on_admin_home: home, on_home: home });
const usual = [builtin("continue_watching", "Continue Watching"), builtin("recently_added", "Recently Added", true), builtin("top_rated", "Top Rated")];

/** The rows, kept as the server keeps them: a PUT arranges them, a DELETE puts them back. */
function serve() {
  let rows = usual;
  const titles: Record<string, string> = { "collection:7": "Heists", ...Object.fromEntries(usual.map((r) => [r.hub, r.title])) };
  const requests = mockApi({
    "GET /api/libraries/1/hub-layout": () => ({ hubs: rows }),
    "PUT /api/libraries/1/hub-layout": (req) => {
      rows = (req.body as { hubs: HubPlace[] }).hubs.map((h) => ({ ...h, title: titles[h.hub] ?? "" }));
      return { hubs: rows };
    },
    "DELETE /api/libraries/1/hub-layout": () => {
      rows = usual;
      return { hubs: rows };
    },
    "GET /api/libraries/1/collections": () => ({
      collections: [{ id: 7, library_id: 1, title: "Heists", summary: null, automatic: false, sort: "release", item_count: 2, poster_url: "", backdrop_url: "", created_at: 0, updated_at: 0 }],
    }),
  });
  return { requests, sent: () => requests.filter((r) => r.method === "PUT").map((r) => (r.body as { hubs: HubPlace[] }).hubs) };
}

/** The rows' titles, in order. */
const rowTitles = () => screen.getAllByRole("listitem").map((li) => li.querySelector(".truncate")?.textContent ?? "");

describe("HubLayoutModal", () => {
  it("shows, moves and promotes rows, and puts them back as usual", async () => {
    const user = userEvent.setup();
    const { requests, sent } = serve();
    renderWithProviders(<HubLayoutModal library={movies} onClose={() => {}} />, { user: { ...testUser, is_admin: true } });

    expect(await screen.findByText("Top Rated")).toBeInTheDocument();
    expect(rowTitles()).toEqual(["Continue Watching", "Recently Added", "Top Rated"]);

    // Top Rated onto everyone else's Home.
    await user.click(screen.getByRole("checkbox", { name: "Show Top Rated on everyone else's Home" }));
    await waitFor(() => expect(sent()).toHaveLength(1));
    expect(sent()[0]?.find((h) => h.hub === "top_rated")).toEqual({ hub: "top_rated", on_library: true, on_admin_home: false, on_home: true });

    // A collection becomes a row, on the library's page, last.
    await user.selectOptions(screen.getByRole("combobox", { name: "Add a collection as a row…" }), "7");
    expect(await screen.findByText("Heists")).toBeInTheDocument();
    expect(sent()[1]?.at(-1)).toEqual({ hub: "collection:7", on_library: true, on_admin_home: false, on_home: false });
    // Nothing left to add.
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();

    // Moved to the top.
    await user.click(screen.getByRole("button", { name: "Move Heists up" }));
    await user.click(screen.getByRole("button", { name: "Move Heists up" }));
    await user.click(screen.getByRole("button", { name: "Move Heists up" }));
    await waitFor(() => expect(rowTitles()[0]).toBe("Heists"));
    expect(sent().at(-1)?.map((h) => h.hub)).toEqual(["collection:7", "continue_watching", "recently_added", "top_rated"]);
    expect(screen.getByRole("button", { name: "Move Heists up" })).toBeDisabled();

    // Taken away again, then everything as usual.
    await user.click(screen.getByRole("button", { name: "Stop showing Heists as a row" }));
    await waitFor(() => expect(screen.queryByText("Heists", { selector: "span" })).not.toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: "Use the usual rows" }));
    await waitFor(() => expect(requests.some((r) => r.method === "DELETE")).toBe(true));
    expect(await screen.findByRole("checkbox", { name: "Show Top Rated on everyone else's Home" })).not.toBeChecked();
  });
});
