import { afterEach, describe, expect, it, vi } from "vitest";
import type { TranscodeInfo } from "@/api/types";
import { makeItem, makeStream } from "@/test/fixtures";
import {
  cardSubtitle,
  channelsLabel,
  codecLabel,
  dynamicRangeBadge,
  dynamicRangeBadges,
  dynamicRangeLabel,
  episodeLabel,
  formatAgo,
  formatBitrate,
  formatBytes,
  formatClock,
  formatDate,
  formatDuration,
  formatRelative,
  formatRemaining,
  formatUptime,
  fullTitle,
  isPlayableVideo,
  mediaBadges,
  mediaLine,
  pluralize,
  progressFraction,
  resolutionLabel,
  seasonTitle,
  streamLabel,
  transcodeSummary,
  unwatchedCount,
} from "./format";

describe("formatDuration", () => {
  it("shows hours and minutes", () => {
    expect(formatDuration(6_720_000)).toBe("1h 52m");
    expect(formatDuration(2_700_000)).toBe("45m");
    expect(formatDuration(3_600_000)).toBe("1h");
  });

  it("rounds to whole minutes", () => {
    expect(formatDuration(7_170_000)).toBe("2h"); // 119.5 minutes
    expect(formatDuration(59_000)).toBe("1m");
  });

  it("shows seconds below half a minute", () => {
    expect(formatDuration(29_000)).toBe("29s");
    expect(formatDuration(400)).toBe("1s");
  });

  it("is empty without a duration", () => {
    expect(formatDuration(0)).toBe("");
    expect(formatDuration(null)).toBe("");
    expect(formatDuration(undefined)).toBe("");
    expect(formatDuration(-5_000)).toBe("");
  });
});

describe("formatClock", () => {
  it("pads minutes only with hours", () => {
    expect(formatClock(3723)).toBe("1:02:03");
    expect(formatClock(123)).toBe("2:03");
    expect(formatClock(36_000)).toBe("10:00:00");
  });

  it("drops fractions", () => {
    expect(formatClock(59.9)).toBe("0:59");
  });

  it("shows 0:00 for missing or invalid times", () => {
    for (const value of [null, undefined, Number.NaN, Number.POSITIVE_INFINITY, -1, 0]) {
      expect(formatClock(value)).toBe("0:00");
    }
  });
});

describe("formatRemaining", () => {
  it("shows what is left", () => {
    expect(formatRemaining(600_000, 2_640_000)).toBe("34m left");
  });

  it("is empty without a duration or once nothing is left", () => {
    expect(formatRemaining(5_000, null)).toBe("");
    expect(formatRemaining(2_640_000, 2_640_000)).toBe("");
    expect(formatRemaining(3_000_000, 2_640_000)).toBe("");
  });
});

describe("formatBytes", () => {
  it("picks a unit and keeps three significant digits", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(150 * 1024 ** 2)).toBe("150 MB");
    expect(formatBytes(5 * 1024 ** 4)).toBe("5.0 TB");
  });

  it("stops at terabytes", () => {
    expect(formatBytes(1024 ** 5)).toBe("1024 TB");
  });

  it("is empty for missing values", () => {
    expect(formatBytes(null)).toBe("");
    expect(formatBytes(Number.NaN)).toBe("");
  });
});

describe("formatBitrate", () => {
  it("uses Mbps from one megabit", () => {
    expect(formatBitrate(8_200_000)).toBe("8.2 Mbps");
    expect(formatBitrate(12_400_000)).toBe("12 Mbps");
    expect(formatBitrate(320_000)).toBe("320 kbps");
    expect(formatBitrate(null)).toBe("");
  });
});

