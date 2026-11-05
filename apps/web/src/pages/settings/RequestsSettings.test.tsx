import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { ArrOptions } from "@/api/requests";
import type { Settings } from "@/api/types";
import { apiError, mockApi, renderWithProviders } from "@/test/render";
import { RequestsSettings } from "./RequestsSettings";

const SETTINGS = {
  server_name: "Den",
  language: "",
  tmdb_api_key: "t0k",
  metadata_language: "en-US",
  tv_metadata: "auto",
  tvdb_api_key: "",
  tvdb_pin: "",
  music_metadata: true,
  hwaccel: "auto",
  hardware_decoding: true,
  tonemapping: "auto",
  max_transcodes: 4,
  transcode_preset: "veryfast",
  throttle_transcodes: true,
  transcode_in_memory: true,
  scan_interval_minutes: 360,
  watch_libraries: true,
  detect_markers: true,
  seek_previews: true,
  previews_in_background: true,
  third_party_apps: false,
  requests: true,
  request_limit_movies: 0,
  request_limit_seasons: 0,
  radarr_url: "http://localhost:7878",
  radarr_api_key_set: true,
  radarr_quality_profile: 0,
  radarr_root_folder: "",
  radarr_anime_quality_profile: 0,
  radarr_anime_root_folder: "",
  radarr_minimum_availability: "released",
  radarr_search: true,
  sonarr_url: "",
  sonarr_api_key_set: false,
  sonarr_quality_profile: 0,
  sonarr_root_folder: "",
  sonarr_language_profile: 0,
  sonarr_anime_quality_profile: 0,
  sonarr_anime_root_folder: "",
  sonarr_season_folders: true,
  sonarr_search: true,
} satisfies Settings;

const RADARR: ArrOptions = {
  version: "5.14.0",
  quality_profiles: [
    { id: 4, name: "HD-1080p" },
    { id: 6, name: "Ultra-HD" },
  ],
  root_folders: [{ path: "/movies", free_bytes: null }],
  language_profiles: [],
};

/** The settings, saved as the server would (keys never come back); Radarr answers the key "n3w" at another address. */
function serve(settings: Settings = SETTINGS) {
  let saved: Settings = settings;
  return mockApi({
    "GET /api/settings": () => saved,
    "PUT /api/settings": (req) => {
      const next: Settings = { ...saved, ...(req.body as Partial<Settings>) };
      delete next.radarr_api_key;
      delete next.sonarr_api_key;
      saved = next;
      return saved;
    },
    "POST /api/settings/arr": (req) => {
      const body = req.body as { url?: string; api_key?: string };
      if (body.url === undefined) return RADARR;
      return body.api_key === "n3w" ? { ...RADARR, version: "5.15.0" } : apiError(400, "bad_request", "Radarr refused the API key");
    },
  });
}

const field = (label: string, id: string) => screen.getByLabelText(label, { selector: `#${id}` });

describe("RequestsSettings", () => {
  it("needs a TMDB key to take requests", async () => {
    serve({ ...SETTINGS, tmdb_api_key: "", requests: false });
    renderWithProviders(<RequestsSettings />);
    expect(await screen.findByRole("switch", { name: "Take requests" })).toBeDisabled();
    expect(screen.getByRole("link", { name: "Server" })).toHaveAttribute("href", "/settings/server#tmdb-key");
  });

  it("asks the saved Radarr what requests can go with, and saves what's chosen", async () => {
    const user = userEvent.setup();
    const requests = serve();
    renderWithProviders(<RequestsSettings />);

    expect(await screen.findByText("Radarr 5.14.0")).toBeInTheDocument();
    // Sonarr has no address: nothing is asked of it.
    expect(requests.filter((r) => r.path === "/api/settings/arr").map((r) => r.body)).toEqual([{ service: "radarr" }]);
    expect(screen.getAllByText("Approving fails until one is chosen.")).toHaveLength(2);

    await user.selectOptions(field("Quality profile", "radarr-profile"), "4");
    await user.selectOptions(field("Root folder", "radarr-folder"), "/movies");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Settings saved")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "PUT")?.body).toEqual({ radarr_quality_profile: 4, radarr_root_folder: "/movies" });
  });

  it("asks for the key again at a new address, and sends them together", async () => {
    const user = userEvent.setup();
    const requests = serve();
    renderWithProviders(<RequestsSettings />);
    await screen.findByText("Radarr 5.14.0");
    const url = field("Address", "radarr-url");
    const key = field("API key", "radarr-key");
    const connect = screen.getAllByRole("button", { name: "Connect" })[0] as HTMLElement;

    // The same address, written otherwise, keeps the saved key.
    await user.type(url, "/");
    expect(key).not.toBeRequired();

    await user.clear(url);
    await user.type(url, "http://nas:7878");
    expect(screen.getByText(/Type Radarr's API key for this address/)).toBeInTheDocument();
    expect(key).toBeRequired();
    expect(connect).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(requests.some((r) => r.method === "PUT")).toBe(false);

    await user.type(key, "wrong");
    await user.click(connect);
    expect(await screen.findByText("Radarr refused the API key")).toBeInTheDocument();
    await user.clear(key);
    await user.type(key, "n3w");
    await user.click(connect);
    expect(await screen.findByText("Radarr 5.15.0")).toBeInTheDocument();
    expect(requests.filter((r) => r.path === "/api/settings/arr").at(-1)?.body).toEqual({ service: "radarr", url: "http://nas:7878", api_key: "n3w" });

    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Settings saved")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "PUT")?.body).toEqual({ radarr_url: "http://nas:7878", radarr_api_key: "n3w" });
  });

  it("forgets the key with the address", async () => {
    const user = userEvent.setup();
    const requests = serve();
    renderWithProviders(<RequestsSettings />);
    await screen.findByText("Radarr 5.14.0");
    await user.clear(field("Address", "radarr-url"));
    expect(screen.getByText("Saving without an address forgets the API key too.")).toBeInTheDocument();
    expect(screen.queryByLabelText("Quality profile")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Settings saved")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "PUT")?.body).toEqual({ radarr_url: "" });
  });
});
