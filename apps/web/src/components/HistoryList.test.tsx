import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { HistoryEntry } from "@/api/types";
import { makeHistoryEntry, makeItem } from "@/test/fixtures";
import { apiError, mockApi, renderWithProviders } from "@/test/render";
import { HistoryList } from "./HistoryList";
import { itemUrl } from "@/lib/urls";

const pilot = makeHistoryEntry(
  1,
  makeItem({ kind: "episode", title: "Pilot", grandparent_title: "Lost", parent_index: 1, index: 1 }),
  { device: "Chrome on Windows" },
);
const jaws = makeHistoryEntry(2, makeItem({ title: "Jaws", year: 1975 }), {
  user: { id: 2, name: "Kid", avatar_color: "#4ade80" },
});
const song = makeHistoryEntry(3, makeItem({ kind: "track", title: "Aerodynamic", grandparent_title: "Daft Punk", parent_title: "Discovery" }));

/** Serves `entries` in pages, like the server. */
function serve(entries: HistoryEntry[]) {
  let current = entries;
  const requests = mockApi({
    "GET /api/history": (req) => {
      const params = new URL(req.path, "http://pley.test").searchParams;
      const offset = Number(params.get("offset") ?? 0);
      const limit = Number(params.get("limit") ?? 20);
      return { total: current.length, entries: current.slice(offset, offset + limit) };
    },
    "DELETE /api/history/1": () => {
      current = current.filter((e) => e.id !== 1);
      return undefined;
    },
    "DELETE /api/history/2": () => apiError(403, "forbidden", "not yours"),
  });
  return requests;
}

describe("HistoryList", () => {
  it("describes each play", async () => {
    serve([pilot, jaws, song]);
    renderWithProviders(<HistoryList scope={{}} empty="Nothing yet" />);

    const rows = await screen.findAllByRole("listitem");
    expect(rows).toHaveLength(3);
    expect(rows[0]).toHaveTextContent("Lost");
    expect(rows[0]).toHaveTextContent("S1 · E1 · Pilot");
    expect(rows[0]).toHaveTextContent("Chrome on Windows");
    expect(rows[1]).toHaveTextContent("Jaws");
    expect(rows[1]).toHaveTextContent("1975");
    expect(rows[2]).toHaveTextContent("Daft Punk · Discovery");
    expect(within(rows[1] as HTMLElement).getByRole("link", { name: "Jaws" })).toHaveAttribute("href", itemUrl(jaws.item.id));
  });

  it("shows who played what in everyone's history", async () => {
    const requests = serve([jaws]);
    renderWithProviders(<HistoryList scope={{ all: true }} showUser empty="Nothing yet" />);

    expect(await screen.findByText("Kid")).toBeInTheDocument();
    expect(requests[0]?.path).toBe("/api/history?all=1&offset=0&limit=20");
  });

  it("loads more on request", async () => {
    const user = userEvent.setup();
    const requests = serve([pilot, jaws, song]);
    renderWithProviders(<HistoryList scope={{ userId: 4 }} pageSize={2} empty="Nothing yet" />);

    expect(await screen.findAllByRole("listitem")).toHaveLength(2);
    await user.click(screen.getByRole("button", { name: "Show more" }));

    await waitFor(() => expect(screen.getAllByRole("listitem")).toHaveLength(3));
    expect(requests.map((r) => r.path)).toEqual([
      "/api/history?user_id=4&offset=0&limit=2",
      "/api/history?user_id=4&offset=2&limit=2",
    ]);
    expect(screen.queryByRole("button", { name: "Show more" })).not.toBeInTheDocument();
  });

  it("removes entries", async () => {
    const user = userEvent.setup();
    serve([pilot, jaws]);
    renderWithProviders(<HistoryList scope={{}} empty="Nothing yet" />);

    const [first] = await screen.findAllByRole("listitem");
    await user.click(within(first as HTMLElement).getByRole("button", { name: "Remove from history" }));

    await waitFor(() => expect(screen.getAllByRole("listitem")).toHaveLength(1));
    expect(screen.queryByText("Lost")).not.toBeInTheDocument();
  });

  it("explains a failed removal", async () => {
    const user = userEvent.setup();
    serve([jaws]);
    renderWithProviders(<HistoryList scope={{}} empty="Nothing yet" />);

    await user.click(await screen.findByRole("button", { name: "Remove from history" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Not yours");
    expect(screen.getAllByRole("listitem")).toHaveLength(1);
  });

  it("says when there is nothing", async () => {
    serve([]);
    renderWithProviders(<HistoryList scope={{}} empty="Nothing played yet" />);
    expect(await screen.findByText("Nothing played yet")).toBeInTheDocument();
  });

  it("shows why it couldn't load", async () => {
    mockApi({ "GET /api/history": () => apiError(500, "internal", "database is locked") });
    renderWithProviders(<HistoryList scope={{}} empty="Nothing yet" />);
    expect(await screen.findByText("Database is locked")).toBeInTheDocument();
  });
});
