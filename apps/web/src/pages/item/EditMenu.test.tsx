import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { ItemDetail } from "@/api/types";
import { makeItem } from "@/test/fixtures";
import { itemUrl } from "@/lib/urls";
import { LocationDisplay, mockApi, renderWithProviders } from "@/test/render";
import { EditDialogs, editActions } from "./EditMenu";

function movie(overrides: Partial<ItemDetail> = {}): ItemDetail {
  return {
    ...makeItem({ id: 5, kind: "movie", title: "Alien: Director's Cut", year: 1979 }),
    original_title: null,
    tagline: null,
    genres: [],
    studio: null,
    content_rating: null,
    tmdb_id: 348,
    imdb_id: null,
    files: [],
    in_watchlist: false,
    ...overrides,
  };
}

describe("EditDialogs", () => {
  it("says which fields come from the NFO file, and an edit's lock wins", () => {
    renderWithProviders(
      <EditDialogs item={movie({ nfo_fields: ["summary", "title"], locked_fields: ["title"] })} dialog="details" onClose={() => {}} />,
    );
    expect(screen.getByText(/Fields from the NFO file next to it win over the metadata source/)).toBeInTheDocument();
    // The title is edited: its lock shows; the summary is the NFO file's.
    expect(screen.getAllByText("From the NFO file")).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Locked: click to unlock" })).toBeInTheDocument();
  });

  it("replaces the labels, suggesting those titles carry", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/labels": () => [{ label: "Kometa", items: 12, rules: 0 }],
      "PATCH /api/items/5": () => movie({ labels: ["Override: Kids"] }),
    });
    renderWithProviders(<EditDialogs item={movie({ labels: ["Kometa", "4K"] })} dialog="details" onClose={() => {}} />);
    await user.click(screen.getByRole("button", { name: "Remove 4K" }));
    await user.type(screen.getByLabelText("Labels"), "Override: Kids{Enter}");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(requests.find((r) => r.method === "PATCH")?.body).toEqual({ labels: ["Kometa", "Override: Kids"] }),
    );
  });

  it("suggests labels profiles' exceptions name before any title carries them", async () => {
    const user = userEvent.setup();
    mockApi({ "GET /api/labels": () => [{ label: "Kids OK", items: 0, rules: 2 }] });
    renderWithProviders(<EditDialogs item={movie({ labels: [] })} dialog="details" onClose={() => {}} />);
    await user.type(screen.getByLabelText("Labels"), "kid");
    const option = await screen.findByRole("option", { name: /Kids OK/ });
    expect(option).toHaveTextContent("In exceptions");
  });

  it("says which country's ratings a rating is read with", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <EditDialogs
        item={movie({ content_rating: "A", content_rating_country: "IN", nfo_fields: ["content_rating"] })}
        dialog="details"
        onClose={() => {}}
      />,
    );
    expect(screen.getByText("Country: India")).toBeInTheDocument();
    // Typed by hand, it has no country.
    await user.clear(screen.getByLabelText("Content rating"));
    await user.type(screen.getByLabelText("Content rating"), "PG");
    expect(screen.queryByText("Country: India")).not.toBeInTheDocument();
    expect(screen.getByText("Read with the ratings of the server’s country, set in Settings › Server")).toBeInTheDocument();
  });

  it("reads a rating as the server does: its written country's, else the region's or common ones", async () => {
    const user = userEvent.setup();
    const country = (code: string) => ({ country: code, name: code, ratings: [] });
    mockApi({ "GET /api/ratings/systems": () => ({ region: "PL", systems: [country("GB"), country("US")] }) });
    renderWithProviders(
      <EditDialogs item={movie({ content_rating: "B", content_rating_country: "IN" })} dialog="details" onClose={() => {}} />,
    );
    const field = screen.getByLabelText("Content rating");
    // Written with its country, it's that country's, whatever the item's.
    await user.clear(field);
    await user.type(field, "gb:15");
    expect(await screen.findByText("Country: United Kingdom")).toBeInTheDocument();
    await user.clear(field);
    await user.type(field, "USA/R");
    expect(screen.getByText("Country: United States")).toBeInTheDocument();
    // Poland has no ratings here: the US's and common names are read.
    await user.clear(field);
    await user.type(field, "PG");
    expect(
      screen.getByText("Read by the US’s ratings and others in common use: the server’s country has none here"),
    ).toBeInTheDocument();
    // "M/12" is a rating, Portugal's, not a country's.
    await user.clear(field);
    await user.type(field, "M/12");
    expect(screen.queryByText(/^Country:/)).not.toBeInTheDocument();
  });

  it("has no labels field where the server gave none", () => {
    renderWithProviders(<EditDialogs item={movie()} dialog="details" onClose={() => {}} />);
    expect(screen.queryByLabelText("Labels")).not.toBeInTheDocument();
  });

  it("says nothing of NFO files for an item without one", () => {
    renderWithProviders(<EditDialogs item={movie()} dialog="details" onClose={() => {}} />);
    expect(screen.queryByText("From the NFO file")).not.toBeInTheDocument();
    expect(screen.queryByText(/NFO file/)).not.toBeInTheDocument();
  });

  it("undoes Fix Match, for the NFO file to decide the match again", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/items/5/matches": () => [],
      "PATCH /api/items/5": () => movie({ locked_fields: [], tmdb_id: 8077, title: "Alien³" }),
    });
    renderWithProviders(<EditDialogs item={movie({ locked_fields: ["match"] })} dialog="match" onClose={() => {}} />);
    expect(screen.getByText(/Matched here by hand/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Undo Fix Match" }));
    await waitFor(() => expect(requests.find((r) => r.method === "PATCH")?.body).toEqual({ match: null }));
    expect(await screen.findByText("Fix Match undone")).toBeInTheDocument();
  });

  it("goes to the movie a copy went into when Fix Match matched it to that one's title", async () => {
    const user = userEvent.setup();
    mockApi({
      "GET /api/items/5/matches": () => [
        { tmdb_id: 679, title: "Aliens", original_title: null, year: 1986, overview: null, poster_url: null },
      ],
      "POST /api/items/5/match": () => movie({ id: 9, tmdb_id: 679, title: "Aliens" }),
    });
    renderWithProviders(
      <>
        <EditDialogs item={movie()} dialog="match" onClose={() => {}} />
        <LocationDisplay />
      </>,
      { route: itemUrl(5) },
    );

    await user.click(await screen.findByRole("button", { name: /Aliens/ }));
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent(itemUrl(9)));
  });

  it("offers no undo for a match nobody fixed", () => {
    mockApi({ "GET /api/items/5/matches": () => [] });
    renderWithProviders(<EditDialogs item={movie()} dialog="match" onClose={() => {}} />);
    expect(screen.queryByRole("button", { name: "Undo Fix Match" })).not.toBeInTheDocument();
  });
});

