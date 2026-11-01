/**
 * Others' work the Samsung app ships, by npm package: Settings →
 * About lists it, and the build (build/notices.ts) puts each package's
 * license text in the package's NOTICES.txt. The build stops on a package
 * in the bundle that isn't named here, so neither list falls behind.
 */
export const THIRD_PARTY: Record<string, { name: string; license: string }> = {
  preact: { name: "Preact", license: "MIT" },
  "@preact/signals": { name: "Preact Signals", license: "MIT" },
  "@preact/signals-core": { name: "Preact Signals", license: "MIT" },
  // Some of Lucide's icons come from Feather, under MIT.
  lucide: { name: "Lucide icons", license: "ISC, MIT" },
  "qrcode-generator": { name: "QR Code generator (Kazuhiko Arase)", license: "MIT" },
  "@fontsource/roboto": { name: "Roboto font", license: "OFL-1.1" },
};

/** About's rows: each name once, with its license. */
export function licenseRows(): [string, string][] {
  const rows: [string, string][] = [];
  for (const k of Object.keys(THIRD_PARTY)) {
    const p = THIRD_PARTY[k]!;
    if (!rows.some(([name]) => name === p.name)) rows.push([p.name, p.license]);
  }
  return rows;
}
