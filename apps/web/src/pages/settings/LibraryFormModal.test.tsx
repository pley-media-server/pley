import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Library } from "@/api/types";
import { mockApi, renderWithProviders } from "@/test/render";
import { LibraryFormModal } from "./LibraryFormModal";

const films: Library = {
  id: 1,
  name: "Films",
  kind: "movies",
  paths: ["/media/films"],
  item_count: 0,
  created_at: 0,
  scanned_at: null,
  scanning: false,
  tmdb_collections: true,
};

function serve() {
  return mockApi({
    "POST /api/fs": () => ({ path: null, parent: null, entries: [] }),
    "PATCH /api/libraries/1": (req) => ({ ...films, ...(req.body as object) }),
    "POST /api/libraries": (req) => ({ ...films, id: 2, ...(req.body as object) }),
  });
}

const sent = (requests: ReturnType<typeof serve>, method: string) => requests.find((r) => r.method === method)?.body;

/** The dialog for `library`, opened as Settings opens it: it takes the library's values as it opens. */
function opened(library: Library) {
  const view = renderWithProviders(<LibraryFormModal open={false} onClose={vi.fn()} library={library} />);
  view.rerender(<LibraryFormModal open onClose={vi.fn()} library={library} />);
}

describe("LibraryFormModal", () => {
  it("turns off a film library's collections from TMDB once that is confirmed", async () => {
    const user = userEvent.setup();
    const requests = serve();
    opened(films);
    const toggle = await screen.findByRole("switch", { name: "Make collections from TMDB" });
    expect(toggle).toHaveAttribute("aria-checked", "true");
    await user.click(toggle);
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    expect(await screen.findByText("Remove collections from TMDB?")).toBeInTheDocument();
    expect(sent(requests, "PATCH")).toBeUndefined();
    await user.click(screen.getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(sent(requests, "PATCH")).toEqual({ name: "Films", paths: ["/media/films"], tmdb_collections: false }));
  });

  it("sends nothing when turning them off is cancelled, and goes back to the form", async () => {
    const user = userEvent.setup();
    const requests = serve();
    opened(films);
    await user.click(await screen.findByRole("switch", { name: "Make collections from TMDB" }));
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await screen.findByText("Remove collections from TMDB?");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(await screen.findByRole("button", { name: "Save changes" })).toBeInTheDocument();
    expect(sent(requests, "PATCH")).toBeUndefined();
  });

  it("asks nothing when turning them on", async () => {
    const user = userEvent.setup();
    const requests = serve();
    opened({ ...films, tmdb_collections: false });
    await user.click(await screen.findByRole("switch", { name: "Make collections from TMDB" }));
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(sent(requests, "PATCH")).toEqual({ name: "Films", paths: ["/media/films"], tmdb_collections: true }));
    expect(screen.queryByText("Remove collections from TMDB?")).not.toBeInTheDocument();
  });

  it("leaves the switch out of a save that doesn't change it", async () => {
    const user = userEvent.setup();
    const requests = serve();
    opened({ ...films, tmdb_collections: false });
    expect(await screen.findByRole("switch", { name: "Make collections from TMDB" })).toHaveAttribute("aria-checked", "false");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(sent(requests, "PATCH")).toEqual({ name: "Films", paths: ["/media/films"] }));
  });

  it("shows the switch for film libraries only", async () => {
    serve();
    opened({ ...films, kind: "shows" });
    expect(await screen.findByRole("button", { name: "Save changes" })).toBeInTheDocument();
    expect(screen.queryByRole("switch")).not.toBeInTheDocument();
  });
});
