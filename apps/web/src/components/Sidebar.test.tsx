import { useState } from "react";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { liveKeys } from "@/api/livetv";
import type { Library } from "@/api/types";
import { mockApi, renderWithProviders } from "@/test/render";
import { makeSummary } from "@/test/requests";
import { Sidebar } from "./Sidebar";

function Harness() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        Open menu
      </button>
      <Sidebar open={open} onClose={() => setOpen(false)} />
    </>
  );
}

describe("Sidebar drawer", () => {
  it("takes the keyboard while open and gives it back to its button when closed", async () => {
    mockApi({
      "GET /api/libraries": () => [],
    });
    const user = userEvent.setup();
    renderWithProviders(<Harness />);

    const opener = screen.getByRole("button", { name: "Open menu" });
    await user.click(opener);
    const drawer = screen.getByRole("dialog", { name: "Menu" });
    expect(drawer).toContainElement(document.activeElement as HTMLElement);
    expect(document.body.style.overflow).toBe("hidden");

    // Shift+Tab from the first control wraps to the drawer's last one, not the page behind.
    await user.tab({ shift: true });
    expect(drawer).toContainElement(document.activeElement as HTMLElement);
    expect(within(drawer).getAllByRole("link").at(-1)).toHaveFocus();

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Menu" })).not.toBeInTheDocument();
    expect(opener).toHaveFocus();
    expect(document.body.style.overflow).toBe("");
  });
});

describe("Sidebar requests", () => {
  const serve = (summary: ReturnType<typeof makeSummary>) =>
    mockApi({
      "GET /api/libraries": () => [],
      "GET /api/requests/summary": () => summary,
    });

  it("shows managers Discover and Requests, with what waits for them and their own news", async () => {
    serve(makeSummary({ access: "auto", manage: true, pending: 3, unseen: 1 }));
    renderWithProviders(<Sidebar open={false} onClose={() => {}} />);
    expect(await screen.findByRole("link", { name: "Discover" })).toHaveAttribute("href", "/discover");
    const requests = screen.getByRole("link", { name: "Requests 4 waiting" });
    expect(requests).toHaveAttribute("href", "/requests");
    expect(within(requests).getByText("4")).toHaveClass("bg-accent");
  });

  it("says so in the danger color while some failed", async () => {
    serve(makeSummary({ access: "auto", manage: true, pending: 1, failed: 1 }));
    renderWithProviders(<Sidebar open={false} onClose={() => {}} />);
    expect(within(await screen.findByRole("link", { name: "Requests 2 waiting" })).getByText("2")).toHaveClass("bg-danger");
  });

  it("counts others' news only", async () => {
    serve(makeSummary({ unseen: 2, pending: 0 }));
    renderWithProviders(<Sidebar open={false} onClose={() => {}} />);
    expect(await screen.findByRole("link", { name: "Your requests 2 waiting" })).toHaveAttribute("href", "/requests");
  });

  it("leaves them out for an account that may not ask, unless it asked before", async () => {
    serve(makeSummary({ access: "off" }));
    const { client, unmount } = renderWithProviders(<Sidebar open={false} onClose={() => {}} />);
    await waitFor(() => expect(client.getQueryState(["requests", "summary"])?.status).toBe("success"));
    expect(screen.queryByRole("link", { name: "Discover" })).not.toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /requests/i })).not.toBeInTheDocument();
    unmount();

    serve(makeSummary({ access: "off", own: 2 }));
    renderWithProviders(<Sidebar open={false} onClose={() => {}} />);
    expect(await screen.findByRole("link", { name: "Your requests" })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Discover" })).not.toBeInTheDocument();
  });
});

describe("Sidebar Live TV", () => {
  it("shows Live TV once it has channels, not to an administrator before", async () => {
    let channels = 0;
    mockApi({
      "GET /api/libraries": () => [],
      "GET /api/live": () => ({ available: channels > 0, allowed: true, channels, sources: channels ? 1 : 0, recent: [] }),
    });
    const { client } = renderWithProviders(<Sidebar open={false} onClose={() => {}} />);

    await waitFor(() => expect(client.getQueryState(liveKeys.summary)?.status).toBe("success"));
    expect(screen.queryByRole("link", { name: "Live TV" })).not.toBeInTheDocument();

    // A source added: the server says live_channels_changed, and the summary is asked for again.
    channels = 5;
    await client.invalidateQueries({ queryKey: liveKeys.all });
    expect(await screen.findByRole("link", { name: "Live TV" })).toHaveAttribute("href", "/live");
  });
});

