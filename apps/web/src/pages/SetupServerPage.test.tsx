import { Route, Routes } from "react-router";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { NetworkSettings, RatingSystems, ServiceTestResult, Settings } from "@/api/types";
import { LocationDisplay, mockApi, renderWithProviders, testUser } from "@/test/render";
import { SetupServerPage, startingChoices } from "./SetupServerPage";

const SETTINGS = {
  server_name: "ds920plus",
  language: "",
  tmdb_api_key: "",
  metadata_language: "en-US",
  country: "",
} as Settings;

const SYSTEMS: RatingSystems = {
  region: "US",
  systems: [
    { country: "GB", name: "United Kingdom", ratings: [] },
    { country: "NZ", name: "New Zealand", ratings: [] },
    { country: "US", name: "United States", ratings: [] },
  ],
};

const NETWORK: NetworkSettings = {
  web: ["0.0.0.0:7539"],
  jellyfin: [],
  jellyfin_on_web: false,
  plex: [],
  plex_on_web: true,
  discovery: true,
  bonjour: false,
  bonjour_adapters: [],
  bonjour_host_name: "",
  bonjour_host_default: "pley-0123456789ab",
  container_network: false,
  fixed: { web: false, jellyfin: false, plex: false, discovery: false, bonjour: false },
  listeners: [],
  adapters: [{ name: "eth0", description: null, up: true, addresses: ["192.168.1.10"], network: null }],
  config_file: "/config/pley.toml",
  windows: false,
};

function renderStep({ network = NETWORK, tmdb }: { network?: NetworkSettings; tmdb?: ServiceTestResult } = {}) {
  const requests = mockApi({
    "GET /api/settings": () => SETTINGS,
    "GET /api/network": () => network,
    "GET /api/ratings/systems": () => SYSTEMS,
    "POST /api/settings/test": () => tmdb ?? { ok: true, message: "TMDB answered", ms: 40 },
    "PUT /api/settings": (req) => ({ ...SETTINGS, ...(req.body as Partial<Settings>) }),
    "PUT /api/network": (req) => ({ ...network, ...(req.body as object) }),
  });
  renderWithProviders(
    <Routes>
      <Route path="/setup/server" element={<SetupServerPage />} />
      <Route path="*" element={<LocationDisplay />} />
    </Routes>,
    { route: "/setup/server" },
  );
  return requests;
}

const puts = (requests: ReturnType<typeof renderStep>, path: string) => requests.filter((r) => r.method === "PUT" && r.path === path);

describe("startingChoices", () => {
  it("starts from what the server has, else from the browser's languages", () => {
    const start = (settings: Partial<Settings>, tags: string[], systems: RatingSystems | undefined = SYSTEMS) =>
      startingChoices({ ...SETTINGS, ...settings }, systems, tags);
    expect(start({ country: "IE", metadata_language: "en-GB" }, ["de-DE"])).toEqual({ country: "IE", metadataLanguage: "en-GB" });
    // The first country whose ratings Pley knows, and the first language TMDB has.
    expect(start({}, ["fr", "en-AT", "en_nz"])).toEqual({ country: "NZ", metadataLanguage: "fr-FR" });
    // Austria's ratings aren't known: German is Germany's.
    expect(start({}, ["de-AT"])).toEqual({ country: "DE", metadataLanguage: "de-DE" });
    expect(start({ metadata_language: "en-GB" }, ["en"])).toEqual({ country: "GB", metadataLanguage: "en-GB" });
    expect(start({}, [], undefined)).toEqual({ country: "US", metadataLanguage: "en-US" });
  });
});