describe("formatRelative", () => {
  const now = 1_700_000_000;
  const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("picks the largest unit that fits", () => {
    vi.useFakeTimers();
    vi.setSystemTime(now * 1000);
    expect(formatRelative(now - 30)).toBe(rtf.format(-30, "second"));
    expect(formatRelative(now - 180)).toBe(rtf.format(-3, "minute"));
    expect(formatRelative(now - 7200)).toBe(rtf.format(-2, "hour"));
    expect(formatRelative(now - 3 * 86_400)).toBe(rtf.format(-3, "day"));
    expect(formatRelative(now - 14 * 86_400)).toBe(rtf.format(-2, "week"));
    expect(formatRelative(now - 60 * 86_400)).toBe(rtf.format(-2, "month"));
    expect(formatRelative(now - 2 * 365 * 86_400)).toBe(rtf.format(-2, "year"));
    expect(formatRelative(now + 5 * 3600)).toBe(rtf.format(5, "hour"));
  });

  it("reads a past time a clock behind the server's puts ahead as now", () => {
    vi.useFakeTimers();
    vi.setSystemTime(now * 1000);
    expect(formatAgo(now + 2)).toBe(rtf.format(0, "second"));
    expect(formatAgo(now - 180)).toBe(rtf.format(-3, "minute"));
    expect(formatAgo(null)).toBe("never");
  });

  it("says never without a time", () => {
    expect(formatRelative(null)).toBe("never");
    expect(formatRelative(0)).toBe("never");
  });
});

describe("formatDate", () => {
  it("formats a calendar date in the local time zone", () => {
    const expected = new Date(2023, 3, 1).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
    expect(formatDate("2023-04-01")).toBe(expected);
  });

  it("returns what it can't parse", () => {
    expect(formatDate("sometime")).toBe("sometime");
    expect(formatDate(null)).toBe("");
  });
});

describe("formatUptime", () => {
  it("shows the two largest units", () => {
    expect(formatUptime(90_061)).toBe("1d 1h");
    expect(formatUptime(3_700)).toBe("1h 1m");
    expect(formatUptime(120)).toBe("2m");
    expect(formatUptime(59)).toBe("0m");
  });
});

describe("pluralize", () => {
  it("uses the singular for exactly one", () => {
    expect(pluralize(1, "movie")).toBe("1 movie");
    expect(pluralize(0, "movie")).toBe("0 movies");
    expect(pluralize(2, "movie")).toBe("2 movies");
    expect(pluralize(3, "child", "children")).toBe("3 children");
  });

  it("formats the count for the locale", () => {
    expect(pluralize(12_345, "movie")).toBe(`${(12_345).toLocaleString()} movies`);
  });
});

describe("episode and season labels", () => {
  it("labels episodes with what is known", () => {
    expect(episodeLabel({ parent_index: 1, index: 3 })).toBe("S1 · E3");
    expect(episodeLabel({ index: 3 })).toBe("E3");
    expect(episodeLabel({ parent_index: 0, index: 0 })).toBe("S0 · E0");
    expect(episodeLabel({})).toBe("");
  });

  it("names seasons", () => {
    expect(seasonTitle({ title: "The Final Season", index: 9 })).toBe("The Final Season");
    expect(seasonTitle({ title: "", index: 0 })).toBe("Specials");
    expect(seasonTitle({ title: "", index: 2 })).toBe("Season 2");
    expect(seasonTitle({ title: "", index: null })).toBe("Season");
  });
});

describe("resolutionLabel", () => {
  it("names the usual resolutions, cropped ones included", () => {
    expect(resolutionLabel(3840, 2160)).toBe("4K");
    expect(resolutionLabel(3840, 1600)).toBe("4K");
    expect(resolutionLabel(2560, 1440)).toBe("1440p");
    expect(resolutionLabel(1920, 800)).toBe("1080p");
    expect(resolutionLabel(1280, 720)).toBe("720p");
    expect(resolutionLabel(720, 576)).toBe("576p");
    expect(resolutionLabel(720, 480)).toBe("480p");
    expect(resolutionLabel(640, 360)).toBe("SD");
  });

  it("is null without dimensions", () => {
    expect(resolutionLabel(null, undefined)).toBeNull();
    expect(resolutionLabel(0, 0)).toBeNull();
  });
});

