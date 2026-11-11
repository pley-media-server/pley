import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { AppGrant } from "@/api/apps";
import type { Library, RatingSystems, User } from "@/api/types";
import { mockApi, renderWithProviders, testUser } from "@/test/render";
import { UsersSettings } from "./UsersSettings";

const admin: User = { ...testUser, id: 1, username: "admin", name: "Admin", is_admin: true };
const kid: User = {
  ...testUser,
  id: 2,
  username: "kid",
  name: "Kiddo",
  display_name: "Kiddo",
  is_admin: false,
  managed: true,
  has_pin: true,
  all_libraries: false,
  library_ids: [2],
  max_age: 7,
};
const ann: User = { ...testUser, id: 3, username: "ann", name: "Ann", is_admin: false, two_step: true };
/** An account without a password: it can't sign in until it gets one. */
const lee: User = {
  ...testUser,
  id: 4,
  username: "lee",
  name: "Lee",
  is_admin: false,
  has_password: false,
  // Without an age limit, labels always shown don't count.
  show_labels: ["Kids OK"],
  hide_labels: ["Scary", "Loud"],
};
const libraries: Library[] = [
  { id: 1, name: "Movies", kind: "movies", paths: [], item_count: 0, created_at: 0, scanned_at: null, scanning: false },
  { id: 2, name: "Cartoons", kind: "shows", paths: [], item_count: 0, created_at: 0, scanned_at: null, scanning: false },
];

/** pley's own app, which Ann allowed for Kiddo. */
const kidsApp: AppGrant = {
  id: 9,
  client_id: "https://pley-media-server.github.io/pley/oauth/android.json",
  name: "Pley for Android",
  first_party: true,
  host: "pley-media-server.github.io",
  scopes: ["library", "play"],
  profile: { id: 2, name: "Kiddo" },
  approved_by: { id: 3, name: "Ann" },
  device: "Android 15",
  ip: "192.0.2.9",
  created_at: 0,
  last_used_at: 0,
};

const ratingSystems: RatingSystems = {
  region: "US",
  systems: [
    {
      country: "US",
      name: "United States",
      ratings: [
        { name: "G", age: 0, kind: "film" },
        { name: "TV-Y7", age: 7, kind: "tv" },
        { name: "PG-13", age: 13, kind: "film" },
      ],
    },
  ],
};

/** The server's answers, with `extra` ones added or put in their place. */
function serve(extra: Parameters<typeof mockApi>[0] = {}) {
  let kidsApps = [kidsApp];
  let annsApps = [{ ...kidsApp, id: 10, profile: { id: 3, name: "Ann" } }];
  return mockApi({
    "GET /api/users/1/apps": () => [],
    "GET /api/users/2/apps": () => kidsApps,
    "DELETE /api/users/2/apps/9": () => {
      kidsApps = [];
      return undefined;
    },
    "DELETE /api/users/2/devices": () => {
      kidsApps = [];
      return undefined;
    },
    // Ann is signed in only through an app.
    "GET /api/users/3/apps": () => annsApps,
    "DELETE /api/users/3/devices": () => {
      annsApps = [];
      return undefined;
    },
    "GET /api/users/4/apps": () => [],
    "GET /api/users": () => [admin, kid, ann, lee],
    "GET /api/ratings/systems": () => ratingSystems,
    "GET /api/labels": () => [{ label: "Override: Kids", items: 3 }],
    "GET /api/libraries": () => libraries,
    "GET /api/users/1/devices": () => [],
    "GET /api/users/2/devices": () => [{ id: "d1", name: "Safari on iPhone", user_agent: null, ip: "192.0.2.4", created_at: 0, last_seen_at: 0, current: false }],
    "POST /api/users": (req) => ({ ...testUser, id: 3, name: (req.body as { username: string }).username }),
    "PATCH /api/users/2": () => ({ ...kid, name: "Kiddo Two" }),
    "DELETE /api/users/2": () => undefined,
    "GET /api/users/3/devices": () => [],
    "DELETE /api/users/3/two-step": () => undefined,
    "GET /api/users/4/devices": () => [],
    ...extra,
  });
}

