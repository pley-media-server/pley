import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { namedCategory, type Category, type Collection } from "@/api/lists";
import { encodeId } from "@/lib/ids";
import { LocationDisplay, mockApi, renderWithProviders, testUser } from "@/test/render";
import { CollectionsTab, groupByCategory, isOfType } from "./CollectionsTab";

const highlights: Category = { id: 1, key: "highlights", title: "Highlights", from_label: null, position: 0 };
const boxSets: Category = { id: 2, key: "box_sets", title: "Box sets", from_label: null, position: 1 };
const seasonal: Category = { id: 3, key: null, title: "Seasonal", from_label: "Seasonal", position: 2 };
const other: Category = { id: 4, key: "other", title: "Other", from_label: null, position: 3 };
const ref = ({ id, key, title }: Category) => ({ id, key, title });

function collection(id: number, title: string): Collection {
  return {
    id,
    library_id: 1,
    title,
    summary: null,
    automatic: false,
    sort: "release",
    smart_filter: null,
    category: ref(other),
    builtin: null,
    home_position: null,
    recommended_position: null,
    item_count: 3,
    poster_url: "",
    backdrop_url: "",
    created_at: 0,
    updated_at: 0,
  };
}

const collections: Collection[] = [
  { ...collection(1, "Alien Collection"), automatic: true, category: ref(boxSets) },
  { ...collection(2, "Recently Added"), smart_filter: "sort=addedAt:desc", builtin: "recently_added", category: ref(highlights) },
  { ...collection(3, "Christmas"), smart_filter: "label=Christmas", category: ref(seasonal) },
  collection(4, "Favorites"),
];

describe("groupByCategory", () => {
  it("groups in the categories' order, Other where it is, empty ones left out", () => {
    const groups = groupByCategory(collections, [highlights, other, boxSets, seasonal, { ...seasonal, id: 9, title: "Empty" }]);
    expect(groups.map((g) => [g.category.title, g.collections.map((c) => c.id)])).toEqual([
      ["Highlights", [2]],
      ["Other", [4]],
      ["Box sets", [1]],
      ["Seasonal", [3]],
    ]);
  });

  it("puts one of a category it doesn't know yet after the rest", () => {
    expect(groupByCategory(collections, [other, highlights]).map((g) => g.category.title)).toEqual([
      "Other",
      "Highlights",
      "Box sets",
      "Seasonal",
    ]);
  });
});

describe("namedCategory", () => {
  it("names a collection's category, but not Other, which only says it has none", () => {
    expect(namedCategory(collections[2]!)?.title).toBe("Seasonal");
    expect(namedCategory(collections[3]!)).toBeNull();
  });
});

describe("isOfType", () => {
  it("tells collections made by hand, smart ones and TMDB's apart", () => {
    const of = (type: Parameters<typeof isOfType>[1]) => collections.filter((c) => isOfType(c, type)).map((c) => c.id);
    expect(of("all")).toEqual([1, 2, 3, 4]);
    expect(of("manual")).toEqual([4]);
    expect(of("smart")).toEqual([2, 3]);
    expect(of("tmdb")).toEqual([1]);
  });
});

