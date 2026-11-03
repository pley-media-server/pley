import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { MediaStream } from "@/api/types";
import { qualitiesFor } from "./quality";
import { SettingsPopover, type SettingsPopoverProps } from "./SettingsPopover";
import { DEFAULT_SUBTITLE_STYLE } from "./subtitles";

function renderPopover(overrides: Partial<SettingsPopoverProps> = {}) {
  const props: SettingsPopoverProps = {
    open: true,
    setOpen: vi.fn(),
    qualities: qualitiesFor({ bitrate: 9_000_000, width: 1920, height: 1080 }),
    quality: "original",
    originalHint: "1080p · 9.0 Mbps",
    onQuality: vi.fn(),
    audioStreams: [],
    audioIndex: null,
    onAudio: vi.fn(),
    subtitleStreams: [],
    subtitleIndex: null,
    onSubtitle: vi.fn(),
    subtitleStyle: DEFAULT_SUBTITLE_STYLE,
    onSubtitleStyle: vi.fn(),
    subtitleOffset: 0,
    onSubtitleOffset: vi.fn(),
    speed: 1,
    onSpeed: vi.fn(),
    sleep: null,
    sleepLeft: null,
    sleepEndLabel: "End of this episode",
    onSleep: vi.fn(),
    ...overrides,
  };
  const { rerender } = render(<SettingsPopover {...props} />);
  return { ...props, rerender: (more: Partial<SettingsPopoverProps>) => rerender(<SettingsPopover {...props} {...more} />) };
}