/** Renders the page as the administrator; returns the requests it makes. */
function renderUsers() {
  const requests = serve();
  renderWithProviders(<UsersSettings />, { user: admin });
  return requests;
}

/** Opens the dialog's tab `name`. */
async function openTab(user: ReturnType<typeof userEvent.setup>, dialog: HTMLElement, name: string) {
  // A tab that needs a look says so in its name too.
  await user.click(within(dialog).getByRole("tab", { name: new RegExp(`^${name}`) }));
}

async function openAddDialog() {
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Add user" }));
  return { user, dialog: screen.getByRole("dialog", { name: "Add user" }) };
}

describe("UsersSettings", () => {
  it("lists users with what sets them apart", async () => {
    renderUsers();
    // The limit shows as its rating once the rating systems are in.
    expect(await screen.findByText("Up to TV-Y7")).toBeInTheDocument();
    const rows = screen.getAllByRole("listitem");
    expect(rows[0]).toHaveTextContent("Admin");
    expect(rows[0]).toHaveTextContent("(you)");
    expect(rows[1]).toHaveTextContent("@kid");
    for (const badge of ["Managed", "Home", "Cartoons", "Up to TV-Y7", "PIN"]) expect(rows[1]).toHaveTextContent(badge);
  });

  it("turns two-step sign-in off for someone who lost their phone", async () => {
    const user = userEvent.setup();
    const requests = renderUsers();
    const rows = await screen.findAllByRole("listitem");
    expect(rows[2]).toHaveTextContent("2FA");
    await user.click(screen.getByRole("button", { name: /Ann/ }));
    const dialog = screen.getByRole("dialog", { name: "Edit Ann" });
    await user.click(within(dialog).getByRole("button", { name: "Turn off" }));
    expect(requests.some((r) => r.method === "DELETE")).toBe(false);
    await user.click(within(dialog).getByRole("button", { name: "Turn off" }));
    expect(await screen.findByText("Two-step sign-in is off for Ann")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "DELETE")?.path).toBe("/api/users/3/two-step");
  });

  it("shows who hasn't set up the two-step sign-in the server requires, and resets it for a new phone", async () => {
    const user = userEvent.setup();
    const bob: User = { ...testUser, id: 5, username: "bob", name: "Bob", is_admin: false };
    const requests = mockApi({
      "GET /api/server": () => ({ name: "Den", version: "1.0.0", setup_required: false, two_step_required: true }),
      "GET /api/users": () => [admin, kid, ann, lee, bob],
      "GET /api/libraries": () => libraries,
      "GET /api/users/3/devices": () => [],
      "GET /api/users/3/apps": () => [],
      "DELETE /api/users/3/two-step": () => undefined,
    });
    renderWithProviders(<UsersSettings />, { user: admin });

    const rows = await screen.findAllByRole("listitem");
    const row = (name: string) => rows.find((r) => r.textContent?.includes(name))!;
    await waitFor(() => expect(row("Bob")).toHaveTextContent("2FA not set up"));
    // Managed profiles and accounts without a password have nothing to set up.
    for (const name of ["Kiddo", "Lee", "Ann"]) expect(row(name)).not.toHaveTextContent("not set up");
    expect(row("Ann")).toHaveTextContent("2FA");

    await user.click(screen.getByRole("button", { name: /Ann/ }));
    const dialog = screen.getByRole("dialog", { name: "Edit Ann" });
    expect(within(dialog).getByText(/Turning it off signs Ann out everywhere/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Turn off" }));
    await user.click(within(dialog).getByRole("button", { name: "Turn off" }));
    expect(await screen.findByText("Ann is signed out, and sets up two-step sign-in again at the next sign-in")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "DELETE")?.path).toBe("/api/users/3/two-step");
  });

  it("says that an account without a password needs one to sign in", async () => {
    const user = userEvent.setup();
    renderUsers();
    await user.click(await screen.findByRole("button", { name: /Lee/ }));
    const dialog = screen.getByRole("dialog", { name: "Edit Lee" });
    expect(within(dialog).getByText("This account has no password: set one so it can sign in.")).toBeInTheDocument();
    expect(within(dialog).queryByText("Leave empty to keep the current one.")).not.toBeInTheDocument();
  });

  it("says an account with a password keeps it unless a new one is typed", async () => {
    const user = userEvent.setup();
    renderUsers();
    await user.click(await screen.findByRole("button", { name: /Ann/ }));
    const dialog = screen.getByRole("dialog", { name: "Edit Ann" });
    expect(within(dialog).getByText("Leave empty to keep the current one.")).toBeInTheDocument();
    expect(within(dialog).queryByText(/has no password/)).not.toBeInTheDocument();
  });

  it("adds an account after checking the form", async () => {
    const requests = renderUsers();
    const { user, dialog } = await openAddDialog();
    const add = within(dialog).getByRole("button", { name: "Add user" });

    await user.click(add);
    expect(within(dialog).getByText("Enter a username.")).toBeInTheDocument();
    await user.type(within(dialog).getByLabelText("Username"), " guest ");
    await user.type(within(dialog).getByLabelText("Password"), "abc");
    await user.click(add);
    expect(within(dialog).getByText("Use at least 8 characters for the password.")).toBeInTheDocument();
    await user.type(within(dialog).getByLabelText("Password"), "-defgh");
    await user.type(within(dialog).getByLabelText("Profile PIN"), "1a2");
    await user.click(add);
    expect(within(dialog).getByText("The PIN must be 4 to 8 digits.")).toBeInTheDocument();
    await user.type(within(dialog).getByLabelText("Profile PIN"), "34");

    await user.click(add);
    expect(await screen.findByText("Added guest")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "POST")?.body).toEqual({
      username: "guest",
      password: "abc-defgh",
      is_admin: false,
      managed: false,
      home: true,
      all_libraries: true,
      library_ids: [],
      max_age: null,
      max_age_system: null,
      show_labels: [],
      hide_labels: [],
      pin: "1234",
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("adds a managed profile without a password, limited to some libraries", async () => {
    const requests = renderUsers();
    const { user, dialog } = await openAddDialog();

    await user.click(within(dialog).getByRole("button", { name: /Managed profile/ }));
    expect(within(dialog).queryByLabelText("Password")).not.toBeInTheDocument();
    expect(within(dialog).getByRole("switch", { name: "Administrator" })).toBeDisabled();
    await user.type(within(dialog).getByLabelText("Username"), "little");
    await openTab(user, dialog, "Access");
    await user.click(within(dialog).getByRole("button", { name: "Only some" }));
    await user.click(within(dialog).getByRole("button", { name: "Add user" }));
    expect(within(dialog).getByText("Pick at least one library, or allow all of them.")).toBeInTheDocument();

    await user.click(within(dialog).getByLabelText(/Cartoons/));
    await user.click(within(dialog).getByRole("button", { name: "Add user" }));
    await waitFor(() => expect(requests.some((r) => r.method === "POST")).toBe(true));
    expect(requests.find((r) => r.method === "POST")?.body).toMatchObject({
      username: "little",
      managed: true,
      is_admin: false,
      all_libraries: false,
      library_ids: [2],
    });
    expect(requests.find((r) => r.method === "POST")?.body).not.toHaveProperty("password");
  });

  it("sets the age limit by rating or by age, showing only that way's fields", async () => {
    const user = userEvent.setup();
    const requests = renderUsers();
    await user.click(await screen.findByRole("button", { name: /Kiddo/ }));
    const dialog = screen.getByRole("dialog", { name: "Edit Kiddo" });
    await openTab(user, dialog, "Access");
    const limit = within(dialog).getByLabelText("Age limit");
    // By rating, in the region's ratings, with the limit as its rating.
    await waitFor(() => expect(limit).toHaveValue("rating"));
    expect(within(dialog).getByLabelText("Rating system")).toHaveValue("US");
    expect(within(dialog).getByLabelText("Rating")).toHaveValue("7");
    expect(within(dialog).getByText(/suitable for age 7: TV-Y7 and gentler ratings/)).toBeInTheDocument();
    expect(within(dialog).queryByLabelText("Age")).not.toBeInTheDocument();
    // By age keeps the limit, and offers every age.
    await user.selectOptions(limit, "By age");
    expect(within(dialog).queryByLabelText("Rating system")).not.toBeInTheDocument();
    expect(within(dialog).getByLabelText("Age")).toHaveValue("7");
    expect(within(dialog).getByRole("option", { name: "8" })).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(requests.find((r) => r.method === "PATCH")?.body).toEqual({ max_age_system: "age" }));
  });

  it("warns that unrated movies are hidden without a TMDB key, linking to it in a new tab", async () => {
    const user = userEvent.setup();
    serve({ "GET /api/settings": () => ({ tmdb_api_key: "" }) });
    renderWithProviders(<UsersSettings />, { user: admin });
    await user.click(await screen.findByRole("button", { name: /Kiddo/ }));
    const dialog = screen.getByRole("dialog", { name: "Edit Kiddo" });
    await openTab(user, dialog, "Access");
    const note = await within(dialog).findByRole("note");
    expect(note).toHaveTextContent("Unrated movies are hidden");
    const link = within(note).getByRole("link", { name: "Add a TMDB key" });
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("href", "/settings/server#tmdb-key");
    // Without a limit, nothing is hidden.
    await user.selectOptions(within(dialog).getByLabelText("Age limit"), "No limit");
    expect(within(dialog).queryByRole("note")).not.toBeInTheDocument();
  });

  it("says nothing of TMDB where it has a key", async () => {
    const user = userEvent.setup();
    serve({ "GET /api/settings": () => ({ tmdb_api_key: "set" }) });
    renderWithProviders(<UsersSettings />, { user: admin });
    await user.click(await screen.findByRole("button", { name: /Kiddo/ }));
    const dialog = screen.getByRole("dialog", { name: "Edit Kiddo" });
    await openTab(user, dialog, "Access");
    await within(dialog).findByLabelText("Rating");
    expect(within(dialog).queryByRole("note")).not.toBeInTheDocument();
  });

  it("lets a limit follow a birthday", async () => {
    const user = userEvent.setup();
    const requests = renderUsers();
    await user.click(await screen.findByRole("button", { name: /Kiddo/ }));
    const dialog = screen.getByRole("dialog", { name: "Edit Kiddo" });
    await openTab(user, dialog, "Access");
    await waitFor(() => expect(within(dialog).getByLabelText("Age limit")).toHaveValue("rating"));
    await user.selectOptions(within(dialog).getByLabelText("Age limit"), "By birthday");
    expect(within(dialog).queryByLabelText("Rating")).not.toBeInTheDocument();
    // Born this month seven years ago: the limit stays 7.
    const now = new Date();
    const year = now.getFullYear() - 7;
    expect(within(dialog).getByLabelText("Year")).toHaveValue(String(year));
    expect(within(dialog).getByLabelText("Birthday month")).toHaveValue(String(now.getMonth() + 1));
    expect(within(dialog).getByText(/suitable for age 7: TV-Y7 and gentler ratings/)).toBeInTheDocument();
    expect(within(dialog).getByText(new RegExp(`ends in .* ${year + 21}`))).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    const month = String(now.getMonth() + 1).padStart(2, "0");
    await waitFor(() => expect(requests.find((r) => r.method === "PATCH")?.body).toEqual({ birth_month: `${year}-${month}` }));
  });

  it("saves only what changed", async () => {
    const user = userEvent.setup();
    const requests = renderUsers();
    await user.click(await screen.findByRole("button", { name: /Kiddo/ }));
    const dialog = screen.getByRole("dialog", { name: "Edit Kiddo" });
    await openTab(user, dialog, "Devices");
    expect(await within(dialog).findByText("Safari on iPhone")).toBeInTheDocument();
    await openTab(user, dialog, "Profile");

    const name = within(dialog).getByLabelText("Display name");
    await user.clear(name);
    await user.type(name, "Kiddo Two");
    await user.click(within(dialog).getByRole("button", { name: "Remove PIN" }));
    await user.click(within(dialog).getByRole("button", { name: "Save" }));

    expect(await screen.findByText("Saved Kiddo Two")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "PATCH")?.body).toEqual({ display_name: "Kiddo Two", pin: null });
  });

  it("sets exceptions by label, refusing one both always and never shown", async () => {
    const user = userEvent.setup();
    const requests = renderUsers();
    await user.click(await screen.findByRole("button", { name: /Kiddo/ }));
    const dialog = screen.getByRole("dialog", { name: "Edit Kiddo" });
    await openTab(user, dialog, "Access");
    // Folded away while there are none.
    expect(within(dialog).getByText("None")).toBeInTheDocument();
    expect(within(dialog).queryByLabelText("Never show")).not.toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Add" }));

    await user.type(within(dialog).getByLabelText("Always show"), "Override: Kids{Enter}");
    await user.type(within(dialog).getByLabelText("Never show"), "Scary{Enter}");
    await user.type(within(dialog).getByLabelText("Always show"), " scary {Enter}");
    expect(within(dialog).getByText("A label can't be both always and never shown: scary.")).toBeInTheDocument();
    // The Access tab says so from the others too.
    await openTab(user, dialog, "Profile");
    expect(within(dialog).getByLabelText("Something here needs a look")).toBeInTheDocument();
    await openTab(user, dialog, "Access");
    await user.click(within(dialog).getByRole("button", { name: "Remove scary" }));
    expect(within(dialog).queryByText(/both always and never/)).not.toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(requests.find((r) => r.method === "PATCH")?.body).toEqual({ show_labels: ["Override: Kids"], hide_labels: ["Scary"] }),
    );
  });

  it("offers Always show only with an age limit, and Never show always", async () => {
    renderUsers();
    const { user, dialog } = await openAddDialog();
    expect(within(dialog).queryByRole("tab", { name: "Devices" })).not.toBeInTheDocument();
    await openTab(user, dialog, "Access");
    await user.click(within(dialog).getByRole("button", { name: "Add" }));
    expect(within(dialog).queryByLabelText("Always show")).not.toBeInTheDocument();
    expect(within(dialog).getByLabelText("Never show")).toBeInTheDocument();
  });

  it("sums up a profile's exceptions until they're opened", async () => {
    const user = userEvent.setup();
    renderUsers();
    await user.click(await screen.findByRole("button", { name: /Lee/ }));
    const dialog = screen.getByRole("dialog", { name: "Edit Lee" });
    await openTab(user, dialog, "Access");
    expect(within(dialog).getByText("Never show 2 labels")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Change" }));
    expect(within(dialog).getByRole("button", { name: "Remove Loud" })).toBeInTheDocument();
  });

  it("closes without a request when nothing changed", async () => {
    const user = userEvent.setup();
    const requests = renderUsers();
    await user.click(await screen.findByRole("button", { name: /Kiddo/ }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Save" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(requests.filter((r) => r.method === "PATCH")).toHaveLength(0);
  });

  it("keeps administrators from locking themselves out", async () => {
    const user = userEvent.setup();
    renderUsers();
    await user.click(await screen.findByRole("button", { name: /Admin/ }));
    const dialog = screen.getByRole("dialog", { name: "Edit Admin" });

    expect(within(dialog).getByRole("switch", { name: "Administrator" })).toBeDisabled();
    expect(within(dialog).getByRole("switch", { name: "Disabled" })).toBeDisabled();
    expect(within(dialog).queryByRole("button", { name: "Delete" })).not.toBeInTheDocument();
    // Their own PIN changes in Account, with the password.
    expect(within(dialog).queryByLabelText("Profile PIN")).not.toBeInTheDocument();
    expect(within(dialog).getByText(/Your own password, PIN and two-step sign-in change in Account/)).toBeInTheDocument();
  });

  it("deletes a user after confirming", async () => {
    const user = userEvent.setup();
    const requests = renderUsers();
    await user.click(await screen.findByRole("button", { name: /Kiddo/ }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Delete" }));

    const confirm = screen.getByRole("dialog", { name: "Delete user?" });
    expect(confirm).toHaveTextContent("Kiddo will lose access");
    await user.click(within(confirm).getByRole("button", { name: "Delete" }));

    expect(await screen.findByText("Deleted Kiddo")).toBeInTheDocument();
    expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/users/2")).toBe(true);
  });

  it("lists someone's apps and signs one out", async () => {
    const user = userEvent.setup();
    const requests = renderUsers();
    await user.click(await screen.findByRole("button", { name: /Kiddo/ }));
    const dialog = screen.getByRole("dialog", { name: "Edit Kiddo" });
    await openTab(user, dialog, "Devices");

    const app = (await within(dialog).findByText("Pley for Android")).closest("li");
    if (!app) throw new Error("no row for the app");
    expect(app).toHaveTextContent("Pley's own app");
    // What it may do on one line, where it was used from on the next.
    expect(within(app).getByText("Allowed by Ann · Libraries · Play")).toBeInTheDocument();
    expect(within(app).getByText(/^Android 15 · 192\.0\.2\.9 · last active /)).toBeInTheDocument();

    await user.click(within(app).getByRole("button", { name: "Sign out Pley for Android" }));
    expect(await screen.findByText("Signed out Pley for Android")).toBeInTheDocument();
    expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/users/2/apps/9")).toBe(true);
    expect(await within(dialog).findByText("No apps use this account.")).toBeInTheDocument();
  });

  it("signs out everywhere an account that is signed in only through an app", async () => {
    const user = userEvent.setup();
    const requests = renderUsers();
    await user.click(await screen.findByRole("button", { name: /Ann/ }));
    const dialog = screen.getByRole("dialog", { name: "Edit Ann" });
    await openTab(user, dialog, "Devices");
    await within(dialog).findByText("Pley for Android");
    expect(within(dialog).getByText("No signed-in devices.")).toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: "Sign out everywhere" }));
    await user.click(within(dialog).getByRole("button", { name: "Sign out everywhere" }));
    expect(await screen.findByText("Ann was signed out everywhere")).toBeInTheDocument();
    expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/users/3/devices")).toBe(true);
    expect(await within(dialog).findByText("No apps use this account.")).toBeInTheDocument();
  });

  it("asks before signing a profile out everywhere, apps too", async () => {
    const user = userEvent.setup();
    const requests = renderUsers();
    await user.click(await screen.findByRole("button", { name: /Kiddo/ }));
    const dialog = screen.getByRole("dialog", { name: "Edit Kiddo" });
    await openTab(user, dialog, "Devices");
    await within(dialog).findByText("Pley for Android");

    await user.click(within(dialog).getByRole("button", { name: "Sign out everywhere" }));
    expect(within(dialog).getByText("Sign out Kiddo everywhere?")).toBeInTheDocument();
    expect(requests.some((r) => r.method === "DELETE")).toBe(false);

    await user.click(within(dialog).getByRole("button", { name: "Sign out everywhere" }));
    expect(await screen.findByText("Kiddo was signed out everywhere")).toBeInTheDocument();
    expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/users/2/devices")).toBe(true);
    expect(await within(dialog).findByText("No apps use this account.")).toBeInTheDocument();
  });
});

describe("UsersSettings: requests", () => {
  const boss: User = { ...admin, request_access: "auto" };
  const bo: User = { ...testUser, id: 6, username: "bo", name: "Bo", is_admin: true, request_access: "auto" };
  const annAsks: User = { ...ann, two_step: false, request_access: "ask" };

  function serveRequests() {
    const requests = mockApi({
      "GET /api/users": () => [boss, annAsks, bo],
      "GET /api/ratings/systems": () => ratingSystems,
      "GET /api/libraries": () => libraries,
      "GET /api/users/1/apps": () => [],
      "GET /api/users/1/devices": () => [],
      "GET /api/users/3/apps": () => [],
      "GET /api/users/3/devices": () => [],
      "GET /api/users/6/apps": () => [],
      "GET /api/users/6/devices": () => [],
      "POST /api/users": (req) => ({ ...testUser, id: 7, is_admin: false, name: (req.body as { username: string }).username }),
      "PATCH /api/users/3": (req) => ({ ...annAsks, ...(req.body as Partial<User>) }),
      "PATCH /api/users/6": (req) => ({ ...bo, ...(req.body as Partial<User>) }),
    });
    renderWithProviders(<UsersSettings />, { user: boss });
    return requests;
  }

  it("changes whether an account may ask, and whether it waits for approval", async () => {
    const user = userEvent.setup();
    const requests = serveRequests();
    await user.click(await screen.findByRole("button", { name: /Ann/ }));
    const dialog = screen.getByRole("dialog", { name: "Edit Ann" });
    await openTab(user, dialog, "Access");
    expect(within(dialog).getByLabelText("Requests")).toHaveValue("ask");
    await user.selectOptions(within(dialog).getByLabelText("Requests"), "auto");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Saved Ann")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "PATCH")?.body).toEqual({ request_access: "auto" });
  });

  it("stops an account asking when it gets an age limit, unless asking is picked too", async () => {
    const user = userEvent.setup();
    const requests = serveRequests();
    await user.click(await screen.findByRole("button", { name: /Ann/ }));
    const dialog = screen.getByRole("dialog", { name: "Edit Ann" });
    await openTab(user, dialog, "Access");
    const select = within(dialog).getByLabelText("Requests");
    await user.selectOptions(within(dialog).getByLabelText("Age limit"), "By rating");
    await user.selectOptions(within(dialog).getByLabelText("Rating"), "TV-Y7 (age 7)");
    expect(select).toHaveValue("off");
    await user.selectOptions(select, "ask");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Saved Ann")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "PATCH")?.body).toEqual({ max_age: 7, request_access: "ask" });
  });

  it("shows administrators' requests as approved as made, and keeps what one demoted gets", async () => {
    const user = userEvent.setup();
    const requests = serveRequests();
    await user.click(await screen.findByRole("button", { name: /Bo/ }));
    const dialog = screen.getByRole("dialog", { name: "Edit Bo" });
    await openTab(user, dialog, "Access");
    expect(within(dialog).getByLabelText("Requests")).toBeDisabled();
    expect(within(dialog).getByLabelText("Requests")).toHaveValue("auto");
    await openTab(user, dialog, "Profile");
    await user.click(within(dialog).getByRole("switch", { name: "Administrator" }));
    await openTab(user, dialog, "Access");
    expect(within(dialog).getByLabelText("Requests")).toBeEnabled();
    expect(within(dialog).getByLabelText("Requests")).toHaveValue("ask");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Saved Bo")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "PATCH")?.body).toEqual({ is_admin: false, request_access: "ask" });
  });

  it("starts new accounts asking, but not managed profiles or those with an age limit", async () => {
    const requests = serveRequests();
    const { user, dialog } = await openAddDialog();
    await openTab(user, dialog, "Access");
    const select = within(dialog).getByLabelText("Requests");
    expect(select).toHaveValue("ask");
    await waitFor(() => expect(within(dialog).getByRole("option", { name: "By rating" })).toBeEnabled());
    await user.selectOptions(within(dialog).getByLabelText("Age limit"), "By rating");
    await user.selectOptions(within(dialog).getByLabelText("Rating"), "TV-Y7 (age 7)");
    expect(select).toHaveValue("off");
    expect(within(dialog).getByText(/leaves out titles above the age limit/)).toBeInTheDocument();
    await user.selectOptions(within(dialog).getByLabelText("Age limit"), "No limit");
    await user.click(within(dialog).getByRole("button", { name: /Managed profile/ }));
    expect(select).toHaveValue("off");

    // Chosen, it stays.
    await user.selectOptions(select, "ask");
    await openTab(user, dialog, "Profile");
    await user.type(within(dialog).getByLabelText("Username"), "little");
    await user.click(within(dialog).getByRole("button", { name: "Add user" }));
    expect(await screen.findByText("Added little")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "POST")?.body).toMatchObject({ managed: true, request_access: "ask" });
  });
});
