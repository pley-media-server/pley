/**
 * The English words in the app's main bundle: the reference for the keys'
 * types (`i18next.d.ts`). A namespace is added here (or to
 * {@link LazyResources}) and as a file in every language's folder.
 */
import common from "./locales/en/common.json";
import components from "./locales/en/components.json";
import item from "./locales/en/item.json";
import pages from "./locales/en/pages.json";
import player from "./locales/en/player.json";
import requests from "./locales/en/requests.json";

export const en = { common, components, item, pages, player, requests };

/**
 * The words of pages whose code loads on demand (the settings, Live TV),
 * English too: they come with those pages (`loadNamespaces`), so the main
 * bundle doesn't carry them for everyone.
 */
export interface LazyResources {
  settings: typeof import("./locales/en/settings.json");
  livetv: typeof import("./locales/en/livetv.json");
}

export type LazyNamespace = keyof LazyResources;

export type Resources = typeof en & LazyResources;
