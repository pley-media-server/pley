/**
 * The English words, built into the app: the reference the other languages
 * are checked against (`i18n.test.ts`), and what the keys' types come from
 * (`i18next.d.ts`). A namespace is added here and as a file in every
 * language's folder.
 */
import common from "./locales/en/common.json";
import components from "./locales/en/components.json";
import item from "./locales/en/item.json";
import livetv from "./locales/en/livetv.json";
import pages from "./locales/en/pages.json";
import player from "./locales/en/player.json";
import requests from "./locales/en/requests.json";
import settings from "./locales/en/settings.json";

export const en = { common, components, item, livetv, pages, player, requests, settings };
