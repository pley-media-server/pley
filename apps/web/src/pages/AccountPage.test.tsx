import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { AppGrant } from "@/api/apps";
import type { CompatibilityToken, CreatedCompatibilityToken } from "@/api/compatibilityTokens";
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

/** The token the server makes next. */
const NEW_TOKEN = "pley_ct_c2VjcmV0LXRva2VuLWZvci10ZXN0cy1vbmx5";

/**
 * The whole page as `user`, with nothing in its history or devices, these
 * apps and these tokens for other apps; returns the requests it makes.
 */
function renderAccount(user: User, apps: AppGrant[] = [], tokens: CompatibilityToken[] = []) {
  let list = apps;
  let tokenList = tokens;
  const requests = mockApi({
    "GET /api/server": () => ({ name: "Home", version: "1.2.3", source: SOURCE, setup_required: false }),
    "GET /api/me/two-step": () => ({ enabled: false, since: null, recovery_codes_left: 0 }),
    "GET /api/history": () => ({ total: 0, entries: [] }),
    "GET /api/me/devices": () => [],
    "GET /api/me/apps": () => list,
    // A new password signs the account's apps out, and ends its tokens.
    "PUT /api/auth/password": () => {
      list = [];
      tokenList = [];
      return undefined;
    },
    "DELETE /api/me/apps/7": () => {
      list = list.filter((a) => a.id !== 7);
      return undefined;
    },
    "GET /api/me/compatibility-tokens": () => tokenList,
    "POST /api/me/compatibility-tokens": (req) => {
      const { name } = req.body as { name: string };
      const made: CompatibilityToken = {
        id: 9,
        name,
        scopes: ["play"],
        created_at: NOW,
        last_used_at: null,
        expires_at: NOW + 90 * 86400,
        device: null,
        ip: null,
      };
      tokenList = [made, ...tokenList];
      return { ...made, token: NEW_TOKEN } satisfies CreatedCompatibilityToken;
    },
    "DELETE /api/me/compatibility-tokens/3": () => {
      tokenList = tokenList.filter((t) => t.id !== 3);
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

/** A Jellyfin app's sign-in, used a moment ago. */
const findroid: CompatibilityToken = {
  id: 3,
  name: "Findroid on Phone",
  scopes: ["play"],
  created_at: NOW - 86400 * 2,
  last_used_at: NOW - 120,
  expires_at: NOW + 86400 * 90,
  device: "Findroid/0.16",
  ip: "192.0.2.9",
};
/** A token made for a script, never used yet. */
const script: CompatibilityToken = {
  id: 4,
  name: "Backup script",
  scopes: ["play"],
  created_at: NOW - 3600,
  last_used_at: null,
  expires_at: NOW + 86400 * 90,
  device: null,
  ip: null,
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

  it("lists the apps signed in and the tokens made, with their last use", async () => {
    renderAccount(testUser, [], [findroid, script]);
    const used = (await screen.findByText("Findroid on Phone")).closest("li");
    expect(used).toHaveTextContent(/Findroid\/0\.16 · 192\.0\.2\.9 · last used 2 minutes ago · made 2 days ago/);
    const unused = screen.getByText("Backup script").closest("li");
    expect(unused).toHaveTextContent(/not used yet · made 1 hour ago/);
  });

  it("makes a token and shows it once", async () => {
    const user = userEvent.setup();
    const requests = renderAccount(testUser);
    await user.click(await screen.findByRole("button", { name: "New token" }));
    await user.click(screen.getByRole("button", { name: "Make token" }));
    expect(screen.getByText("Name the token after the app that will use it.")).toBeInTheDocument();

    await user.type(screen.getByLabelText("Name"), " Backup script ");
    await user.click(screen.getByRole("button", { name: "Make token" }));
    const shown = await screen.findByLabelText("New token");
    expect(shown).toHaveValue(NEW_TOKEN);
    const made = requests.find((r) => r.method === "POST" && r.path === "/api/me/compatibility-tokens");
    expect(made?.body).toEqual({ name: "Backup script" });
    expect(screen.getByText(/Pley shows it only once/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Copy" }));
    expect(await screen.findByText("Token copied")).toBeInTheDocument();
    expect(await navigator.clipboard.readText()).toBe(NEW_TOKEN);
    // Listed, and once done, never shown again.
    expect((await screen.findByText("Not used yet", { exact: false })).closest("li")).toHaveTextContent("Backup script");
    await user.click(screen.getByRole("button", { name: "Done" }));
    expect(screen.queryByLabelText("New token")).not.toBeInTheDocument();
    expect(screen.queryByText(NEW_TOKEN)).not.toBeInTheDocument();
  });

  it("revokes an app's sign-in", async () => {
    const user = userEvent.setup();
    const requests = renderAccount(testUser, [], [findroid, script]);
    await user.click(await screen.findByRole("button", { name: "Revoke Findroid on Phone" }));
    expect(await screen.findByText("Revoked Findroid on Phone")).toBeInTheDocument();
    expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/me/compatibility-tokens/3")).toBe(true);
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Revoke Findroid on Phone" })).not.toBeInTheDocument(),
    );
    expect(screen.getByText("Backup script")).toBeInTheDocument();
  });

  it("lists the tokens again once a new password ended them", async () => {
    const user = userEvent.setup();
    renderAccount(testUser, [], [findroid]);
    await screen.findByText("Findroid on Phone");
    await user.type(screen.getByLabelText("Current password"), "old-password-1");
    await user.type(screen.getByLabelText("New password"), "new-password-2");
    await user.type(screen.getByLabelText("Confirm new password"), "new-password-2");
    await user.click(screen.getByRole("button", { name: "Update password" }));
    expect(await screen.findByText(/No apps are signed in/)).toBeInTheDocument();
  });

  it("links to the source code where the server says it is offered", async () => {
    renderAccount({ ...testUser, is_admin: false });
    const link = await within(screen.getByTestId("build")).findByRole("link", { name: "Source code" });
    expect(link).toHaveAttribute("href", SOURCE);
  });
});
