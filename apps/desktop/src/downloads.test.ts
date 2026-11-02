import { describe as group, expect, it } from "vitest";
import { describe, formatBytes, fraction, serverName, type Download } from "./downloads";
import type { SavedServer } from "./servers";

const download = (over: Partial<Download>): Download => ({
  id: 1,
  server: "http://192.168.1.20:7539",
  name: "Vertigo (1958).mkv",
  size: 8_804_682_752,
  received: 0,
  state: "done",
  error: null,
  ...over,
});

group("downloads", () => {
  const saved: SavedServer[] = [{ url: "http://192.168.1.20:7539", name: "Home", id: null, last_used: 0 }];

  it("say where they came from by the server's name, else its address", () => {
    expect(serverName("http://192.168.1.20:7539", saved)).toBe("Home");
    expect(serverName("http://nas:7539", saved)).toBe("nas");
    expect(serverName("https://pley.example.com", [])).toBe("pley.example.com");
  });

  it("say how far they came", () => {
    expect(describe(download({}), "Home")).toBe("8.2 GB · Home");
    expect(describe(download({ state: "downloading", received: 3_650_722_201 }), "Home")).toBe("41% · 3.4 GB of 8.2 GB");
    expect(describe(download({ state: "downloading", size: null, received: 1_048_576 }), "Home")).toBe("1.0 MB · Home");
    expect(describe(download({ state: "queued" }), "Home")).toBe("Waiting its turn · Home");
    expect(describe(download({ state: "preparing", preparing: 0.42 }), "Home")).toBe("Preparing on Home · 42%");
    expect(describe(download({ state: "preparing", preparing: null }), "Home")).toBe("Preparing on Home");
    expect(describe(download({ state: "waiting" }), "Home")).toBe("Waiting for Home to answer");
    expect(describe(download({ state: "failed", error: "This file is no longer on the server." }), "Home")).toBe(
      "This file is no longer on the server.",
    );
  });

  it("fill their bar as the file comes, or as the server makes it", () => {
    expect(fraction(download({ state: "downloading", received: 4_402_341_376 }))).toBe(0.5);
    expect(fraction(download({ state: "preparing", preparing: 0.42, size: null }))).toBe(0.42);
    expect(fraction(download({ state: "queued", size: null }))).toBeNull();
  });

  it("say sizes as the web app does", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(150 * 1024 ** 2)).toBe("150 MB");
  });
});
