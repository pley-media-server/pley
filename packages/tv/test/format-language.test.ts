import { afterEach, describe, expect, it } from "vitest";
import type { MediaFile, MediaStream } from "../src/api/types";
import {
  automaticAudioLabel,
  dayAndTime,
  episodeLabel,
  formatBitrate,
  formatBytes,
  formatDuration,
  formatRuntime,
  formatTimeLeft,
  languageName,
  pluralize,
  relativeTime,
  seasonTitle,
  shortDate,
  streamLabel,
  timeUntil,
  twoLetterCode,
} from "../src/format/format";
import { mediaFacts, streamBlock } from "../src/format/media-info";
import { setLanguage } from "../src/i18n";

/** What files, times and counts are like in French and Spanish; English is pinned by the other tests. */

const subtitle = (over: Partial<MediaStream>): MediaStream =>
  ({ index: 3, kind: "subtitle", codec: "subrip", language: "eng", title: null, default: false, forced: false, ...over }) as MediaStream;

afterEach(() => setLanguage("en"));

describe("languages and tracks", () => {
  it("are named in the language shown", () => {
    setLanguage("fr");
    expect(languageName("eng")).toBe("Anglais");
    expect(languageName("en")).toBe("Anglais");
    expect(languageName("pt-BR")).toBe("Portugais (BR)");
    expect(languageName("und")).toBe("Langue inconnue");
    expect(languageName("xx")).toBe("XX");
    setLanguage("es");
    expect(languageName("ger")).toBe("Alemán");
    expect(languageName("cmn")).toBe("Mandarín");
  });

  it("find a language's English name in a track's title", () => {
    setLanguage("fr");
    expect(streamLabel(subtitle({ title: "English SDH", forced: true }))).toBe("Anglais – SDH (SRT, Forcés)");
    expect(streamLabel(subtitle({ title: "Anglais" }))).toBe("Anglais (SRT)");
    expect(streamLabel(subtitle({ language: null, title: null, external: true }))).toBe("Inconnu (SRT, Externes)");
    const audio = { index: 1, kind: "audio", codec: "eac3", channels: 2, language: "spa", title: "Commentary", default: true, forced: false } as MediaStream;
    setLanguage("es");
    expect(streamLabel(audio)).toBe("Español – Commentary (EAC3 Estéreo)");
    expect(automaticAudioLabel({ streams: [audio], selected_audio: 1 } as unknown as MediaFile)).toBe("Automática (Español)");
  });

  it("keep their two-letter codes, whatever the language shown", () => {
    setLanguage("fr");
    expect(twoLetterCode("fre")).toBe("fr");
    expect(twoLetterCode("chi")).toBe("zh");
    expect(twoLetterCode("tgl")).toBe("tl");
    expect(twoLetterCode("fil")).toBe("fil");
    expect(twoLetterCode("xyz")).toBe("xyz");
  });
});

describe("durations, counts, episodes and sizes", () => {
  it("are written as French writes them", () => {
    setLanguage("fr");
    expect(formatDuration(6_720_000)).toBe("1 h 52 min");
    expect(formatDuration(29_000)).toBe("29 s");
    expect(formatRuntime(2_700_000)).toBe("45 min");
    expect(formatTimeLeft(0, 1_560_000)).toBe("Reste 26 min");
    expect(episodeLabel(1, 3)).toBe("S1 · É3");
    expect(seasonTitle(null, 0)).toBe("Épisodes spéciaux");
    expect(seasonTitle(null, 2)).toBe("Saison 2");
    // French counts 0 as one.
    expect(pluralize(0, "episode")).toBe("0 épisode");
    expect(pluralize(2, "season")).toBe("2 saisons");
    expect(pluralize(12_345, "movie")).toBe(`${new Intl.NumberFormat("fr").format(12_345)} films`);
    expect(formatBytes(1536)).toBe("1,5 Ko");
    expect(formatBitrate(8_200_000)).toBe("8,2 Mbps");
  });

  it("are written as Spanish writes them", () => {
    setLanguage("es");
    expect(formatDuration(6_720_000)).toBe("1 h 52 min");
    expect(episodeLabel(2, 5)).toBe("T2 · E5");
    expect(pluralize(1, "album")).toBe("1 álbum");
    expect(pluralize(2, "album")).toBe("2 álbumes");
    expect(formatBytes(1536)).toBe("1,5 KB");
  });

  it("count English words the format area doesn't have in English", () => {
    setLanguage("fr");
    expect(pluralize(2, "more movie", "more movies")).toBe("2 more movies");
  });
});

describe("relative times and dates", () => {
  const now = new Date(2026, 10, 5, 12).getTime();
  const at = now / 1000;

  it("are in the language's words, dates as Intl writes them", () => {
    setLanguage("fr");
    expect(relativeTime(at - 10, now)).toBe("à l’instant");
    expect(relativeTime(at - 3 * 3600, now)).toBe("il y a 3 heures");
    expect(relativeTime(at - 86_400, now)).toBe("hier");
    expect(timeUntil(at + 10, now)).toBe("maintenant");
    expect(timeUntil(at + 86_400, now)).toBe("demain");
    expect(timeUntil(at + 3 * 86_400, now)).toBe("dans 3 jours");
    expect(dayAndTime(at + 3600, { time: true, hours24: true }, now)).toBe("aujourd’hui à 13:00");
    const later = at + 5 * 86_400;
    expect(shortDate(later, now)).toBe(new Intl.DateTimeFormat("fr", { month: "short", day: "numeric" }).format(new Date(later * 1000)));
    setLanguage("es");
    expect(relativeTime(at - 3 * 86_400, now)).toBe("hace 3 días");
    expect(timeUntil(at + 2 * 3600, now)).toBe("en 2 horas");
    expect(dayAndTime(at + 3600, { time: true, hours24: true }, now)).toBe("hoy a las 13:00");
  });
});

describe("the media info", () => {
  it("names its facts in the language shown", () => {
    setLanguage("fr");
    expect(streamBlock(subtitle({ forced: true }), "Sous-titres").facts).toEqual([
      { label: "Format", value: "SRT, texte" },
      { label: "Langue", value: "Anglais" },
      { label: "Attributs", value: "Forcés" },
    ]);
    const file = { id: 1, path: "/a/b.mkv", container: "mkv", size: 1536, streams: [] } as unknown as MediaFile;
    expect(mediaFacts(file)[0]).toEqual({
      title: "Fichier",
      facts: [
        { label: "Nom", value: "b.mkv" },
        { label: "Conteneur", value: "MKV" },
        { label: "Taille", value: "1,5 Ko" },
      ],
    });
  });
});
