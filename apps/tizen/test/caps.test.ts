import { describe, expect, it, vi } from "vitest";
import { tierOf, tizenCaps } from "../src/caps";
import { avplayError, statusOf } from "../src/engine";
import { yearOfTizen, type TizenFacts } from "../src/facts";
import { keyMap, localeTag, REGISTERED, tizenPlatform } from "../src/platform";

function tv(over: Partial<TizenFacts>): TizenFacts {
  return {
    tizen: 6,
    year: 2021,
    chromium: 76,
    model: "QN65QN90AAFXZA",
    firmware: "T-NKMAKUC-1600.5",
    uhd: true,
    uhd8k: false,
    hdr: true,
    av1: true,
    truehd: false,
    ac4: true,
    outputMode: "PCM",
    ...over,
  };
}

/** Real sets, as Samsung's APIs describe them (docs/TV_PLATFORMS.md). */
const SETS: Record<string, TizenFacts> = {
  "2018 NU8000, Tizen 4.0, TV speakers": tv({ tizen: 4, year: 2018, chromium: 56, model: "UN55NU8000FXZA", av1: null, truehd: null, ac4: null }),
  "2019 Q90R, Tizen 5.0, Dolby Digital Plus to a receiver": tv({
    tizen: 5,
    year: 2019,
    chromium: 63,
    model: "QN65Q90RAFXZA",
    av1: null,
    truehd: null,
    ac4: null,
    outputMode: "DOLBY_DIGITAL_PLUS",
  }),
  "2019 Q900R 8K, Tizen 5.0": tv({ tizen: 5, year: 2019, chromium: 63, model: "QN65Q900RBFXZA", uhd8k: true, av1: null, truehd: null, ac4: null }),
  "2020 TU7000, Tizen 5.5": tv({ tizen: 5.5, year: 2020, chromium: 69, model: "UN50TU7000FXZA", av1: null, truehd: null, ac4: null }),
  "2021 QN90A, Tizen 6.0, Dolby Digital Plus to a receiver": tv({ outputMode: "DOLBY_DIGITAL_PLUS" }),
  "2023 S95C OLED, Tizen 7.0": tv({ tizen: 7, year: 2023, chromium: 94, model: "QN65S95CAFXZA" }),
  "2025 QN90F, Tizen 9.0": tv({ tizen: 9, year: 2025, chromium: 120, model: "QN65QN90FAFXZA" }),
  "2020 HD set T5300, Tizen 5.5, no HDR": tv({ tizen: 5.5, year: 2020, chromium: 69, model: "UN43T5300AFXZA", uhd: false, hdr: false, av1: null, truehd: null, ac4: null }),
};

describe("Samsung capabilities, per set", () => {
  for (const [name, facts] of Object.entries(SETS)) {
    it(name, () => {
      expect(tizenCaps(facts).caps).toMatchSnapshot();
    });
  }
});

describe("Samsung capabilities, rules", () => {
  it("never claims Dolby Vision: no Samsung TV shows it", () => {
    for (const facts of Object.values(SETS)) {
      const caps = tizenCaps(facts).caps;
      expect(caps.dolby_vision).toBe(false);
      expect(caps.hdr_types).not.toContain("dolby_vision");
      expect(caps.dolby_vision_profiles).toEqual([]);
    }
  });

  it("shows HDR10+ from 2017, HLG from 2018, on HDR panels only", () => {
    expect(tizenCaps(tv({ year: 2017, tizen: 3 })).caps.hdr_types).toEqual(["hdr10", "hdr10plus"]);
    expect(tizenCaps(tv({ year: 2018, tizen: 4 })).caps.hdr_types).toEqual(["hdr10", "hdr10plus", "hlg"]);
    expect(tizenCaps(tv({ hdr: false })).caps.hdr_types).toEqual([]);
  });

  it("decodes DTS only until 2017 and AV1 from 2020", () => {
    expect(tizenCaps(tv({ year: 2017, tizen: 3 })).caps.audio_decode).toContain("dts");
    expect(tizenCaps(tv({ year: 2018, tizen: 4 })).caps.audio_decode).not.toContain("dts");
    expect(tizenCaps(tv({ year: 2019, tizen: 5, av1: null })).caps.video_codecs).not.toContain("av1");
    expect(tizenCaps(tv({ year: 2020, tizen: 5.5, av1: null })).caps.video_codecs).toContain("av1");
    expect(tizenCaps(tv({ av1: false })).caps.video_codecs).not.toContain("av1");
  });

  it("sends Dolby to a receiver as the TV's sound output is set", () => {
    expect(tizenCaps(tv({ outputMode: "DOLBY_DIGITAL_PLUS" })).caps.audio_passthrough).toEqual(["ac3", "eac3"]);
    expect(tizenCaps(tv({ outputMode: "DOLBY" })).caps.audio_passthrough).toEqual(["ac3"]);
    expect(tizenCaps(tv({ outputMode: "PCM" })).caps.audio_passthrough).toEqual([]);
    expect(tizenCaps(tv({ outputMode: null })).caps.audio_passthrough).toEqual([]);
  });

  it("sends TrueHD and DTS only when Settings asks", () => {
    expect(tizenCaps(tv({}), { truehd: true, dts: true }).caps.audio_passthrough).toEqual(["truehd", "dts"]);
  });

  it("asks for one size per stream: AVPlay may go black moving between sizes", () => {
    for (const facts of Object.values(SETS)) expect(tizenCaps(facts).caps.hls.variants).toBeUndefined();
  });

  it("keeps fMP4 HLS for 2020 sets and later", () => {
    expect(tizenCaps(tv({ year: 2019, tizen: 5 })).caps.hls.segments).toEqual(["ts"]);
    expect(tizenCaps(tv({ year: 2020, tizen: 5.5 })).caps.hls.segments).toEqual(["ts", "fmp4"]);
  });

  it("rates sets by year", () => {
    expect(tierOf(tv({ year: 2018 }))).toBe("low");
    expect(tierOf(tv({ year: 2020 }))).toBe("mid");
    expect(tierOf(tv({ year: 2023 }))).toBe("high");
  });

  it("knows each Tizen version's year", () => {
    expect([4, 5, 5.5, 6, 6.5, 7, 8, 9, 10].map(yearOfTizen)).toEqual([2018, 2019, 2020, 2021, 2022, 2023, 2024, 2025, 2026]);
  });
});

