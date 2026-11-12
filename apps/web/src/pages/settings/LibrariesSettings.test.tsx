import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Library } from "@/api/types";
import { Sidebar } from "@/components/Sidebar";
import { apiError, mockApi, renderWithProviders, testUser, type FakeRequest } from "@/test/render";
import { LibrariesSettings } from "./LibrariesSettings";

function library(id: number, name: string, kind: Library["kind"]): Library {
  return { id, name, kind, paths: [`/media/${name.toLowerCase()}`], item_count: 0, created_at: 0, scanned_at: null, scanning: false };
}

const libraries = [library(1, "Movies", "movies"), library(2, "TV", "shows"), library(3, "Music", "music")];
const names = libraries.map((l) => l.name);

/**
 * The libraries, the same in the shared order and the user's (who hasn't
 * sorted their own); a new order is saved as asked unless `order` answers
 * otherwise (`save` saves it), and `refetch` answers the libraries after the
 * first time (both may hold the answer back).
 */
function serve({
  list = libraries,
  order,
  refetch,
}: { list?: Library[]; order?: (req: FakeRequest, save: () => Library[]) => unknown; refetch?: () => unknown } = {}) {
  let current = list;
  let fetched = 0;
  return mockApi({
    "GET /api/libraries": () => (fetched++ > 0 && refetch ? refetch() : current),
    "GET /api/activity": () => ({ scans: [], sessions: [] }),
    "GET /api/import/plex": () => ({ state: "idle", step: null, done: 0, total: 0, error: null, report: null, first_pass_done: false }),
    "GET /api/playlists": () => ({ playlists: [] }),
    "PUT /api/libraries/order": (req) => {
      const save = () => {
        const { ids } = req.body as { ids: number[] };
        current = ids.flatMap((id) => list.filter((l) => l.id === id));
        return current;
      };
      return order ? order(req, save) : save();
    },
  });
}

/** Where the settings page fetches the libraries: in the shared order. */
const SHARED = "/api/libraries?order=shared";

/** An answer to give later: `answer` sends it. */
function heldBack() {
  let answer: (value: unknown) => void = () => {};
  const reply = new Promise((resolve) => {
    answer = resolve;
  });
  return { reply: () => reply, answer: (value: unknown) => answer(value) };
}

/** Holds back every answer to a new order: `answers[n]` sends the n-th. */
function heldBackOrders() {
  const answers: ReturnType<typeof heldBack>[] = [];
  const order = () => {
    const held = heldBack();
    answers.push(held);
    return held.reply();
  };
  return { answers, order };
}

const refused = () => apiError(409, "conflict", "the libraries have changed: the order must list each of them once");

/** Lets the page catch up with the cache, which tells it on a timer. */
const settle = () => act(() => new Promise((resolve) => setTimeout(resolve, 10)));

/** The new orders sent. */
const puts = (requests: FakeRequest[]) => requests.filter((r) => r.method === "PUT");

/** The libraries a part of the page lists, in order. */
function namesIn(element: HTMLElement): string[] {
  return within(element)
    .getAllByRole("link")
    .map((link) => link.textContent ?? "")
    .filter((name) => names.includes(name));
}

/** The row of a library in the settings list. */
function rowOf(name: string): HTMLElement {
  const row = within(screen.getByRole("list", { name: "Libraries" }))
    .getByRole("link", { name })
    .closest<HTMLElement>("[data-library]");
  if (!row) throw new Error(`no row for ${name}`);
  return row;
}

/** What a library is dragged by. */
function gripOf(name: string): HTMLElement {
  const grip = rowOf(name).querySelector<HTMLElement>("[data-drag-handle]");
  if (!grip) throw new Error(`no grip for ${name}`);
  return grip;
}

/** A drag's data, as browsers hand it over. */
function dataTransfer() {
  return { effectAllowed: "", dropEffect: "", setData: vi.fn(), setDragImage: vi.fn() };
}