describe("codec and channel labels", () => {
  it("names known codecs and upper-cases the rest", () => {
    expect(codecLabel("h264")).toBe("H.264");
    expect(codecLabel("HEVC")).toBe("HEVC");
    expect(codecLabel("hdmv_pgs_subtitle")).toBe("PGS");
    expect(codecLabel("cinepak")).toBe("CINEPAK");
    expect(codecLabel(null)).toBe("");
  });

  it("names channel layouts", () => {
    expect(channelsLabel(1)).toBe("Mono");
    expect(channelsLabel(2)).toBe("Stereo");
    expect(channelsLabel(6)).toBe("5.1");
    expect(channelsLabel(8)).toBe("7.1");
    expect(channelsLabel(5)).toBe("5ch");
    expect(channelsLabel(undefined, "quad")).toBe("quad");
    expect(channelsLabel(0)).toBe("");
  });
});

describe("streamLabel", () => {
  it("describes audio by language, title and format", () => {
    expect(streamLabel(makeStream({ kind: "audio", codec: "eac3", language: "eng", channels: 6 }))).toBe("English (EAC3 5.1)");
    expect(streamLabel(makeStream({ kind: "audio", codec: "aac", language: "eng", channels: 2, title: "Commentary" }))).toBe(
      "English – Commentary (AAC Stereo)",
    );
    expect(streamLabel(makeStream({ kind: "audio", codec: "aac", channels: 2, title: "Director's track" }))).toBe(
      "Director's track (AAC Stereo)",
    );
    expect(streamLabel(makeStream({ kind: "audio", codec: "ac3", language: "eng", title: "English 5.1", channels: 6 }))).toBe(
      "English (AC3 5.1)",
    );
    expect(streamLabel(makeStream({ kind: "audio", codec: "opus" }))).toBe("Unknown (Opus)");
  });

  it("describes subtitles with their flags", () => {
    expect(
      streamLabel(makeStream({ kind: "subtitle", codec: "subrip", language: "fre", forced: true, external: true })),
    ).toBe("French (SRT, Forced, External)");
    expect(streamLabel(makeStream({ kind: "subtitle", codec: "hdmv_pgs_subtitle", language: "pt-BR", title: "SDH" }))).toBe(
      "Portuguese (BR) – SDH (PGS)",
    );
  });

  it("tells apart subtitles whose titles name their language", () => {
    const sub = (title: string) => streamLabel(makeStream({ kind: "subtitle", codec: "subrip", language: "eng", title }));
    // Forced by its title alone, as the server counts it: never two "English (SRT)".
    expect(sub("English")).toBe("English (SRT)");
    expect(sub("English (Forced)")).toBe("English (SRT, Forced)");
    expect(sub("Forced")).toBe("English (SRT, Forced)");
    expect(sub("Signs & Songs")).toBe("English – Signs & Songs (SRT, Forced)");
    expect(sub("English SDH")).toBe("English – SDH (SRT)");
    expect(sub("English [CC]")).toBe("English – CC (SRT)");
    // A word that only contains "forced" or "signs" doesn't count.
    expect(sub("Designs")).toBe("English – Designs (SRT)");
  });

  it("describes video by resolution, codec and dynamic range", () => {
    const dv = makeStream({
      codec: "hevc",
      width: 3840,
      height: 2160,
      dynamic_range: "dolby_vision",
      dv_profile: 8,
      dv_bl_compat: 1,
      bit_depth: 10,
    });
    expect(streamLabel(dv)).toBe("4K HEVC Dolby Vision P8.1 10-bit");
    expect(streamLabel(makeStream({ width: 1920, height: 1080, bit_depth: 8, dynamic_range: "sdr" }))).toBe("1080p H.264");
  });
});

