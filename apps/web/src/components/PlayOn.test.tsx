import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { CastRequest, PlayingSession, RemoteDevice } from "@/api/types";
import { makeItem, makeReceiver, makeSession } from "@/test/fixtures";
import { apiError, mockApi, renderWithProviders, testUser, type FakeRequest } from "@/test/render";
import { PlayOn } from "./PlayOn";
import { RemoteButton } from "./Remote";

/** A TV playing an episode, which takes titles and channels. */
const tv: RemoteDevice = makeReceiver();
/** A phone playing nothing, which takes titles only (no Live TV there). */
const phone: RemoteDevice = makeReceiver({ receiver_id: "phone-1", device_id: "app-3", name: "Phone", kind: "phone", casts: ["item"] });
const playing: PlayingSession = makeSession({
  session_id: "tv",
  item: makeItem({ id: 9, kind: "episode", title: "Pilot", grandparent_title: "Test Show", parent_index: 1, index: 1 }),
});

/** The server: the user's devices, what they play, and how a cast to each is answered. */
function serve(cast: (req: FakeRequest) => unknown = () => undefined, receivers: RemoteDevice[] = [tv, phone]): FakeRequest[] {
  return mockApi({
    "GET /api/sessions": () => [playing],
    "GET /api/remote/devices": () => receivers,
    "POST /api/remote/devices/tv-1/cast": cast,
    "POST /api/remote/devices/phone-1/cast": cast,
  });
}

const casts = (requests: FakeRequest[]) =>
  requests.filter((r) => r.method === "POST" && r.path.endsWith("/cast")).map((r) => [r.path, r.body as CastRequest]);

async function choose(label: string, device: string) {
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: label }));
  await user.click(within(screen.getByRole("menu", { name: label })).getByRole("menuitem", { name: device }));
}

