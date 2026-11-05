import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { ServiceTestResult, Settings } from "@/api/types";
import { mockApi, renderWithProviders, testQueryClient, testUser } from "@/test/render";
import { ServerSettings } from "./ServerSettings";
import { TestButton } from "./SettingsForm";

const SETTINGS = {
  server_name: "Den",
  language: "",
  tmdb_api_key: "",
  metadata_language: "en-US",
  tv_metadata: "auto",
  tvdb_api_key: "",
  tvdb_pin: "",
  music_metadata: true,
  hwaccel: "auto",
  hardware_decoding: true,
  tonemapping: "auto",
  max_transcodes: 4,
  transcode_preset: "veryfast",
  throttle_transcodes: true,
  transcode_in_memory: true,
  scan_interval_minutes: 360,
  watch_libraries: true,
  detect_markers: true,
  seek_previews: true,
  previews_in_background: true,
  third_party_apps: false,
} satisfies Settings;

describe("ServerSettings", () => {
  it("opens again with the settings it has, without a spinner that stays", async () => {
    mockApi({ "GET /api/settings": () => SETTINGS, "GET /api/system": () => ({}) });
    const first = renderWithProviders(<ServerSettings />);
    expect(await screen.findByLabelText("Server name")).toHaveValue("Den");
    first.unmount();
    // The second time the settings come from the cache straight away.
    renderWithProviders(<ServerSettings />, { client: first.client });
    expect(await screen.findByLabelText("Server name")).toHaveValue("Den");
  });

  it("lets apps by other publishers sign in", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/settings": () => SETTINGS,
      "GET /api/system": () => ({}),
      "PUT /api/settings": (req) => ({ ...SETTINGS, ...(req.body as Partial<Settings>) }),
    });
    renderWithProviders(<ServerSettings />);
    const toggle = await screen.findByRole("switch", { name: "Apps by other publishers" });
    expect(toggle).not.toBeChecked();
    expect(screen.getByText(/turning it off signs them out\./)).toBeInTheDocument();

    await user.click(toggle);
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Settings saved")).toBeInTheDocument();
    await waitFor(() => expect(requests.find((r) => r.method === "PUT")?.body).toEqual({ third_party_apps: true }));
    expect(screen.getByRole("switch", { name: "Apps by other publishers" })).toBeChecked();
  });

  it("requires two-step sign-in only from an administrator who uses it", async () => {
    mockApi({ "GET /api/settings": () => ({ ...SETTINGS, require_two_step: false }), "GET /api/system": () => ({}), "GET /api/users": () => [] });
    renderWithProviders(<ServerSettings />, { user: { ...testUser, two_step: false } });
    expect(await screen.findByRole("switch", { name: "Require two-step sign-in" })).toBeDisabled();
    expect(screen.getByText(/Turn on two-step sign-in for your own account first/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Account" })).toHaveAttribute("href", "/account");
  });

  it("says who gets signed out, requires two-step sign-in, and has the server's status read again", async () => {
    const user = userEvent.setup();
    let required = false;
    const requests = mockApi({
      "GET /api/settings": () => ({ ...SETTINGS, require_two_step: required }),
      "GET /api/system": () => ({}),
      "GET /api/users": () => [
        { ...testUser, two_step: true },
        { ...testUser, id: 2, name: "Bob" },
        { ...testUser, id: 3, name: "Cy" },
        { ...testUser, id: 4, name: "Kid", managed: true, has_password: false },
        { ...testUser, id: 5, name: "Old", disabled: true },
      ],
      "PUT /api/settings": (req) => {
        required = !!(req.body as Partial<Settings>).require_two_step;
        return { ...SETTINGS, ...(req.body as Partial<Settings>) };
      },
    });
    const client = testQueryClient();
    // What the sign-in pages and the account page read before.
    client.setQueryData(["server"], { name: "Den", version: "1.0.0", setup_required: false, two_step_required: false });
    renderWithProviders(<ServerSettings />, { user: { ...testUser, two_step: true }, client });

    const toggle = await screen.findByRole("switch", { name: "Require two-step sign-in" });
    expect(toggle).toBeEnabled();
    expect(toggle).not.toBeChecked();
    await user.click(toggle);
    expect(await screen.findByText(/Saving signs out 2 people who haven't set it up/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Settings saved")).toBeInTheDocument();
    await waitFor(() => expect(requests.find((r) => r.method === "PUT")?.body).toEqual({ require_two_step: true }));
    await waitFor(() => expect(client.getQueryState(["server"])?.isInvalidated).toBe(true));
    expect(screen.queryByText(/Saving signs out/)).not.toBeInTheDocument();
  });

  it("says who can't sign in while the server can't use its key", async () => {
    mockApi({
      "GET /api/settings": () => ({ ...SETTINGS, require_two_step: true }),
      "GET /api/system": () => ({ secret_key_ok: false }),
      "GET /api/users": () => [],
    });
    renderWithProviders(<ServerSettings />, { user: { ...testUser, two_step: false } });
    const toggle = await screen.findByRole("switch", { name: "Require two-step sign-in" });
    expect(toggle).toBeChecked();
    expect(toggle).toBeEnabled();
    expect(await screen.findByText(/can't use its secret.key/)).toBeInTheDocument();
    expect(screen.getByText(/haven't set up two-step sign-in can't sign in/)).toBeInTheDocument();
  });

  it("lists apps again once turning other publishers off signed them out", async () => {
    const user = userEvent.setup();
    mockApi({
      "GET /api/settings": () => ({ ...SETTINGS, third_party_apps: true }),
      "GET /api/system": () => ({}),
      "PUT /api/settings": (req) => ({ ...SETTINGS, ...(req.body as Partial<Settings>) }),
    });
    const client = testQueryClient();
    // What the account page and the users dialog showed before.
    client.setQueryData(["me", "apps"], []);
    client.setQueryData(["users", 2, "apps"], []);
    renderWithProviders(<ServerSettings />, { client });

    await user.click(await screen.findByRole("switch", { name: "Apps by other publishers" }));
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Settings saved")).toBeInTheDocument();
    await waitFor(() => expect(client.getQueryState(["me", "apps"])?.isInvalidated).toBe(true));
    expect(client.getQueryState(["users", 2, "apps"])?.isInvalidated).toBe(true);
  });

  it("keeps apps by other publishers off when the server doesn't say", async () => {
    const older: Partial<Settings> = { ...SETTINGS };
    delete older.third_party_apps;
    mockApi({ "GET /api/settings": () => older, "GET /api/system": () => ({}) });
    renderWithProviders(<ServerSettings />);
    expect(await screen.findByRole("switch", { name: "Apps by other publishers" })).not.toBeChecked();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("greys out previews for whole libraries without seek previews", async () => {
    const user = userEvent.setup();
    mockApi({ "GET /api/settings": () => SETTINGS, "GET /api/system": () => ({}) });
    renderWithProviders(<ServerSettings />);
    const whole = await screen.findByRole("switch", { name: "Previews for whole libraries" });
    expect(whole).toBeEnabled();
    await user.click(screen.getByRole("switch", { name: "Seek previews" }));
    expect(whole).toBeDisabled();
    expect(whole).toBeChecked();
    expect(screen.getByText("Needs Seek previews.")).toBeInTheDocument();
  });

  it("greys out intro detection when the transcoder's FFmpeg has no Chromaprint, and says why", async () => {
    mockApi({
      "GET /api/settings": () => SETTINGS,
      "GET /api/system": () => ({ os: "windows", transcoder: { available: true, managed: true, encoders: ["libx264"], accels: [], chromaprint: false } }),
    });
    renderWithProviders(<ServerSettings />);
    const detect = await screen.findByRole("switch", { name: "Detect intros and credits" });
    // The current value stays, greyed out with the reason.
    expect(detect).toBeDisabled();
    expect(detect).toBeChecked();
    expect(screen.getByText(/lacks Chromaprint/)).toBeInTheDocument();
  });

  it("shows when library work runs automatically, as learned, and sets hours of its own", async () => {
    const user = userEvent.setup();
    // Evenings watched, after midnight not seen, the night quiet.
    const hours = Array.from({ length: 168 }, (_, at) => {
      const h = at % 24;
      return { playing: h >= 19 ? 0.6 : h < 2 ? null : 0, open: h >= 2 && h < 7 };
    });
    const requests = mockApi({
      "GET /api/settings": () => ({ ...SETTINGS, library_work: "auto", library_work_from: 1, library_work_to: 7, library_work_max_cpu: 50 }),
      "GET /api/system": () => ({}),
      "GET /api/system/library-work": () => ({ mode: "auto", waiting: null, resumes_at: null, learned: true, now: 93, hours }),
      "PUT /api/settings": (req) => ({ ...SETTINGS, library_work: "auto", ...(req.body as Partial<Settings>) }),
    });
    renderWithProviders(<ServerSettings />);
    const mode = await screen.findByLabelText("When library work runs");
    expect(mode).toHaveValue("auto");
    // The week, an hour a cell.
    expect(await screen.findAllByRole("img", { name: /something played 60% of the time$/ })).toHaveLength(7 * 5);
    expect(screen.getAllByRole("img", { name: /not seen enough yet$/ })).toHaveLength(7 * 2);
    expect(screen.getAllByRole("img", { name: /library work may begin$/ })).toHaveLength(7 * 5);
    expect(screen.getByText(/^Learned from what played/)).toBeInTheDocument();

    await user.selectOptions(mode, "hours");
    expect(screen.queryByText("When people watch")).not.toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText("To"), "6");
    await user.selectOptions(screen.getByLabelText("Wait for other programs"), "100");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(requests.find((r) => r.method === "PUT")?.body).toEqual({ library_work: "hours", library_work_to: 6, library_work_max_cpu: 100 }),
    );
  });

  it("leaves library work out on a server before it, and greys it out with nothing to run", async () => {
    const user = userEvent.setup();
    mockApi({ "GET /api/settings": () => SETTINGS, "GET /api/system": () => ({}) });
    const first = renderWithProviders(<ServerSettings />);
    expect(await screen.findByRole("switch", { name: "Detect intros and credits" })).toBeInTheDocument();
    expect(screen.queryByLabelText("When library work runs")).not.toBeInTheDocument();
    first.unmount();

    mockApi({ "GET /api/settings": () => ({ ...SETTINGS, library_work: "idle", library_work_max_cpu: 50 }), "GET /api/system": () => ({}) });
    renderWithProviders(<ServerSettings />);
    const mode = await screen.findByLabelText("When library work runs");
    expect(mode).toBeEnabled();
    await user.click(screen.getByRole("switch", { name: "Detect intros and credits" }));
    await user.click(screen.getByRole("switch", { name: "Seek previews" }));
    expect(mode).toBeDisabled();
    expect(screen.getByLabelText("Wait for other programs")).toBeDisabled();
    expect(screen.getByText("Needs Detect intros and credits, or Previews for whole libraries.")).toBeInTheDocument();
  });

  it("sets up subtitle search, sending the password only when it changes", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/settings": () => ({ ...SETTINGS, opensubtitles_api_key: "key", opensubtitles_username: "ana", opensubtitles_password_set: true }),
      "GET /api/system": () => ({}),
      "PUT /api/settings": (req) => ({ ...SETTINGS, ...(req.body as Partial<Settings>), opensubtitles_password: undefined, opensubtitles_password_set: true }),
      "POST /api/settings/test": (): ServiceTestResult => ({ ok: true, message: "OpenSubtitles accepted the key and signed the account in.", ms: 240 }),
    });
    renderWithProviders(<ServerSettings />);
    const password = await screen.findByLabelText("OpenSubtitles password");
    expect(password).toHaveValue("");
    expect(password).toHaveAttribute("placeholder", "Saved");
    expect(screen.getByLabelText("OpenSubtitles username")).toHaveValue("ana");
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();

    // The saved password is tested when none is typed.
    await user.click(screen.getByRole("button", { name: "Test OpenSubtitles" }));
    expect(await screen.findByText("OpenSubtitles accepted the key and signed the account in.")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "POST")?.body).toEqual({ service: "opensubtitles", api_key: "key", username: "ana" });

    await user.type(password, "s3cret");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Settings saved")).toBeInTheDocument();
    await waitFor(() => expect(requests.find((r) => r.method === "PUT")?.body).toEqual({ opensubtitles_password: "s3cret" }));
    expect(screen.getByLabelText("OpenSubtitles password")).toHaveValue("");

    // Another account: its password is asked for again.
    const username = screen.getByLabelText("OpenSubtitles username");
    await user.clear(username);
    await user.type(username, "bo");
    expect(screen.getByLabelText("OpenSubtitles password")).toHaveAttribute("placeholder", "Password");
  });

  it("leaves the transcoder to its own page", async () => {
    mockApi({ "GET /api/settings": () => SETTINGS, "GET /api/system": () => ({}) });
    renderWithProviders(<ServerSettings />);
    expect(await screen.findByLabelText("Server name")).toBeInTheDocument();
    expect(screen.queryByLabelText("Hardware acceleration")).not.toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: "Throttle transcodes" })).not.toBeInTheDocument();
  });
});

