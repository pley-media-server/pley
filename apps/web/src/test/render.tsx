import type { ReactElement, ReactNode } from "react";
import { MemoryRouter, useLocation } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import { vi } from "vitest";
import type { TwoStepSetup, User } from "@/api/types";
import { AuthUserProvider } from "@/auth";
import { ToastProvider } from "@/components/Toast";

export const testUser: User = {
  id: 1,
  username: "admin",
  name: "Admin",
  display_name: null,
  is_admin: true,
  managed: false,
  home: true,
  has_pin: false,
  avatar_url: null,
  avatar_color: null,
  all_libraries: true,
  library_ids: [],
  max_age: null,
  disabled: false,
  created_at: 1_700_000_000,
  audio_language: null,
  subtitle_language: null,
  subtitle_mode: "forced",
  has_password: true,
  two_step: false,
};

/** No retries (queries that ask for some get them right away) and no garbage collection. */
export function testQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: { queries: { retry: false, retryDelay: 0, gcTime: Infinity }, mutations: { retry: false } },
  });
}

/** The app's providers: queries, a router at `route`, toasts and a signed-in user. */
export function TestProviders({
  children,
  route = "/",
  user = testUser,
  client,
}: {
  children: ReactNode;
  route?: string;
  user?: User;
  client: QueryClient;
}) {
  return (
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[route]}>
        <ToastProvider>
          <AuthUserProvider user={user}>{children}</AuthUserProvider>
        </ToastProvider>
      </MemoryRouter>
    </QueryClientProvider>
  );
}

/** Shows where the router is: `screen.getByTestId("location")` has the path and query. */
export function LocationDisplay() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname + location.search + location.hash}</div>;
}

/** Render with {@link TestProviders}. */
export function renderWithProviders(
  ui: ReactElement,
  { route, user, client = testQueryClient() }: { route?: string; user?: User; client?: QueryClient } = {},
) {
  const wrapper = ({ children }: { children: ReactNode }) => (
    <TestProviders route={route} user={user} client={client}>
      {children}
    </TestProviders>
  );
  return { client, ...render(ui, { wrapper }) };
}

export interface FakeRequest {
  method: string;
  /** Path and query string. */
  path: string;
  /** The JSON body, parsed. */
  body: unknown;
  init: RequestInit;
}

/** A JSON body, a `Response` (errors, other types) or `undefined` (204). */
type Handler = (req: FakeRequest) => unknown;

/** An error answer like the server's: `{ code, message }`. */
export function apiError(status: number, code: string, message: string): Response {
  return new Response(JSON.stringify({ code, message }), {
    status,
    headers: { "content-type": "application/json" },
  });
}

/** A secret to add to an authenticator app, as the server shows it. */
export const testSetup: TwoStepSetup = {
  secret: "JBSWY3DPEHPK3PXP",
  uri: "otpauth://totp/Pley:admin?secret=JBSWY3DPEHPK3PXP&issuer=Pley",
  qr: { size: 21, path: "M0 0h7v1h-7z" },
  expires_at: 0,
  id: "QXJHVZJ5TOFQGNLX4DPMHWU4KE",
};

/**
 * The server's answer to a sign-in (or a switch, or an invitation) that sets
 * up two-step sign-in first, which it requires: `setup` is the secret to add.
 */
export function twoStepSetupAnswer(
  setup: TwoStepSetup = testSetup,
  message = "this server requires two-step sign-in: add Pley to an authenticator app, then enter the 6-digit code it shows",
): Response {
  return new Response(JSON.stringify({ code: "two_step_setup", message, setup }), {
    status: 401,
    headers: { "content-type": "application/json" },
  });
}

/**
 * Stub `fetch` with handlers keyed by "METHOD /path" (without the query
 * string). Requests without a handler answer 404. Returns the requests made,
 * in order.
 */
export function mockApi(handlers: Record<string, Handler>): FakeRequest[] {
  const requests: FakeRequest[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init: RequestInit = {}) => {
      const url = new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url, "http://pley.test");
      const method = (init.method ?? "GET").toUpperCase();
      const body = typeof init.body === "string" ? (JSON.parse(init.body) as unknown) : undefined;
      const req = { method, path: url.pathname + url.search, body, init };
      requests.push(req);
      const handler = handlers[`${method} ${url.pathname}`];
      if (!handler) return apiError(404, "not_found", `no handler for ${method} ${url.pathname}`);
      const result = await handler(req);
      if (result instanceof Response) return result;
      return result === undefined
        ? new Response(null, { status: 204 })
        : new Response(JSON.stringify(result), { status: 200, headers: { "content-type": "application/json" } });
    }),
  );
  return requests;
}
