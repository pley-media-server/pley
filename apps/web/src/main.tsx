import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { startLanguage } from "./i18n";
import "./index.css";

// After pley is upgraded, a page that was open asks for pieces of the old
// version, which are gone: load the new version instead. Once a minute at
// most, so a server that's down doesn't keep the page reloading.
window.addEventListener("vite:preloadError", (event) => {
  const key = "pley.reloadedForUpdate";
  try {
    if (Date.now() - Number(sessionStorage.getItem(key) ?? 0) < 60_000) return;
    sessionStorage.setItem(key, String(Date.now()));
  } catch {
    return;
  }
  event.preventDefault();
  window.location.reload();
});

const root = document.getElementById("root");
if (!root) throw new Error("#root element missing");

// The first page shows in the language of the last visit (or the browser's),
// its words loaded first.
void startLanguage().then(() =>
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  ),
);