describe("dynamic range", () => {
  it("labels each format", () => {
    expect(dynamicRangeLabel({ dynamic_range: "dolby_vision" })).toBe("Dolby Vision");
    expect(dynamicRangeLabel({ dynamic_range: "dolby_vision", dv_profile: 5, dv_bl_compat: 0 })).toBe("Dolby Vision P5");
    expect(dynamicRangeLabel({ dynamic_range: "dolby_vision", dv_profile: 8, dv_bl_compat: 4 })).toBe("Dolby Vision P8.4");
    // Profile 7's base layer (Blu-ray, compatibility 6) isn't named: never "P7.6".
    expect(dynamicRangeLabel({ dynamic_range: "dolby_vision", dv_profile: 7, dv_bl_compat: 6 })).toBe("Dolby Vision P7");
    expect(dynamicRangeLabel({ dynamic_range: "dolby_vision", dv_profile: 7, dv_bl_compat: 6, dv_el: "fel" })).toBe(
      "Dolby Vision P7 FEL",
    );
    expect(dynamicRangeLabel({ dynamic_range: "dolby_vision", dv_profile: 7, dv_bl_compat: 6, dv_el: "mel" })).toBe(
      "Dolby Vision P7 MEL",
    );
    expect(dynamicRangeLabel({ dynamic_range: "dolby_vision", dv_profile: 8, dv_bl_compat: 1, hdr10plus: true })).toBe(
      "Dolby Vision P8.1 + HDR10+",
    );
    expect(dynamicRangeLabel({ dynamic_range: "hdr10plus" })).toBe("HDR10+");
    expect(dynamicRangeLabel({ dynamic_range: "hdr10" })).toBe("HDR10");
    expect(dynamicRangeLabel({ dynamic_range: "hlg" })).toBe("HLG");
    expect(dynamicRangeLabel({ dynamic_range: "sdr", hdr: true })).toBeNull();
  });

  it("falls back to the HDR flag of older servers", () => {
    expect(dynamicRangeLabel({ hdr: true })).toBe("HDR");
    expect(dynamicRangeLabel({ hdr: false })).toBeNull();
    expect(dynamicRangeLabel(null)).toBeNull();
  });

  it("badges say DV, as Android and the TV apps do", () => {
    expect(dynamicRangeBadge({ dynamic_range: "dolby_vision", dv_profile: 8, dv_bl_compat: 1 })).toBe("DV P8.1");
    expect(dynamicRangeBadge({ dynamic_range: "dolby_vision", dv_profile: 7, dv_bl_compat: 6 })).toBe("DV P7");
    expect(dynamicRangeBadge({ dynamic_range: "dolby_vision", dv_profile: 7, dv_bl_compat: 6, dv_el: "fel" })).toBe("DV P7 FEL");
    expect(dynamicRangeBadge({ dynamic_range: "dolby_vision" })).toBe("DV");
    expect(dynamicRangeBadge({ dynamic_range: "hdr10plus" })).toBe("HDR10+");
    expect(dynamicRangeBadge({ hdr: true })).toBe("HDR");
    expect(dynamicRangeBadge({ dynamic_range: "sdr" })).toBeNull();
    expect(dynamicRangeBadge(undefined)).toBeNull();
  });

  it("Dolby Vision + HDR10+ takes both badges", () => {
    expect(dynamicRangeBadges({ dynamic_range: "dolby_vision", dv_profile: 8, dv_bl_compat: 1, hdr10plus: true })).toEqual([
      "DV P8.1",
      "HDR10+",
    ]);
    expect(dynamicRangeBadges({ dynamic_range: "dolby_vision", dv_profile: 8, dv_bl_compat: 1, hdr10plus: false })).toEqual([
      "DV P8.1",
    ]);
    expect(dynamicRangeBadges({ dynamic_range: "hdr10plus", hdr10plus: true })).toEqual(["HDR10+"]);
    expect(dynamicRangeBadges({ dynamic_range: "sdr" })).toEqual([]);
  });
});

