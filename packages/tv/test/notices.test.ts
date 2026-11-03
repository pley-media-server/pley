import { describe, expect, it } from "vitest";
import { noticesText, packageOf, TEXTS, type Shipped } from "../build/notices.ts";
import { licenseRows, THIRD_PARTY } from "../src/licenses";

/** The licenses of what the Samsung app ships: in Settings → About, and in full in the package's NOTICES.txt. */

const pkg = (id: string, text = `The MIT License (MIT)\n\nCopyright (c) ${id}`): Shipped => ({ id, version: "1.0.0", homepage: null, text });

describe("the apps' notices", () => {
  it("know which package a bundle input comes from", () => {
    expect(packageOf("../../node_modules/preact/dist/preact.module.js")).toBe("preact");
    expect(packageOf("..\\..\\node_modules\\@preact\\signals\\dist\\signals.mjs")).toBe("@preact/signals");
    expect(packageOf("../../node_modules/a/node_modules/lucide/dist/esm/icons/x.js")).toBe("lucide");
    expect(packageOf("../../packages/tv/src/index.tsx")).toBeNull();
  });

  it("list each package, then its license in full", () => {
    const text = noticesText("Pley for Samsung TV", [pkg("qrcode-generator", TEXTS["qrcode-generator"]!), pkg("preact")]);
    expect(text.startsWith("Pley for Samsung TV includes the following open-source software")).toBe(true);
    expect(text).toContain("- Preact: preact 1.0.0 (MIT)\n- QR Code generator (Kazuhiko Arase): qrcode-generator 1.0.0 (MIT)");
    expect(text).toContain("Copyright (c) 2009 Kazuhiko Arase");
    expect(text).toContain("Copyright (c) preact");
  });

  it("stop the build on a package nobody named, or one without a license text", () => {
    expect(() => noticesText("Pley for Samsung TV", [pkg("left-pad")])).toThrow(/licenses\.ts/);
    expect(() => noticesText("Pley for Samsung TV", [pkg("preact", " ")])).toThrow(/no license text/);
  });

  it("name each in About once, with its license", () => {
    const rows = licenseRows();
    expect(rows).toContainEqual(["Roboto font", "OFL-1.1"]);
    expect(rows).toContainEqual(["Lucide icons", "ISC, MIT"]);
    expect(rows.filter(([name]) => name === "Preact Signals")).toHaveLength(1);
    expect(rows).toHaveLength(new Set(Object.keys(THIRD_PARTY).map((k) => THIRD_PARTY[k]!.name)).size);
  });
});
