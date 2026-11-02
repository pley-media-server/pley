import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import type { KeyValue } from "../../src/auth/tokens";
import type { HttpRequest, HttpResponse, Transport } from "../../src/net/transport";
import { NetworkError } from "../../src/net/transport";
import type { Platform } from "../../src/platform";
import { AppState } from "../../src/state";
import { UiContext, type Ui } from "../../src/ui/context";
import { FocusManager } from "../../src/ui/focus";
import { Input } from "../../src/ui/input";
import { AppContext } from "../../src/ui/screens/common";
import { ServersScreen } from "../../src/ui/screens/onboarding/servers";

/** Typing a server's address: the TV's keyboard, never a number pad. */

const SERVER = "https://pley.example.com";

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

/** Pley at SERVER; nothing anywhere else. */
const transport: Transport = async (req: HttpRequest) => {
  if (req.url === `${SERVER}/api/server`) return answer({ name: "Den", version: "1.0.0" });
  throw new NetworkError("Can't reach the server");
};

let root: HTMLElement | null = null;
let detach: (() => void) | null = null;

const settle = () => act(() => new Promise<void>((resolve) => setTimeout(resolve, 20)));

async function show() {
  document.body.innerHTML = "";
  root = document.createElement("div");
  document.body.appendChild(root);
  const focus = new FocusManager();
  focus.setPage(root);
  const input = new Input(focus);
  detach = input.attach(document);
  const restarts: unknown[] = [];
  const ui = {
    focus,
    input,
    claim: (el: HTMLElement, _fid: string | null, initial: boolean) => {
      const current = focus.current;
      if (initial && focus.inScope(el) && !(current && current.isConnected && focus.inScope(current))) focus.focus(el);
    },
    nav: { restart: (to: unknown) => restarts.push(to) },
    reducedMotion: true,
  } as unknown as Ui;
  const platform = { name: "tizen", clientId: "test", storage: memoryStore(), keepAwake: () => {} } as unknown as Platform;
  const app = new AppState(platform, transport);
  act(() =>
    render(
      <AppContext.Provider value={app}>
        <UiContext.Provider value={ui}>
          <ServersScreen />
        </UiContext.Provider>
      </AppContext.Provider>,
      root!,
    ),
  );
  await settle();
  return { page: root, ui, input, restarts };
}

function press(input: Input, key: "ok" | "back") {
  act(() => {
    input.dispatch(key, "down", false);
    input.dispatch(key, "up", false);
  });
}

afterEach(() => {
  detach?.();
  detach = null;
  if (root) act(() => render(null, root!));
  root = null;
});

describe("entering a server's address", () => {
  it("opens the TV's keyboard on an empty field, and its OK connects", async () => {
    const { page, ui, input, restarts } = await show();
    expect(ui.focus.current?.textContent).toBe("Enter an address");
    press(input, "ok");
    const field = page.querySelector<HTMLInputElement>(".field input")!;
    expect(field.type).toBe("url");
    expect(field.value).toBe("");
    expect(ui.input.typing).toBe(field);
    // The browser's focus on the input is what opens the TV's keyboard.
    expect(document.activeElement).toBe(field);
    expect(page.querySelector(".pad-key")).toBeNull();

    act(() => {
      field.value = SERVER;
      field.dispatchEvent(new Event("input", { bubbles: true }));
    });
    press(input, "ok");
    await settle();
    expect(ui.input.typing).toBeNull();
    expect(restarts).toEqual([{ name: "link", server: SERVER }]);
  });

  it("types the remote's digits once the keyboard is closed, and Back goes back to the list", async () => {
    const { page, ui, input } = await show();
    press(input, "ok");
    press(input, "back");
    expect(ui.input.typing).toBeNull();
    act(() => {
      for (const key of ["digit1", "digit0", "period", "digit2", "backspace", "digit5"] as const) input.dispatch(key, "down", false);
    });
    expect(page.querySelector<HTMLInputElement>(".field input")!.value).toBe("10.5");
    press(input, "back");
    expect(page.querySelector(".field")).toBeNull();
    expect(page.textContent).toContain("Choose your server");
  });
});