describe("LibrariesSettings", () => {
  it("shows a library waiting for Plex's labels now when asked", async () => {
    const user = userEvent.setup();
    const pending = { ...libraries[0]!, labels_pending: true };
    let shown = false;
    const requests = mockApi({
      "GET /api/libraries": () => [shown ? libraries[0] : pending, ...libraries.slice(1)],
      "GET /api/activity": () => ({ scans: [], sessions: [] }),
      "GET /api/import/plex": () => ({ state: "idle", step: null, done: 0, total: 0, error: null, report: null, first_pass_done: false }),
      "GET /api/playlists": () => ({ playlists: [] }),
      "PATCH /api/libraries/1": () => {
        shown = true;
        return libraries[0];
      },
    });
    renderWithProviders(<LibrariesSettings />);
    expect(await screen.findByText(/^Waiting for Plex's labels/)).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Show it now" })).toHaveLength(1);
    await user.click(screen.getByRole("button", { name: "Show it now" }));
    expect(requests.find((r) => r.method === "PATCH")?.body).toEqual({ labels_pending: false });
    await waitFor(() => expect(screen.queryByText(/^Waiting for Plex's labels/)).not.toBeInTheDocument());
  });

  it("moves a library with its buttons and keeps the focus on it", async () => {
    const user = userEvent.setup();
    const requests = serve();
    renderWithProviders(<LibrariesSettings />);
    const list = await screen.findByRole("list", { name: "Libraries" });
    expect(namesIn(list)).toEqual(["Movies", "TV", "Music"]);
    expect(screen.getByRole("button", { name: "Move Movies up" })).toBeDisabled();

    screen.getByRole("button", { name: "Move Movies down" }).focus();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(namesIn(list)).toEqual(["TV", "Movies", "Music"]));
    expect(screen.getByRole("button", { name: "Move Movies down" })).toHaveFocus();
    expect(screen.getByRole("status")).toHaveTextContent("Movies moved to position 2 of 3");
    await waitFor(() => expect(puts(requests)).toHaveLength(1));
    expect(puts(requests)[0]?.body).toEqual({ ids: [2, 1, 3] });

    // Last in the list, its "down" button stops working: the focus goes up.
    await user.keyboard("{Enter}");
    await waitFor(() => expect(namesIn(list)).toEqual(["TV", "Music", "Movies"]));
    expect(screen.getByRole("button", { name: "Move Movies down" })).toBeDisabled();
    await waitFor(() => expect(screen.getByRole("button", { name: "Move Movies up" })).toHaveFocus());
    expect(screen.getByRole("status")).toHaveTextContent("Movies moved to position 3 of 3");
    await waitFor(() => expect(puts(requests)).toHaveLength(2));
    expect(puts(requests)[1]?.body).toEqual({ ids: [2, 3, 1] });
  });

  it("moves a library picked up by its grip where it is dropped", async () => {
    const requests = serve();
    renderWithProviders(<LibrariesSettings />);
    const list = await screen.findByRole("list", { name: "Libraries" });
    // The rest of the card keeps its text selectable.
    expect(rowOf("Music")).not.toHaveAttribute("draggable");
    expect(gripOf("Music")).toHaveAttribute("draggable", "true");

    const drag = dataTransfer();
    fireEvent.dragStart(rowOf("TV"), { dataTransfer: drag });
    fireEvent.drop(rowOf("Movies"), { dataTransfer: drag });
    expect(namesIn(list)).toEqual(["Movies", "TV", "Music"]);

    fireEvent.dragStart(gripOf("Music"), { dataTransfer: drag });
    // The whole card goes with the pointer.
    expect(drag.setDragImage).toHaveBeenCalledWith(rowOf("Music"), expect.any(Number), expect.any(Number));
    fireEvent.dragOver(rowOf("Movies"), { dataTransfer: drag });
    fireEvent.drop(rowOf("Movies"), { dataTransfer: drag });

    await waitFor(() => expect(namesIn(list)).toEqual(["Music", "Movies", "TV"]));
    await waitFor(() => expect(puts(requests)).toHaveLength(1));
    expect(puts(requests)[0]?.body).toEqual({ ids: [3, 1, 2] });
  });

  it("takes a drop in the gap where the line shows it", async () => {
    const requests = serve();
    renderWithProviders(<LibrariesSettings />);
    const list = await screen.findByRole("list", { name: "Libraries" });

    const drag = dataTransfer();
    fireEvent.dragStart(gripOf("Movies"), { dataTransfer: drag });
    fireEvent.dragOver(rowOf("Music"), { dataTransfer: drag });
    // Off the cards, between them: the drop is still taken (cancelled).
    expect(fireEvent.dragOver(list, { dataTransfer: drag })).toBe(false);
    fireEvent.drop(list, { dataTransfer: drag });

    await waitFor(() => expect(namesIn(list)).toEqual(["TV", "Music", "Movies"]));
    await waitFor(() => expect(puts(requests)).toHaveLength(1));
    expect(puts(requests)[0]?.body).toEqual({ ids: [2, 3, 1] });
  });

  it("takes a drop just past the last library, and shows no line off the list", async () => {
    const requests = serve();
    renderWithProviders(<LibrariesSettings />);
    const list = await screen.findByRole("list", { name: "Libraries" });
    const lineUnder = (name: string) => rowOf(name).querySelector<HTMLElement>(":scope > [aria-hidden]:not([data-drag-handle])");

    const drag = dataTransfer();
    fireEvent.dragStart(gripOf("Movies"), { dataTransfer: drag });
    fireEvent.dragOver(rowOf("Music"), { dataTransfer: drag });
    const strip = lineUnder("Music");
    expect(strip).not.toBeNull();
    // Off the list: the line goes.
    fireEvent.dragLeave(list, { dataTransfer: drag, relatedTarget: document.body });
    expect(lineUnder("Music")).toBeNull();

    // Back over Music, then just under its card, on the line's strip.
    fireEvent.dragOver(rowOf("Music"), { dataTransfer: drag });
    const under = lineUnder("Music");
    if (!under) throw new Error("no line under Music");
    expect(fireEvent.dragOver(under, { dataTransfer: drag })).toBe(false);
    fireEvent.drop(under, { dataTransfer: drag });
    await waitFor(() => expect(namesIn(list)).toEqual(["TV", "Music", "Movies"]));
    await waitFor(() => expect(puts(requests)).toHaveLength(1));
  });

  it("moves the library picked up when the list changes during the drag", async () => {
    const requests = serve();
    const { client } = renderWithProviders(<LibrariesSettings />);
    const list = await screen.findByRole("list", { name: "Libraries" });

    const drag = dataTransfer();
    fireEvent.dragStart(gripOf("TV"), { dataTransfer: drag });
    // Another administrator deletes Movies meanwhile: TV is first now.
    await act(() => client.setQueryData(["libraries", "shared"], [libraries[1], libraries[2]]));
    await waitFor(() => expect(namesIn(list)).toEqual(["TV", "Music"]));
    expect(rowOf("TV")).toHaveClass("opacity-40");
    fireEvent.dragOver(rowOf("Music"), { dataTransfer: drag });
    fireEvent.drop(rowOf("Music"), { dataTransfer: drag });

    await waitFor(() => expect(puts(requests)).toHaveLength(1));
    expect(puts(requests)[0]?.body).toEqual({ ids: [3, 2] });
  });

  it("shows the new order in the sidebar before the server answers, while the sidebar shows the shared one", async () => {
    const user = userEvent.setup();
    const saved = heldBack();
    serve({ order: (_req, save) => saved.reply().then(save) });
    const { client } = renderWithProviders(
      <>
        <Sidebar open={false} onClose={() => {}} />
        <LibrariesSettings />
      </>,
    );
    const sidebar = await screen.findByRole("navigation", { name: "Main" });
    await waitFor(() => expect(namesIn(sidebar)).toEqual(["Movies", "TV", "Music"]));

    await user.click(await screen.findByRole("button", { name: "Move Music up" }));
    await waitFor(() => expect(namesIn(sidebar)).toEqual(["Movies", "Music", "TV"]));
    saved.answer([libraries[0], libraries[2], libraries[1]]);
    await waitFor(() => expect(client.isMutating()).toBe(0));
    await settle();
    expect(namesIn(sidebar)).toEqual(["Movies", "Music", "TV"]);
  });

  it("keeps a move on screen when the libraries are fetched before it is saved", async () => {
    const user = userEvent.setup();
    const saved = heldBack();
    // Fetched again, they still come in the old order.
    const requests = serve({ order: saved.reply, refetch: () => libraries });
    const { client } = renderWithProviders(<LibrariesSettings />);
    const list = await screen.findByRole("list", { name: "Libraries" });

    await user.click(screen.getByRole("button", { name: "Move Music up" }));
    await waitFor(() => expect(namesIn(list)).toEqual(["Movies", "Music", "TV"]));
    // What a server event does (a scan finished, another tab's change).
    await act(() => client.invalidateQueries({ queryKey: ["libraries"] }));
    await settle();
    expect(requests.filter((r) => r.path === SHARED)).toHaveLength(2);
    expect(namesIn(list)).toEqual(["Movies", "Music", "TV"]);

    saved.answer([libraries[0], libraries[2], libraries[1]]);
    await waitFor(() => expect(client.isMutating()).toBe(0));
    await settle();
    expect(namesIn(list)).toEqual(["Movies", "Music", "TV"]);
  });

  it("saves quick moves one after the other, showing the last", async () => {
    const user = userEvent.setup();
    const { answers, order } = heldBackOrders();
    const requests = serve({ order });
    const { client } = renderWithProviders(<LibrariesSettings />);
    const list = await screen.findByRole("list", { name: "Libraries" });

    await user.click(screen.getByRole("button", { name: "Move Music up" }));
    await waitFor(() => expect(namesIn(list)).toEqual(["Movies", "Music", "TV"]));
    await user.click(screen.getByRole("button", { name: "Move Music up" }));
    await waitFor(() => expect(namesIn(list)).toEqual(["Music", "Movies", "TV"]));
    await waitFor(() => expect(answers).toHaveLength(1));
    // The second waits for the first to be answered.
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(puts(requests)).toHaveLength(1);

    answers[0]?.answer([libraries[0], libraries[2], libraries[1]]);
    await waitFor(() => expect(answers).toHaveLength(2));
    await settle();
    expect(puts(requests)[1]?.body).toEqual({ ids: [3, 1, 2] });
    expect(namesIn(list)).toEqual(["Music", "Movies", "TV"]);

    answers[1]?.answer([libraries[2], libraries[0], libraries[1]]);
    await waitFor(() => expect(client.isMutating()).toBe(0));
    await settle();
    expect(namesIn(list)).toEqual(["Music", "Movies", "TV"]);
  });

  it("puts the old order back when the server refuses the new one", async () => {
    const user = userEvent.setup();
    const saved = heldBack();
    const refetched = heldBack();
    const requests = serve({ order: saved.reply, refetch: refetched.reply });
    renderWithProviders(<LibrariesSettings />);
    const list = await screen.findByRole("list", { name: "Libraries" });

    await user.click(screen.getByRole("button", { name: "Move Music up" }));
    await waitFor(() => expect(namesIn(list)).toEqual(["Movies", "Music", "TV"]));
    saved.answer(refused());

    // Back at once, before the libraries are fetched again (in case some
    // came or went meanwhile), and said so.
    expect(await screen.findByText("The libraries have changed: the order must list each of them once")).toBeInTheDocument();
    await waitFor(() => expect(namesIn(list)).toEqual(["Movies", "TV", "Music"]));
    expect(screen.getByRole("status")).toHaveTextContent("Music moved back to position 3 of 3");
    expect(screen.getByRole("button", { name: "Move Music up" })).toHaveFocus();
    await waitFor(() => expect(requests.filter((r) => r.path === SHARED)).toHaveLength(2));
    refetched.answer(libraries);
  });

  it("keeps a saved move when a later one is refused", async () => {
    const user = userEvent.setup();
    const { answers, order } = heldBackOrders();
    const refetched = heldBack();
    serve({ order, refetch: refetched.reply });
    renderWithProviders(<LibrariesSettings />);
    const list = await screen.findByRole("list", { name: "Libraries" });

    await user.click(screen.getByRole("button", { name: "Move Music up" }));
    await waitFor(() => expect(namesIn(list)).toEqual(["Movies", "Music", "TV"]));
    await user.click(screen.getByRole("button", { name: "Move Music up" }));
    await waitFor(() => expect(namesIn(list)).toEqual(["Music", "Movies", "TV"]));
    await waitFor(() => expect(answers).toHaveLength(1));
    answers[0]?.answer([libraries[0], libraries[2], libraries[1]]);
    await waitFor(() => expect(answers).toHaveLength(2));

    // Refused, before the libraries come back from the server: the list
    // shows the order last saved, with the first move.
    answers[1]?.answer(refused());
    await waitFor(() => expect(namesIn(list)).toEqual(["Movies", "Music", "TV"]));
    expect(screen.getByRole("status")).toHaveTextContent("Music moved back to position 2 of 3");
    refetched.answer([libraries[0], libraries[2], libraries[1]]);
  });

  it("keeps the focus on a library whose move is undone", async () => {
    const user = userEvent.setup();
    const saved = heldBack();
    serve({ list: libraries.slice(0, 2), order: saved.reply });
    renderWithProviders(<LibrariesSettings />);
    const list = await screen.findByRole("list", { name: "Libraries" });

    screen.getByRole("button", { name: "Move Movies down" }).focus();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(namesIn(list)).toEqual(["TV", "Movies"]));
    // Last, its "down" button stops working: the focus goes up.
    await waitFor(() => expect(screen.getByRole("button", { name: "Move Movies up" })).toHaveFocus());

    // First again once refused: now "up" stops working, the focus goes down.
    saved.answer(refused());
    await waitFor(() => expect(namesIn(list)).toEqual(["Movies", "TV"]));
    await waitFor(() => expect(screen.getByRole("button", { name: "Move Movies down" })).toHaveFocus());
  });

  it("puts the saved order back when quick moves are all refused", async () => {
    const user = userEvent.setup();
    const { answers, order } = heldBackOrders();
    serve({ order });
    renderWithProviders(<LibrariesSettings />);
    const list = await screen.findByRole("list", { name: "Libraries" });

    await user.click(screen.getByRole("button", { name: "Move Music up" }));
    await waitFor(() => expect(namesIn(list)).toEqual(["Movies", "Music", "TV"]));
    await user.click(screen.getByRole("button", { name: "Move Music up" }));
    await waitFor(() => expect(namesIn(list)).toEqual(["Music", "Movies", "TV"]));
    await waitFor(() => expect(answers).toHaveLength(1));
    answers[0]?.answer(refused());
    await waitFor(() => expect(answers).toHaveLength(2));
    await settle();
    // The second move still shows until it is answered.
    expect(namesIn(list)).toEqual(["Music", "Movies", "TV"]);

    answers[1]?.answer(refused());
    await waitFor(() => expect(namesIn(list)).toEqual(["Movies", "TV", "Music"]));
    expect(screen.getByRole("status")).toHaveTextContent("Music moved back to position 3 of 3");
  });

  it("keeps an order saved after a profile switch out of the new profile's cache", async () => {
    const user = userEvent.setup();
    const saved = heldBack();
    serve({ order: saved.reply });
    const { client, unmount } = renderWithProviders(<LibrariesSettings />);
    client.setQueryData(["me"], testUser);
    await user.click(await screen.findByRole("button", { name: "Move Music up" }));
    const [move] = client.getMutationCache().getAll();

    // The administrator switches to a profile that sees fewer libraries
    // before the answer comes, which starts the cache over.
    unmount();
    client.clear();
    client.setQueryData(["me"], { ...testUser, id: 2, is_admin: false });
    saved.answer([libraries[0], libraries[2], libraries[1]]);

    await waitFor(() => expect(move?.state.status).toBe("success"));
    expect(client.getQueryData(["libraries", "shared"])).toBeUndefined();
    expect(client.getQueryData(["libraries"])).toBeUndefined();
  });

  it("leaves a user's own order alone in the sidebar", async () => {
    const user = userEvent.setup();
    const own = [libraries[2], libraries[1], libraries[0]] as Library[];
    mockApi({
      "GET /api/libraries": (req) => (req.path === SHARED ? libraries : own),
      "GET /api/activity": () => ({ scans: [], sessions: [] }),
      "GET /api/import/plex": () => ({ state: "idle", step: null, done: 0, total: 0, error: null, report: null, first_pass_done: false }),
      "GET /api/playlists": () => ({ playlists: [] }),
      "PUT /api/libraries/order": () => [libraries[0], libraries[2], libraries[1]],
    });
    const { client } = renderWithProviders(
      <>
        <Sidebar open={false} onClose={() => {}} />
        <LibrariesSettings />
      </>,
    );
    const sidebar = await screen.findByRole("navigation", { name: "Main" });
    const list = await screen.findByRole("list", { name: "Libraries" });
    await waitFor(() => expect(namesIn(sidebar)).toEqual(["Music", "TV", "Movies"]));
    expect(namesIn(list)).toEqual(["Movies", "TV", "Music"]);
    expect(screen.getByText("Everyone sees them in this order, unless they sort their own.", { exact: false })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Move Music up" }));
    await waitFor(() => expect(namesIn(list)).toEqual(["Movies", "Music", "TV"]));
    await waitFor(() => expect(client.isMutating()).toBe(0));
    await settle();
    expect(namesIn(sidebar)).toEqual(["Music", "TV", "Movies"]);
  });

  it("has nothing to move with a single library", async () => {
    serve({ list: [libraries[0] as Library] });
    renderWithProviders(<LibrariesSettings />);
    await screen.findByRole("list", { name: "Libraries" });

    expect(screen.queryByRole("button", { name: /^Move / })).not.toBeInTheDocument();
    expect(rowOf("Movies")).not.toHaveAttribute("draggable");
    expect(rowOf("Movies").querySelector("[data-drag-handle]")).toBeNull();
    expect(screen.queryByText("Everyone sees them in this order", { exact: false })).not.toBeInTheDocument();
  });
});
