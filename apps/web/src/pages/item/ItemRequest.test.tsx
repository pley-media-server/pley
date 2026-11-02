import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ItemRequest } from "@/api/requests";
import type { ItemDetail } from "@/api/types";
import { makeItem } from "@/test/fixtures";
import { apiError, mockApi, renderWithProviders, testUser } from "@/test/render";
import { makeRequest, makeSummary } from "@/test/requests";
import { ItemRequestLine } from "./ItemRequest";

const ann = { ...testUser, id: 2, username: "ann", name: "Ann", is_admin: false };

const dark: ItemDetail = {
  ...makeItem({ id: 40, kind: "show", title: "Dark", year: 2017 }),
  original_title: null,
  tagline: null,
  genres: [],
  studio: null,
  content_rating: null,
  tmdb_id: 70523,
  imdb_id: null,
  files: [],
  in_watchlist: false,
  cast: [],
  crew: [],
  extras: [],
};

function standing(overrides: Partial<ItemRequest> = {}): ItemRequest {
  return { kind: "show", tmdb_id: 70523, status: "partial", request: null, askable: [3, 4], ...overrides };
}

describe("ItemRequestLine", () => {
  it("offers the seasons a library's show lacks, on Discover's page with its picker", async () => {
    mockApi({ "GET /api/requests/summary": () => makeSummary(), "GET /api/items/40/request": () => standing() });
    renderWithProviders(<ItemRequestLine item={dark} />, { user: ann });
    expect(await screen.findByRole("link", { name: "Request more seasons" })).toHaveAttribute("href", "/discover/show/70523?pick=1");
  });

  it("shows where a request for it is", async () => {
    const request = makeRequest({ id: 5, kind: "show", tmdb_id: 70523, title: "Dark", seasons: [3], status: "approved", stage: "searching", mine: true });
    mockApi({ "GET /api/requests/summary": () => makeSummary(), "GET /api/items/40/request": () => standing({ request, askable: [4] }) });
    renderWithProviders(<ItemRequestLine item={dark} />, { user: ann });
    expect(await screen.findByText("Searching")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "See the request" })).toHaveAttribute("href", "/discover/show/70523");
  });

  it("shows nothing when there's nothing to ask for, or no TMDB match", async () => {
    const requests = mockApi({ "GET /api/requests/summary": () => makeSummary(), "GET /api/items/40/request": () => standing({ status: "library", askable: [] }) });
    const { container, unmount } = renderWithProviders(<ItemRequestLine item={dark} />, { user: ann });
    await waitFor(() => expect(requests.some((r) => r.path === "/api/items/40/request")).toBe(true));
    expect(container).toBeEmptyDOMElement();
    unmount();

    mockApi({ "GET /api/requests/summary": () => makeSummary(), "GET /api/items/40/request": () => apiError(404, "not_found", "no TMDB match") });
    const other = renderWithProviders(<ItemRequestLine item={dark} />, { user: ann });
    await waitFor(() => expect(other.client.getQueryState(["requests", "item", 40])?.status).toBe("error"));
    expect(other.container).toBeEmptyDOMElement();
  });

  it("never asks for an account without requests, and offers no seasons to one that may not ask", async () => {
    const requests = mockApi({ "GET /api/requests/summary": () => makeSummary({ access: "off" }), "GET /api/items/40/request": () => standing() });
    const { client, unmount } = renderWithProviders(<ItemRequestLine item={dark} />, { user: ann });
    await waitFor(() => expect(client.getQueryState(["requests", "summary"])?.status).toBe("success"));
    expect(requests.some((r) => r.path === "/api/items/40/request")).toBe(false);
    unmount();

    // It asked before: its request shows, but nothing more may be asked for.
    const request = makeRequest({ id: 5, kind: "show", tmdb_id: 70523, title: "Dark", mine: true });
    mockApi({ "GET /api/requests/summary": () => makeSummary({ access: "off", own: 1 }), "GET /api/items/40/request": () => standing({ request }) });
    renderWithProviders(<ItemRequestLine item={dark} />, { user: ann });
    expect(await screen.findByText("Waiting for approval")).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Request more seasons" })).not.toBeInTheDocument();
  });
});
