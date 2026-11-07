import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { ItemDetail } from "@/api/types";
import { makeItem } from "@/test/fixtures";
import { mockApi, renderWithProviders } from "@/test/render";
import { EditDialogs } from "./EditMenu";

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
    expect(screen.getByText("Read with the metadata region’s ratings")).toBeInTheDocument();
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

  it("offers no undo for a match nobody fixed", () => {
    mockApi({ "GET /api/items/5/matches": () => [] });
    renderWithProviders(<EditDialogs item={movie()} dialog="match" onClose={() => {}} />);
    expect(screen.queryByRole("button", { name: "Undo Fix Match" })).not.toBeInTheDocument();
  });
});