describe("transcodeSummary", () => {
  const base: TranscodeInfo = {
    job_id: "j1",
    state: "running",
    video: "transcode",
    audio: "copy",
    video_in: "HEVC 3840×2160 Dolby Vision P8.1",
    video_out: "H.264 1920×1080",
    audio_in: "EAC3 5.1",
    audio_out: null,
    encoder: "h264_nvenc",
    decoder: "cuda",
    tonemap: "libplacebo",
    subtitle_burn: false,
    speed: 3.21,
    fps: 77,
    transcoded_ms: 0,
    segments_ready: 0,
    segments_total: 0,
    last_error: null,
  };

  it("lists conversions, hardware and speed", () => {
    expect(transcodeSummary(base)).toBe(
      "HEVC 3840×2160 Dolby Vision P8.1 → H.264 1920×1080 · EAC3 5.1 (copy) · h264_nvenc · cuda decode · libplacebo · 3.2×",
    );
  });

  it("leaves out the encoder when the video is copied", () => {
    expect(
      transcodeSummary({
        ...base,
        video: "copy",
        video_in: null,
        audio: "transcode",
        audio_in: "TrueHD 7.1",
        audio_out: "AAC Stereo",
        decoder: "cpu",
        tonemap: null,
        subtitle_burn: true,
        speed: null,
      }),
    ).toBe("Video (copy) · TrueHD 7.1 → AAC Stereo · burn-in subs");
  });

  it("says the CPU decodes only when the GPU does the rest", () => {
    expect(transcodeSummary({ ...base, decoder: "cpu", pipeline: "cpu", tonemap: null, speed: null })).toBe(
      "HEVC 3840×2160 Dolby Vision P8.1 → H.264 1920×1080 · EAC3 5.1 (copy) · h264_nvenc",
    );
    // The GPU couldn't decode it: why the transcode is slow.
    expect(transcodeSummary({ ...base, decoder: "cpu", pipeline: "hybrid", speed: null })).toBe(
      "HEVC 3840×2160 Dolby Vision P8.1 → H.264 1920×1080 · EAC3 5.1 (copy) · h264_nvenc · cpu decode · libplacebo",
    );
  });

  it("says which of a stream's sizes plays", () => {
    expect(transcodeSummary({ ...base, video_out: "H.264 1280×720", rendition: 1, variants: 3, speed: null })).toBe(
      "HEVC 3840×2160 Dolby Vision P8.1 → H.264 1280×720 · EAC3 5.1 (copy) · h264_nvenc · cuda decode · libplacebo · size 2 of 4",
    );
  });

  it("names a video transcode without details", () => {
    expect(
      transcodeSummary({ ...base, video_in: null, video_out: null, audio: "none", decoder: null, tonemap: null, speed: 0 }),
    ).toBe("Video transcode · h264_nvenc");
  });
});

describe("cardSubtitle", () => {
  it("describes each kind", () => {
    expect(cardSubtitle(makeItem({ kind: "movie", year: 1999 }))).toBe("1999");
    expect(cardSubtitle(makeItem({ kind: "movie" }))).toBe("");
    expect(cardSubtitle(makeItem({ kind: "show", child_count: 3, leaf_count: 30 }))).toBe("3 seasons");
    expect(cardSubtitle(makeItem({ kind: "show", child_count: 0, leaf_count: 1 }))).toBe("1 episode");
    expect(cardSubtitle(makeItem({ kind: "show", year: 2005 }))).toBe("2005");
    expect(cardSubtitle(makeItem({ kind: "season", leaf_count: 10 }))).toBe("10 episodes");
    expect(cardSubtitle(makeItem({ kind: "episode", parent_index: 2, index: 5 }))).toBe("S2 · E5");
    expect(cardSubtitle(makeItem({ kind: "artist", child_count: 1 }))).toBe("1 album");
    expect(cardSubtitle(makeItem({ kind: "album", parent_title: "Daft Punk", year: 2001 }))).toBe("Daft Punk · 2001");
    expect(cardSubtitle(makeItem({ kind: "track", grandparent_title: "Daft Punk", parent_title: "Discovery" }))).toBe(
      "Daft Punk · Discovery",
    );
    expect(cardSubtitle(makeItem({ kind: "extra", extra_type: "trailer" }))).toBe("");
  });
});

