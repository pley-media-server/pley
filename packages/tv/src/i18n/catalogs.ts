/**
 * Every language's words, by area: one file per area and language in
 * `locales/`, so the screens' words are edited apart. English is the
 * reference for the keys' types (`Key` in ./index.ts). An area is added
 * here and as a file in every language's folder; the test
 * (test/i18n.test.ts) fails on one left out.
 *
 * All five languages are in the app's script, a TV app's files being on
 * the TV; the build writes them as strings for JSON.parse
 * (build/build-app.mjs), which the TVs' engines read faster than code.
 */
import enBrowse from "./locales/en/browse.json";
import enCommon from "./locales/en/common.json";
import enFormat from "./locales/en/format.json";
import enLivetv from "./locales/en/livetv.json";
import enOnboarding from "./locales/en/onboarding.json";
import enPlayer from "./locales/en/player.json";
import enRequests from "./locales/en/requests.json";
import enSettings from "./locales/en/settings.json";
import enShell from "./locales/en/shell.json";
import esBrowse from "./locales/es/browse.json";
import esCommon from "./locales/es/common.json";
import esFormat from "./locales/es/format.json";
import esLivetv from "./locales/es/livetv.json";
import esOnboarding from "./locales/es/onboarding.json";
import esPlayer from "./locales/es/player.json";
import esRequests from "./locales/es/requests.json";
import esSettings from "./locales/es/settings.json";
import esShell from "./locales/es/shell.json";
import frBrowse from "./locales/fr/browse.json";
import frCommon from "./locales/fr/common.json";
import frFormat from "./locales/fr/format.json";
import frLivetv from "./locales/fr/livetv.json";
import frOnboarding from "./locales/fr/onboarding.json";
import frPlayer from "./locales/fr/player.json";
import frRequests from "./locales/fr/requests.json";
import frSettings from "./locales/fr/settings.json";
import frShell from "./locales/fr/shell.json";
import zhCNBrowse from "./locales/zh-CN/browse.json";
import zhCNCommon from "./locales/zh-CN/common.json";
import zhCNFormat from "./locales/zh-CN/format.json";
import zhCNLivetv from "./locales/zh-CN/livetv.json";
import zhCNOnboarding from "./locales/zh-CN/onboarding.json";
import zhCNPlayer from "./locales/zh-CN/player.json";
import zhCNRequests from "./locales/zh-CN/requests.json";
import zhCNSettings from "./locales/zh-CN/settings.json";
import zhCNShell from "./locales/zh-CN/shell.json";
import zhTWBrowse from "./locales/zh-TW/browse.json";
import zhTWCommon from "./locales/zh-TW/common.json";
import zhTWFormat from "./locales/zh-TW/format.json";
import zhTWLivetv from "./locales/zh-TW/livetv.json";
import zhTWOnboarding from "./locales/zh-TW/onboarding.json";
import zhTWPlayer from "./locales/zh-TW/player.json";
import zhTWRequests from "./locales/zh-TW/requests.json";
import zhTWSettings from "./locales/zh-TW/settings.json";
import zhTWShell from "./locales/zh-TW/shell.json";

const en = {
  browse: enBrowse,
  common: enCommon,
  format: enFormat,
  livetv: enLivetv,
  onboarding: enOnboarding,
  player: enPlayer,
  requests: enRequests,
  settings: enSettings,
  shell: enShell,
};

export type Catalog = typeof en;
export type Area = keyof Catalog;

type Words = { [K in Area]: object };

const fr: Words = {
  browse: frBrowse,
  common: frCommon,
  format: frFormat,
  livetv: frLivetv,
  onboarding: frOnboarding,
  player: frPlayer,
  requests: frRequests,
  settings: frSettings,
  shell: frShell,
};

const es: Words = {
  browse: esBrowse,
  common: esCommon,
  format: esFormat,
  livetv: esLivetv,
  onboarding: esOnboarding,
  player: esPlayer,
  requests: esRequests,
  settings: esSettings,
  shell: esShell,
};

const zhCN: Words = {
  browse: zhCNBrowse,
  common: zhCNCommon,
  format: zhCNFormat,
  livetv: zhCNLivetv,
  onboarding: zhCNOnboarding,
  player: zhCNPlayer,
  requests: zhCNRequests,
  settings: zhCNSettings,
  shell: zhCNShell,
};

const zhTW: Words = {
  browse: zhTWBrowse,
  common: zhTWCommon,
  format: zhTWFormat,
  livetv: zhTWLivetv,
  onboarding: zhTWOnboarding,
  player: zhTWPlayer,
  requests: zhTWRequests,
  settings: zhTWSettings,
  shell: zhTWShell,
};

export const catalogs: { en: Catalog; fr: Words; es: Words; "zh-CN": Words; "zh-TW": Words } = { en, fr, es, "zh-CN": zhCN, "zh-TW": zhTW };
