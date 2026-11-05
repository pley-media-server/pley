import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import { Api, ApiError } from "../../src/api/client";
import type { User } from "../../src/api/types";
import type { KeyValue, Tokens } from "../../src/auth/tokens";
import { currentLanguage, navigatorLanguages, setDeviceLanguages, setLanguage, type Language } from "../../src/i18n";
import { NetworkError, type HttpRequest, type HttpResponse, type Transport } from "../../src/net/transport";
import type { Platform } from "../../src/platform";
import { AppState } from "../../src/state";
import { UiContext, type Ui } from "../../src/ui/context";
import { FocusManager } from "../../src/ui/focus";
import { Input } from "../../src/ui/input";
import { AppContext } from "../../src/ui/screens/common";
import { SettingsScreen } from "../../src/ui/screens/settings";
import type { Entry } from "../../src/ui/shell/navigator";

/**
 * The language the app is in: the one shown last until the server says,
 * then the account's, the server's or the TV's; the Language row in
 * Settings; Accept-Language on the API's requests.
 */

const SERVER = "http://192.168.1.20:7539";

function memoryStore(): KeyValue {
  const kept = new Map<string, string>();
  return {
    get: (k) => kept.get(k) ?? null,
    set: (k, v) => void kept.set(k, v),
    remove: (k) => void kept.delete(k),
  };
}

function answer(body: unknown, status = 200): HttpResponse {
  return { status, header: (name) => (name.toLowerCase() === "content-type" ? "application/json" : null), body };
}

function user(language: string | null | undefined): User {
  return {
    id: 7,
    username: "ann",
    name: "Ann",
    display_name: null,
    avatar_url: null,
    avatar_color: null,
    audio_language: null,
    subtitle_language: null,
    subtitle_mode: "forced",
    ...(language === undefined ? {} : { language }),
  };
}

/** Something to resolve later: the server answering when the test says. */
function later<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

const settle = () => act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)));

/**
 * The app with SERVER chosen, its `GET /api/server` answered by `server`;
 * signed in, with an account whose language `account` says.
 */
function setup(o: { remembered?: Language; device?: string[]; server: () => Promise<HttpResponse>; signedIn?: boolean; account?: string | null }) {
  const store = memoryStore();
  store.set("pley.server", SERVER);
  if (o.remembered) store.set("pley.language", o.remembered);
  const transport: Transport = (req: HttpRequest) => (req.url === `${SERVER}/api/server` ? o.server() : Promise.reject(new NetworkError("Can't reach the server")));
  const platform = { name: "tizen", clientId: "test", storage: store, languages: () => o.device ?? [] } as unknown as Platform;
  const app = new AppState(platform, transport);
  const asked: string[] = [];
  let account = user(o.account);
  const missing = () => Promise.reject(new ApiError(404, "not_found", "not found"));
  (app.api as { value: unknown }).value = {
    me: () => Promise.resolve(account),
    libraries: () => Promise.resolve([]),
    profiles: () => Promise.resolve([]),
    liveSummary: missing,
    requestSummary: missing,
    home: () => {
      asked.push("home");
      return Promise.resolve({ hubs: [] });
    },
    setLanguage: (language: Language | null) => {
      asked.push(`PUT language ${language}`);
      account = user(language);
      return Promise.resolve(account);
    },
  };
  if (o.signedIn) (app as unknown as { tokens: unknown }).tokens = { signedIn: true, scopes: () => ["library", "play"] };
  return { app, store, asked };
}

afterEach(() => {
  setLanguage("en");
  setDeviceLanguages(navigatorLanguages);
});

describe("the language the app is in", () => {
  it("is the one shown last until the server answers, then the server's", async () => {
    const server = later<HttpResponse>();
    const { store } = setup({ remembered: "es", device: ["en-US"], server: () => server.promise });
    expect(currentLanguage()).toBe("es");
    expect(document.documentElement.lang).toBe("es");
    server.resolve(answer({ name: "Den", version: "1.0.0", language: "fr" }));
    await settle();
    expect(currentLanguage()).toBe("fr");
    expect(store.get("pley.language")).toBe("fr");
  });

  it("is the TV's at the first start, and when the server leaves it to each device", async () => {
    const server = later<HttpResponse>();
    setup({ device: ["de-DE", "es-MX"], server: () => server.promise });
    expect(currentLanguage()).toBe("es");
    server.resolve(answer({ name: "Den", version: "1.0.0", language: null }));
    await settle();
    expect(currentLanguage()).toBe("es");
  });

  it("stays while the server doesn't answer, and follows it once its events come back", async () => {
    let reply: () => Promise<HttpResponse> = () => Promise.reject(new NetworkError("Can't reach the server"));
    const { app } = setup({ remembered: "fr", device: ["en-US"], server: () => reply() });
    await settle();
    expect(currentLanguage()).toBe("fr");
    reply = () => Promise.resolve(answer({ name: "Den", version: "1.0.0", language: "es" }));
    (app as unknown as { reconnected(): void }).reconnected();
    await settle();
    expect(currentLanguage()).toBe("es");
  });

  it("signed in, waits for who is watching, then is the account's, and Home is asked again in it", async () => {
    const { app, asked } = setup({ remembered: "en", server: () => Promise.resolve(answer({ version: "1.0.0", language: "fr" })), signedIn: true, account: "es" });
    await settle();
    expect(currentLanguage()).toBe("en");
    await app.loadAccount();
    expect(currentLanguage()).toBe("es");
    expect(asked).toContain("home");
  });

  it("signed in, is the server's for an account without its own", async () => {
    const { app } = setup({ server: () => Promise.resolve(answer({ version: "1.0.0", language: "fr" })), signedIn: true, account: null });
    await app.loadAccount();
    await settle();
    expect(currentLanguage()).toBe("fr");
  });

  it("is the profile's choice at once, and the server's again without one", async () => {
    const { app, asked } = setup({ device: ["en-US"], server: () => Promise.resolve(answer({ version: "1.0.0", language: null })), signedIn: true, account: null });
    await app.loadAccount();
    await app.setAccountLanguage("fr");
    expect(asked).toContain("PUT language fr");
    expect(currentLanguage()).toBe("fr");
    await app.setAccountLanguage(null);
    expect(currentLanguage()).toBe("en");
  });
});

