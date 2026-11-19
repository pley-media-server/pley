import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Collection, CollectionArtwork } from "@/api/lists";
import { mockApi, renderWithProviders } from "@/test/render";
import { CollectionPosterModal } from "./CollectionPosterModal";

function collection(artwork: CollectionArtwork, icon: string | null = null, color: string | null = null): Collection {
  return {
    id: 7,
    library_id: 1,
    title: "Favorites",
    summary: null,
    automatic: false,
    sort: "release",
    smart_filter: null,
    category: { id: 3, key: "other", title: "Other" },
    builtin: null,
    home_position: null,
    recommended_position: null,
    artwork,
    icon,
    icon_color: color,
    item_count: 4,
    poster_url: "/api/collections/7/images/poster?v=1",
    backdrop_url: "",
    created_at: 0,
    updated_at: 0,
  };
}

const detail = (c: Collection) => ({ ...c, items: [] });

// The collection's library, for the icon a color background starts with.
const libraries = { "GET /api/libraries": () => [{ id: 1, name: "Films", kind: "movies", paths: [], item_count: 4, created_at: 0, scanned_at: 0, scanning: false }] };

describe("CollectionPosterModal", () => {
  it("puts an icon in the color chosen over the posters", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const requests = mockApi({ ...libraries, "PATCH /api/collections/7": () => detail(collection("icon", "ghost", "teal")) });
    renderWithProviders(<CollectionPosterModal collection={detail(collection("automatic"))} onClose={onClose} />);

    expect(screen.getByRole("radio", { name: "Posters" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: "No icon" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    await user.click(screen.getByRole("radio", { name: "Ghost" }));
    await user.click(screen.getByRole("radio", { name: "Teal" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(requests.find((r) => r.method === "PATCH")?.body).toEqual({ icon: "ghost", icon_color: "teal", background: "posters" });
  });

  it("shows its library's icon on slate for no icon when it has no posters", async () => {
    const user = userEvent.setup();
    mockApi(libraries);
    const { container } = renderWithProviders(
      <CollectionPosterModal collection={detail(collection("icon", "star", "amber"))} onClose={vi.fn()} />,
    );
    const preview = () => container.ownerDocument.querySelector(".w-28");

    await waitFor(() => expect(preview()?.querySelector(".lucide-star")).not.toBeNull());
    await user.click(screen.getByRole("radio", { name: "No icon" }));
    // Its films have no posters: the server draws a films library's clapperboard.
    await waitFor(() => expect(preview()?.querySelector(".lucide-clapperboard")).not.toBeNull());
  });

  it("doesn't preview a TMDB collection's poster with the icon being taken off", async () => {
    const user = userEvent.setup();
    mockApi(libraries);
    const tmdb = { ...collection("icon", "star", "amber"), automatic: true };
    const { container } = renderWithProviders(<CollectionPosterModal collection={detail(tmdb)} onClose={vi.fn()} />);
    const shown = () => container.ownerDocument.querySelector(`.w-28 img[src*="/api/collections/7/images/poster"]`);

    await user.click(screen.getByRole("radio", { name: "No icon" }));
    // Its poster now has the icon on it; TMDB's isn't one the web app has.
    await waitFor(() => expect(container.ownerDocument.querySelector(".w-28 .lucide-star")).toBeNull());
    expect(shown()).toBeNull();
  });

  it("starts from the icon and color the collection has, and takes the icon off", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const requests = mockApi({ ...libraries, "DELETE /api/collections/7/artwork/poster": () => detail(collection("automatic")) });
    renderWithProviders(<CollectionPosterModal collection={detail(collection("icon", "star", "amber"))} onClose={onClose} />);

    expect(screen.getByRole("radio", { name: "Star" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: "Amber" })).toHaveAttribute("aria-checked", "true");

    await user.click(screen.getByRole("radio", { name: "No icon" }));
    expect(screen.queryByRole("radio", { name: "Amber" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/collections/7/artwork/poster")).toBe(true);
  });

  it("draws a color background with its library's icon to start with, and always an icon", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const requests = mockApi({ ...libraries, "PATCH /api/collections/7": () => detail(collection("color", "clapperboard", "blue")) });
    renderWithProviders(<CollectionPosterModal collection={detail(collection("automatic"))} onClose={onClose} />);
    await waitFor(() => expect(requests.some((r) => r.path === "/api/libraries")).toBe(true));

    await user.click(screen.getByRole("radio", { name: "Color" }));
    expect(screen.getByRole("radio", { name: "Clapperboard" })).toHaveAttribute("aria-checked", "true");
    // No icon stays in its place, turned off, so the icons don't move.
    expect(screen.getByRole("radio", { name: "No icon" })).toBeDisabled();
    expect(screen.getByRole("radio", { name: "No icon" })).toHaveAttribute("aria-checked", "false");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(requests.find((r) => r.method === "PATCH")?.body).toEqual({ icon: "clapperboard", icon_color: "blue", background: "color" });
  });

  it("uploads the picture picked", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const requests = mockApi({ ...libraries, "POST /api/collections/7/artwork/poster/upload": () => detail(collection("custom")) });
    const { container } = renderWithProviders(<CollectionPosterModal collection={detail(collection("automatic"))} onClose={onClose} />);

    await user.click(screen.getByRole("radio", { name: "Picture" }));
    expect(screen.getByRole("button", { name: "Upload a picture…" })).toBeEnabled();
    const input = container.ownerDocument.querySelector<HTMLInputElement>('input[type="file"]')!;
    await user.upload(input, new File(["png"], "poster.png", { type: "image/png" }));

    await waitFor(() => expect(onClose).toHaveBeenCalled());
    const sent = requests.find((r) => r.method === "POST");
    expect(sent?.path).toBe("/api/collections/7/artwork/poster/upload");
    expect(sent?.init.body).toBeInstanceOf(File);
  });
});
