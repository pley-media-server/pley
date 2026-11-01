import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { AppGrant } from "@/api/apps";
import type { User } from "@/api/types";
import { AudioPlayerProvider } from "@/audio/AudioPlayer";
import { type FakeRequest, mockApi, renderWithProviders, testUser } from "@/test/render";
import { AccountPage, PlaybackCard } from "./AccountPage";

/** The last account change: queries refetched after it may come later. */
function lastSave(requests: FakeRequest[]) {
  return requests.filter((r) => r.method === "PATCH" && r.path === "/api/me").at(-1);
}

function serve() {
  return mockApi({ "PATCH /api/me": (req) => ({ ...testUser, ...(req.body as Partial<User>) }) });
}

describe("PlaybackCard", () => {
  it("saves the audio language and when subtitles come on", async () => {
    const user = userEvent.setup();
    const requests = serve();
    renderWithProviders(<PlaybackCard />);

    // Only forced subtitles: there is no language to read them in.
    expect(screen.queryByLabelText("Subtitle language")).not.toBeInTheDocument();
    expect(screen.getByText(/for signs and foreign dialogue/)).toBeInTheDocument();

    await user.selectOptions(screen.getByLabelText("Audio language"), "French");
    await waitFor(() => expect(lastSave(requests)?.body).toEqual({ audio_language: "fre" }));
    expect(await screen.findByText("Audio language saved")).toBeInTheDocument();

    await user.selectOptions(screen.getByLabelText("Subtitles"), "Always");
    await waitFor(() => expect(lastSave(requests)?.body).toEqual({ subtitle_mode: "always" }));
  });

  it("asks for the language subtitles are read in", async () => {
    const user = userEvent.setup();
    const requests = serve();
    const viewer: User = { ...testUser, audio_language: "fre", subtitle_mode: "foreign" };
    renderWithProviders(<PlaybackCard />, { user: viewer });

    const language = screen.getByLabelText("Subtitle language");
    expect(language).toHaveValue("");
    expect(screen.queryByText(/without one, only forced subtitles/)).not.toBeInTheDocument();
    await user.selectOptions(language, "English");
    await waitFor(() => expect(lastSave(requests)?.body).toEqual({ subtitle_language: "eng" }));

    // The audio language back to the file's: nothing says which language is read.
    await user.selectOptions(screen.getByLabelText("Audio language"), "The file's default");
    await waitFor(() => expect(lastSave(requests)?.body).toEqual({ audio_language: null }));
  });

  it("has titles already loaded asked for again, since they start with the tracks these pick", async () => {
    const user = userEvent.setup();
    serve();
    const { client } = renderWithProviders(<PlaybackCard />);
    client.setQueryData(["item", 7], { id: 7 });
    await user.selectOptions(screen.getByLabelText("Subtitles"), "Always");
    await waitFor(() => expect(client.getQueryState(["item", 7])?.isInvalidated).toBe(true));
  });

  it("explains that a language is needed", () => {
    serve();
    renderWithProviders(<PlaybackCard />, { user: { ...testUser, subtitle_mode: "always" } });
    expect(screen.getByText(/without one, only forced subtitles come on/)).toBeInTheDocument();
  });
});

/** Where a modified server offers its source. */
const SOURCE = "https://example.org/pley-fork";

/**
 * The whole page as `user`, with nothing in its history or devices and
 * these apps; returns the requests it makes.
 */
function renderAccount(user: User, apps: AppGrant[] = []) {
  let list = apps;
  const requests = mockApi({
    "GET /api/server": () => ({ name: "Home", version: "1.2.3", source: SOURCE, setup_required: false }),
    "GET /api/me/two-step": () => ({ enabled: false, since: null, recovery_codes_left: 0 }),
    "GET /api/history": () => ({ total: 0, entries: [] }),
    "GET /api/me/devices": () => [],
    "GET /api/me/apps": () => list,
    // A new password signs the account's apps out.
    "PUT /api/auth/password": () => {
      list = [];
      return undefined;
    },
    "DELETE /api/me/apps/7": () => {
      list = list.filter((a) => a.id !== 7);
      return undefined;
    },
  });
  renderWithProviders(
    <AudioPlayerProvider>
      <AccountPage />
    </AudioPlayerProvider>,
    { user },
  );
  return requests;
}

const NOW = Math.floor(Date.now() / 1000);
/** pley's own app, on the account's phone. */
const android: AppGrant = {
  id: 7,
  client_id: "https://pley-media-server.github.io/pley/oauth/android.json",
  name: "Pley for Android",
  first_party: true,
  host: "pley-media-server.github.io",
  scopes: ["library", "play", "playlists", "profiles"],
  profile: { id: 1, name: "Admin" },
  approved_by: { id: 1, name: "Admin" },
  device: "Android 15",
  ip: "192.0.2.8",
  created_at: NOW - 86400 * 3,
  last_used_at: NOW - 3600,
};
/** An app by another publisher the account allowed for a child's profile. */
const other: AppGrant = {
  id: 8,
  client_id: "https://apps.example.com/client.json",
  name: "Movie Night",
  first_party: false,
  host: "apps.example.com",
  scopes: ["library"],
  profile: { id: 2, name: "Kiddo" },
  approved_by: { id: 1, name: "Admin" },
  device: null,
  ip: null,
  created_at: NOW - 86400,
  last_used_at: NOW - 60,
};

