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
import { LinkScreen, TWO_STEP_REQUIRED } from "../../src/ui/screens/onboarding/link";

/** Signing in with a code: what the screen says about two-step sign-in on a server that requires it. */

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

/** A server whose status says `status`; the code it gives waits for approval. */
function server(status: object | "down"): Transport {
  return async (req: HttpRequest) => {
    const path = req.url.slice(SERVER.length);
    if (path === "/api/server") {
      if (status === "down") throw new NetworkError("Can't reach the server");
      return answer(status);
    }
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
        verification_uri_complete: `${SERVER}/link?code=BCDFGHJK`,
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

async function show(status: object | "down") {
  document.body.innerHTML = "";
  root = document.createElement("div");
  document.body.appendChild(root);
  const focus = new FocusManager();
  focus.setPage(root);
  const input = new Input(focus);
  detach = input.attach(document);
  const ui = { focus, input, claim: () => {}, nav: { restart: () => {} }, reducedMotion: true } as unknown as Ui;
  const platform = { name: "tizen", clientId: "test", storage: memoryStore(), keepAwake: () => {} } as unknown as Platform;
  const app = new AppState(platform, server(status));
  act(() =>
    render(
      <AppContext.Provider value={app}>
        <UiContext.Provider value={ui}>
          <LinkScreen server={SERVER} />
        </UiContext.Provider>
      </AppContext.Provider>,
      root!,
    ),
  );
  // Its effects ran as that ended: the status and the code come back.
  await act(() => new Promise<void>((resolve) => setTimeout(resolve, 20)));
  return root;
}

afterEach(() => {
  detach?.();
  detach = null;
  if (root) act(() => render(null, root!));
  root = null;
});

describe("signing in with a code", () => {
  it("says so when the server requires two-step sign-in", async () => {
    const page = await show({ name: "Den", version: "1.0.0", two_step_required: true });
    expect(page.querySelector(".link-left .t-body")?.textContent).toBe(TWO_STEP_REQUIRED);
    expect(page.textContent).toContain("BCDF");
  });

  it("says nothing of it otherwise, or when the server doesn't say", async () => {
    for (const status of [{ name: "Den", version: "1.0.0", two_step_required: false }, { name: "Den", version: "0.9.0" }, "down" as const]) {
      const page = await show(status);
      expect(page.textContent).not.toContain("two-step");
      act(() => render(null, page));
    }
  });
});
