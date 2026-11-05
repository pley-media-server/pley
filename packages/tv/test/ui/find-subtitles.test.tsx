import { signal } from "@preact/signals";
import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { ApiError } from "../../src/api/client";
import type { SubtitleResult, User } from "../../src/api/types";
import { formatNumber, setLanguage } from "../../src/i18n";
import type { PlaybackController, PlayerState } from "../../src/player/controller";
import type { AppState } from "../../src/state";
import { UiContext, type Ui } from "../../src/ui/context";
import { FocusManager } from "../../src/ui/focus";
import { Input } from "../../src/ui/input";
import type { Key } from "../../src/ui/keys";
import { forgetSubtitleSearch, PlayerPanels, type PlayerPanel } from "../../src/ui/player/panels";
import { AppContext } from "../../src/ui/screens/common";

/** The player's Subtitles panel and Find subtitles…, on a controller that answers as the test says. */

let root: HTMLElement | null = null;
let detach: (() => void) | null = null;

const result = (over: Partial<SubtitleResult>): SubtitleResult => ({
  id: "opensubtitles:1",
  provider: "opensubtitles",
  language: "en",
  name: "Heat.1995.1080p.BluRay",
  format: "srt",
  hearing_impaired: false,
  forced: false,
  downloads: 1234,
  matched: "hash",
  ...over,
});

const flush = () => act(() => new Promise<void>((r) => setTimeout(r, 0)));

function setup(opts: { found: (language: string | null) => Promise<SubtitleResult[]>; admin?: boolean; panel?: PlayerPanel }) {
  document.body.innerHTML = "";
  root = document.createElement("div");
  document.body.appendChild(root);
  const focus = new FocusManager();
  focus.setPage(root);
  const input = new Input(focus);
  detach = input.attach(document);
  const ui = {
    focus,
    input,
    claim: (el: HTMLElement, _fid: string | null, initial: boolean) => {
      if (initial && focus.inScope(el)) focus.focus(el);
    },
  } as unknown as Ui;
  const me = { id: 1, subtitle_language: "eng", is_admin: opts.admin ?? false } as User;
  const app = { me: signal(me) } as unknown as AppState;
  const asked: (string | null)[] = [];
  const added: string[] = [];
  const events: string[] = [];
  const controller = {
    findSubtitles: (language: string | null) => {
      asked.push(language);
      return opts.found(language);
    },
    addFoundSubtitle: (r: SubtitleResult) => {
      added.push(r.id);
      return Promise.resolve();
    },
    selectSubtitle: () => undefined,
    setSubtitleOffset: () => undefined,
  } as unknown as PlaybackController;
  const state = { subtitles: [], subtitleSync: false, subtitleOffsetMs: 0 } as unknown as PlayerState;
  let panel: PlayerPanel = opts.panel ?? "subtitles";
  const draw = () =>
    act(() =>
      render(
        <AppContext.Provider value={app}>
          <UiContext.Provider value={ui}>
            <PlayerPanels
              panel={panel}
              state={state}
              controller={controller}
              positionMs={0}
              onClose={() => events.push("close")}
              onOpen={(next) => {
                events.push(`open ${next}`);
                panel = next;
                draw();
              }}
              stats={() => null}
              engineName="Fake"
            />
          </UiContext.Provider>
        </AppContext.Provider>,
        root!,
      ),
    );
  draw();
  const press = (key: Key) => {
    act(() => input.dispatch(key, "down", false));
    act(() => input.dispatch(key, "up", false));
  };
  const rows = () => Array.from(root!.querySelectorAll(".panel-row")).map((r) => r.textContent);
  const notes = () => Array.from(root!.querySelectorAll(".panel-note")).map((r) => r.textContent);
  return { root: root!, focus, press, rows, notes, asked, added, events };
}

beforeEach(() => forgetSubtitleSearch());
afterEach(() => {
  setLanguage("en");
  detach?.();
  detach = null;
  if (root) act(() => render(null, root!));
  root = null;
});