const NO_PASSWORD = "This account has no password: an administrator can give it one.";

describe("AccountPage", () => {
  it("tells an account without a password that an administrator can give it one", () => {
    renderAccount({ ...testUser, is_admin: false, has_password: false });
    expect(screen.getByText(NO_PASSWORD)).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Change password" })).not.toBeInTheDocument();
  });

  it("lets an account with a password change it", () => {
    renderAccount(testUser);
    expect(screen.getByRole("heading", { name: "Change password" })).toBeInTheDocument();
    expect(screen.queryByText(NO_PASSWORD)).not.toBeInTheDocument();
  });

  it("says nothing about passwords to a managed profile", () => {
    renderAccount({ ...testUser, is_admin: false, managed: true, has_password: false });
    expect(screen.queryByRole("heading", { name: "Change password" })).not.toBeInTheDocument();
    expect(screen.queryByText(NO_PASSWORD)).not.toBeInTheDocument();
  });

  it("lists the apps that use the account, and what they may do", async () => {
    renderAccount(testUser, [android, other]);
    const card = (await screen.findByText("Pley for Android")).closest("ul");
    if (!card) throw new Error("no list of apps");
    const [own, theirs] = within(card).getAllByRole("listitem");

    expect(own).toHaveTextContent("Pley's own app");
    expect(own).toHaveTextContent("Libraries · Play · Playlists · Profiles");
    expect(own).toHaveTextContent(/Android 15 · 192\.0\.2\.8 · last active 1 hour ago · allowed 3 days ago/);
    expect(own).not.toHaveTextContent(/Using|Allowed by/);

    // Others by host first; the name is only what the app calls itself.
    expect(theirs).toHaveTextContent("apps.example.com");
    expect(theirs).toHaveTextContent("calls itself “Movie Night”");
    expect(theirs).not.toHaveTextContent("Pley's own app");
    expect(theirs).toHaveTextContent("Using Kiddo · Libraries");
  });

  it("says who allowed an app that uses the profile", async () => {
    const kiddo: User = { ...testUser, id: 2, name: "Kiddo", is_admin: false, managed: true, has_password: false };
    renderAccount(kiddo, [other]);
    expect(await screen.findByText("apps.example.com")).toBeInTheDocument();
    expect(screen.getByText(/Allowed by Admin/)).toBeInTheDocument();
    expect(screen.queryByText(/Using Kiddo/)).not.toBeInTheDocument();
  });

  it("signs an app out", async () => {
    const user = userEvent.setup();
    const requests = renderAccount(testUser, [android, other]);
    await user.click(await screen.findByRole("button", { name: "Sign out Pley for Android" }));

    expect(await screen.findByText("Signed out Pley for Android")).toBeInTheDocument();
    expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/me/apps/7")).toBe(true);
    await waitFor(() => expect(screen.queryByRole("button", { name: "Sign out Pley for Android" })).not.toBeInTheDocument());
    expect(screen.getByText("apps.example.com")).toBeInTheDocument();
  });

  it("leads to the page where a TV's code is typed", async () => {
    renderAccount(testUser, [android]);
    await screen.findByText("Pley for Android");
    expect(screen.getByRole("link", { name: "Sign in on a TV" })).toHaveAttribute("href", "/link");
  });

  it("lists the apps again once a new password signed them out", async () => {
    const user = userEvent.setup();
    renderAccount(testUser, [android]);
    await screen.findByText("Pley for Android");
    await user.type(screen.getByLabelText("Current password"), "old-password-1");
    await user.type(screen.getByLabelText("New password"), "new-password-2");
    await user.type(screen.getByLabelText("Confirm new password"), "new-password-2");
    await user.click(screen.getByRole("button", { name: "Update password" }));
    expect(await screen.findByText("Password changed")).toBeInTheDocument();
    expect(
      await screen.findByText("No apps use your account. Apps ask on a Pley page before they get access."),
    ).toBeInTheDocument();
  });

  it("never says an app was used in the future when this device's clock is behind", async () => {
    const ahead = Math.floor(Date.now() / 1000) + 5;
    renderAccount(testUser, [{ ...android, created_at: ahead, last_used_at: ahead }]);
    const row = (await screen.findByText("Pley for Android")).closest("li");
    expect(row).toHaveTextContent(/last active now · allowed now/);
    expect(row).not.toHaveTextContent(/\bin \d+ seconds?/);
  });

  it("says how apps get access when none has it", async () => {
    renderAccount(testUser);
    expect(
      await screen.findByText("No apps use your account. Apps ask on a Pley page before they get access."),
    ).toBeInTheDocument();
  });

  it("links to the source code where the server says it is offered", async () => {
    renderAccount({ ...testUser, is_admin: false });
    const link = await within(screen.getByTestId("build")).findByRole("link", { name: "Source code" });
    expect(link).toHaveAttribute("href", SOURCE);
  });
});
