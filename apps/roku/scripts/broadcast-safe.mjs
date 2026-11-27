// Roku's broadcast-safe range, which its certification (requirement 6.4)
// asks of a channel's store artwork and splash screen: a color's luma
// (BT.709, on its 0-255 channels) no darker than #161616's and no lighter
// than #EBEBEB's, as bslint's color-cert rule reckons it. The app's own
// screens draw the themes' exact colors. scripts/render-icons.mjs at the
// repository's root puts the splash screens and the home screen's poster
// artwork through it, and broadcast-check.mjs holds them to the range.

const WEIGHTS = [0.2126, 0.7152, 0.0722];
const luma = (c) => WEIGHTS[0] * c[0] + WEIGHTS[1] * c[1] + WEIGHTS[2] * c[2];
const DARKEST = luma([0x16, 0x16, 0x16]);
const LIGHTEST = luma([0xeb, 0xeb, 0xeb]);

/**
 * A color ("#rrggbb") within the range, moved the least way that keeps its
 * hue: each channel raised (or lowered) by the same amount, a channel at
 * its end leaving the rest to the others, until the luma reaches the
 * range's end. A color within the range comes back as it is.
 */
export function broadcastSafe(hex) {
  let c = [1, 3, 5].map((at) => parseInt(hex.slice(at, at + 2), 16));
  const start = luma(c);
  if (start >= DARKEST && start <= LIGHTEST) return hex.toLowerCase();
  const up = start < DARKEST;
  const target = up ? DARKEST : LIGHTEST;
  for (let pass = 0; pass < 3; pass++) {
    const free = c.map((v) => (up ? v < 255 : v > 0));
    const weight = free.reduce((sum, f, k) => sum + (f ? WEIGHTS[k] : 0), 0);
    if (weight === 0) break;
    const step = (target - luma(c)) / weight;
    c = c.map((v, k) => (free[k] ? Math.min(255, Math.max(0, v + step)) : v));
  }
  c = c.map(Math.round);
  while (luma(c) < DARKEST) c = c.map((v) => Math.min(255, v + 1));
  while (luma(c) > LIGHTEST) c = c.map((v) => Math.max(0, v - 1));
  return "#" + c.map((v) => v.toString(16).padStart(2, "0")).join("");
}
