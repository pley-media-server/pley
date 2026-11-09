import { describe, expect, it } from "vitest";
import { browserMetadataLanguage, metadataLanguageChoices } from "./metadataLanguages";

describe("browserMetadataLanguage", () => {
  it("takes the browser's language as it is, else in the country, else its first, else US English", () => {
    expect(browserMetadataLanguage(["pt-BR", "en"], "PT")).toBe("pt-BR");
    expect(browserMetadataLanguage(["es"], "MX")).toBe("es-MX");
    expect(browserMetadataLanguage(["de-AT"], "AT")).toBe("de-DE");
    expect(browserMetadataLanguage(["fr_CA"], "CA")).toBe("fr-FR");
    expect(browserMetadataLanguage(["en-NZ"], "NZ")).toBe("en-US");
    expect(browserMetadataLanguage(["eo", "ja"], "JP")).toBe("ja-JP");
    expect(browserMetadataLanguage(["eo"], "US")).toBe("en-US");
    expect(browserMetadataLanguage([], "GB")).toBe("en-US");
  });
});

describe("metadataLanguageChoices", () => {
  it("names each language, and keeps one saved that isn't offered", () => {
    const choices = metadataLanguageChoices("en-NZ");
    expect(choices.find((c) => c.value === "en-GB")?.label).toBe("British English");
    expect(choices.some((c) => c.value === "en-NZ")).toBe(true);
    expect(metadataLanguageChoices("en-US").some((c) => c.value === "en-NZ")).toBe(false);
  });
});