describe("TestButton", () => {
  it("tests the key as typed and says how it went", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "POST /api/settings/test": ({ body }): ServiceTestResult =>
        (body as { api_key: string }).api_key === "good"
          ? { ok: true, message: "TMDB accepted the key.", ms: 182 }
          : { ok: false, message: "TMDB rejected this key.", ms: 95 },
    });
    const { rerender } = renderWithProviders(<TestButton label="Test the TMDB key" test={{ service: "tmdb", api_key: "good" }} />);

    await user.click(screen.getByRole("button", { name: "Test the TMDB key" }));
    expect(await screen.findByText("TMDB accepted the key.")).toBeInTheDocument();
    expect(screen.getByText("182 ms")).toBeInTheDocument();
    expect(requests.at(-1)?.body).toEqual({ service: "tmdb", api_key: "good" });

    // A new key, a new answer (the settings page remounts it by key).
    rerender(<TestButton key="bad" label="Test the TMDB key" test={{ service: "tmdb", api_key: "bad" }} />);
    expect(screen.queryByText("TMDB accepted the key.")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Test the TMDB key" }));
    expect(await screen.findByText("TMDB rejected this key.")).toBeInTheDocument();
    expect(screen.queryByText("95 ms")).not.toBeInTheDocument();
  });
});
