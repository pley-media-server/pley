import { describe, expect, it } from "vitest";
import {
  CODE_LETTERS,
  codeLetters,
  hasStrayLetter,
  isQuickConnectCode,
  isWholeCode,
  quickConnectCodeInHash,
  showCode,
  typedCode,
} from "./userCode";

describe("the address of Jellyfin's web app that apps open", () => {
  it("gives its Quick Connect code, whole, and nothing else", () => {
    expect(quickConnectCodeInHash("#/quickconnect?code=123456&serverId=abc")).toBe("123456");
    expect(quickConnectCodeInHash("#/QuickConnect?serverId=abc&code=000042")).toBe("000042");
    for (const hash of [
      "",
      "#",
      "#/quickconnect",
      "#/quickconnect?code=12345",
      "#/quickconnect?code=1234567",
      "#/quickconnect?code=12a456",
      "#/quickconnect?code=%3Cscript%3E",
      "#/home?code=123456",
      "#/x/quickconnect?code=123456",
    ]) {
      expect(quickConnectCodeInHash(hash), hash).toBe("");
    }
  });
});

describe("the codes TVs show", () => {
  it("are 20 consonants: no vowels, no Y, no digits", () => {
    expect(CODE_LETTERS).toHaveLength(20);
    expect(CODE_LETTERS).toMatch(/^[^AEIOUY0-9]+$/);
  });

  it("take the letters as people type them, in capitals", () => {
    expect(codeLetters("bcdf-ghjk")).toBe("BCDFGHJK");
    expect(codeLetters(" BCDF GHJK ")).toBe("BCDFGHJK");
    expect(codeLetters("bc2d.f_g")).toBe("BCDFG");
    // Only the first 8, and nothing a capital would turn into letters.
    expect(codeLetters("BCDFGHJKLM")).toBe("BCDFGHJK");
    expect(codeLetters("ßbcd")).toBe("BCD");
    expect(codeLetters("")).toBe("");
  });

  it("show as the TV does, as far as they're typed", () => {
    expect(showCode("")).toBe("");
    expect(showCode("BCDF")).toBe("BCDF");
    expect(showCode("BCDFG")).toBe("BCDF-G");
    expect(showCode("BCDFGHJK")).toBe("BCDF-GHJK");
  });

  it("are whole with 8 of their letters", () => {
    expect(isWholeCode("BCDFGHJK")).toBe(true);
    expect(isWholeCode("ZXWVTSRQ")).toBe(true);
    expect(isWholeCode("BCDFGHJ")).toBe(false);
    expect(isWholeCode("BCDFGHJA")).toBe(false);
    expect(hasStrayLetter("BCDFGHJA")).toBe(true);
    expect(hasStrayLetter("BCDY")).toBe(true);
    expect(hasStrayLetter("BCD")).toBe(false);
  });
});

describe("Jellyfin apps' Quick Connect codes", () => {
  it("are 6 digits, told from a TV's code by what is typed first", () => {
    expect(typedCode("123 456")).toBe("123456");
    expect(typedCode(" 12-34a56 7")).toBe("123456");
    expect(typedCode("bcdf 2ghjk")).toBe("BCDFGHJK");
    expect(typedCode("")).toBe("");
    expect(isQuickConnectCode("123456")).toBe(true);
    expect(isQuickConnectCode("12345")).toBe(false);
    expect(isQuickConnectCode("BCDFGH")).toBe(false);
    expect(isWholeCode("123456")).toBe(false);
  });

  it("show as the app does", () => {
    expect(showCode("123456")).toBe("123456");
    expect(showCode("12345")).toBe("12345");
  });
});