describe("CollectionsTab", () => {
  function renderTab(isAdmin = false) {
    mockApi({
      "GET /api/libraries/1/collections": () => ({ collections }),
      "GET /api/collection-categories": () => ({ categories: [highlights, boxSets, seasonal, other] }),
    });
    renderWithProviders(<CollectionsTab libraryId={1} kind="movies" />, { user: { ...testUser, is_admin: isAdmin } });
  }

  it("lists collections under their categories and filters them", async () => {
    const user = userEvent.setup();
    renderTab(true);
    const sections = await screen.findAllByRole("region");
    expect(sections.map((s) => s.getAttribute("aria-label"))).toEqual(["Highlights", "Box sets", "Seasonal", "Other"]);
    expect(within(sections[2]!).getAllByText("Christmas").length).toBeGreaterThan(0);

    await user.selectOptions(screen.getByLabelText("Type"), "smart");
    expect(screen.getAllByRole("region").map((s) => s.getAttribute("aria-label"))).toEqual(["Highlights", "Seasonal"]);

    // One group shown has no heading: the filter says which it is.
    await user.selectOptions(screen.getByLabelText("Category"), String(seasonal.id));
    expect(screen.queryAllByRole("region")).toHaveLength(0);
    expect(screen.queryByRole("heading", { name: /Seasonal/ })).not.toBeInTheDocument();
    expect(screen.getAllByText("Christmas").length).toBeGreaterThan(0);
    expect(screen.getByText("1 collection")).toBeInTheDocument();

    await user.selectOptions(screen.getByLabelText("Type"), "tmdb");
    expect(screen.getByText("No collections match these filters")).toBeInTheDocument();
    await user.click(screen.getAllByRole("button", { name: "Clear filters" })[0]!);
    expect(screen.getAllByRole("region")).toHaveLength(4);
  });

  it("keeps the filters in the address, for Back to find them", async () => {
    const user = userEvent.setup();
    mockApi({
      "GET /api/libraries/1/collections": () => ({ collections }),
      "GET /api/collection-categories": () => ({ categories: [highlights, boxSets, seasonal] }),
    });
    renderWithProviders(
      <>
        <CollectionsTab libraryId={1} kind="movies" />
        <LocationDisplay />
      </>,
      { route: `/library/1?tab=collections&category=${encodeId(seasonal.id)}` },
    );
    expect(await screen.findByText("1 collection")).toBeInTheDocument();
    expect(screen.getByLabelText("Category")).toHaveValue(String(seasonal.id));

    await user.selectOptions(screen.getByLabelText("Type"), "smart");
    expect(screen.getByTestId("location")).toHaveTextContent(`/library/1?tab=collections&category=${encodeId(seasonal.id)}&type=smart`);
    await user.click(screen.getAllByRole("button", { name: "Clear filters" })[0]!);
    expect(screen.getByTestId("location")).toHaveTextContent(/^\/library\/1\?tab=collections$/);
  });

  it("lets a chosen category go once it's gone, showing them all", async () => {
    mockApi({
      "GET /api/libraries/1/collections": () => ({ collections }),
      "GET /api/collection-categories": () => ({ categories: [highlights, boxSets, seasonal] }),
    });
    renderWithProviders(
      <>
        <CollectionsTab libraryId={1} kind="movies" />
        <LocationDisplay />
      </>,
      { route: `/library/1?tab=collections&category=${encodeId(9)}` },
    );
    expect(await screen.findAllByRole("region")).toHaveLength(4);
    expect(screen.getByLabelText("Category")).toHaveValue("all");
    expect(screen.queryByRole("button", { name: "Clear filters" })).not.toBeInTheDocument();
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent(/^\/library\/1\?tab=collections$/));
  });

  it("heads no group when there is only one", async () => {
    mockApi({
      "GET /api/libraries/1/collections": () => ({ collections: [collection(4, "Favorites"), collection(5, "Westerns")] }),
      "GET /api/collection-categories": () => ({ categories: [highlights, boxSets, seasonal] }),
    });
    renderWithProviders(<CollectionsTab libraryId={1} kind="movies" />);
    expect(await screen.findAllByText("Westerns")).not.toHaveLength(0);
    expect(screen.queryByRole("heading", { name: /Other/ })).not.toBeInTheDocument();
    expect(screen.queryAllByRole("region")).toHaveLength(0);
    // Nothing to choose between: no Category filter either.
    expect(screen.queryByLabelText("Category")).not.toBeInTheDocument();
  });

  it("waits for the categories before grouping", async () => {
    let answer: (value: unknown) => void = () => {};
    const categories = new Promise((resolve) => (answer = resolve));
    const requests = mockApi({
      "GET /api/libraries/1/collections": () => ({ collections }),
      "GET /api/collection-categories": () => categories,
    });
    renderWithProviders(<CollectionsTab libraryId={1} kind="movies" />);
    await waitFor(() => expect(requests.some((r) => r.path === "/api/libraries/1/collections")).toBe(true));
    await screen.findByText("4 collections");
    // Not everything under Other while they're on their way.
    expect(screen.queryByText("Favorites")).not.toBeInTheDocument();

    answer({ categories: [highlights, boxSets, seasonal] });
    const sections = await screen.findAllByRole("region");
    expect(sections.map((s) => s.getAttribute("aria-label"))).toEqual(["Highlights", "Box sets", "Seasonal", "Other"]);
  });

  it("offers administrators the categories", async () => {
    renderTab(true);
    expect(await screen.findByRole("button", { name: "Categories" })).toBeInTheDocument();
  });

  it("offers others the categories to filter by, not how collections were made", async () => {
    renderTab();
    await screen.findAllByRole("region");
    expect(screen.queryByLabelText("Type")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Category")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Categories" })).not.toBeInTheDocument();
  });
});