function audiobook(overrides: Partial<ItemDetail> = {}): ItemDetail {
  return movie({
    ...makeItem({ id: 7, kind: "audiobook", title: "Small Hours", year: 1987 }),
    tmdb_id: null,
    book: { authors: ["Vale, Imogen"], narrators: [], series: null, series_index: null, openlibrary_id: "OL3W" },
    ...overrides,
  });
}

describe("editing an audiobook", () => {
  it("offers its details, its match and its cover, not a background", () => {
    expect(editActions(audiobook()).map((a) => [a.dialog, a.label])).toEqual([
      ["details", "Edit details…"],
      ["match", "Fix match…"],
      ["poster", "Choose cover…"],
    ]);
  });

  it("edits who wrote and reads a book and its series, a name to a line", async () => {
    const user = userEvent.setup();
    const requests = mockApi({ "PATCH /api/items/7": () => audiobook() });
    renderWithProviders(<EditDialogs item={audiobook()} dialog="details" onClose={() => {}} />);
    // A film's fields aren't a book's.
    expect(screen.queryByLabelText("Studio")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Content rating")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Authors")).toHaveValue("Vale, Imogen");

    await user.type(screen.getByLabelText("Narrators"), "June Ashby{Enter}Ada Quill");
    await user.type(screen.getByLabelText("Series"), "Night Porters");
    await user.type(screen.getByLabelText("Book in series"), "2.5");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(requests.find((r) => r.method === "PATCH")?.body).toEqual({
        narrators: ["June Ashby", "Ada Quill"],
        series: "Night Porters",
        series_index: 2.5,
      }),
    );
  });

  it("matches a book to an Open Library work or a recording, marking the one it has", async () => {
    const user = userEvent.setup();
    const work = { asin: null, narrators: [], year: 1987, poster_url: null };
    const requests = mockApi({
      "GET /api/items/7/matches": () => [
        { ...work, openlibrary_id: "OL3W", title: "Small Hours", authors: ["Imogen Vale"] },
        { ...work, openlibrary_id: "OL9W", title: "Small Hours: A Porter's Year", authors: ["Imogen Vale"] },
        {
          openlibrary_id: null,
          asin: "B0HOURS001",
          title: "Small Hours (Unabridged)",
          authors: ["Imogen Vale"],
          narrators: ["June Ashby"],
          year: 2019,
          poster_url: null,
        },
      ],
      "POST /api/items/7/match": () => audiobook({ locked_fields: ["match"] }),
    });
    renderWithProviders(<EditDialogs item={audiobook()} dialog="match" onClose={() => {}} />);

    // Searched with the book's own writer.
    await screen.findByText("Open Library · OL9W");
    expect(requests.find((r) => r.path.startsWith("/api/items/7/matches"))?.path).toContain("author=Vale%2C+Imogen");
    expect(screen.getByRole("button", { name: /OL3W/ })).toHaveTextContent("Current");
    expect(screen.getByText("Read by June Ashby")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /B0HOURS001/ }));
    await waitFor(() =>
      expect(requests.find((r) => r.method === "POST")?.body).toEqual({ asin: "B0HOURS001" }),
    );
  });

  it("says a book's match was chosen by hand, and undoes it", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/items/7/matches": () => [],
      "PATCH /api/items/7": () => audiobook(),
    });
    renderWithProviders(<EditDialogs item={audiobook({ locked_fields: ["match"] })} dialog="match" onClose={() => {}} />);
    expect(screen.getByText(/the book’s files don’t change this match/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Undo Fix Match" }));
    await waitFor(() => expect(requests.find((r) => r.method === "PATCH")?.body).toEqual({ match: null }));
  });
});
