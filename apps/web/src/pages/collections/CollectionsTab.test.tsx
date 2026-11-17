import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { Category, Collection } from "@/api/lists";
import { mockApi, renderWithProviders, testUser } from "@/test/render";
import { CollectionsTab, groupByCategory, isOfType } from "./CollectionsTab";

const highlights: Category = { id: 1, key: "highlights", title: "Highlights", from_label: null, position: 0 };
const franchises: Category = { id: 2, key: "franchises", title: "Franchises", from_label: null, position: 1 };
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
  { ...collection(1, "Alien Collection"), automatic: true, category: ref(franchises) },
  { ...collection(2, "Recently Added"), smart_filter: "sort=addedAt:desc", builtin: "recently_added", category: ref(highlights) },
  { ...collection(3, "Christmas"), smart_filter: "label=Christmas", category: ref(seasonal) },
  collection(4, "Favourites"),
];

describe("groupByCategory", () => {
  it("groups in the categories' order, Other where it is, empty ones left out", () => {
    const groups = groupByCategory(collections, [highlights, other, franchises, seasonal, { ...seasonal, id: 9, title: "Empty" }]);
    expect(groups.map((g) => [g.category.title, g.collections.map((c) => c.id)])).toEqual([
      ["Highlights", [2]],
      ["Other", [4]],
      ["Franchises", [1]],
      ["Seasonal", [3]],
    ]);
  });

  it("puts one of a category it doesn't know yet after the rest", () => {
    expect(groupByCategory(collections, [other, highlights]).map((g) => g.category.title)).toEqual([
      "Other",
      "Highlights",
      "Franchises",
      "Seasonal",
    ]);
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
      "GET /api/collection-categories": () => ({ categories: [highlights, franchises, seasonal, other] }),
    });
    renderWithProviders(<CollectionsTab libraryId={1} kind="movies" />, { user: { ...testUser, is_admin: isAdmin } });
  }

  it("lists collections under their categories and filters them", async () => {
    const user = userEvent.setup();
    renderTab(true);
    const sections = await screen.findAllByRole("region");
    expect(sections.map((s) => s.getAttribute("aria-label"))).toEqual(["Highlights", "Franchises", "Seasonal", "Other"]);
    expect(within(sections[2]!).getAllByText("Christmas").length).toBeGreaterThan(0);

    await user.selectOptions(screen.getByLabelText("Type"), "smart");
    expect(screen.getAllByRole("region").map((s) => s.getAttribute("aria-label"))).toEqual(["Highlights", "Seasonal"]);

    await user.selectOptions(screen.getByLabelText("Category"), String(seasonal.id));
    expect(screen.getAllByRole("region").map((s) => s.getAttribute("aria-label"))).toEqual(["Seasonal"]);
    expect(screen.getByText("1 collection")).toBeInTheDocument();

    await user.selectOptions(screen.getByLabelText("Type"), "tmdb");
    expect(screen.getByText("No collections match these filters")).toBeInTheDocument();
    await user.click(screen.getAllByRole("button", { name: "Clear filters" })[0]!);
    expect(screen.getAllByRole("region")).toHaveLength(4);
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