describe("AVPlay", () => {
  it("reads its errors as the recovery ladder does", () => {
    expect(avplayError("PLAYER_ERROR_NOT_SUPPORTED_FILE", null, null)).toMatchObject({ kind: "unsupported", container: true });
    expect(avplayError("PLAYER_ERROR_NOT_SUPPORTED_FORMAT", null, null)).toMatchObject({ kind: "unsupported", container: false });
    expect(avplayError("PLAYER_ERROR_CONNECTION_FAILED", null, null)).toMatchObject({ kind: "network" });
    expect(avplayError("PLAYER_ERROR_CONNECTION_FAILED", null, 410)).toMatchObject({ kind: "stopped" });
    expect(avplayError("PLAYER_ERROR_CONNECTION_FAILED", null, 404)).toMatchObject({ kind: "http", status: 404 });
    expect(avplayError("PLAYER_ERROR_GENEREIC", null, null)).toMatchObject({ kind: "other" });
  });

  it("finds the HTTP status in its error messages", () => {
    expect(statusOf('{"http_response_header":"HTTP/1.1 410 Gone\\r\\n"}')).toBe(410);
    expect(statusOf('{"connect_error":{"curl":-23}}')).toBeNull();
  });
});

describe("Samsung's remote", () => {
  it("maps keys by the names the TV gives, else Samsung's documented codes", () => {
    const map = keyMap([
      { name: "MediaPlayPause", code: 10252 },
      { name: "ColorF0Red", code: 403 },
      { name: "Caption", code: 10221 },
      { name: "Info", code: 457 },
    ]);
    expect(map[10252]).toBe("playPause");
    expect(map[10009]).toBe("back");
    expect(map[10221]).toBe("captions");
    expect(keyMap(null)[415]).toBe("play");
  });

  it("has Live TV's channel keys, by name or by Samsung's codes", () => {
    const map = keyMap([
      { name: "ChannelUp", code: 427 },
      { name: "ChannelDown", code: 428 },
      { name: "ChannelList", code: 10073 },
    ]);
    expect([map[427], map[428], map[10073]]).toEqual(["channelUp", "channelDown", "channelList"]);
    expect(keyMap(null)[458]).toBe("guide");
    expect(REGISTERED).toEqual(expect.arrayContaining(["ChannelUp", "ChannelDown", "ChannelList", "Guide"]));
  });
});

describe("Samsung's language", () => {
  it("reads Tizen's LOCALE as a language tag", () => {
    expect(localeTag("fr_FR")).toBe("fr-FR");
    expect(localeTag("es_US.UTF-8")).toBe("es-US");
    expect(localeTag("en")).toBe("en");
    expect(localeTag("")).toBeNull();
    expect(localeTag(null)).toBeNull();
    expect(localeTag("not a locale")).toBeNull();
  });

  it("puts the TV's own language before the web engine's", () => {
    const asked: string[] = [];
    vi.stubGlobal("tizen", {
      systeminfo: {
        getPropertyValue: (property: string, ok: (l: { language: string; country: string }) => void) => {
          asked.push(property);
          ok({ language: "es_MX", country: "es_MX" });
        },
      },
    });
    const platform = tizenPlatform();
    expect(asked).toEqual(["LOCALE"]);
    expect(platform.languages!()).toEqual(["es-MX"].concat(navigator.languages));
  });

  it("goes by the web engine's languages where Tizen doesn't say", () => {
    expect(tizenPlatform().languages!()).toEqual(navigator.languages.slice());
  });
});
