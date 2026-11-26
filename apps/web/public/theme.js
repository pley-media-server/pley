// The theme and accent of this device's last visit (src/theme), on the page
// before it first draws so no other flashes. A file of its own: the server's
// Content Security Policy runs no script written into the page.
try {
  var theme = localStorage.getItem("pley.theme");
  if (theme && /^[a-z]{1,32}$/.test(theme)) document.documentElement.setAttribute("data-theme", theme);
  var accent = localStorage.getItem("pley.accent");
  if (accent && /^[a-z]{1,32}$/.test(accent)) document.documentElement.setAttribute("data-accent", accent);
} catch (e) {
  // No storage: Midnight, in its own accent.
}