describe("Sidebar playlists", () => {
  const movies: Library = { id: 1, name: "Movies", kind: "movies", paths: [], item_count: 0, created_at: 0, scanned_at: null, scanning: false };

  it("are one more row under the libraries, with or without any playlists", async () => {
    mockApi({
      "GET /api/libraries": () => [movies],
    });
    renderWithProviders(<Sidebar open={false} onClose={() => {}} />);
    const row = await screen.findByRole("link", { name: "Playlists" });
    expect(row).toHaveAttribute("href", "/playlists");
    const links = screen.getAllByRole("link").map((a) => a.textContent);
    expect(links.indexOf("Playlists")).toBe(links.indexOf("Movies") + 1);
  });

  it("are left out while there are no libraries", async () => {
    mockApi({
      "GET /api/libraries": () => [],
    });
    const { client } = renderWithProviders(<Sidebar open={false} onClose={() => {}} />);
    await waitFor(() => expect(client.getQueryState(["libraries"])?.status).toBe("success"));
    expect(screen.queryByRole("link", { name: "Playlists" })).not.toBeInTheDocument();
  });
});

describe("Sidebar narrowed", () => {
  const shows: Library = { id: 2, name: "TV Shows", kind: "shows", paths: [], item_count: 0, created_at: 0, scanned_at: null, scanning: false };

  function Narrowing() {
    const [collapsed, setCollapsed] = useState(false);
    return <Sidebar open={false} onClose={() => {}} collapsed={collapsed} onToggle={() => setCollapsed((c) => !c)} />;
  }

  /** The tip with these words: hidden from screen readers, which have the names already. */
  const tip = (text: string) => screen.findByText(text, { selector: '[role="tooltip"]' });

  it("narrows to its icons, each still named, and widens again", async () => {
    mockApi({ "GET /api/libraries": () => [shows] });
    const user = userEvent.setup();
    renderWithProviders(<Narrowing />);
    const nav = screen.getByRole("navigation", { name: "Main" });
    expect(await within(nav).findByRole("link", { name: "TV Shows" })).toBeInTheDocument();
    expect(within(nav).getByText("Libraries")).toBeInTheDocument();

    // An icon, named for screen readers and in its tip, not written.
    const collapse = within(nav).getByRole("button", { name: "Collapse sidebar" });
    expect(collapse).not.toHaveAttribute("title");
    expect(within(nav).queryByText("Collapse sidebar")).not.toBeInTheDocument();
    await user.hover(collapse);
    expect(await tip("Collapse sidebar")).toBeInTheDocument();

    await user.click(collapse);
    // Still read out by name, and shown in a tip on hover.
    const library = within(nav).getByRole("link", { name: "TV Shows" });
    expect(library).not.toHaveAttribute("title");
    await user.hover(library);
    expect(await tip("TV Shows")).toBeInTheDocument();
    await user.hover(within(nav).getByRole("link", { name: "Home" }));
    expect(await tip("Home")).toBeInTheDocument();
    // The heading is a line now, still read out.
    expect(within(nav).getByText("Libraries")).toHaveClass("sr-only");
    const expand = within(nav).getByRole("button", { name: "Expand sidebar" });
    expect(expand).toHaveAttribute("aria-expanded", "false");

    await user.click(expand);
    expect(within(nav).getByRole("button", { name: "Collapse sidebar" })).toHaveAttribute("aria-expanded", "true");
  });

  it("keeps the requests' count on a narrow sidebar, named in its tip", async () => {
    mockApi({ "GET /api/libraries": () => [], "GET /api/requests/summary": () => makeSummary({ access: "auto", manage: true, pending: 3, unseen: 1 }) });
    const user = userEvent.setup();
    renderWithProviders(<Sidebar open={false} onClose={() => {}} collapsed onToggle={() => {}} />);
    await user.hover(await screen.findByRole("link", { name: "Requests 4 waiting" }));
    expect(await tip("Requests")).toBeInTheDocument();
  });

  it("names nothing in tips while wide, where the names are written, but Sort libraries, after a longer wait", async () => {
    const films: Library = { ...shows, id: 1, name: "Movies", kind: "movies" };
    mockApi({ "GET /api/libraries": () => [films, shows] });
    const user = userEvent.setup();
    renderWithProviders(<Sidebar open={false} onClose={() => {}} collapsed={false} onToggle={() => {}} />);
    await user.hover(await screen.findByRole("link", { name: "TV Shows" }));
    await new Promise((r) => setTimeout(r, 500));
    expect(screen.queryByRole("tooltip", { hidden: true })).toBeNull();

    const sort = screen.getByRole("button", { name: "Sort libraries" });
    expect(sort).not.toHaveAttribute("title");
    await user.hover(sort);
    await new Promise((r) => setTimeout(r, 300));
    expect(screen.queryByRole("tooltip", { hidden: true })).toBeNull();
    expect(await tip("Sort libraries")).toBeInTheDocument();
  });
});
