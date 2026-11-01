import { useState } from "react";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { liveKeys } from "@/api/livetv";
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
      "GET /api/playlists": () => ({ playlists: [] }),
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
      "GET /api/playlists": () => ({ playlists: [] }),
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

  it("says so in the danger colour while some failed", async () => {
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
      "GET /api/playlists": () => ({ playlists: [] }),
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