describe("the API's requests", () => {
  it("ask for the language shown", async () => {
    const languages: (string | undefined)[] = [];
    const tokens = { accessToken: () => Promise.resolve("pley_at_a"), doubtServer: () => undefined } as unknown as Tokens;
    const api = new Api("https://pley.example.com", tokens, async (req) => {
      languages.push(req.headers?.["Accept-Language"]);
      return answer({ hubs: [] });
    });
    await api.home();
    setLanguage("fr");
    await api.home();
    expect(languages).toEqual(["en", "fr"]);
  });
});

describe("Settings' Language row", () => {
  let root: HTMLElement | null = null;
  let detach: (() => void) | null = null;

  afterEach(() => {
    detach?.();
    detach = null;
    if (root) act(() => render(null, root!));
    root = null;
  });

  async function show(serverLanguage: string | null, account: string | null) {
    const { app, asked } = setup({ device: ["en-US"], server: () => Promise.resolve(answer({ version: "1.0.0", language: serverLanguage })), signedIn: true, account });
    await app.loadAccount();
    await settle();
    document.body.innerHTML = "";
    root = document.createElement("div");
    document.body.appendChild(root);
    const focus = new FocusManager();
    focus.setPage(root);
    const input = new Input(focus);
    detach = input.attach(document);
    const toasts: string[] = [];
    const ui = {
      focus,
      input,
      claim: (el: HTMLElement, _fid: string | null, initial: boolean) => {
        const current = focus.current;
        if (initial && focus.inScope(el) && !(current && current.isConnected && focus.inScope(current))) focus.focus(el);
      },
      nav: { restart: () => undefined, open: () => undefined },
      api: app.api.value,
      toast: (text: string) => toasts.push(text),
      scale: 1,
      reducedMotion: true,
    } as unknown as Ui;
    const entry = { id: 1, key: { name: "settings" }, state: {} } as unknown as Entry;
    act(() =>
      render(
        <AppContext.Provider value={app}>
          <UiContext.Provider value={ui}>
            <SettingsScreen entry={entry} />
          </UiContext.Provider>
        </AppContext.Provider>,
        root!,
      ),
    );
    const row = () => root!.querySelector<HTMLElement>('[data-fid="row-language"]')!;
    const press = (key: "ok" | "down" | "up") => {
      act(() => input.dispatch(key, "down", false));
      act(() => input.dispatch(key, "up", false));
    };
    return { app, asked, focus, row, press, toasts };
  }

  it("says the server's language for a profile without its own, in it", async () => {
    const { row } = await show("fr", null);
    expect(row().querySelector(".row-label")!.textContent).toBe("Langue");
    expect(row().querySelector(".row-value")!.textContent).toBe("Langue du serveur (Français)");
  });

  it("says the TV's when the server leaves it to each device", async () => {
    const { row } = await show(null, null);
    expect(row().querySelector(".row-value")!.textContent).toBe("Server default (this device's language, English)");
  });

  it("offers each language named in itself, and shows the one picked at once", async () => {
    const { asked, focus, row, press } = await show(null, null);
    act(() => {
      focus.focus(row());
    });
    press("ok");
    const panel = () => Array.from(document.querySelectorAll(".side-panel [data-f]")).map((el) => el.textContent);
    expect(panel()).toEqual(["Server default (this device's language, English)", "English", "Français", "Español"]);
    press("down");
    press("down");
    press("down");
    press("ok");
    await settle();
    expect(asked).toContain("PUT language es");
    expect(currentLanguage()).toBe("es");
    expect(row().querySelector(".row-label")!.textContent).toBe("Idioma");
    expect(row().querySelector(".row-value")!.textContent).toBe("Español");
  });
});
