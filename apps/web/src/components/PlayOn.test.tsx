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

  it("offers Cast devices for a movie or an episode alone, marked as such", async () => {
    const castDevice = makeReceiver({
      receiver_id: "cast:9f8e",
      device_id: "cast:9f8e",
      name: "Bedroom TV",
      casts: ["item"],
      protocol: "cast",
    });
    serve(undefined, [tv, castDevice]);
    const user = userEvent.setup();
    renderWithProviders(
      <>
        <PlayOn kind="item" video cast={() => ({ item_id: 7 })} />
        <PlayOn kind="item" label="Shuffle on…" cast={() => ({ playlist_id: 2 })} />
      </>,
    );

    await user.click(await screen.findByRole("button", { name: "Play on…" }));
    const items = within(screen.getByRole("menu", { name: "Play on…" })).getAllByRole("menuitem");
    expect(items.map((i) => i.getAttribute("aria-label"))).toEqual(["Living room TV", "Bedroom TV"]);
    expect(items[1]).toHaveAccessibleDescription("Google Cast");
    await user.keyboard("{Escape}");

    await user.click(screen.getByRole("button", { name: "Shuffle on…" }));
    expect(within(screen.getByRole("menu", { name: "Shuffle on…" })).getAllByRole("menuitem").map((i) => i.getAttribute("aria-label"))).toEqual([
      "Living room TV",
    ]);
  });

  it("says why a Cast device didn't play it", async () => {
    const castDevice = makeReceiver({ receiver_id: "cast:9f8e", device_id: "cast:9f8e", name: "Bedroom TV", casts: ["item"], protocol: "cast" });
    mockApi({
      "GET /api/sessions": () => [],
      "GET /api/remote/devices": () => [castDevice],
      "POST /api/remote/devices/cast%3A9f8e/cast": () => apiError(409, "cast_unreachable", "Pley doesn't listen on an address the device can reach"),
    });
    renderWithProviders(<PlayOn kind="item" video cast={() => ({ item_id: 7 })} />);
    await choose("Play on…", "Bedroom TV");
    expect(await screen.findByText("Bedroom TV can't reach this server")).toBeInTheDocument();
  });

  it("says where a cast is starting, and that it plays, however long the device takes", async () => {
    const castDevice = makeReceiver({ receiver_id: "cast:9f8e", device_id: "cast:9f8e", name: "Bedroom TV", casts: ["item"], protocol: "cast" });
    let answer = () => {};
    mockApi({
      "GET /api/sessions": () => [],
      "GET /api/remote/devices": () => [castDevice],
      "POST /api/remote/devices/cast%3A9f8e/cast": () =>
        new Promise<undefined>((done) => {
          answer = () => done(undefined);
        }),
    });
    const { rerender } = renderWithProviders(<PlayOn kind="item" video cast={() => ({ item_id: 7 })} />);
    await choose("Play on…", "Bedroom TV");
    const starting = await screen.findByRole("button", { name: "Starting on Bedroom TV…" });
    expect(starting).toBeDisabled();

    // The page drops the button (a player closing) before the device answers.
    rerender(<></>);
    act(() => answer());
    expect(await screen.findByText("Playing on Bedroom TV")).toBeInTheDocument();
  });

  it("pairs with an AirPlay device that asks for the code it shows, sent with OK when its length isn't known, then plays it there", async () => {
    const appleTv = makeReceiver({ receiver_id: "airplay:a1b2c3d4e5f6", device_id: "airplay:a1b2c3d4e5f6", name: "Den Apple TV", casts: ["item"], protocol: "airplay" });
    let paired = false;
    const requests = mockApi({
      "GET /api/sessions": () => [],
      "GET /api/remote/devices": () => [appleTv],
      "POST /api/remote/devices/airplay%3Aa1b2c3d4e5f6/cast": () => (paired ? undefined : apiError(409, "pairing_needed", "the device wants a PIN")),
      "POST /api/remote/devices/airplay%3Aa1b2c3d4e5f6/pairing": (req) => {
        const pin = (req.body as { pin?: string }).pin;
        if (pin === undefined) return undefined;
        if (pin !== "4821") return apiError(403, "wrong_pin", "that isn't the PIN the device shows");
        paired = true;
        return undefined;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<PlayOn kind="item" video cast={() => ({ item_id: 7 })} />);

    await user.click(await screen.findByRole("button", { name: "Play on…" }));
    const item = within(screen.getByRole("menu", { name: "Play on…" })).getByRole("menuitem", { name: "Den Apple TV" });
    expect(item).toHaveAccessibleDescription("AirPlay");
    await user.click(item);
    const asking = await screen.findByRole("dialog", { name: "Pair with Den Apple TV" });
    expect(asking).toHaveAccessibleDescription("Type the code shown on the TV. Everyone in your home can then play on it.");
    await user.keyboard("1111{Enter}");
    await waitFor(() => expect(within(asking).getByRole("alert")).toHaveTextContent("That isn't the code Den Apple TV shows"));
    await user.keyboard("4821{Enter}");
    expect(await screen.findByText("Playing on Den Apple TV")).toBeInTheDocument();
    expect(asking).not.toBeInTheDocument();
    const pairings = requests.filter((r) => r.path.endsWith("/pairing")).map((r) => r.body);
    expect(pairings).toEqual([{}, { pin: "1111" }, { pin: "4821" }]);
    expect(casts(requests)).toHaveLength(2);
  });

  it("sends an Apple TV's code at its fourth digit, and has the TV show it again", async () => {
    const appleTv = makeReceiver({ receiver_id: "airplay:a1b2c3d4e5f6", device_id: "airplay:a1b2c3d4e5f6", name: "Den Apple TV", casts: ["item"], protocol: "airplay" });
    let paired = false;
    const requests = mockApi({
      "GET /api/sessions": () => [],
      "GET /api/remote/devices": () => [appleTv],
      "POST /api/remote/devices/airplay%3Aa1b2c3d4e5f6/cast": () => (paired ? undefined : apiError(409, "pairing_needed", "the device wants a PIN")),
      "POST /api/remote/devices/airplay%3Aa1b2c3d4e5f6/pairing": (req) => {
        const pin = (req.body as { pin?: string }).pin;
        if (pin === undefined) return { digits: 4 };
        if (pin !== "4821") return apiError(403, "wrong_pin", "that isn't the PIN the device shows");
        paired = true;
        return undefined;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<PlayOn kind="item" video cast={() => ({ item_id: 7 })} />);

    await user.click(await screen.findByRole("button", { name: "Play on…" }));
    await user.click(within(screen.getByRole("menu", { name: "Play on…" })).getByRole("menuitem", { name: "Den Apple TV" }));
    const asking = await screen.findByRole("dialog", { name: "Pair with Den Apple TV" });
    await user.keyboard("12");
    await user.click(within(asking).getByRole("button", { name: "No code on the TV? Show it again" }));
    await waitFor(() => expect(requests.filter((r) => r.path.endsWith("/pairing"))).toHaveLength(2));
    expect(await within(asking).findByLabelText("0 digits entered")).toBeInTheDocument();
    await user.keyboard("4821");
    expect(await screen.findByText("Playing on Den Apple TV")).toBeInTheDocument();
    const pairings = requests.filter((r) => r.path.endsWith("/pairing")).map((r) => r.body);
    expect(pairings).toEqual([{}, {}, { pin: "4821" }]);
  });

  it("stays busy while an AirPlay device is asked for its PIN, and says to pick it again once it closed", async () => {
    const appleTv = makeReceiver({ receiver_id: "airplay:a1b2c3d4e5f6", device_id: "airplay:a1b2c3d4e5f6", name: "Den Apple TV", casts: ["item"], protocol: "airplay" });
    let showPin = () => {};
    let answerCast: (r: Response) => void = () => {};
    let waits = false;
    const requests = mockApi({
      "GET /api/sessions": () => [],
      "GET /api/remote/devices": () => [appleTv],
      "POST /api/remote/devices/airplay%3Aa1b2c3d4e5f6/cast": () =>
        waits
          ? new Promise<Response>((done) => {
              answerCast = done;
            })
          : apiError(409, "pairing_needed", "the device wants a PIN"),
      "POST /api/remote/devices/airplay%3Aa1b2c3d4e5f6/pairing": () =>
        new Promise<undefined>((done) => {
          showPin = () => done(undefined);
        }),
    });
    const { rerender } = renderWithProviders(<PlayOn kind="item" video cast={() => ({ item_id: 7 })} />);
    await choose("Play on…", "Den Apple TV");
    // The device is asked to show its PIN: the button can't start another.
    await waitFor(() => expect(requests.filter((r) => r.path.endsWith("/pairing"))).toHaveLength(1));
    expect(screen.getByRole("button", { name: "Starting on Den Apple TV…" })).toBeDisabled();
    act(() => showPin());
    expect(await screen.findByRole("dialog", { name: "Pair with Den Apple TV" })).toBeInTheDocument();
    await userEvent.setup().keyboard("{Escape}");

    // Gone from the page before the device asks for pairing: no PIN is
    // asked for that no one could type.
    waits = true;
    await choose("Play on…", "Den Apple TV");
    await waitFor(() => expect(casts(requests)).toHaveLength(2));
    rerender(<></>);
    act(() => answerCast(apiError(409, "pairing_needed", "the device wants a PIN")));
    expect(await screen.findByText("Den Apple TV wants pairing first: choose it in Play on… again")).toBeInTheDocument();
    expect(requests.filter((r) => r.path.endsWith("/pairing"))).toHaveLength(1);
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
    await waitFor(() => expect(requests.filter((r) => r.path === "/api/remote/devices?household=1&nearby=1")).toHaveLength(2));
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
      expect(casts(requests)).toEqual([["/api/remote/devices/attic/cast", { item_id: 7, profile_id: 2, device_id: "app-6" }]]);
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
        ["/api/remote/devices/attic/cast", { item_id: 7, shuffle: true, profile_id: 2, device_id: "app-6", pin: "1111" }],
        ["/api/remote/devices/attic/cast", { item_id: 7, shuffle: true, profile_id: 2, device_id: "app-6", pin: "1234" }],
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
        { item_id: 7, profile_id: 2, device_id: "app-6" },
        { item_id: 7, profile_id: 2, device_id: "app-6", pin: "1234" },
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

    it("leaves the pad for what isn't the PIN's refusal: a cast not allowed, no answer", async () => {
      let answer: () => unknown = () => apiError(403, "not_allowed", "you can't cast to this device");
      serveAttic(() => answer());
      renderWithProviders(<PlayOn kind="item" cast={() => ({ item_id: 7 })} />, { user: withPin });
      const user = userEvent.setup();

      await choose("Play on…", "Attic TV");
      await screen.findByRole("dialog", { name: "Play on Attic TV" });
      await user.keyboard("1234{Enter}");
      expect(await screen.findByText("You can't cast to this device")).toBeInTheDocument();
      expect(screen.queryByRole("dialog")).toBeNull();
      expect(screen.getByRole("button", { name: "Play on…" })).toHaveFocus();
      act(() => screen.getByRole("button", { name: "Dismiss" }).click());

      answer = () => {
        throw new TypeError("Failed to fetch");
      };
      await choose("Play on…", "Attic TV");
      await screen.findByRole("dialog", { name: "Play on Attic TV" });
      await user.keyboard("1234{Enter}");
      expect(await screen.findByText("Can't reach the server")).toBeInTheDocument();
      expect(screen.queryByRole("dialog")).toBeNull();
    });

    it("keeps no PIN in the mutation cache once a cast is done with", async () => {
      serveAttic();
      const { client } = renderWithProviders(<PlayOn kind="item" cast={() => ({ item_id: 7 })} />, { user: withPin });
      const kept = () => JSON.stringify(client.getMutationCache().getAll().map((m) => m.state.variables));
      const user = userEvent.setup();

      await choose("Play on…", "Attic TV");
      const asking = await screen.findByRole("dialog", { name: "Play on Attic TV" });
      await user.keyboard("1111{Enter}");
      await waitFor(() => expect(within(asking).getByRole("alert")).toHaveTextContent("Wrong PIN"));
      await waitFor(() => expect(kept()).not.toContain("1111"));

      await user.keyboard("1234{Enter}");
      expect(await screen.findByText("Playing on Attic TV")).toBeInTheDocument();
      await waitFor(() => expect(kept()).not.toContain("1234"));
      expect(client.getMutationCache().getAll()).toEqual([]);
    });
  });
});
