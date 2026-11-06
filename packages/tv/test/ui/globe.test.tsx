import { render, type VNode } from "preact";
import { act } from "preact/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import type { KeyValue } from "../../src/auth/tokens";
import { currentLanguage, setLanguage } from "../../src/i18n";
import type { HttpRequest, HttpResponse, Transport } from "../../src/net/transport";
import { NetworkError } from "../../src/net/transport";
import type { Platform } from "../../src/platform";
import { AppState } from "../../src/state";
import { UiContext, type Ui } from "../../src/ui/context";
import { FocusManager } from "../../src/ui/focus";
import { Input } from "../../src/ui/input";
import { AppContext } from "../../src/ui/screens/common";
import { LinkScreen } from "../../src/ui/screens/onboarding/link";
import { ServersScreen } from "../../src/ui/screens/onboarding/servers";

/** The globe of the screens before signing in: this TV's language, kept on the TV. */

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

/** Pley at SERVER, its language `language`, giving a code that waits for approval. */
function pley(language: string | null): Transport {
  return async (req: HttpRequest) => {
    const path = req.url.slice(SERVER.length);
    if (path === "/api/server") return answer({ name: "Den", version: "1.0.0", language });
    if (path === "/.well-known/oauth-authorization-server") {
      return answer(
        JSON.stringify({
          issuer: SERVER,
          token_endpoint: `${SERVER}/api/oauth/token`,
          revocation_endpoint: `${SERVER}/api/oauth/revoke`,
          device_authorization_endpoint: `${SERVER}/api/oauth/device`,
          scopes_supported: ["library", "play", "playlists", "profiles"],
          grant_types_supported: ["refresh_token", "urn:ietf:params:oauth:grant-type:device_code"],
        }),
      );
    }
    if (path === "/api/oauth/device") {
      return answer({
        device_code: "d",
        user_code: "BCDF-GHJK",
        verification_uri: `${SERVER}/link`,
        expires_in: 600,
        interval: 5,
      });
    }
    if (path === "/api/oauth/token") return answer({ error: "authorization_pending" }, 400);
    throw new NetworkError("Can't reach the server");
  };
}

let root: HTMLElement | null = null;
let detach: (() => void) | null = null;

const settle = () => act(() => new Promise<void>((resolve) => setTimeout(resolve, 20)));

async function show(screen: VNode, o: { device?: string[]; store?: KeyValue; server?: string | null } = {}) {
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
      const current = focus.current;
      if (initial && focus.inScope(el) && !(current && current.isConnected && focus.inScope(current))) focus.focus(el);
    },
    nav: { restart: () => undefined },
    reducedMotion: true,
  } as unknown as Ui;
  const store = o.store ?? memoryStore();
  const platform = { name: "tizen", clientId: "test", storage: store, languages: () => o.device ?? ["en-US"], keepAwake: () => {} } as unknown as Platform;
  const app = new AppState(platform, pley(o.server ?? null));
  act(() =>
    render(
      <AppContext.Provider value={app}>
        <UiContext.Provider value={ui}>{screen}</UiContext.Provider>
      </AppContext.Provider>,
      root!,
    ),
  );
  await settle();
  const page = root;
  const press = (key: "ok" | "down" | "up" | "back") => {
    act(() => input.dispatch(key, "down", false));
    act(() => input.dispatch(key, "up", false));
  };
  const globe = () => page.querySelector<HTMLElement>('[data-fid="language-globe"]');
  const rows = () => Array.from(document.querySelectorAll<HTMLElement>(".side-panel [data-f]"));
  const checked = () => rows().filter((r) => r.querySelector(".check")).map((r) => r.textContent);
  return { app, page, focus, store, press, globe, rows, checked };
}

afterEach(() => {
  detach?.();
  detach = null;
  if (root) act(() => render(null, root!));
  root = null;
  setLanguage("en");
});

