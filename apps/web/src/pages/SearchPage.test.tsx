import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { makeItem } from "@/test/fixtures";
import { apiError, mockApi, renderWithProviders } from "@/test/render";
import { makeSummary, makeTitle } from "@/test/requests";
import { SearchPage } from "./SearchPage";
import { personUrl } from "@/lib/urls";

function search(q: string) {
  return renderWithProviders(<SearchPage />, { route: `/search?q=${encodeURIComponent(q)}` });
}

describe("SearchPage", () => {
  it("groups the results by kind, people first", async () => {
    const requests = mockApi({
      "GET /api/search": () => ({
        items: [
          makeItem({ id: 1, kind: "episode", title: "The Heat Is On", parent_index: 2, index: 4 }),
          makeItem({ id: 2, title: "Heat", year: 1995 }),
          makeItem({ id: 3, title: "The Heat", year: 2013 }),
        ],
        people: [{ id: 9, name: "Heath Ledger" }],
      }),
    });
    search("heat ");

    expect(await screen.findByRole("heading", { name: "Results for “heat ”" })).toBeInTheDocument();
    const headings = screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent);
    expect(headings).toEqual(["People 1", "Movies 2", "Episodes 1"]);
    expect(screen.getByRole("link", { name: /Heath Ledger/ })).toHaveAttribute("href", personUrl(9));

    const movies = screen.getByRole("heading", { name: "Movies 2" }).closest("section") as HTMLElement;
    expect(within(movies).getByText("1995")).toBeInTheDocument();
    // The query is trimmed.
    expect(requests.find((r) => r.path.startsWith("/api/search"))?.path).toBe("/api/search?q=heat&limit=60");
    // Without requests (here, a server without them), nothing from TMDB.
    expect(screen.queryByRole("heading", { name: "Discover" })).not.toBeInTheDocument();
    expect(requests.some((r) => r.path.startsWith("/api/discover"))).toBe(false);
  });

  it("adds what TMDB has that the libraries don't, where this account may ask", async () => {
    const requests = mockApi({
      "GET /api/search": () => ({ items: [makeItem({ id: 2, title: "Dune", year: 2021 })] }),
      "GET /api/requests/summary": () => makeSummary(),
      "GET /api/discover/search": () => ({
        page: 1,
        total_pages: 1,
        results: [makeTitle({ tmdb_id: 841, title: "Dune", year: 1984 }), makeTitle({ tmdb_id: 693134, title: "Dune: Part Two", year: 2024 })],
      }),
    });
    search("dune ");

    expect(await screen.findByRole("link", { name: "See all" })).toHaveAttribute("href", "/discover?q=dune");
    const heading = screen.getByRole("heading", { name: "Discover" });
    expect(within(heading).getByRole("link")).toHaveAttribute("href", "/discover?q=dune");
    expect(screen.getByRole("link", { name: "Dune: Part Two, 2024, Movie" })).toHaveAttribute("href", "/discover/movie/693134");
    // After the libraries' results.
    const headings = screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent);
    expect(headings).toEqual(["Movies 1", "Discover"]);
    expect(requests.find((r) => r.path.startsWith("/api/discover/search"))?.path).toBe("/api/discover/search?q=dune&missing=1");
  });

  it("asks TMDB for as much of a long query as the server takes", async () => {
    const requests = mockApi({
      "GET /api/search": () => ({ items: [makeItem({ id: 2, title: "Dune", year: 2021 })] }),
      "GET /api/requests/summary": () => makeSummary(),
      "GET /api/discover/search": () => ({ page: 1, total_pages: 1, results: [makeTitle()] }),
    });
    search("dune ".repeat(60));

    expect(await screen.findByRole("heading", { name: "Discover" })).toBeInTheDocument();
    const asked = new URL(requests.find((r) => r.path.startsWith("/api/discover/search"))?.path ?? "", "http://pley.test");
    expect(asked.searchParams.get("q")).toBe("dune ".repeat(40).trim());
  });

  it("shows TMDB's titles under a line when the libraries have nothing", async () => {
    mockApi({
      "GET /api/search": () => ({ items: [] }),
      "GET /api/requests/summary": () => makeSummary(),
      "GET /api/discover/search": () => ({ page: 1, total_pages: 1, results: [makeTitle()] }),
    });
    search("matrix");
    expect(await screen.findByText("Nothing in your libraries matches.")).toBeInTheDocument();
    expect(await screen.findByRole("link", { name: "The Matrix, 1999, Movie" })).toBeInTheDocument();
    expect(screen.queryByText("No results for “matrix”")).not.toBeInTheDocument();
  });

  it("never holds the libraries' results back for TMDB", async () => {
    mockApi({
      "GET /api/search": () => ({ items: [makeItem({ id: 2, title: "Heat", year: 1995 })] }),
      "GET /api/requests/summary": () => makeSummary(),
      "GET /api/discover/search": () => new Promise(() => {}),
    });
    search("heat");
    expect(await screen.findByRole("heading", { name: "Movies 1" })).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Discover" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Discover" }).closest("section")).toHaveAttribute("aria-busy", "true");
  });

  it("says what went wrong with TMDB without hiding the libraries' results", async () => {
    mockApi({
      "GET /api/search": () => ({ items: [makeItem({ id: 2, title: "Heat", year: 1995 })] }),
      "GET /api/requests/summary": () => makeSummary(),
      "GET /api/discover/search": () => apiError(503, "tmdb_down", "TMDB didn't answer"),
    });
    search("heat");
    expect(await screen.findByRole("alert")).toHaveTextContent("TMDB isn't answering");
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Movies 1" })).toBeInTheDocument();
  });

  it("invites a search without a query", () => {
    const requests = mockApi({});
    search("  ");
    expect(screen.getByText("Search your libraries")).toBeInTheDocument();
    expect(requests).toHaveLength(0);
  });

  it("says when nothing matched", async () => {
    mockApi({ "GET /api/search": () => ({ items: [] }) });
    search("zzz");
    expect(await screen.findByText("No results for “zzz”")).toBeInTheDocument();
  });

  it("explains failures", async () => {
    mockApi({ "GET /api/search": () => apiError(500, "internal", "index is rebuilding") });
    search("heat");
    expect(await screen.findByText("Index is rebuilding")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Couldn't search your libraries" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
  });
});