describe("fullTitle", () => {
  it("adds context to episodes, seasons and tracks", () => {
    expect(
      fullTitle(makeItem({ kind: "episode", grandparent_title: "The Office", parent_index: 2, index: 3, title: "Office Olympics" })),
    ).toBe("The Office — S2 · E3 — Office Olympics");
    expect(fullTitle(makeItem({ kind: "season", parent_title: "The Office", title: "", index: 2 }))).toBe(
      "The Office — Season 2",
    );
    expect(fullTitle(makeItem({ kind: "track", title: "One More Time", grandparent_title: "Daft Punk" }))).toBe(
      "One More Time — Daft Punk",
    );
  });

  it("adds the year to movies", () => {
    expect(fullTitle(makeItem({ title: "Heat", year: 1995 }))).toBe("Heat (1995)");
    expect(fullTitle(makeItem({ title: "Heat" }))).toBe("Heat");
  });
});

describe("watch progress", () => {
  const state = (position_ms: number) => ({ position_ms, watched: false, play_count: 0 });

  it("is the played fraction, clamped", () => {
    expect(progressFraction({ user_state: state(30_000), duration_ms: 120_000 })).toBe(0.25);
    expect(progressFraction({ user_state: state(200_000), duration_ms: 120_000 })).toBe(1);
    expect(progressFraction({ user_state: state(30_000), duration_ms: null })).toBe(0);
    expect(progressFraction({ user_state: state(0), duration_ms: 120_000 })).toBe(0);
  });

  it("counts unwatched episodes", () => {
    expect(unwatchedCount(makeItem({ kind: "show", leaf_count: 10, viewed_leaf_count: 4 }))).toBe(6);
    expect(unwatchedCount(makeItem({ kind: "show", leaf_count: 10 }))).toBe(10);
    expect(unwatchedCount(makeItem({ kind: "show", leaf_count: 3, viewed_leaf_count: 5 }))).toBe(0);
    expect(unwatchedCount(makeItem({ kind: "movie" }))).toBe(0);
  });

  it("knows what the player plays", () => {
    expect(isPlayableVideo({ kind: "movie" })).toBe(true);
    expect(isPlayableVideo({ kind: "episode" })).toBe(true);
    expect(isPlayableVideo({ kind: "extra" })).toBe(true);
    expect(isPlayableVideo({ kind: "show" })).toBe(false);
    expect(isPlayableVideo({ kind: "track" })).toBe(false);
  });
});

describe("media summaries", () => {
  it("say on the poster what a title's files are like", () => {
    const media = { bitrate: 42_000_000, size: 25_100_000_000, width: 3840, height: 2160, video_codec: "hevc", dynamic_range: "dolby_vision" } as const;
    expect(mediaBadges(media)).toEqual(["4K", "DV"]);
    expect(mediaLine(media)).toBe("42 Mbps · 23.4 GB");
    expect(mediaBadges({ ...media, width: 1920, height: 1080, dynamic_range: "sdr", video_codec: "h264" })).toEqual([]);
    expect(mediaBadges({ ...media, width: 1920, height: 1080, dynamic_range: "hdr10" })).toEqual(["HDR10"]);
  });

  it("make do with what is known", () => {
    const unknown = { bitrate: null, size: null, width: null, height: null, video_codec: null, dynamic_range: null };
    expect(mediaBadges(unknown)).toEqual([]);
    expect(mediaLine(unknown)).toBe("Bitrate unknown");
    expect(mediaLine(undefined)).toBe("");
  });
});
