import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { Activity, SessionInfo } from "@/api/types";
import { makeItem } from "@/test/fixtures";
import { apiError, mockApi, renderWithProviders } from "@/test/render";
import { DashboardSettings } from "./DashboardSettings";

const playing: SessionInfo = {
  session_id: "abc",
  user_id: 2,
  username: "sam",
  item: makeItem({ id: 7, title: "Jaws", duration_ms: 6_000_000 }),
  method: "transcode",
  position_ms: 60_000,
  state: "playing",
  started_at: 1_700_000_000,
  updated_at: 1_700_000_000,
  transcode: null,
};

function serve(stop: () => unknown = () => undefined) {
  let activity: Activity = { scans: [], sessions: [playing] };
  return mockApi({
    "GET /api/activity": () => activity,
    "DELETE /api/sessions/abc": () => {
      const res = stop();
      if (!(res instanceof Response)) activity = { scans: [], sessions: [] };
      return res;
    },
  });
}

describe("DashboardSettings stopping playback", () => {
  it("stops a session, with a message for the viewer", async () => {
    const user = userEvent.setup();
    const requests = serve();
    renderWithProviders(<DashboardSettings />);

    await user.click(await screen.findByRole("button", { name: "Stop Jaws for sam" }));
    const dialog = screen.getByRole("dialog", { name: "Stop playback" });
    expect(dialog).toHaveTextContent("Jaws stops for sam right away.");
    await user.type(within(dialog).getByLabelText("Message"), "Server going down for updates");
    await user.click(within(dialog).getByRole("button", { name: "Stop" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    const stop = requests.find((r) => r.method === "DELETE" && r.path === "/api/sessions/abc");
    expect(stop?.body).toEqual({ message: "Server going down for updates" });
    expect(await screen.findByText("Playback stopped")).toBeInTheDocument();
    expect(await screen.findByText("Nobody is watching anything right now.")).toBeInTheDocument();
  });

  it("sends no message when none is written", async () => {
    const user = userEvent.setup();
    const requests = serve();
    renderWithProviders(<DashboardSettings />);
    await user.click(await screen.findByRole("button", { name: "Stop Jaws for sam" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Stop" }));
    await waitFor(() => expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/sessions/abc")).toBe(true));
    expect(requests.find((r) => r.method === "DELETE")?.body).toBeUndefined();
  });

  it("keeps the dialog and says why when it can't", async () => {
    const user = userEvent.setup();
    serve(() => apiError(403, "forbidden", "only administrators can stop other people's playback"));
    renderWithProviders(<DashboardSettings />);
    await user.click(await screen.findByRole("button", { name: "Stop Jaws for sam" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Stop" }));
    expect(await screen.findByText("Only administrators can stop other people's playback")).toBeInTheDocument();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });
});

describe("DashboardSettings library work", () => {
  it("says what waits and why", async () => {
    let activity: Activity = {
      scans: [],
      sessions: [],
      seasons_pending: 37,
      previews_pending: 1,
      library_work: { mode: "auto", waiting: "hours", resumes_at: null, learned: true },
    };
    mockApi({ "GET /api/activity": () => activity });
    const { client } = renderWithProviders(<DashboardSettings />);
    expect(await screen.findByText("37 seasons left · what's likely to be watched soon first, the rest in quieter hours")).toBeInTheDocument();
    expect(screen.getByText("1 file left · what's likely to be watched soon first, the rest in quieter hours")).toBeInTheDocument();

    // Other programs keep the processors busy: everything waits.
    activity = { ...activity, library_work: { mode: "hours", waiting: "busy", resumes_at: null, learned: true } };
    await client.invalidateQueries({ queryKey: ["activity"] });
    expect(await screen.findByText("37 seasons left · waits while other programs keep the processors busy")).toBeInTheDocument();

    // Nothing waits.
    activity = { ...activity, library_work: null };
    await client.invalidateQueries({ queryKey: ["activity"] });
    expect(await screen.findByText("37 seasons left")).toBeInTheDocument();
  });

  it("says how many files' previews are made at once", async () => {
    let activity: Activity = {
      scans: [],
      sessions: [],
      previews_pending: 12,
      previews_running: 3,
      library_work: { mode: "auto", waiting: null, resumes_at: null, learned: true },
    };
    mockApi({ "GET /api/activity": () => activity });
    const { client } = renderWithProviders(<DashboardSettings />);
    expect(await screen.findByText("12 files left · 3 at a time")).toBeInTheDocument();

    // One at a time, or a server before it: as ever.
    activity = { ...activity, previews_running: 1, library_work: { mode: "auto", waiting: "hours", resumes_at: null, learned: true } };
    await client.invalidateQueries({ queryKey: ["activity"] });
    expect(await screen.findByText("12 files left · what's likely to be watched soon first, the rest in quieter hours")).toBeInTheDocument();
  });
});
