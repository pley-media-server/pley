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
// Shared with the native Samsung app's tests, which must answer the same.
import fixture from "../fixtures/language.json";

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
 * signed in, with an account whose language `account` says. With
 * `languageRoute: false`, the server answers a choice of language 404, as a
 * Pley from before languages does.
 */
function setup(o: {
  remembered?: Language;
  device?: string[];
  choice?: Language;
  server: () => Promise<HttpResponse>;
  signedIn?: boolean;
  account?: string | null;
  languageRoute?: boolean;
}) {
  const store = memoryStore();
  store.set("pley.server", SERVER);
  if (o.remembered) store.set("pley.language", o.remembered);
  if (o.choice) store.set("pley.deviceLanguage", o.choice);
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
      if (o.languageRoute === false) return missing();
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
  it("on a TV in a language Pley doesn't have, is the one shown last until the server answers, then the server's", async () => {
    const server = later<HttpResponse>();
    const { store } = setup({ remembered: "es", device: ["de-DE"], server: () => server.promise });
    expect(currentLanguage()).toBe("es");
    expect(document.documentElement.lang).toBe("es");
    server.resolve(answer({ name: "Den", version: "1.0.0", language: "fr" }));
    await settle();
    expect(currentLanguage()).toBe("fr");
    expect(store.get("pley.language")).toBe("fr");
  });

  it("is the TV's at the first start, and before the server's", async () => {
    const server = later<HttpResponse>();
    setup({ device: ["de-DE", "es-MX"], server: () => server.promise });
    expect(currentLanguage()).toBe("es");
    server.resolve(answer({ name: "Den", version: "1.0.0", language: "fr" }));
    await settle();
    expect(currentLanguage()).toBe("es");
  });

  it("stays while the server doesn't answer, and follows it once its events come back", async () => {
    let reply: () => Promise<HttpResponse> = () => Promise.reject(new NetworkError("Can't reach the server"));
    const { app } = setup({ remembered: "fr", device: ["de-DE"], server: () => reply() });
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

  it("signed in, is the server's for an account without its own, on a TV in a language Pley doesn't have", async () => {
    const { app } = setup({ server: () => Promise.resolve(answer({ version: "1.0.0", language: "fr" })), signedIn: true, account: null });
    await app.loadAccount();
    await settle();
    expect(currentLanguage()).toBe("fr");
  });

  it("is the profile's choice at once, and the TV's again without one", async () => {
    const { app, asked } = setup({ device: ["en-US"], server: () => Promise.resolve(answer({ version: "1.0.0", language: null })), signedIn: true, account: null });
    await app.loadAccount();
    await app.setAccountLanguage("fr");
    expect(asked).toContain("PUT language fr");
    expect(currentLanguage()).toBe("fr");
    await app.setAccountLanguage(null);
    expect(currentLanguage()).toBe("en");
  });

  it("signed in, is the one picked on the TV for an account without its own, before the TV's and the server's", async () => {
    const { app } = setup({ device: ["en-US"], choice: "es", server: () => Promise.resolve(answer({ version: "1.0.0", language: "fr" })), signedIn: true, account: null });
    await app.loadAccount();
    await settle();
    expect(currentLanguage()).toBe("es");
  });

  it("signed in, is the account's own before the one picked on the TV", async () => {
    const { app } = setup({ device: ["en-US"], choice: "es", server: () => Promise.resolve(answer({ version: "1.0.0", language: null })), signedIn: true, account: "fr" });
    await app.loadAccount();
    await settle();
    expect(currentLanguage()).toBe("fr");
  });

  it("on a server from before languages, which names none, is chosen on this TV: nothing goes to the server", async () => {
    const { app, asked, store } = setup({ device: ["en-US"], server: () => Promise.resolve(answer({ name: "Den", version: "0.1.0" })), signedIn: true });
    await app.loadAccount();
    await settle();
    expect(app.languageOnDevice.value).toBe(true);
    await app.chooseLanguage("fr");
    expect(asked).not.toContain("PUT language fr");
    expect(currentLanguage()).toBe("fr");
    expect(app.deviceChoice.value).toBe("fr");
    expect(store.get("pley.deviceLanguage")).toBe("fr");
    // Automatic forgets the pick: the TV's own again.
    await app.chooseLanguage(null);
    expect(currentLanguage()).toBe("en");
    expect(store.get("pley.deviceLanguage")).toBeNull();
  });

  it("is the account's on a server that names a language, even one Pley here doesn't have, as the shared fixture has it", async () => {
    for (const c of fixture.keepsLanguage) {
      const { app } = setup({ device: ["en-US"], server: () => Promise.resolve(answer(c.server)), signedIn: true });
      await settle();
      // The server has said.
      expect([c, app.serverLanguage.value === undefined]).toEqual([c, false]);
      expect([c, !app.languageOnDevice.value]).toEqual([c, c.out]);
    }
  });

  it("on a server that answers the choice 404, is this TV's from then on", async () => {
    const { app, asked } = setup({ device: ["en-US"], server: () => Promise.resolve(answer({ version: "1.0.0", language: "en" })), signedIn: true, account: null, languageRoute: false });
    await app.loadAccount();
    await settle();
    expect(app.languageOnDevice.value).toBe(false);
    await app.chooseLanguage("es");
    expect(asked.filter((a) => a.startsWith("PUT"))).toEqual(["PUT language es"]);
    expect(currentLanguage()).toBe("es");
    expect(app.deviceChoice.value).toBe("es");
    expect(app.languageOnDevice.value).toBe(true);
    await app.chooseLanguage("fr");
    expect(asked.filter((a) => a.startsWith("PUT"))).toEqual(["PUT language es"]);
    expect(currentLanguage()).toBe("fr");
  });

  it("on a current server, is still the account's, whatever the server answers", async () => {
    const { app } = setup({ device: ["en-US"], server: () => Promise.resolve(answer({ version: "1.0.0", language: "en" })), signedIn: true, account: null });
    await app.loadAccount();
    await settle();
    (app.api.value as unknown as { setLanguage: () => Promise<never> }).setLanguage = () => Promise.reject(new ApiError(400, "bad_request", "language is one of en, fr, es, or null"));
    await expect(app.chooseLanguage("fr")).rejects.toThrow("language is one of en, fr, es, or null");
    expect(currentLanguage()).toBe("en");
    expect(app.deviceChoice.value).toBeNull();
    expect(app.languageOnDevice.value).toBe(false);
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

  /** Settings on its Profile; `serverLanguage` left out is a Pley from before languages, which names none, and its accounts have none. */
  async function show(serverLanguage: string | null | undefined, account: string | null | undefined, device = ["en-US"], choice?: Language, languageRoute?: boolean) {
    const said = serverLanguage === undefined ? { version: "0.1.0" } : { version: "1.0.0", language: serverLanguage };
    const { app, asked } = setup({ device, choice, server: () => Promise.resolve(answer(said)), signedIn: true, account, languageRoute });
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

  it("says what Automatic is for a profile without its own: the TV's language", async () => {
    const { row } = await show("fr", null);
    expect(row().querySelector(".row-label")!.textContent).toBe("Language");
    expect(row().querySelector(".row-value")!.textContent).toBe("Automatic (English)");
  });

  it("says the server's, in it, on a TV in a language Pley doesn't have", async () => {
    const { row } = await show("fr", null, ["de-DE"]);
    expect(row().querySelector(".row-label")!.textContent).toBe("Langue");
    expect(row().querySelector(".row-value")!.textContent).toBe("Automatique (Français)");
  });

  it("says the one picked on the TV with the globe, as Automatic is that", async () => {
    const { row } = await show("fr", null, ["en-US"], "es");
    expect(row().querySelector(".row-value")!.textContent).toBe("Automático (Español)");
  });

  it("offers each language named in itself, and shows the one picked at once", async () => {
    const { asked, focus, row, press } = await show("en", null);
    act(() => {
      focus.focus(row());
    });
    press("ok");
    const panel = () => Array.from(document.querySelectorAll(".side-panel [data-f]")).map((el) => el.textContent);
    expect(panel()).toEqual(["Automatic (English)", "English", "Français", "Español"]);
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

  it("on a server from before languages, is this TV's: the one picked with the globe, Automatic the TV's own", async () => {
    const { asked, focus, row, press, toasts } = await show(undefined, undefined, ["en-US"], "es");
    expect(row().querySelector(".row-label")!.textContent).toBe("Idioma");
    expect(row().querySelector(".row-value")!.textContent).toBe("Español");
    act(() => {
      focus.focus(row());
    });
    press("ok");
    const panel = () => Array.from(document.querySelectorAll(".side-panel [data-f]")).map((el) => el.textContent);
    // Automatic names the TV's own language, not the one picked.
    expect(panel()).toEqual(["Automático (English)", "English", "Français", "Español"]);
    press("up");
    press("up");
    press("up");
    press("ok");
    await settle();
    // Automatic forgets the pick, at once and with no word; nothing goes to the server.
    expect(currentLanguage()).toBe("en");
    expect(row().querySelector(".row-value")!.textContent).toBe("Automatic (English)");
    expect(asked.filter((a) => a.startsWith("PUT"))).toEqual([]);
    expect(toasts).toEqual([]);
  });

  it("on a server that answers the choice 404, keeps it on this TV, with no word", async () => {
    const { app, asked, focus, row, press, toasts } = await show("en", null, ["en-US"], undefined, false);
    act(() => {
      focus.focus(row());
    });
    press("ok");
    press("down");
    press("down");
    press("ok");
    await settle();
    expect(asked.filter((a) => a.startsWith("PUT"))).toEqual(["PUT language fr"]);
    expect(currentLanguage()).toBe("fr");
    expect(app.deviceChoice.value).toBe("fr");
    expect(row().querySelector(".row-value")!.textContent).toBe("Français");
    expect(toasts).toEqual([]);
  });

  it("draws the rest of Settings again in the new language", async () => {
    await show(null, null);
    const categories = () => Array.from(root!.querySelectorAll(".settings-categories .row-label")).map((el) => el.textContent);
    const audio = () => root!.querySelector('[data-fid="row-audio"]')!;
    expect(categories()).toEqual(["Profile", "Playback", "Audio", "Subtitles and display", "Server", "Diagnostics", "About", "Sign out"]);
    expect(audio().querySelector(".row-value")!.textContent).toBe("The file's default");
    act(() => setLanguage("fr"));
    expect(root!.querySelector(".t-page")!.textContent).toBe("Paramètres");
    expect(categories()).toEqual(["Profil", "Lecture", "Audio", "Sous-titres et affichage", "Serveur", "Diagnostic", "À propos", "Se déconnecter"]);
    expect(audio().querySelector(".row-label")!.textContent).toBe("Langue audio");
    expect(audio().querySelector(".row-value")!.textContent).toBe("Celle du fichier par défaut");
  });
});