describe("SettingsPopover", () => {
  it("has one menu for quality, no separate playback method", () => {
    renderPopover();
    expect(screen.getByRole("button", { name: /Quality\s*Original/ })).toBeInTheDocument();
    expect(screen.queryByText("Playback")).not.toBeInTheDocument();
  });

  it("offers the original, Automatic and the lower qualities, without saying how they're made", async () => {
    const user = userEvent.setup();
    const props = renderPopover();
    await user.click(screen.getByRole("button", { name: /Quality/ }));
    const options = screen.getAllByRole("menuitemradio");
    expect(options.map((o) => o.textContent)).toEqual([
      "Original1080p · 9.0 Mbps",
      "AutomaticDynamically adjust to conditions",
      // 8 Mbps is too close to the file's 9 to play any better.
      "4 Mbps 720p",
      "2 Mbps 480p",
    ]);
    expect(options[0]).toHaveAttribute("aria-checked", "true");

    await user.click(screen.getByRole("menuitemradio", { name: /4 Mbps/ }));
    expect(props.onQuality).toHaveBeenCalledWith("4m");
    expect(props.setOpen).toHaveBeenCalledWith(false);
  });

  it("says what Automatic plays, and why when lower", async () => {
    const user = userEvent.setup();
    renderPopover({ quality: "auto", autoNow: "Original" });
    expect(screen.getByRole("button", { name: /Quality\s*Automatic · Original/ })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Quality/ }));
    const automatic = screen.getByRole("menuitemradio", { name: /Automatic/ });
    expect(automatic).toHaveAttribute("aria-checked", "true");
    expect(automatic).toHaveTextContent("Now Original");
    // The Original row stays the file as it is.
    expect(screen.getAllByRole("menuitemradio")[0]).toHaveTextContent("Original1080p · 9.0 Mbps");
  });

  it("says when Automatic went lower, and lets it be picked again to start afresh", async () => {
    const user = userEvent.setup();
    const props = renderPopover({ quality: "auto", autoNow: "4 Mbps 720p", autoWhy: "Lowered for your connection" });
    expect(screen.getByRole("button", { name: /Quality\s*Automatic · 4 Mbps 720p/ })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Quality/ }));
    const automatic = screen.getByRole("menuitemradio", { name: /Automatic/ });
    expect(automatic).toHaveTextContent("Now 4 Mbps 720p · Lowered for your connection");
    await user.click(automatic);
    expect(props.onQuality).toHaveBeenCalledWith("auto");
  });

  const english: MediaStream = {

    index: 3,
    kind: "subtitle",
    codec: "subrip",
    language: "eng",
    title: null,
    default: false,
    forced: false,
    text_based: true,
  };

  it("styles text subtitles: their size, background, font and position", async () => {
    const user = userEvent.setup();
    const props = renderPopover({ subtitleStreams: [english], subtitleIndex: 3 });
    await user.click(screen.getByRole("button", { name: /Subtitle style\s*Normal/ }));
    await user.click(screen.getByRole("menuitemradio", { name: "Large" }));
    expect(props.onSubtitleStyle).toHaveBeenCalledWith({ ...DEFAULT_SUBTITLE_STYLE, size: "large" });
    await user.click(screen.getByRole("menuitemradio", { name: "Dark box" }));
    expect(props.onSubtitleStyle).toHaveBeenCalledWith({ ...DEFAULT_SUBTITLE_STYLE, background: "box" });
    expect(screen.getAllByRole("menuitemradio").map((o) => o.textContent)).toEqual([
      ...["Small", "Normal", "Large", "Huge"],
      ...["None", "Dark box"],
      ...["Default", "Serif", "Monospace", "Casual"],
      ...["Bottom", "A little higher", "Higher", "Highest"],
    ]);
    await user.click(screen.getByRole("menuitemradio", { name: "Serif" }));
    expect(props.onSubtitleStyle).toHaveBeenCalledWith({ ...DEFAULT_SUBTITLE_STYLE, font: "serif" });
    await user.click(screen.getByRole("menuitemradio", { name: "Higher" }));
    expect(props.onSubtitleStyle).toHaveBeenCalledWith({ ...DEFAULT_SUBTITLE_STYLE, position: "higher" });
  });

  it("plays faster or slower, from half to twice the speed", async () => {
    const user = userEvent.setup();
    const props = renderPopover({ speed: 1.5 });
    await user.click(screen.getByRole("button", { name: /Speed\s*1.5×/ }));
    expect(screen.getAllByRole("menuitemradio").map((o) => o.textContent)).toEqual(["0.5×", "0.75×", "Normal", "1.25×", "1.5×", "2×"]);
    expect(screen.getByRole("menuitemradio", { name: "1.5×" })).toHaveAttribute("aria-checked", "true");
    await user.click(screen.getByRole("menuitemradio", { name: "Normal" }));
    expect(props.onSpeed).toHaveBeenCalledWith(1);
    expect(props.setOpen).toHaveBeenCalledWith(false);
  });

  it("offers a style only while text subtitles show: pictures keep their own look", () => {
    const pgs: MediaStream = { ...english, index: 4, codec: "hdmv_pgs_subtitle", text_based: false };
    const { rerender } = renderPopover({ subtitleStreams: [english, pgs] });
    // Off: nothing to style.
    expect(screen.queryByRole("button", { name: /Subtitle style/ })).not.toBeInTheDocument();
    rerender({ subtitleIndex: 4 });
    expect(screen.queryByRole("button", { name: /Subtitle style/ })).not.toBeInTheDocument();
    rerender({ subtitleIndex: 3 });
    expect(screen.getByRole("button", { name: /Subtitle style\s*Normal/ })).toBeInTheDocument();
  });

  it("goes back to the menu from the style when the subtitles go off", async () => {
    const user = userEvent.setup();
    const { rerender } = renderPopover({ subtitleStreams: [english], subtitleIndex: 3 });
    await user.click(screen.getByRole("button", { name: /Subtitle style/ }));
    expect(screen.getByRole("menuitemradio", { name: "Large" })).toBeInTheDocument();
    rerender({ subtitleIndex: null });
    expect(screen.queryByRole("menuitemradio", { name: "Large" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Subtitles\s*Off/ })).toBeInTheDocument();
  });

  it("moves text subtitles earlier or later, 0.1 s at a time", async () => {
    const user = userEvent.setup();
    const props = renderPopover({ subtitleStreams: [english], subtitleIndex: 3, subtitleOffset: 0.5 });
    // Said next to the subtitles chosen.
    await user.click(screen.getByRole("button", { name: /Subtitles.*\+0\.5 s/ }));
    expect(screen.getByTestId("subtitle-offset")).toHaveTextContent("+0.5 s");

    await user.click(screen.getByRole("button", { name: "Show subtitles later" }));
    expect(props.onSubtitleOffset).toHaveBeenLastCalledWith(0.6);
    await user.click(screen.getByRole("button", { name: "Show subtitles earlier" }));
    expect(props.onSubtitleOffset).toHaveBeenLastCalledWith(0.4);
    await user.click(screen.getByRole("button", { name: "Reset" }));
    expect(props.onSubtitleOffset).toHaveBeenLastCalledWith(0);
    // The menu stays open to try again.
    expect(props.setOpen).not.toHaveBeenCalled();
  });

  it("stops at ten seconds, and has nothing to reset at 0", async () => {
    const user = userEvent.setup();
    renderPopover({ subtitleStreams: [english], subtitleIndex: 3, subtitleOffset: 10 });
    await user.click(screen.getByRole("button", { name: /Subtitles/ }));
    expect(screen.getByRole("button", { name: "Show subtitles later" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Reset" })).toBeEnabled();
  });

  it("lists picture subtitles by name alone", async () => {
    const user = userEvent.setup();
    const pgs: MediaStream = { ...english, index: 4, codec: "hdmv_pgs_subtitle", text_based: false };
    renderPopover({ subtitleStreams: [pgs] });
    await user.click(screen.getByRole("button", { name: /Subtitles/ }));
    expect(screen.getAllByRole("menuitemradio")[1]).not.toHaveTextContent("burn");
  });

  it("opens the playback details from the bottom of the menu, not at its top", async () => {
    const user = userEvent.setup();
    renderPopover({ details: <p>Direct play</p> });
    expect(screen.queryByText("Direct play")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Playback details" }));
    expect(screen.getByText("Direct play")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Playback details" }));
    expect(screen.getByRole("button", { name: /Quality/ })).toBeInTheDocument();
  });

  it("doesn't offer to move subtitles that are off or burned in", async () => {
    const user = userEvent.setup();
    const pgs: MediaStream = { ...english, index: 4, codec: "hdmv_pgs_subtitle", text_based: false };
    renderPopover({ subtitleStreams: [english, pgs], subtitleIndex: 4, subtitleOffset: 0.5 });
    expect(screen.getByRole("button", { name: /Subtitles/ })).not.toHaveTextContent("+0.5 s");
    await user.click(screen.getByRole("button", { name: /Subtitles/ }));
    expect(screen.queryByText("Timing")).not.toBeInTheDocument();
  });

  it("sets the sleep timer", async () => {
    const user = userEvent.setup();
    const props = renderPopover();
    await user.click(screen.getByRole("button", { name: /Sleep timer\s*Off/ }));
    expect(screen.getAllByRole("menuitemradio").map((o) => o.textContent)).toEqual([
      "Off",
      "15 minutes",
      "30 minutes",
      "45 minutes",
      "60 minutes",
      "90 minutes",
      "End of this episode",
    ]);
    await user.click(screen.getByRole("menuitemradio", { name: "45 minutes" }));
    expect(props.onSleep).toHaveBeenCalledWith(45);
    expect(props.setOpen).toHaveBeenCalledWith(false);
  });

  it("says how long the sleep timer has left", async () => {
    const user = userEvent.setup();
    const props = renderPopover({ sleep: 30, sleepLeft: 754 });
    await user.click(screen.getByRole("button", { name: /Sleep timer\s*12:34 left/ }));
    expect(screen.getByRole("menuitemradio", { name: /30 minutes/ })).toHaveAttribute("aria-checked", "true");
    await user.click(screen.getByRole("menuitemradio", { name: "Off" }));
    expect(props.onSleep).toHaveBeenCalledWith(null);
  });
});