describe("the globe on Choose your server", () => {
  it("names the language shown, and lists Automatic (the TV's) and each language named in itself", async () => {
    const { focus, press, globe, rows, checked } = await show(<ServersScreen />);
    expect(globe()!.textContent).toBe("English");
    expect(globe()!.getAttribute("aria-label")).toBe("Choose a language, English");
    // Not where focus starts: that stays on the way to a server.
    expect(focus.current?.textContent).toBe("Enter an address");
    act(() => void focus.focus(globe()));
    press("ok");
    expect(document.querySelector(".side-panel .panel-title")!.textContent).toBe("Choose a language");
    expect(rows().map((r) => r.textContent)).toEqual(["Automatic (English)", "English", "Français", "Español"]);
    expect(checked()).toEqual(["Automatic (English)"]);
    expect(focus.current?.textContent).toBe("Automatic (English)");
  });

  it("shows the one picked at once, keeps it on the TV, and gives focus back to the globe", async () => {
    const { focus, store, page, press, globe, rows, checked } = await show(<ServersScreen />);
    act(() => void focus.focus(globe()));
    press("ok");
    press("down");
    press("down");
    press("ok");
    expect(currentLanguage()).toBe("fr");
    expect(store.get("pley.deviceLanguage")).toBe("fr");
    expect(document.querySelector(".side-panel")).toBeNull();
    expect(page.querySelector(".servers-header .t-page")!.textContent).toBe("Choisissez votre serveur");
    expect(globe()!.textContent).toBe("Français");
    expect(focus.current).toBe(globe());

    press("ok");
    expect(rows().map((r) => r.textContent)).toEqual(["Automatique (English)", "English", "Français", "Español"]);
    expect(checked()).toEqual(["Français"]);
    expect(focus.current?.textContent).toBe("Français");
  });

  it("forgets the pick with Automatic, and shows the TV's language again", async () => {
    const store = memoryStore();
    store.set("pley.deviceLanguage", "es");
    const { focus, press, globe } = await show(<ServersScreen />, { store, device: ["fr-FR"] });
    expect(currentLanguage()).toBe("es");
    act(() => void focus.focus(globe()));
    press("ok");
    // From the choice in force, Español, up to Automatic.
    press("up");
    press("up");
    press("up");
    press("ok");
    expect(currentLanguage()).toBe("fr");
    expect(store.get("pley.deviceLanguage")).toBeNull();
    expect(globe()!.textContent).toBe("Français");
  });

  it("is on the address screen too, and Back there closes its list, not the screen", async () => {
    const { focus, page, press, globe, rows } = await show(<ServersScreen />);
    press("ok");
    expect(page.querySelector(".address-entry")).not.toBeNull();
    act(() => void focus.focus(globe()));
    press("ok");
    expect(rows()).toHaveLength(4);
    press("back");
    expect(document.querySelector(".side-panel")).toBeNull();
    expect(page.querySelector(".address-entry")).not.toBeNull();
    press("ok");
    press("down");
    press("down");
    press("down");
    press("ok");
    expect(currentLanguage()).toBe("es");
    expect(page.querySelector(".address-entry .t-title")!.textContent).toBe("Introduce la dirección de tu servidor");
  });
});

describe("the globe on the sign-in code", () => {
  it("draws the screen again in the language picked, with the same code", async () => {
    const { focus, page, press, globe } = await show(<LinkScreen server={SERVER} />);
    expect(page.textContent).toContain("BCDF");
    act(() => void focus.focus(globe()));
    press("ok");
    press("down");
    press("down");
    press("down");
    press("ok");
    expect(currentLanguage()).toBe("es");
    expect(page.querySelector(".link-left .t-page")!.textContent).toBe("Iniciar sesión en Pley");
    expect(page.textContent).toContain("BCDF");
  });

  it("says what Automatic is with the server's language, on a TV in a language Pley doesn't have", async () => {
    const { focus, press, globe, rows } = await show(<LinkScreen server={SERVER} />, { device: ["de-DE"], server: "fr" });
    expect(currentLanguage()).toBe("fr");
    act(() => void focus.focus(globe()));
    press("ok");
    expect(rows()[0]!.textContent).toBe("Automatique (Français)");
  });
});

describe("the language picked on the TV", () => {
  it("is shown at the first start, before the TV's", async () => {
    const store = memoryStore();
    store.set("pley.deviceLanguage", "es");
    await show(<ServersScreen />, { store, device: ["fr-FR"] });
    expect(currentLanguage()).toBe("es");
  });
});