describe("PlayOn", () => {
  it("offers the devices that play what the page shows, as the remote lists them", async () => {
    serve();
    const user = userEvent.setup();
    renderWithProviders(
      <>
        <PlayOn kind="item" cast={() => ({ item_id: 7 })} />
        <PlayOn kind="live" label="Watch on…" cast={() => ({ channel_id: 3 })} />
      </>,
    );

    await user.click(await screen.findByRole("button", { name: "Play on…" }));
    const items = within(screen.getByRole("menu", { name: "Play on…" })).getAllByRole("menuitem");
    expect(items.map((i) => i.getAttribute("aria-label"))).toEqual(["Living room TV", "Phone"]);
    expect(items[0]).toHaveAccessibleDescription("Test Show — S1 · E1 — Pilot");
    expect(items[1]).toHaveAccessibleDescription("Ready to play");

    await user.click(screen.getByRole("button", { name: "Watch on…" }));
    expect(within(screen.getByRole("menu", { name: "Watch on…" })).getAllByRole("menuitem").map((i) => i.getAttribute("aria-label"))).toEqual([
      "Living room TV",
    ]);
  });

  it("isn't there while no device takes it", async () => {
    serve(undefined, [phone]);
    const { client } = renderWithProviders(<PlayOn kind="live" label="Watch on…" cast={() => ({ channel_id: 3 })} />);
    await waitFor(() => expect(client.getQueryData(["remote", "devices"])).toEqual([phone]));
    expect(screen.queryByRole("button", { name: "Watch on…" })).toBeNull();
  });

  it("plays it there, says so, and opens the remote on that device", async () => {
    const requests = serve();
    renderWithProviders(
      <>
        <RemoteButton />
        <PlayOn kind="item" label="Shuffle on…" cast={() => ({ item_id: 7, shuffle: true })} />
      </>,
    );

    await choose("Shuffle on…", "Phone");
    expect(casts(requests)).toEqual([["/api/remote/devices/phone-1/cast", { item_id: 7, shuffle: true }]]);
    expect(await screen.findByText("Playing on Phone")).toBeInTheDocument();
    const remote = await screen.findByRole("dialog", { name: "Your devices" });
    // Its controls show once its player lists what it plays.
    expect(within(remote).getAllByRole("listitem").map((i) => i.textContent)).toContain("PhoneReady to play");
    expect(within(remote).queryByRole("group")).toBeNull();
    expect(screen.getByRole("button", { name: /^Remote/ })).toHaveFocus();
  });

  it("says why a device didn't take it, and asks again which devices there are when one went", async () => {
    let answer: Response = apiError(404, "not_found", "no such device");
    const requests = serve(() => answer);
    renderWithProviders(<PlayOn kind="item" cast={() => ({ item_id: 7 })} />);

    const said = async (device: string, words: string) => {
      await choose("Play on…", device);
      expect(await screen.findByRole("alert")).toHaveTextContent(words);
      // The next toast, on its own.
      act(() => screen.getByRole("button", { name: "Dismiss" }).click());
    };
    await said("Phone", "Phone isn't on any more");
    await waitFor(() => expect(requests.filter((r) => r.path === "/api/remote/devices")).toHaveLength(2));
    answer = apiError(409, "not_castable", "this device doesn't play that");
    await said("Living room TV", "Living room TV can't play that");
    answer = apiError(429, "too_many_requests", "slow down");
    await said("Living room TV", "Too many at once: try again in a moment");
    answer = apiError(400, "bad_request", "only movies, episodes, shows, seasons, tracks, albums and artists can be cast");
    await said("Living room TV", "Only movies, episodes, shows, seasons, tracks, albums and artists can be cast");
  });

  describe("to a device on another profile of the household", () => {
    /** A TV Sam's profile is on. */
    const attic: RemoteDevice = makeReceiver({ receiver_id: "attic", device_id: "app-6", name: "Attic TV", profile: { id: 2, name: "Sam" } });
    const withPin = { ...testUser, has_pin: true };

    /** The server: the TVs, and the user's PIN, 1234, when one is given; `cast` may answer otherwise. */
    function serveAttic(cast?: (req: FakeRequest) => unknown): FakeRequest[] {
      const pinned = (pin?: string) => (pin === undefined || pin === "1234" ? undefined : apiError(403, "wrong_pin", "wrong PIN"));
      return mockApi({
        "GET /api/sessions": () => [playing],
        "GET /api/remote/devices": () => [tv, attic],
        "POST /api/remote/devices/attic/cast": (req) => (cast ? cast(req) : pinned((req.body as CastRequest).pin)),
      });
    }

    it("lists it by its profile, after the user's own, and plays there as the user, naming the profile it was on", async () => {
      const requests = serveAttic();
      renderWithProviders(<PlayOn kind="item" cast={() => ({ item_id: 7 })} />);

      const user = userEvent.setup();
      await user.click(await screen.findByRole("button", { name: "Play on…" }));
      const items = within(screen.getByRole("menu", { name: "Play on…" })).getAllByRole("menuitem");
      expect(items.map((i) => i.getAttribute("aria-label"))).toEqual(["Living room TV", "Attic TV"]);
      expect(items[1]).toHaveAccessibleDescription("On Sam's profile");

      await user.click(items[1]!);
      expect(casts(requests)).toEqual([["/api/remote/devices/attic/cast", { item_id: 7, profile_id: 2 }]]);
      expect(await screen.findByText("Playing on Attic TV")).toBeInTheDocument();
    });

    it("asks for the user's PIN first when their profile has one, and lets them try again", async () => {
      const requests = serveAttic();
      renderWithProviders(<PlayOn kind="item" label="Shuffle on…" cast={() => ({ item_id: 7, shuffle: true })} />, { user: withPin });

      await choose("Shuffle on…", "Attic TV");
      const asking = await screen.findByRole("dialog", { name: "Shuffle on Attic TV" });
      expect(asking).toHaveAccessibleDescription("Attic TV is on Sam's profile: it switches to yours first.");
      expect(asking).toHaveFocus();
      expect(casts(requests)).toEqual([]);

      const user = userEvent.setup();
      await user.keyboard("1111{Enter}");
      await waitFor(() => expect(within(asking).getByRole("alert")).toHaveTextContent("Wrong PIN"));
      // The pad starts over for the next try.
      expect(within(asking).getByLabelText("0 digits entered")).toBeInTheDocument();

      await user.click(within(asking).getByRole("button", { name: "1" }));
      await user.click(within(asking).getByRole("button", { name: "2" }));
      await user.keyboard("34");
      await user.click(within(asking).getByRole("button", { name: "Shuffle on Attic TV" }));
      expect(await screen.findByText("Playing on Attic TV")).toBeInTheDocument();
      expect(screen.queryByRole("dialog")).toBeNull();
      expect(casts(requests)).toEqual([
        ["/api/remote/devices/attic/cast", { item_id: 7, shuffle: true, profile_id: 2, pin: "1111" }],
        ["/api/remote/devices/attic/cast", { item_id: 7, shuffle: true, profile_id: 2, pin: "1234" }],
      ]);
    });

    it("asks for it when the server says the profile has one since it was loaded", async () => {
      let first = true;
      const requests = serveAttic((req) => {
        if (!first) return undefined;
        first = false;
        return (req.body as CastRequest).pin ? undefined : apiError(403, "pin_needed", "this profile is protected by a PIN");
      });
      renderWithProviders(<PlayOn kind="item" cast={() => ({ item_id: 7 })} />);

      await choose("Play on…", "Attic TV");
      const asking = await screen.findByRole("dialog", { name: "Play on Attic TV" });
      expect(screen.queryByText("Playing on Attic TV")).toBeNull();
      await userEvent.setup().keyboard("1234{Enter}");
      expect(await screen.findByText("Playing on Attic TV")).toBeInTheDocument();
      expect(asking).not.toBeInTheDocument();
      expect(casts(requests).map(([, body]) => body)).toEqual([
        { item_id: 7, profile_id: 2 },
        { item_id: 7, profile_id: 2, pin: "1234" },
      ]);
    });

    it("says when the PIN is locked or tried too often, and stops the keys once it is locked", async () => {
      let answer = apiError(429, "too_many_requests", "too many attempts: try again in 15 minutes");
      serveAttic(() => answer);
      renderWithProviders(<PlayOn kind="item" cast={() => ({ item_id: 7 })} />, { user: withPin });

      await choose("Play on…", "Attic TV");
      const asking = await screen.findByRole("dialog", { name: "Play on Attic TV" });
      const user = userEvent.setup();
      await user.keyboard("1234{Enter}");
      await waitFor(() => expect(within(asking).getByRole("alert")).toHaveTextContent("Too many attempts: try again in 15 minutes"));
      expect(within(asking).getByRole("button", { name: "1" })).toBeEnabled();

      answer = apiError(403, "pin_locked", "this profile's PIN is locked after too many wrong tries");
      await user.keyboard("1234{Enter}");
      await waitFor(() => expect(within(asking).getByRole("alert")).toHaveTextContent("This profile's PIN is locked after too many wrong tries"));
      expect(within(asking).getByRole("button", { name: "1" })).toBeDisabled();
    });

    it("goes back to the menu's button on Cancel or Escape, having sent nothing", async () => {
      const requests = serveAttic();
      renderWithProviders(<PlayOn kind="item" cast={() => ({ item_id: 7 })} />, { user: withPin });
      const user = userEvent.setup();

      await choose("Play on…", "Attic TV");
      await user.click(within(await screen.findByRole("dialog", { name: "Play on Attic TV" })).getByRole("button", { name: "Cancel" }));
      expect(screen.queryByRole("dialog")).toBeNull();
      expect(screen.getByRole("button", { name: "Play on…" })).toHaveFocus();

      await choose("Play on…", "Attic TV");
      await screen.findByRole("dialog", { name: "Play on Attic TV" });
      await user.keyboard("12{Escape}");
      expect(screen.queryByRole("dialog")).toBeNull();
      expect(screen.getByRole("button", { name: "Play on…" })).toHaveFocus();
      expect(casts(requests)).toEqual([]);
    });

    it("says why a device didn't take it once the PIN was right", async () => {
      serveAttic(() => apiError(404, "not_found", "no such device"));
      renderWithProviders(<PlayOn kind="item" cast={() => ({ item_id: 7 })} />, { user: withPin });

      await choose("Play on…", "Attic TV");
      await screen.findByRole("dialog", { name: "Play on Attic TV" });
      await userEvent.setup().keyboard("1234{Enter}");
      expect(await screen.findByText("Attic TV isn't on any more")).toBeInTheDocument();
      expect(screen.queryByRole("dialog")).toBeNull();
    });
  });
});