describe("SetupServerPage", () => {
  it("shows what Pley will use, changes what's asked, turns Bonjour on and goes on to Coming from Plex", async () => {
    const user = userEvent.setup();
    const requests = renderStep();
    expect(await screen.findByText("ds920plus")).toBeInTheDocument();
    expect(screen.getByText("English")).toBeInTheDocument();
    expect(screen.getByText("United States")).toBeInTheDocument();
    expect(screen.getByText("No TMDB key")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Get a free API key" })).toHaveAttribute("href", "https://www.themoviedb.org/settings/api");
    expect(screen.getByText("Find this server by themselves")).toBeInTheDocument();
    // Nothing to fill in until a row is opened.
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Change: Server name" }));
    expect(screen.getByLabelText("Server name")).toHaveValue("ds920plus");
    await user.clear(screen.getByLabelText("Server name"));
    await user.type(screen.getByLabelText("Server name"), "Den");
    await user.click(screen.getByRole("button", { name: "Change: Country" }));
    await screen.findByRole("option", { name: "New Zealand" });
    await user.selectOptions(screen.getByLabelText("Country"), "NZ");
    await user.click(screen.getByRole("button", { name: "Continue" }));

    expect(await screen.findByTestId("location")).toHaveTextContent("/setup/start");
    expect(puts(requests, "/api/settings")[0]?.body).toEqual({
      server_name: "Den",
      language: "en",
      metadata_language: "en-US",
      country: "NZ",
      tmdb_api_key: "",
    });
    // Where Pley listens stays as it was: only Bonjour changes.
    expect(puts(requests, "/api/network")[0]?.body).toEqual({
      web: ["0.0.0.0:7539"],
      jellyfin: [],
      jellyfin_on_web: false,
      plex: [],
      plex_on_web: true,
      discovery: true,
      bonjour: true,
      bonjour_adapters: [],
      bonjour_host_name: "",
    });
  });

  it("has titles and summaries in the browser's language, which can be changed", async () => {
    const user = userEvent.setup();
    const languages = vi.spyOn(navigator, "languages", "get").mockReturnValue(["de-AT", "en"]);
    try {
      const requests = renderStep();
      expect(await screen.findByText("German (Germany)")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Change: Titles and summaries" }));
      await user.selectOptions(screen.getByLabelText("Titles and summaries"), "en-GB");
      await user.click(screen.getByRole("button", { name: "Continue" }));
      await waitFor(() => expect(puts(requests, "/api/settings")[0]?.body).toMatchObject({ metadata_language: "en-GB" }));
    } finally {
      languages.mockRestore();
    }
  });

  it("says when TMDB refuses the key, and goes on without it when asked", async () => {
    const user = userEvent.setup();
    const requests = renderStep({ tmdb: { ok: false, message: "Invalid API key", ms: 0 } });
    await user.click(await screen.findByRole("button", { name: "Change: Movie details" }));
    await user.type(screen.getByLabelText("Movie details"), "nope");
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("TMDB didn't take this key. Check it, or continue without it.")).toBeInTheDocument();
    expect(puts(requests, "/api/settings")).toHaveLength(0);

    await user.click(screen.getByRole("button", { name: "Continue without the key" }));
    expect(await screen.findByTestId("location")).toHaveTextContent("/setup/start");
    expect(puts(requests, "/api/settings")[0]?.body).toMatchObject({ tmdb_api_key: "" });
  });

  it("saves a key TMDB takes", async () => {
    const user = userEvent.setup();
    const requests = renderStep();
    expect(await screen.findByRole("button", { name: "Change: Movie details" })).toHaveTextContent("Add a key");
    await user.click(screen.getByRole("button", { name: "Change: Movie details" }));
    await user.type(screen.getByLabelText("Movie details"), " abc123 ");
    await user.click(screen.getByRole("button", { name: "Continue" }));
    await waitFor(() => expect(puts(requests, "/api/settings")[0]?.body).toMatchObject({ tmdb_api_key: "abc123" }));
    expect(requests.find((r) => r.path === "/api/settings/test")?.body).toEqual({ service: "tmdb", api_key: "abc123" });
  });

  it("leaves Bonjour off on Docker's own network, and says why", async () => {
    const user = userEvent.setup();
    const requests = renderStep({
      network: { ...NETWORK, container_network: true, adapters: [{ name: "eth0", description: null, up: true, addresses: ["172.17.0.2"], network: null }] },
    });
    expect(await screen.findByText("Need its address")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Change: Apple devices" }));
    expect(screen.getByRole("switch", { name: "Apple devices find this server" })).not.toBeChecked();
    expect(screen.getByText(/Pley runs on Docker's own network/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByTestId("location")).toHaveTextContent("/setup/start");
    expect(puts(requests, "/api/network")).toHaveLength(0);
  });

  it("doesn't ask about Bonjour when the command line decides it", async () => {
    renderStep({ network: { ...NETWORK, fixed: { ...NETWORK.fixed, discovery: true, bonjour: true } } });
    expect(await screen.findByText("ds920plus")).toBeInTheDocument();
    expect(screen.queryByText("Apple devices")).not.toBeInTheDocument();
  });

  it("doesn't ask about Bonjour when Pley listens on this computer only", async () => {
    renderStep({ network: { ...NETWORK, web: ["127.0.0.1:7539"] } });
    expect(await screen.findByText("ds920plus")).toBeInTheDocument();
    expect(screen.queryByText("Apple devices")).not.toBeInTheDocument();
  });

  it("sends anyone but an administrator home", async () => {
    mockApi({});
    renderWithProviders(
      <Routes>
        <Route path="/setup/server" element={<SetupServerPage />} />
        <Route path="*" element={<LocationDisplay />} />
      </Routes>,
      { route: "/setup/server", user: { ...testUser, is_admin: false } },
    );
    expect(await screen.findByTestId("location")).toHaveTextContent(/^\/$/);
  });
});