describe("Find subtitles…", () => {
  it("is in the Subtitles panel, even without subtitles, and opens the search", () => {
    const t = setup({ found: () => new Promise(() => undefined) });
    expect(t.rows()).toEqual(["Find subtitles…"]);
    expect(t.focus.current?.textContent).toBe("Find subtitles…");
    t.press("ok");
    expect(t.events).toEqual(["open find"]);
  });

  it("lists what the server finds in the profile's language, and adds the one picked", async () => {
    const t = setup({
      panel: "find",
      found: () => Promise.resolve([result({}), result({ id: "opensubtitles:2", name: "Heat.DVDRip", matched: "title", hearing_impaired: true, downloads: 1 })]),
    });
    expect(t.notes()).toEqual(["Searching…"]);
    await flush();
    expect(t.asked).toEqual(["eng"]);
    expect(t.rows()).toEqual([
      "LanguageEnglish",
      "Heat.1995.1080p.BluRayMade for this file · English · SRT · 1,234 downloads",
      "Heat.DVDRipEnglish · SRT · SDH · 1 download",
    ]);
    expect(t.focus.current?.textContent).toContain("Heat.1995.1080p.BluRay");
    t.press("ok");
    await flush();
    expect(t.added).toEqual(["opensubtitles:1"]);
    expect(t.events).toEqual(["close"]);
  });

  it("searches again in a language picked from the list", async () => {
    const t = setup({ panel: "find", found: () => Promise.resolve([]) });
    await flush();
    expect(t.notes()).toEqual(["No English subtitles found for this video."]);
    expect(t.focus.current?.textContent).toBe("LanguageEnglish");
    t.press("ok");
    const french = Array.from(t.root.querySelectorAll<HTMLElement>(".panel-row")).filter((r) => r.textContent === "French")[0]!;
    act(() => {
      t.focus.focus(french);
    });
    t.press("ok");
    await flush();
    expect(t.asked).toEqual(["eng", "fre"]);
    expect(t.notes()).toEqual(["No French subtitles found for this video."]);
  });

  it("says it isn't set up, where an administrator sets it up, and is then left out for others", async () => {
    const unset = () => Promise.reject(new ApiError(503, "unavailable", "no key", { code: "unavailable", setup: false }));
    const admin = setup({ panel: "find", admin: true, found: unset });
    await flush();
    expect(admin.notes()).toEqual(["Subtitle search isn't set up on this server. Set it up in Pley on the web: Settings → Server → Subtitles."]);
    act(() => render(null, admin.root));

    const other = setup({ found: unset });
    expect(other.rows()).toEqual([]);
    act(() => render(null, other.root));
    const again = setup({ admin: true, found: unset });
    expect(again.rows()).toEqual(["Find subtitles…Subtitle search isn't set up"]);
  });

  it("draws its words again when the language changes", async () => {
    const t = setup({ panel: "find", found: () => Promise.resolve([result({})]) });
    await flush();
    expect(t.rows()[1]).toContain("Made for this file");
    act(() => setLanguage("fr"));
    expect(t.root.querySelector(".panel-title")?.textContent).toBe("Trouver des sous-titres");
    expect(t.rows()[0]).toMatch(/^Langue/);
    // The count as French writes it, whatever the engine's space between thousands.
    expect(t.rows()[1]).toContain("Conçus pour ce fichier · ");
    expect(t.rows()[1]).toContain(`${formatNumber(1234)} téléchargements`);
    act(() => setLanguage("es"));
    expect(t.rows()[1]).toContain(`${formatNumber(1234)} descargas`);
  });

  it("is left out on a server too old to search", async () => {
    const t = setup({ panel: "find", found: () => Promise.reject(new ApiError(404, "not_found", "no route")) });
    await flush();
    expect(t.notes()).toEqual(["This server can't search for subtitles: it needs an update."]);
    act(() => render(null, t.root));
    expect(setup({ admin: true, found: () => Promise.resolve([]) }).rows()).toEqual([]);
  });
});
