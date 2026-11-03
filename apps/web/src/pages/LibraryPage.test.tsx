import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route, Routes } from "react-router";
import { afterEach, describe, expect, it } from "vitest";
import type { Library } from "@/api/types";
import { leftTab, leftView, tabStorageKey, viewStorageKey } from "@/lib/libraryView";
import { libraryUrl } from "@/lib/urls";
import { makeItem } from "@/test/fixtures";
import { LocationDisplay, mockApi, renderWithProviders, testUser } from "@/test/render";
import { LibraryPage } from "./LibraryPage";

const MOVIES: Library = {
  id: 3,
  name: "Movies",
  kind: "movies",
  paths: [],
  item_count: 342,
  created_at: 1_700_000_000,
  scanned_at: null,
  scanning: false,
};

const KEY = viewStorageKey(testUser.id, MOVIES.id);
const TAB_KEY = tabStorageKey(testUser.id, MOVIES.id);

function open(query = "") {
  const requests = mockApi({
    "GET /api/libraries": () => [MOVIES],
    "GET /api/libraries/3/items": (req) =>
      req.path.includes("watched=false")
        ? { total: 23, items: [makeItem({ id: 7, title: "Heat" })] }
        : { total: 342, items: [makeItem({ id: 7, title: "Heat" }), makeItem({ id: 8, title: "Alien" })] },
    "GET /api/libraries/3/filters": () => ({
      genres: ["Drama"],
      years: [1995],
      resolutions: ["4k", "1080"],
      dynamic_ranges: ["hdr", "dolby_vision", "hdr10", "sdr"],
    }),
    "GET /api/libraries/3/hubs": () => ({ hubs: [] }),
  });
  renderWithProviders(
    <>
      <Routes>
        <Route path="/library/:id" element={<LibraryPage />} />
      </Routes>
      <LocationDisplay />
    </>,
    { route: libraryUrl(MOVIES.id, query) },
  );
  /** The listings asked for so far. */
  return () => requests.filter((r) => r.path.startsWith("/api/libraries/3/items"));
}

describe("LibraryPage", () => {
  afterEach(() => window.localStorage.clear());

  it("opens a library as it was left, listing it once", async () => {
    window.localStorage.setItem(KEY, JSON.stringify("sort=added&watched=false"));
    const listings = open();

    expect(await screen.findByText("23 of 342 movies")).toBeInTheDocument();
    expect(screen.getByTestId("location")).toHaveTextContent(`${libraryUrl(MOVIES.id)}?sort=added&watched=false`);
    expect(listings()).toHaveLength(1);
    expect(listings()[0]?.path).toContain("sort=added");
    expect(listings()[0]?.path).toContain("watched=false");
  });

  it("keeps what is chosen for the next time, and an address's own view first", async () => {
    window.localStorage.setItem(KEY, JSON.stringify("sort=year"));
    const user = userEvent.setup();
    open("sort=title&order=desc");
    expect(await screen.findByText("342 movies")).toBeInTheDocument();
    expect(screen.getByTestId("location")).toHaveTextContent("?sort=title&order=desc");

    await user.selectOptions(screen.getByRole("combobox", { name: "Watched filter" }), "unwatched");
    expect(leftView(KEY)).toBe("sort=title&order=desc&watched=false");
    await waitFor(() => expect(screen.getByText("23 of 342 movies")).toBeInTheDocument());

    await user.click(screen.getByRole("button", { name: "Clear filters" }));
    expect(leftView(KEY)).toBe("sort=title&order=desc");
  });

  it("filters by resolution and dynamic range, as the library has them", async () => {
    const user = userEvent.setup();
    const listings = open();
    expect(await screen.findByText("342 movies")).toBeInTheDocument();

    const resolution = await screen.findByRole("combobox", { name: "Resolution" });
    const options = (select: HTMLElement) => Array.from(select.querySelectorAll("option")).map((o) => o.textContent);
    expect(options(resolution)).toEqual(["All resolutions", "4K", "1080p"]);
    const range = screen.getByRole("combobox", { name: "Dynamic range" });
    expect(options(range)).toEqual(["All dynamic ranges", "Any HDR", "Dolby Vision", "HDR", "SDR"]);

    await user.selectOptions(resolution, "4k");
    await user.selectOptions(range, "dolby_vision");
    expect(leftView(KEY)).toBe("resolution=4k&dynamic_range=dolby_vision");
    await waitFor(() => expect(listings().at(-1)?.path).toContain("resolution=4k&dynamic_range=dolby_vision"));
    expect(screen.getByRole("button", { name: "Save as…" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Clear filters" }));
    expect(leftView(KEY)).toBe("");
  });

  it("drops a resolution it doesn't know", async () => {
    const listings = open("resolution=8k&dynamic_range=sdr");
    expect(await screen.findByRole("combobox", { name: "Resolution" })).toHaveValue("");
    expect(screen.getByRole("combobox", { name: "Dynamic range" })).toHaveValue("sdr");
    expect(listings().at(-1)?.path).not.toContain("8k");
    expect(listings().at(-1)?.path).toContain("dynamic_range=sdr");
  });

  it("opens on the tab it was last on, and remembers the one chosen", async () => {
    window.localStorage.setItem(TAB_KEY, JSON.stringify("recommended"));
    const user = userEvent.setup();
    const listings = open();
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent("?tab=recommended"));
    expect(screen.getByRole("tab", { name: "recommended" })).toHaveAttribute("aria-selected", "true");
    expect(listings()).toHaveLength(0);

    await user.click(screen.getByRole("tab", { name: "library" }));
    expect(await screen.findByText("342 movies")).toBeInTheDocument();
    expect(leftTab(TAB_KEY)).toBe("library");
    expect(screen.getByTestId("location")).toHaveTextContent(new RegExp(`^${libraryUrl(MOVIES.id)}$`));

    await user.click(screen.getByRole("tab", { name: "collections" }));
    await waitFor(() => expect(leftTab(TAB_KEY)).toBe("collections"));
  });

  it("opens on Library when nothing usable was kept, or the address has a view", async () => {
    window.localStorage.setItem(TAB_KEY, JSON.stringify("upcoming"));
    open();
    expect(await screen.findByText("342 movies")).toBeInTheDocument();
    expect(leftTab(TAB_KEY)).toBe("library");
  });

  it("leaves an address with a sort on Library, whatever tab was last", async () => {
    window.localStorage.setItem(TAB_KEY, JSON.stringify("recommended"));
    open("sort=year");
    expect(await screen.findByText("342 movies")).toBeInTheDocument();
    expect(screen.getByTestId("location")).toHaveTextContent("?sort=year");
  });
});
