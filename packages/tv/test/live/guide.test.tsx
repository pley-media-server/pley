import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LiveChannel, LiveGuide as Guide, LiveProgram } from "../../src/api/types";
import { FocusManager } from "../../src/ui/focus";
import { Input } from "../../src/ui/input";
import { UiContext, type Ui } from "../../src/ui/context";
import { ChannelPages } from "../../src/live/pages";
import { LiveGuide } from "../../src/ui/screens/live-guide";
import type { Entry } from "../../src/ui/shell/navigator";

/** A half hour, so the guide's columns line up with the programs. */
const BASE = 1_800_000_000;

function program(channel: number, start: number, stop: number, title: string): LiveProgram {
  const p = { channel_id: channel, start: BASE + start, stop: BASE + stop, title, subtitle: null, description: null, categories: [], image: null };
  return { ...p, season: null, episode: null, is_new: false, is_live: false, rating: null };
}

function channel(id: number, hasGuide = true): LiveChannel {
  return { id, source_id: 1, name: `Ch ${id}`, number: String(id), group: "", logo: null, favorite: false, has_guide: hasGuide, now: null, next: null };
}

const PROGRAMS: Record<string, LiveProgram[]> = {
  "1": [program(1, 0, 1800, "A"), program(1, 1800, 5400, "B")],
  "2": [program(2, 0, 3600, "C"), program(2, 3600, 7200, "D")],
};

async function setup() {
  document.body.innerHTML = "";
  const root = document.createElement("div");
  document.body.appendChild(root);
  const focus = new FocusManager();
  focus.setPage(root);
  const input = new Input(focus);
  const asked: string[] = [];
  const channels = [channel(1), channel(2), channel(3, false)];
  const api = {
    liveGuide: (from: number, to: number, ids: number[]): Promise<Guide> => {
      asked.push(`${from - BASE}..${to - BASE}: ${ids.join(",")}`);
      return Promise.resolve({ from, to, programs: PROGRAMS });
    },
    liveBrowse: () => Promise.resolve({ total: channels.length, offset: 0, channels }),
  };
  const pages = new ChannelPages(api, { kind: "all" });
  const ui = { focus, input, api, scale: 2, claim: () => undefined, toRail: () => false } as unknown as Ui;
  const entry: Entry = { key: { name: "live" }, id: 1, memory: null, state: {} };
  const watched: number[] = [];
  const details: (string | null)[] = [];
  let categories = 0;
  const page = (jump: { index: number; n: number } | null) => (
    <UiContext.Provider value={ui}>
      <LiveGuide
        entry={entry}
        pages={pages}
        category={{ kind: "all" }}
        now={BASE + 900}
        minute={0}
        jump={jump}
        hours24
        onWatch={(c) => watched.push(c.id)}
        onDetails={(_c, p) => details.push(p ? p.title : null)}
        onCategories={() => categories++}
        focusAbove={() => false}
        version={0}
      />
    </UiContext.Provider>
  );
  act(() => render(page(null), root));
  // The page of channels, then their guide.
  for (let i = 0; i < 3; i++) {
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
  }
  const el = (fid: string) => root.querySelector<HTMLElement>(`[data-fid="${fid}"]`);
  const press = (key: "left" | "right" | "up" | "down" | "ok") =>
    act(() => {
      input.dispatch(key, "down", false);
      input.dispatch(key, "up", false);
    });
  const at = () => focus.current?.getAttribute("data-fid");
  return { focus, el, press, at, asked, watched, details, entry, categories: () => categories, jump: (index: number) => act(() => render(page({ index, n: Date.now() }), root)) };
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime((BASE + 900) * 1000);
});
afterEach(() => {
  vi.useRealTimers();
});

describe("the guide", () => {
  it("asks for its chunks from half an hour before now", async () => {
    const { asked } = await setup();
    // Only the channels shown are asked for.
    expect(asked[0]).toBe("-1800..19800: 1,2,3");
  });

  it("opens the categories from the channels' column, and brings a channel asked for into view", async () => {
    const { focus, el, press, at, categories, jump } = await setup();
    act(() => void focus.focus(el("guide/1")));
    press("left");
    expect(categories()).toBe(1);
    jump(2);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(at()).toMatch(/^guide\/3\//);
  });

  it("moves program by program, and keeps the time focus is at between channels", async () => {
    const { focus, el, press, at } = await setup();
    act(() => void focus.focus(el(`guide/1/${BASE}`)));
    press("right");
    expect(at()).toBe(`guide/1/${BASE + 1800}`);
    // B began at 0:30: in Ch 2, what is on then (C, from 0:00).
    press("down");
    expect(at()).toBe(`guide/2/${BASE}`);
    // No guide: the channel's one cell.
    press("down");
    expect(at()).toMatch(/^guide\/3\/gap\//);
    // Left of the first cell is the channel, and Up stays in the channels.
    press("left");
    expect(at()).toBe("guide/3");
    press("up");
    expect(at()).toBe("guide/2");
  });

  it("watches what's on with OK, and shows a later program's details", async () => {
    const { focus, el, press, watched, details } = await setup();
    act(() => void focus.focus(el(`guide/1/${BASE}`)));
    press("ok");
    expect(watched).toEqual([1]);
    act(() => void focus.focus(el(`guide/1/${BASE + 1800}`)));
    press("ok");
    expect(details).toEqual(["B"]);
    act(() => void focus.focus(el("guide/2")));
    press("ok");
    expect(watched).toEqual([1, 2]);
  });

  it("remembers where it was for Back", async () => {
    const { focus, el, press, entry } = await setup();
    act(() => void focus.focus(el(`guide/1/${BASE}`)));
    press("down");
    expect(entry.state.guideRow).toBe(1);
    expect(entry.state.guideCursor).toBe(BASE);
  });
});
