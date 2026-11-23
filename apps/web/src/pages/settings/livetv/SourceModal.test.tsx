import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { LiveSource } from "@/api/livetv";
import { mockApi, renderWithProviders } from "@/test/render";
import { SourceModal } from "./SourceModal";

const source: LiveSource = {
  id: 7,
  uid: "a1b2c3",
  name: "Provider",
  kind: "m3u",
  url: "http://provider.test/list.m3u",
  username: "",
  password_set: false,
  guide_url: "",
  user_agent: "",
  max_connections: 2,
  enabled: true,
  sort_groups: false,
  failover: false,
  created_at: 0,
  channels_at: 0,
  channels_error: null,
  guide_at: null,
  guide_error: null,
  account: null,
  channel_count: 40,
  hidden_count: 0,
  guided_count: 0,
  connections_in_use: 0,
  refreshing: false,
};

describe("SourceModal", () => {
  it("saves a source's groups A to Z", async () => {
    const user = userEvent.setup();
    const requests = mockApi({ "PUT /api/live/sources/7": ({ body }) => ({ ...source, ...(body as object) }) });
    renderWithProviders(<SourceModal source={source} open onClose={() => {}} />);

    const sort = screen.getByRole("checkbox", { name: /Sort groups A to Z/ });
    expect(sort).not.toBeChecked();
    await user.click(sort);
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(requests.some((r) => r.method === "PUT")).toBe(true));
    const saved = requests.find((r) => r.method === "PUT")!.body as Record<string, unknown>;
    expect(saved.sort_groups).toBe(true);
    expect(saved.max_connections).toBe(2);
  });

  it("saves a source's channels playing on another source when it's full", async () => {
    const user = userEvent.setup();
    const requests = mockApi({ "PUT /api/live/sources/7": ({ body }) => ({ ...source, ...(body as object) }) });
    renderWithProviders(<SourceModal source={source} open onClose={() => {}} />);

    const failover = screen.getByRole("checkbox", { name: /When it's full, play its channels on another source/ });
    expect(failover).not.toBeChecked();
    await user.click(failover);
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(requests.some((r) => r.method === "PUT")).toBe(true));
    const saved = requests.find((r) => r.method === "PUT")!.body as Record<string, unknown>;
    expect(saved.failover).toBe(true);
    expect(saved.sort_groups).toBe(false);
  });
});
