// The theme and accent of this device's last visit (src/theme), on the page
// before it first draws so no other flashes. A file of its own: the server's
// Content Security Policy runs no script written into the page.
// Each theme's page color and kind, for the browser's own bars (a phone's, an
// installed app's) until the app draws: the styles load after this file, so
// it can't read them. palettes.test.ts keeps them as index.css has them.
var BARS = {
  midnight: ["#0b0d10", "dark"],
  midday: ["#f3f5f8", "light"],
  twilight: ["#140e22", "dark"],
  starlight: ["#f6f0e5", "light"],
  moonlight: ["#0c0d0e", "dark"],
  nightshade: ["#0a0f0c", "dark"],
  greenlight: ["#f1f6f2", "light"],
  sunshine: ["#fff7e0", "light"],
  blacklight: ["#000000", "dark"],
};
try {
  var theme = localStorage.getItem("pley.theme");
  if (theme && /^[a-z]{1,32}$/.test(theme)) document.documentElement.setAttribute("data-theme", theme);
  var accent = localStorage.getItem("pley.accent");
  if (accent && /^[a-z]{1,32}$/.test(accent)) document.documentElement.setAttribute("data-accent", accent);
  var bars = Object.prototype.hasOwnProperty.call(BARS, theme) ? BARS[theme] : null;
  if (bars) {
    document.querySelector('meta[name="theme-color"]').setAttribute("content", bars[0]);
    document.querySelector('meta[name="color-scheme"]').setAttribute("content", bars[1]);
  }
} catch (e) {
  // No storage: Midnight, in its own accent.
}
