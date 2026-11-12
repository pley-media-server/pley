import type { ReactNode } from "react";
import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { mockApi, TestProviders, testQueryClient } from "@/test/render";
import { keys } from "./hooks";
import type { ImportProgress } from "./plexImport";
import type { ServerEvent } from "./types";

/** The listeners on the server's events: what the stream would call. */
const { listeners } = vi.hoisted(() => ({ listeners: new Set<(ev: ServerEvent) => void>() }));
vi.mock("./events", async (original) => ({
  ...(await original<typeof import("./events")>()),
  onServerEvent: (listener: (ev: ServerEvent) => void) => {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
}));

const { IMPORT_EVENT_MS, plexImportKeys, usePlexImport } = await import("./plexImport");

const running: ImportProgress = { state: "running", step: "edits", done: 1, total: 40, error: null, report: null, first_pass_done: true };

describe("usePlexImport", () => {
  it("fetches the import again once for a burst of events, however many views show it", async () => {
    const requests = mockApi({ "GET /api/import/plex": () => running });
    const client = testQueryClient();
    const wrapper = ({ children }: { children: ReactNode }) => <TestProviders client={client}>{children}</TestProviders>;
    const view = renderHook(
      () => {
        usePlexImport();
        usePlexImport();
        usePlexImport();
      },
      { wrapper },
    );
    const fetched = () => requests.filter((r) => r.method === "GET" && r.path === "/api/import/plex").length;
    await waitFor(() => expect(fetched()).toBe(1));
    expect(listeners.size).toBe(1);

    // An event per title: the first fetches at once, the rest once more after a while.
    act(() => {
      for (let i = 0; i < 20; i++) for (const listener of listeners) listener({ type: "import_changed" });
    });
    await waitFor(() => expect(fetched()).toBe(2));
    await new Promise((resolve) => setTimeout(resolve, IMPORT_EVENT_MS + 100));
    expect(fetched()).toBe(3);

    view.unmount();
    expect(listeners.size).toBe(0);
  });

  it("fetches the profiles again once the import ends, as it opens their libraries then", async () => {
    let progress = running;
    mockApi({ "GET /api/import/plex": () => progress });
    const client = testQueryClient();
    const wrapper = ({ children }: { children: ReactNode }) => <TestProviders client={client}>{children}</TestProviders>;
    const view = renderHook(() => usePlexImport(), { wrapper });
    await waitFor(() => expect(client.getQueryData(plexImportKeys.progress)).toEqual(running));
    client.setQueryData(keys.users, []);

    progress = { ...running, state: "done", step: null };
    act(() => {
      for (const listener of listeners) listener({ type: "import_changed" });
    });
    await waitFor(() => expect(client.getQueryState(keys.users)?.isInvalidated).toBe(true));
    view.unmount();
  });
});
