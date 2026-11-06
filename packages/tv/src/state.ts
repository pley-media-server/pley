import { computed, signal } from "@preact/signals";
import { Api, ApiError, isApiError, unreachable } from "./api/client";
import { EventStream } from "./api/events";
import type { Hub, HouseholdProfile, ItemDetail, Library, LiveChannel, LiveSummary, MediaRequest, RequestSummary, ServerEvent, TitleKind, TitleStatus, User } from "./api/types";
import { origin } from "./auth/addresses";
import { probe } from "./auth/servers";
import { SignedOutError, Tokens, type KeyValue } from "./auth/tokens";
import { deviceLanguage, language, navigatorLanguages, pickLanguage, setDeviceLanguages, setLanguage, supportedLanguage, type Language } from "./i18n";
import { xhrTransport, type Transport } from "./net/transport";
import type { Platform, Probe } from "./platform";
import { CAST_SWITCH_WAIT_MS, receiverQuery } from "./player/cast";
import { Reporter } from "./player/reporter";
import { isSummary, newsKeys, newsText, readyNews, readyText, statusFromRequest, titleKey } from "./requests";
import type { SubtitleFont, SubtitlePosition } from "./subs/style";
import { OffsetStore } from "./subs/sync";

/** What a person sets on this TV (Settings). */
export interface Settings {
  subtitleSize: "small" | "normal" | "large" | "largest";
  subtitleBox: boolean;
  subtitleFont: SubtitleFont;
  subtitlePosition: SubtitlePosition;
  /** Up Next plays after its 10 s countdown. */
  autoplayNext: boolean;
  reduceMotion: boolean;
  /** The compact readout over the video while the controls are hidden. */
  playbackDetails: boolean;
  /** The quality last picked ("auto", "original", "8m"…); Automatic until one is. */
  quality: string;
  /** Send TrueHD to the receiver as it is (the TV can't promise it). */
  passthroughTrueHd: boolean;
  /** Send DTS to the receiver as it is. */
  passthroughDts: boolean;
}

const DEFAULT_SETTINGS: Settings = {
  subtitleSize: "normal",
  subtitleBox: false,
  subtitleFont: "default",
  subtitlePosition: "bottom",
  autoplayNext: true,
  reduceMotion: false,
  playbackDetails: false,
  quality: "auto",
  passthroughTrueHd: false,
  passthroughDts: false,
};

const SERVER_KEY = "pley.server";
const SETTINGS_KEY = "pley.settings";
/** Set once a quality is picked: the settings saved before Automatic held Original whether picked or not. */
const QUALITY_PICKED_KEY = "pley.qualityPicked";
const HOME_KEY = "pley.home";
/** How many profiles the server's household has, as last heard; kept per server (`pley.household.<server>`). */
const HOUSEHOLD_KEY = "pley.household";
/** Who is watching and their libraries, as last heard: the rail and the library pages before the server answers. */
const ME_KEY = "pley.me";
const LIBRARIES_KEY = "pley.libraries";
/** Whether Live TV shows, as last heard: the rail draws its entry before the server answers. */
const LIVE_KEY = "pley.live";
const SERVERS_KEY = "pley.servers";
/** The household's profiles, as last heard; kept per server (`pley.householdProfiles.<server>`). */
const PROFILES_KEY = "pley.householdProfiles";
/** How long an item's details are kept for pages and playbacks. */
const ITEM_TTL_MS = 5 * 60_000;
/** Watch state changes come in bursts (an episode, its season and show; a play reaching 90 %): gathered this long. */
const STATE_GATHER_MS = 300;
/** How long "Pley is offline" shows: as long as an error's toast. */
const OFFLINE_NOTICE_MS = 6_000;
/** While the server is out of reach it is asked this often (and given as long to answer). */
const OFFLINE_ASK_MS = 5_000;
/** The requests summary as last heard: the rail has its entry before the server answers. */
const REQUESTS_KEY = "pley.requests";
/** The ready-to-watch news announced, per server and profile (`pley.requestNews.<server>.<profile>`). */
const NEWS_KEY = "pley.requestNews";
/** Request events come in bursts (a scan, an import, a manager's round): the summary is asked for once, this long after the last. */
const SUMMARY_GATHER_MS = 1_000;
/** …and the pages that show requests hear once, this long after the last. */
const REQUEST_GATHER_MS = 300;
/** While the events are down, the summary is asked for this often (and the pages when it moved). */
export const SUMMARY_POLL_MS = 60_000;
/** Ready-to-watch notices kept while they can't show (a video plays): the last three. */
const NOTICES_KEPT = 3;
/** The language shown last: the next start shows it before the server answers. */
const LANGUAGE_KEY = "pley.language";
/** The language picked on this TV with the globe, as the web app keeps its own. */
const DEVICE_LANGUAGE_KEY = "pley.deviceLanguage";

/** News of requests popped up over the pages: on TVs, only what is ready to watch. */
export interface RequestNotice {
  id: number;
  text: string;
  /** Open goes to the title in the library; `null` goes to Your requests. */
  itemId: number | null;
  /** The title's poster (TMDB's, through the server); none for news of several. */
  poster: string | null;
}

/** A title's badge as a request event left it, newer than the pages that drew it before `at`. */
export interface TitleMark {
  status: TitleStatus;
  item_id: number | null;
  at: number;
}

/** What changed in requests: one title (`kind:tmdb_id`), or `null` for many. */
export interface RequestChange {
  n: number;
  title: string | null;
}

/** A server this TV signed in to or chose, newest first on the server list. */
export interface SavedServer {
  url: string;
  name: string;
  lastUsed: number;
}

function readJson<T>(store: KeyValue, key: string): T | null {
  try {
    const text = store.get(key);
    return text ? (JSON.parse(text) as T) : null;
  } catch {
    return null;
  }
}

/**
 * What the first screen is, from what the TV kept (no network): the Android
 * app's StartGate. `pin`: "Who's watching?" opened on the PIN pad of the
 * profile the TV was left on.
 */
export type Start = { page: "servers" } | { page: "link"; server: string } | { page: "profiles" } | { page: "pin" } | { page: "home" };

/** What the first screen is chosen by, all kept on the TV. */
export interface StartFacts {
  server: string | null;
  signedIn: boolean;
  /** How many profiles the household had when last heard (1 while unknown). */
  household: number;
  /** Settings' "Always start as"; `null` when not set. */
  alwaysStartAs: number | null;
  /** The profile the TV was left on, as last heard; `null` when it never heard. */
  me: Pick<User, "has_pin"> | null;
}

/**
 * The first screen (the Android app's startKey): no server, Servers; no
 * sign-in, Link; otherwise Home as the profile the TV was left on, but never
 * past its PIN. In a household of several profiles, one with a PIN asks
 * first: "Who's watching?", or its pad with "Always start as"; and so does
 * one the TV doesn't know has none.
 */
export function startPage(f: StartFacts): Start["page"] {
  if (!f.server) return "servers";
  if (!f.signedIn) return "link";
  if (f.household <= 1) return "home";
  if (f.me && !f.me.has_pin) return "home";
  if (f.alwaysStartAs === null || !f.me) return "profiles";
  return "pin";
}

/**
 * The app's state: the server signed in to, its API and tokens, who is
 * watching, the libraries, the settings, Home as last seen (drawn first at
 * the next launch), the TV's capabilities, and the server's events.
 */
export class AppState {
  readonly platform: Platform;
  readonly http: Transport;
  readonly store: KeyValue;
  readonly reporter = new Reporter();
  readonly offsets: OffsetStore;

  readonly server = signal<string | null>(null);
  readonly api = signal<Api | null>(null);
  readonly me = signal<User | null>(null);
  readonly libraries = signal<Library[] | null>(null);
  /** Whether Live TV shows (the rail's entry), and the channels watched last; `null` until the server says (or on a server without it). */
  readonly live = signal<LiveSummary | null>(null);
  /** Bumped when Live TV's channels or guide change on the server: its pages ask again. */
  readonly liveChanges = signal(0);
  /** The live channel watched last in this launch, and the category it was in: Live TV opens back at it. */
  liveLastWatched: { id: number; category: string } | null = null;
  /** Live channels the pages showed lately, so the player starts with one without asking. */
  private readonly liveKnown = new Map<number, LiveChannel>();
  readonly profiles = signal<HouseholdProfile[] | null>(null);
  readonly settings = signal<Settings>(DEFAULT_SETTINGS);
  readonly home = signal<Hub[] | null>(null);
  readonly homeError = signal<unknown>(null);
  /** The last try at who is watching and the libraries failed (and none were saved): the library pages offer Retry. */
  readonly accountError = signal<unknown>(null);
  /** Bumped by the server's events, so screens that list things fetch again. */
  readonly changes = signal(0);
  /** The server signed the TV out: show sign-in. */
  readonly signedOut = signal(false);
  /** Whether the TV shows the app (not another app, its home screen or standby). */
  readonly visible = signal(true);
  /**
   * The server answered a screen nothing (the Android app's TvOfflineState):
   * what shows is what was saved. Raised by a load the server couldn't
   * answer, lowered by its events connecting or a screen's fresh answer
   * after a failure.
   */
  readonly offline = signal(false);
  /** "Pley is offline" shows, for 6 s from going offline; then the rail's avatar keeps a mark. */
  readonly offlineNotice = signal(false);
  /**
   * The requests summary (docs/API.md, "Requests"): whether Discover and
   * the requests show, and the rail's number. `null` while unknown, and for
   * a server without requests.
   */
  readonly requests = signal<RequestSummary | null>(null);
  /** Bumped when requests changed (their events, or the summary's `changed_at` while the events are down): pages that show requests ask again. */
  readonly requestChanges = signal<RequestChange>({ n: 0, title: null });
  /** Titles' badges as request events left them, by `kind:tmdb_id`: rows already drawn show them without asking again. */
  readonly marks = signal<Record<string, TitleMark>>({});
  /** Ready-to-watch news waiting to pop up, oldest first; the shell shows the first when nothing plays. */
  readonly notices = signal<RequestNotice[]>([]);
  /** The events aren't connected: pages that show requests ask on their own meanwhile. */
  readonly eventsDown = signal(false);
  /**
   * The server's language for accounts without their own (`GET /api/server`):
   * `undefined` until it answers, `null` from a Pley from before languages,
   * which names none.
   */
  readonly serverLanguage = signal<string | null | undefined>(undefined);
  /**
   * The language picked on this TV with the globe of the screens before
   * signing in (`null`: Automatic, the TV's own): before the TV's, and what
   * an account's Automatic is ({@link setDeviceLanguage}).
   */
  readonly deviceChoice = signal<Language | null>(null);
  /**
   * The server keeps no account's language: a Pley from before languages,
   * whose `GET /api/server` names none, or one that answered a choice 404.
   * Settings' Language row is then this TV's, as the globe picks it
   * ({@link chooseLanguage}).
   */
  readonly languageOnDevice = computed(() => this.serverLanguage.value === null || this.languageMissing.value);
  /**
   * "Who's watching?" was answered in this launch (a profile picked, a PIN
   * checked, or a sign-in approved on a phone): the current profile's PIN
   * isn't asked again until the app starts over.
   */
  profileGatePassed = false;

  private tokens: Tokens | null = null;
  private events: EventStream | null = null;
  private readonly eventListeners: ((e: ServerEvent) => void)[] = [];
  private capsPromise: Promise<Probe | null> | null = null;
  private readonly items = new Map<number, { detail: ItemDetail; at: number }>();
  private libraryTimer: ReturnType<typeof setTimeout> | null = null;
  /** Watch state changes heard in the last 0.3 s, asked for again in one round. */
  private stateTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly stateIds = new Set<number>();
  private readonly stateParents = new Set<number>();
  /** Playbacks on top: Home and the pages wait to ask again until they end (their own reports change the watch state too). */
  private holds = 0;
  /** The libraries' order changes on their way to the server, one at a time. */
  private orderQueue: Promise<unknown> = Promise.resolve();
  private noticeTimer: ReturnType<typeof setTimeout> | null = null;
  private awayTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly leavers: (() => void)[] = [];
  /** Another profile, sign-in or server since: what was asked for the one before is dropped. */
  private generation = 0;
  /** The server has no requests (404) or an older build of them (403): not asked again until the events reconnect. */
  private requestsMissing = false;
  private summaryTimer: ReturnType<typeof setTimeout> | null = null;
  private pollTimer: ReturnType<typeof setTimeout> | null = null;
  private changeTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly changedTitles = new Set<string>();
  /** News events heard since the last summary: the `unseen` they raised needs no catching up. */
  private newsHeard = 0;
  /** This profile's news was caught up with since it was picked (or one waits for who it is). */
  private caughtUp = false;
  private catchUpWaiting = false;
  private catchingUp = false;
  /** Pages that draw titles' badges: request events fetch their request for its badge only meanwhile. */
  private markWatchers = 0;
  /**
   * Asked for in one round: the requests events named in the last 0.3 s,
   * with their titles, and the titles whose details say their badge.
   */
  private readonly markNamed = new Map<number, { kind: TitleKind; tmdbId: number }>();
  private readonly markAsked = new Map<string, { kind: TitleKind; tmdbId: number }>();
  private markTimer: ReturnType<typeof setTimeout> | null = null;
  private noticeIds = 0;
  private noticeOnScreen: number | null = null;
  /** Who is watching was heard from the server since the profile, sign-in or server last changed: its language counts. */
  private accountHeard = false;
  /** The server in use answered a choice of language 404 (or 405): it has no route for it. */
  private readonly languageMissing = signal(false);

  constructor(platform: Platform, http: Transport = xhrTransport) {
    this.platform = platform;
    this.http = http;
    this.store = platform.storage;
    this.offsets = new OffsetStore(platform.storage, () => this.me.value?.id ?? null);
    setDeviceLanguages(() => this.deviceLanguages());
    this.deviceChoice.value = supportedLanguage(this.store.get(DEVICE_LANGUAGE_KEY)) ?? null;
    // The language shown last, until the server says; at the first start, the one picked here or the TV's.
    setLanguage(supportedLanguage(this.store.get(LANGUAGE_KEY)) ?? this.deviceChoice.value ?? deviceLanguage(this.deviceLanguages()) ?? "en");
    const kept = readJson<Partial<Settings>>(this.store, SETTINGS_KEY) ?? {};
    // Original was the default before Automatic, and was saved along with any other setting: unless picked, it is Automatic now.
    if (kept.quality === "original" && this.store.get(QUALITY_PICKED_KEY) !== "1") delete kept.quality;
    this.settings.value = { ...DEFAULT_SETTINGS, ...kept };
    const server = this.store.get(SERVER_KEY);
    if (server) {
      // Kept for the server in use before they were kept per server.
      for (const key of [HOUSEHOLD_KEY, PROFILES_KEY]) {
        const old = this.store.get(key);
        if (old === null) continue;
        this.store.remove(key);
        const home = origin(server);
        if (home) this.store.set(`${key}.${home}`, old);
      }
      this.use(server);
    }
    // What was saved is trusted only in shape: a store another build wrote, or a damaged one, is left out.
    const home = readJson<Hub[]>(this.store, HOME_KEY);
    const me = readJson<User>(this.store, ME_KEY);
    const libraries = readJson<Library[]>(this.store, LIBRARIES_KEY);
    const live = readJson<LiveSummary>(this.store, LIVE_KEY);
    const requests = readJson<unknown>(this.store, REQUESTS_KEY);
    this.home.value = Array.isArray(home) ? home : null;
    this.me.value = me && typeof me === "object" && typeof me.id === "number" ? me : null;
    this.libraries.value = Array.isArray(libraries) ? libraries : null;
    this.live.value = live && typeof live === "object" && typeof live.available === "boolean" && Array.isArray(live.recent) ? live : null;
    this.requests.value = isSummary(requests) ? requests : null;
  }

  /** The first screen, from what the TV kept. */
  start(): Start {
    const server = this.server.value;
    const page = startPage({
      server,
      signedIn: !!this.tokens?.signedIn,
      household: this.householdSize(),
      alwaysStartAs: this.alwaysStartAs(),
      me: this.me.value,
    });
    return page === "link" ? { page, server: server ?? "" } : { page };
  }

  /** Signs in to `server` from now on (its tokens, if the TV has them). */
  use(server: string): void {
    const home = origin(server);
    if (!home) return;
    if (this.server.value === home && this.api.value) return;
    this.events?.stop();
    this.events = null;
    if (this.server.value && this.server.value !== home) this.leaveServer();
    this.server.value = home;
    this.store.set(SERVER_KEY, home);
    this.tokens = new Tokens(home, this.platform.clientId, this.store, this.http, () => {
      this.signedOut.value = true;
      this.events?.stop();
      this.followLanguage();
    });
    this.api.value = new Api(home, this.tokens, this.http);
    this.signedOut.value = false;
    this.serverLanguage.value = undefined;
    this.languageMissing.value = false;
    void this.loadServerLanguage();
  }

  get signedIn(): boolean {
    return !!this.tokens?.signedIn;
  }

  get tokenStore(): Tokens | null {
    return this.tokens;
  }

  /** The servers this TV chose, most recently used first. */
  savedServers(): SavedServer[] {
    const list = readJson<SavedServer[]>(this.store, SERVERS_KEY);
    if (!Array.isArray(list)) return [];
    return list.filter((s) => s && typeof s.url === "string").sort((a, b) => b.lastUsed - a.lastUsed);
  }

  /** Keeps `url` on the server list, as used now. */
  rememberServer(url: string, name: string): void {
    const home = origin(url);
    if (!home) return;
    const list = this.savedServers().filter((s) => s.url !== home);
    list.unshift({ url: home, name, lastUsed: Date.now() });
    this.store.set(SERVERS_KEY, JSON.stringify(list.slice(0, 20)));
  }

  /** Whether the TV has a sign-in for `url`. */
  hasTokens(url: string): boolean {
    const home = origin(url);
    return !!home && new Tokens(home, this.platform.clientId, this.store, this.http).signedIn;
  }

  /**
   * This TV's own id for its playbacks ("tizen-1f3a…"), kept for good: the
   * server ends a client's earlier sessions when it starts another, so two
   * TVs of a household must never share one.
   */
  installId(): string {
    let id = this.store.get("pley.installId");
    if (!id) {
      const bytes = new Uint8Array(16);
      try {
        crypto.getRandomValues(bytes);
      } catch {
        for (let i = 0; i < bytes.length; i++) bytes[i] = Math.floor(Math.random() * 256);
      }
      id = `${this.platform.name}-${Array.prototype.map.call(bytes, (b: number) => (b < 16 ? "0" : "") + b.toString(16)).join("")}`;
      this.store.set("pley.installId", id);
    }
    return id;
  }

  /** The profile "Always start as" names for this server; `null` asks who's watching. */
  alwaysStartAs(): number | null {
    const v = this.store.get(`pley.alwaysStartAs.${this.server.value ?? ""}`);
    return v ? Number(v) : null;
  }

  setAlwaysStartAs(id: number | null): void {
    const key = `pley.alwaysStartAs.${this.server.value ?? ""}`;
    if (id === null) this.store.remove(key);
    else this.store.set(key, String(id));
  }

  /** How many profiles the household had when last asked (1 while unknown). */
  householdSize(): number {
    const n = Number(this.store.get(`${HOUSEHOLD_KEY}.${this.server.value ?? ""}`) ?? "1");
    return n > 0 ? n : 1;
  }

  /** The household as last fetched, for "Who's watching?" before the server answers. */
  savedHousehold(): HouseholdProfile[] {
    const list = readJson<HouseholdProfile[]>(this.store, `${PROFILES_KEY}.${this.server.value ?? ""}`);
    return Array.isArray(list) ? list : [];
  }

  keepHousehold(profiles: HouseholdProfile[]): void {
    this.profiles.value = profiles;
    const server = this.server.value ?? "";
    this.store.set(`${HOUSEHOLD_KEY}.${server}`, String(profiles.length));
    this.store.set(`${PROFILES_KEY}.${server}`, JSON.stringify(profiles));
  }

  /** `fn` runs when what plays must end: a profile switch, signing out, another server, the app closing. */
  onLeave(fn: () => void): void {
    this.leavers.push(fn);
  }

  /** Ends what plays in the background (the music's session) before the profile, the sign-in, the server or the app goes. */
  endPlayback(): void {
    for (const fn of this.leavers.slice()) fn();
  }

  /**
   * The app acts as another profile now: what was shown for the one before
   * goes, and its event stream, which would bring its news here, starts
   * over. Its summary still on the way is dropped, so this one's is asked
   * for after it.
   */
  async switched(): Promise<void> {
    this.endPlayback();
    this.forgetAccount();
    this.home.value = null;
    this.homeError.value = null;
    this.items.clear();
    this.me.value = null;
    this.live.value = null;
    this.forgetLive();
    this.events?.restart();
    void this.loadRequests(true);
    await this.loadAccount();
    void this.loadHome();
  }

  /**
   * Moves the app to `profile` for a cast from that profile's remote, with
   * its `ticket` (docs/API.md, "Casting"). What plays ends first, and its
   * last reports go out with this profile's sign-in, for up to
   * CAST_SWITCH_WAIT_MS: what hasn't gone by then is dropped, never sent as
   * the new profile, whose watch history it isn't. Then the ticket is
   * redeemed and the app acts as the new profile (`switched`). A refused
   * ticket rejects, and the app stays as it is, what played ended.
   */
  async switchForCast(profile: number, ticket: string): Promise<void> {
    const tokens = this.tokens;
    if (!tokens) throw new SignedOutError();
    this.endPlayback();
    // The player's last reports are made as its page goes, once drawn.
    await new Promise((r) => setTimeout(r, 0));
    await this.reporter.flush(CAST_SWITCH_WAIT_MS);
    this.reporter.drop();
    await tokens.switchProfile(profile, null, ticket);
    // As a switch on "Who's watching?": who is watching failing to load is the pages' Retry's.
    await this.switched().catch(() => undefined);
  }

  /** Forgets a server of the list: its sign-in ends, and what was saved from it goes. */
  async forget(url: string): Promise<void> {
    const home = origin(url);
    if (!home) return;
    this.store.set(SERVERS_KEY, JSON.stringify(this.savedServers().filter((s) => s.url !== home)));
    const tokens = home === this.server.value && this.tokens ? this.tokens : new Tokens(home, this.platform.clientId, this.store, this.http);
    if (home === this.server.value) this.forgetServer();
    await tokens.signOut().catch(() => undefined);
  }

  /** Forgets the server in use (Change server). */
  forgetServer(): void {
    this.endPlayback();
    this.events?.stop();
    this.events = null;
    this.store.remove(SERVER_KEY);
    const server = this.server.value ?? "";
    this.store.remove(`${HOUSEHOLD_KEY}.${server}`);
    this.store.remove(`${PROFILES_KEY}.${server}`);
    this.leaveServer();
    this.server.value = null;
    this.api.value = null;
    this.tokens = null;
    this.serverLanguage.value = undefined;
    this.languageMissing.value = false;
  }

  /** What was shown from the server in use goes: another one comes, or none. */
  private leaveServer(): void {
    this.endPlayback();
    this.forgetAccount();
    this.items.clear();
    this.home.value = null;
    this.homeError.value = null;
    this.accountError.value = null;
    this.me.value = null;
    this.libraries.value = null;
    this.live.value = null;
    this.forgetLive();
    this.profiles.value = null;
    this.offline.value = false;
    this.offlineNotice.value = false;
    this.profileGatePassed = false;
  }

  async signOut(): Promise<void> {
    this.events?.stop();
    // The music's session ends while the sign-in that made it still works.
    this.endPlayback();
    await this.reporter.flush(2_000);
    await this.tokens?.signOut();
    this.forgetAccount();
    this.home.value = null;
    this.signedOut.value = true;
    this.followLanguage();
  }

  updateSettings(patch: Partial<Settings>): void {
    if ("passthroughTrueHd" in patch || "passthroughDts" in patch) this.capsPromise = null;
    this.settings.value = { ...this.settings.value, ...patch };
    this.store.set(SETTINGS_KEY, JSON.stringify(this.settings.value));
    if ("quality" in patch) this.store.set(QUALITY_PICKED_KEY, "1");
  }

  // ---- the language

  /** The TV's languages, its own first. */
  deviceLanguages(): readonly string[] {
    return this.platform.languages?.() ?? navigatorLanguages();
  }

  /** The server's language, from `GET /api/server`; asked again when its events reconnect if it didn't answer. */
  private async loadServerLanguage(): Promise<void> {
    const server = this.server.value;
    if (!server) return;
    let info;
    try {
      info = await probe(this.http, server);
    } catch {
      return;
    }
    if (!info || this.server.value !== server) return;
    this.serverLanguage.value = info.language;
    this.followLanguage();
  }

  /**
   * The language the app is in (packages/tv/test/fixtures/language.json):
   * the account's as soon as it is heard; else, once who is watching was
   * heard (signed in), the one picked on this TV, else the TV's, or for a
   * TV in a language Pley doesn't have, the server language once the server
   * has said it. Until then the last one shown stays, as going by another
   * meanwhile would flash it.
   */
  private followLanguage(): void {
    const account = this.signedIn && this.accountHeard ? this.me.value?.language : null;
    const choice = this.deviceChoice.value;
    if (!supportedLanguage(account)) {
      if (this.signedIn && !this.accountHeard) return;
      if (this.serverLanguage.value === undefined && !choice && !deviceLanguage(this.deviceLanguages())) return;
    }
    this.showLanguage(pickLanguage(account, this.serverLanguage.value, this.deviceLanguages(), choice));
  }

  /**
   * Picks this TV's language (`null`: Automatic, the TV's own again), kept
   * on the TV. Signed out (the globe of the screens before signing in), or
   * on a server that keeps no account's language, it shows at once,
   * Automatic as what it says it is; signed in, unless the account has its
   * own.
   */
  setDeviceLanguage(next: Language | null): void {
    this.deviceChoice.value = next;
    if (next) this.store.set(DEVICE_LANGUAGE_KEY, next);
    else this.store.remove(DEVICE_LANGUAGE_KEY);
    if (this.signedIn && !this.languageOnDevice.peek()) this.followLanguage();
    else this.showLanguage(pickLanguage(null, this.serverLanguage.value, this.deviceLanguages(), next));
  }

  /**
   * A choice on Settings' Language row (`null`: Automatic): the profile's
   * language on all its devices ({@link setAccountLanguage}), or where the
   * server keeps none ({@link languageOnDevice}) this TV's, as the globe
   * picks it ({@link setDeviceLanguage}). A server that answers it 404 (or
   * 405) has no route for it: the choice is this TV's, and so are the next.
   */
  async chooseLanguage(next: Language | null): Promise<void> {
    if (this.languageOnDevice.peek()) {
      this.setDeviceLanguage(next);
      return;
    }
    const api = this.api.value;
    try {
      await this.setAccountLanguage(next);
    } catch (e) {
      if (!isApiError(e, 404) && !isApiError(e, 405)) throw e;
      // Another server since: the answer was the one before's.
      if (this.api.value !== api) return;
      this.languageMissing.value = true;
      this.setDeviceLanguage(next);
    }
  }

  /** The app in `next` from now on, and at the next start. */
  private showLanguage(next: Language): void {
    this.store.set(LANGUAGE_KEY, next);
    if (next === language.peek()) return;
    setLanguage(next);
    // The server writes Home's and the libraries' row titles: asked again in the new language.
    if (this.signedIn) this.refresh();
  }

  /** The profile's own language on all its devices (`null`: the server's), shown once the server has it (`PUT /api/me/language`). */
  async setAccountLanguage(next: Language | null): Promise<void> {
    const api = this.api.value;
    if (!api) throw new SignedOutError();
    const who = this.me.value?.id;
    const me = await api.setLanguage(next);
    // Another profile, sign-in or server since: the answer was the one before's.
    if (this.api.value !== api || this.me.value?.id !== who) return;
    this.me.value = me;
    this.store.set(ME_KEY, JSON.stringify(me));
    this.accountHeard = true;
    this.followLanguage();
  }

  // ---- what the server has

  /**
   * What was saved of the profile signed in (Home, who it is, its
   * libraries, its requests): another profile, sign-in or server comes.
   */
  private forgetAccount(): void {
    this.store.remove(HOME_KEY);
    this.store.remove(ME_KEY);
    this.store.remove(LIBRARIES_KEY);
    this.store.remove(LIVE_KEY);
    this.store.remove(REQUESTS_KEY);
    this.generation++;
    this.accountHeard = false;
    this.requests.value = null;
    this.marks.value = {};
    this.markNamed.clear();
    this.markAsked.clear();
    if (this.markTimer !== null) clearTimeout(this.markTimer);
    this.markTimer = null;
    this.notices.value = [];
    this.noticeOnScreen = null;
    this.newsHeard = 0;
    this.caughtUp = false;
    this.catchUpWaiting = false;
    if (this.summaryTimer !== null) clearTimeout(this.summaryTimer);
    this.summaryTimer = null;
  }

  /**
   * Who is watching, the libraries and the household, after sign-in or a
   * profile switch; kept, so the rail and the library pages have them
   * before the server answers next time. A failure is kept for the library
   * pages' Retry, and the server out of reach shows as such.
   */
  async loadAccount(): Promise<void> {
    const api = this.api.value;
    if (!api) return;
    // What requests are for it, alongside.
    void this.loadRequests();
    let me: User;
    let libraries: Library[];
    try {
      [me, libraries] = await Promise.all([api.me(), api.libraries()]);
    } catch (e) {
      this.accountError.value = e;
      if (unreachable(e)) this.serverAway();
      throw e;
    }
    this.accountError.value = null;
    this.me.value = me;
    this.store.set(ME_KEY, JSON.stringify(me));
    this.keepLibraries(libraries);
    this.accountHeard = true;
    this.followLanguage();
    api.profiles().then(
      (profiles) => this.keepHousehold(profiles),
      () => undefined,
    );
    void this.loadLive();
    // The news waited to know whose it is.
    if (this.catchUpWaiting) void this.catchUp();
  }

  /**
   * Whether Live TV shows for this profile (`GET /api/live`), kept so the
   * rail has it at the next launch before the server answers. A server
   * without Live TV (`404`) shows none; one out of reach leaves what was
   * kept.
   */
  async loadLive(): Promise<void> {
    const api = this.api.value;
    if (!api) return;
    const who = this.me.value?.id;
    let summary: LiveSummary | null;
    try {
      summary = await api.liveSummary();
    } catch (e) {
      if (!isApiError(e, 404) && !isApiError(e, 403)) return;
      summary = null;
    }
    // Another profile, sign-in or server since: the answer was the one before's.
    if (this.api.value !== api || this.me.value?.id !== who) return;
    this.live.value = summary;
    if (summary) this.store.set(LIVE_KEY, JSON.stringify(summary));
    else this.store.remove(LIVE_KEY);
    // The remotes are told this TV takes channels now, or no longer does.
    if (!!summary?.available !== this.receivesLive) this.events?.restart();
  }

  /** Live channels a page showed: the player starts with one of them at once. The 500 seen last are kept. */
  keepLiveChannel(c: LiveChannel): void {
    this.liveKnown.delete(c.id);
    this.liveKnown.set(c.id, c);
    if (this.liveKnown.size > 500) this.liveKnown.delete(this.liveKnown.keys().next().value!);
  }

  knownLiveChannel(id: number): LiveChannel | null {
    return this.liveKnown.get(id) ?? null;
  }

  /** Another profile, sign-in or server: its Live TV goes. */
  private forgetLive(): void {
    this.liveKnown.clear();
    this.liveLastWatched = null;
  }

  private keepLibraries(libraries: Library[]): void {
    this.libraries.value = libraries;
    this.store.set(LIBRARIES_KEY, JSON.stringify(libraries));
  }

  /**
   * The profile's own order of its libraries (`ids`, each once), or the
   * order everyone sees again (`null`), on every list of them on all its
   * devices. Changes go one at a time, in the order they were made; the
   * server's answer is the list from then on, and Home asks again, since
   * its rows of each library follow the order.
   */
  orderLibraries(ids: number[] | null): Promise<Library[]> {
    const run = this.orderQueue.then(async () => {
      const api = this.api.value;
      const who = this.me.value?.id;
      if (!api) throw new SignedOutError();
      const libraries = ids ? await api.sortLibraries(ids) : await api.forgetLibraryOrder();
      // Another profile, sign-in or server since: the answer was the one before's.
      if (this.api.value === api && this.me.value?.id === who) {
        this.keepLibraries(libraries);
        void this.loadHome();
      }
      return libraries;
    });
    this.orderQueue = run.catch(() => undefined);
    return run;
  }

  async loadHome(): Promise<void> {
    const api = this.api.value;
    if (!api) return;
    try {
      const { hubs } = await api.home();
      const failedBefore = this.homeError.value !== null;
      this.home.value = hubs;
      this.homeError.value = null;
      this.store.set(HOME_KEY, JSON.stringify(hubs));
      if (failedBefore) this.serverAnswered();
    } catch (e) {
      this.homeError.value = e;
      if (unreachable(e)) this.serverAway();
    }
  }

  // ---- the server out of reach

  /**
   * A screen's load got no answer: the notice shows for 6 s (again each
   * time the server goes), then the rail's mark. Meanwhile the server is
   * asked every 5 s, so the event stream, backing off up to 30 s between
   * tries, reconnects as soon as it answers and the screens come back.
   */
  serverAway(): void {
    if (this.offline.value) return;
    this.offline.value = true;
    this.offlineNotice.value = true;
    if (this.noticeTimer !== null) clearTimeout(this.noticeTimer);
    this.noticeTimer = setTimeout(() => {
      this.noticeTimer = null;
      this.offlineNotice.value = false;
    }, OFFLINE_NOTICE_MS);
    this.askWhileAway();
  }

  private askWhileAway(): void {
    if (this.awayTimer !== null) clearTimeout(this.awayTimer);
    this.awayTimer = setTimeout(() => {
      this.awayTimer = null;
      const api = this.api.value;
      if (!this.offline.value || !api) return;
      if (!this.visible.value) {
        this.askWhileAway();
        return;
      }
      api.reachable(OFFLINE_ASK_MS).then(
        () => {
          if (!this.offline.value) return;
          // A stream waiting to reconnect connects now, and its connection lowers it; one still
          // connected (a page failed in a short blip) can't, so the app does, and asks again.
          if (this.events?.retryNow()) {
            this.askWhileAway();
            return;
          }
          this.serverAnswered();
          this.items.clear();
          this.refresh();
        },
        () => this.askWhileAway(),
      );
    }, OFFLINE_ASK_MS);
  }

  /** The server answers again: its events connected, or a screen that had failed got a fresh answer. */
  serverAnswered(): void {
    if (!this.offline.value) return;
    if (this.noticeTimer !== null) clearTimeout(this.noticeTimer);
    this.noticeTimer = null;
    if (this.awayTimer !== null) clearTimeout(this.awayTimer);
    this.awayTimer = null;
    this.offline.value = false;
    this.offlineNotice.value = false;
  }

  /**
   * Item details the app saw lately, to start a page or a playback without
   * waiting. The server's events take out what changed; a detail older than
   * 5 minutes goes anyway, in case one was missed (a resume point moved on
   * another device while the event stream was down).
   */
  cachedItem(id: number): ItemDetail | null {
    const kept = this.items.get(id);
    if (!kept) return null;
    if (Date.now() - kept.at > ITEM_TTL_MS) {
      this.items.delete(id);
      return null;
    }
    return kept.detail;
  }

  async item(id: number): Promise<ItemDetail> {
    const api = this.api.value!;
    const detail = await api.item(id);
    this.keepItem(detail);
    return detail;
  }

  /** Keeps a detail fetched elsewhere (an item page) for the next page or playback. */
  keepItem(detail: ItemDetail): void {
    this.items.delete(detail.id);
    this.items.set(detail.id, { detail, at: Date.now() });
    if (this.items.size > 40) this.items.delete(this.items.keys().next().value!);
  }

  /**
   * Tracks were picked for `detail`'s title (the whole show for an
   * episode), or the profile's defaults changed (`detail` null): the
   * details kept name the tracks they started with before. `detail`, with
   * the choice in it, is kept; the others go.
   */
  tracksChanged(detail: ItemDetail | null): void {
    this.items.clear();
    if (detail) this.keepItem(detail);
  }

  /**
   * A playback is on top (`true`) or has ended (`false`). Meanwhile Home and
   * the pages wait to ask again; once it ends they do, and the details kept
   * go, since its own reports moved resume points and watched marks.
   */
  holdRefresh(on: boolean): void {
    this.holds = Math.max(0, this.holds + (on ? 1 : -1));
    if (on || this.holds > 0) return;
    this.items.clear();
    void this.loadHome();
    this.changes.value++;
  }

  /** Home and the pages that list things ask again; while a playback is on top, once it ends. */
  private refresh(): void {
    if (this.holds > 0) return;
    void this.loadHome();
    // Who is watching and the libraries, when the last try failed or none came yet.
    if (this.accountError.value !== null || !this.libraries.value) void this.loadAccount().catch(() => undefined);
    else void this.loadLive();
    this.changes.value++;
  }

  // ---- the TV's capabilities

  /** The capabilities, probed once (after the first screen is drawn), again after Settings change them. */
  caps(fresh = false): Promise<Probe | null> {
    // Diagnostics' Check again: asked of the TV anew.
    if (fresh) this.capsPromise = null;
    if (!this.capsPromise) {
      const s = this.settings.value;
      this.capsPromise = this.platform.probe({ truehd: s.passthroughTrueHd, dts: s.passthroughDts }).catch(() => null);
    }
    return this.capsPromise;
  }

  // ---- the server's events

  onEvent(listener: (e: ServerEvent) => void): () => void {
    this.eventListeners.push(listener);
    return () => {
      const i = this.eventListeners.indexOf(listener);
      if (i >= 0) this.eventListeners.splice(i, 1);
    };
  }

  /** Follows the server's events while the app is shown and signed in. */
  listen(on: boolean): void {
    const server = this.server.value;
    if (!on || !server || !this.tokens?.signedIn) {
      this.events?.stop();
      // Hidden, nothing asks: shown again, the events connect (or fail, and the asking starts over).
      if (this.pollTimer !== null) clearTimeout(this.pollTimer);
      this.pollTimer = null;
      return;
    }
    if (!this.events) {
      this.events = new EventStream(
        server,
        this.tokens,
        (e) => this.handleEvent(e),
        () => this.reconnected(),
        () => this.eventsLost(),
        () => {
          this.receivesLive = !!this.live.value?.available;
          return receiverQuery(this.installId(), this.receivesLive);
        },
      );
    }
    this.events.start();
  }

  /** Whether the stream last opened took Live TV's channels from remotes: it opens again when that changes. */
  private receivesLive = false;

  private lastSync = 0;

  private reconnected(): void {
    // Nothing is replayed: what is older than 30 s, or anything after the server was out of reach, is fetched again.
    const away = this.offline.value;
    const down = this.eventsDown.value;
    this.serverAnswered();
    this.eventsDown.value = false;
    if (this.pollTimer !== null) clearTimeout(this.pollTimer);
    this.pollTimer = null;
    // The server may have restarted, newer: one without requests is asked again.
    this.requestsMissing = false;
    void this.loadRequests();
    if (this.serverLanguage.value === undefined) void this.loadServerLanguage();
    if (down) this.requestsChanged(null);
    if (away || Date.now() - this.lastSync > 30_000) {
      this.lastSync = Date.now();
      this.items.clear();
      this.refresh();
    }
  }

  /** The events stopped (or couldn't connect): until they connect again, the summary is asked for every 60 s. */
  private eventsLost(): void {
    this.eventsDown.value = true;
    this.pollRequests();
  }

  private pollRequests(): void {
    if (this.pollTimer !== null) return;
    this.pollTimer = setTimeout(() => {
      this.pollTimer = null;
      if (!this.eventsDown.value || !this.events) return;
      if (this.visible.value) void this.loadRequests();
      this.pollRequests();
    }, SUMMARY_POLL_MS);
  }

  private handleEvent(e: ServerEvent): void {
    switch (e.type) {
      case "library_changed":
        // Changes come in bursts during scans: once 1.5 s after the last.
        if (this.libraryTimer !== null) clearTimeout(this.libraryTimer);
        this.libraryTimer = setTimeout(() => {
          this.libraryTimer = null;
          this.refresh();
        }, 1_500);
        break;
      case "libraries_changed":
        void this.loadAccount().catch(() => undefined);
        this.refresh();
        break;
      case "watchlist_changed":
        this.refresh();
        break;
      case "live_channels_changed":
        void this.loadLive();
        this.liveChanges.value++;
        break;
      case "user_state_changed": {
        // The item, its parent and grandparent; with `children`, everything under the first as well.
        for (const id of e.item_ids) this.stateIds.add(id);
        if (e.children && e.item_ids.length) this.stateParents.add(e.item_ids[0]!);
        if (this.stateTimer === null) this.stateTimer = setTimeout(() => this.applyStateChanges(), STATE_GATHER_MS);
        break;
      }
      case "requests_changed":
        this.requestsSoon();
        this.requestsChanged(e.kind && typeof e.tmdb_id === "number" ? titleKey({ kind: e.kind, tmdb_id: e.tmdb_id }) : null);
        if (typeof e.request_id === "number" && e.kind && typeof e.tmdb_id === "number") this.markFromRequest(e.request_id, e.kind, e.tmdb_id);
        break;
      case "request_available": {
        this.newsHeard++;
        this.requestsSoon();
        const complete = e.complete !== false;
        if (e.kind && typeof e.tmdb_id === "number") {
          this.requestsChanged(titleKey({ kind: e.kind, tmdb_id: e.tmdb_id }));
          // A movie is in the library at once; a show wholly ready may still lack seasons nobody asked for: its details say.
          if (!complete) this.markTitle(e.kind, e.tmdb_id, "partial", e.item_id ?? null);
          else if (e.kind === "movie") this.markTitle(e.kind, e.tmdb_id, "library", e.item_id ?? null);
          else this.markFromTitle(e.kind, e.tmdb_id);
        } else {
          this.requestsChanged(null);
        }
        if (this.remember(newsKeys(e.request_id, complete, e.seasons))) {
          this.notify({ text: readyText(e.title, e.seasons, complete), itemId: e.item_id ?? null, poster: e.poster_url ?? null });
        }
        break;
      }
      case "request_decided":
        // Declined news raises `unseen`; TVs show it as the number and in the lists, never as a pop-up.
        this.newsHeard++;
        this.requestsSoon();
        this.requestsChanged(null);
        break;
      case "request_created":
      case "request_failed":
        this.requestsSoon();
        this.requestsChanged(null);
        break;
      case "requests_settings_changed":
        this.requestsMissing = false;
        this.requestsSoon();
        break;
      default:
        break;
    }
    for (const l of this.eventListeners.slice()) l(e);
  }

  // ---- requests

  /**
   * The requests summary: at start, after a profile switch, when the
   * events reconnect, on request events (once, 1 s after a burst) and
   * every 60 s while the events are down. A server without requests (404)
   * or with an older build of them (403) has none until its events
   * reconnect or the app starts; one out of reach keeps what was heard.
   * One at a time: `changed` (an event came since the one in flight was
   * asked for) asks once more after it.
   */
  loadRequests(changed = false): Promise<void> {
    if (this.summaryLoad) {
      if (changed) this.summaryAgain = true;
      return this.summaryLoad;
    }
    const after = (): Promise<void> | undefined => {
      this.summaryLoad = null;
      if (!this.summaryAgain) return;
      this.summaryAgain = false;
      return this.loadRequests();
    };
    const run = this.fetchRequests().then(after, after);
    this.summaryLoad = run;
    return run;
  }

  private summaryLoad: Promise<void> | null = null;
  private summaryAgain = false;

  private async fetchRequests(): Promise<void> {
    const api = this.api.value;
    if (!api || !this.tokens?.signedIn || this.requestsMissing) return;
    const generation = this.generation;
    let summary: RequestSummary;
    try {
      summary = await api.requestSummary();
    } catch (e) {
      if (this.api.value !== api || this.generation !== generation) return;
      if (e instanceof ApiError && (e.status === 404 || e.status === 403)) {
        this.requestsMissing = true;
        this.keepRequests(null);
      }
      return;
    }
    if (this.api.value !== api || this.generation !== generation) return;
    const before = this.requests.value;
    this.keepRequests(summary);
    // Asked instead of hearing: what is on screen asks again when something changed.
    if (this.eventsDown.value && before && summary.changed_at !== before.changed_at) this.requestsChanged(null);
    // News no event explained (it came while the app wasn't listening): caught up with, once at start as well.
    const unexplained = !!before && summary.unseen > before.unseen && this.newsHeard === 0;
    this.newsHeard = 0;
    if (summary.enabled && summary.sign_in_again !== "requests" && summary.unseen > 0 && (!this.caughtUp || unexplained)) void this.catchUp();
  }

  private keepRequests(summary: RequestSummary | null): void {
    this.requests.value = summary;
    if (summary) this.store.set(REQUESTS_KEY, JSON.stringify(summary));
    else this.store.remove(REQUESTS_KEY);
  }

  /** The summary once, a second after the last request event of a burst. */
  private requestsSoon(): void {
    if (this.summaryTimer !== null) clearTimeout(this.summaryTimer);
    this.summaryTimer = setTimeout(() => {
      this.summaryTimer = null;
      void this.loadRequests(true);
    }, SUMMARY_GATHER_MS);
  }

  /** The pages that show requests ask again, once for a burst: about one title (`kind:tmdb_id`), or about any (`null`). */
  requestsChanged(title: string | null): void {
    this.changedTitles.add(title ?? "*");
    if (this.changeTimer !== null) return;
    this.changeTimer = setTimeout(() => {
      this.changeTimer = null;
      const titles = Array.from(this.changedTitles);
      this.changedTitles.clear();
      const one = titles.length === 1 && titles[0] !== "*" ? titles[0]! : null;
      this.requestChanges.value = { n: this.requestChanges.value.n + 1, title: one };
    }, REQUEST_GATHER_MS);
  }

  /** A page draws titles' badges: request events keep them right meanwhile. Returns the end of it. */
  watchMarks(): () => void {
    this.markWatchers++;
    let done = false;
    return () => {
      if (done) return;
      done = true;
      this.markWatchers--;
    };
  }

  /** A title's badge from now on, in every row already drawn (asking, an event). */
  markTitle(kind: TitleKind, tmdbId: number, status: TitleStatus, itemId: number | null): void {
    this.marks.value = { ...this.marks.value, [titleKey({ kind, tmdb_id: tmdbId })]: { status, item_id: itemId, at: Date.now() } };
  }

  /**
   * A request changed: its title's badge follows it (`statusFromRequest`).
   * The requests named within 0.3 s are asked for once each, then the
   * details of the titles only they can tell, and the badges change
   * together, so the pages that draw them do once.
   */
  private markFromRequest(id: number, kind: TitleKind, tmdbId: number): void {
    if (!this.api.value || this.markWatchers <= 0) return;
    this.markNamed.set(id, { kind, tmdbId });
    this.markSoon();
  }

  /** A title's badge from its details (`GET /api/discover/{kind}/{tmdb_id}`), in the next round. */
  private markFromTitle(kind: TitleKind, tmdbId: number): void {
    if (!this.api.value || this.markWatchers <= 0) return;
    this.markAsked.set(titleKey({ kind, tmdb_id: tmdbId }), { kind, tmdbId });
    this.markSoon();
  }

  private markSoon(): void {
    if (this.markTimer === null) this.markTimer = setTimeout(() => this.markRound(), REQUEST_GATHER_MS);
  }

  private markRound(): void {
    this.markTimer = null;
    const named = Array.from(this.markNamed.entries());
    const asked = new Map(this.markAsked);
    this.markNamed.clear();
    this.markAsked.clear();
    const api = this.api.value;
    if (!api || this.markWatchers <= 0) return;
    const generation = this.generation;
    type Found = { key: string; status: TitleStatus; itemId: number | null };
    const requests = named.map(([id, t]) =>
      api.request(id).then(
        (r) => ({ t, status: statusFromRequest(r), itemId: r.item_id }),
        (e: unknown) => ({ t, status: isApiError(e, 404) ? statusFromRequest(null) : null, itemId: null }),
      ),
    );
    void Promise.all(requests)
      .then((answers) => {
        if (this.generation !== generation) return [];
        const found: Found[] = [];
        for (const a of answers) {
          const key = titleKey({ kind: a.t.kind, tmdb_id: a.t.tmdbId });
          if (a.status === "ask") asked.set(key, a.t);
          else if (a.status !== null) found.push({ key, status: a.status, itemId: a.itemId });
        }
        const titles = Array.from(asked.entries()).map(([key, t]) =>
          api.discoverTitle(t.kind, t.tmdbId).then(
            (d): Found | null => ({ key, status: d.status, itemId: d.item_id }),
            () => null,
          ),
        );
        // What the details say comes last, over what a request said of the same title.
        return Promise.all(titles).then((told) => found.concat(told.filter((x): x is Found => x !== null)));
      })
      .then((all) => {
        if (this.generation !== generation || !all.length) return;
        const marks = { ...this.marks.value };
        const at = Date.now();
        for (const a of all) marks[a.key] = { status: a.status, item_id: a.itemId, at };
        this.marks.value = marks;
      });
  }

  /** A pop-up that waits its turn (while a video plays, among the last three); the one on screen stays until it closes. */
  private notify(n: Omit<RequestNotice, "id">): void {
    const all = this.notices.value.concat([{ ...n, id: ++this.noticeIds }]);
    const waiting = all.filter((x) => x.id !== this.noticeOnScreen).slice(-NOTICES_KEPT);
    this.notices.value = all.filter((x) => x.id === this.noticeOnScreen || waiting.indexOf(x) >= 0);
  }

  /** The shell shows this notice (`null`: none). */
  showingNotice(id: number | null): void {
    this.noticeOnScreen = id;
  }

  /** The notice was opened, closed or timed out. */
  dropNotice(id: number): void {
    if (id === this.noticeOnScreen) this.noticeOnScreen = null;
    this.notices.value = this.notices.value.filter((n) => n.id !== id);
  }

  /** The news announced to this profile on this server: the newest `updated_at`, and each piece once. */
  private newsKept(): { key: string; at: number; keys: string[] } | null {
    const server = this.server.value;
    const me = this.me.value;
    if (!server || !me) return null;
    const key = `${NEWS_KEY}.${server}.${me.id}`;
    const kept = readJson<{ at?: unknown; keys?: unknown }>(this.store, key);
    const at = kept && typeof kept.at === "number" ? kept.at : 0;
    const keys = kept && Array.isArray(kept.keys) ? kept.keys.filter((k): k is string => typeof k === "string") : [];
    return { key, at, keys };
  }

  /** Keeps `keys` as announced; false when every one was already (or whose news it is isn't known yet: it is announced anyway). */
  private remember(keys: string[], at = 0): boolean {
    const kept = this.newsKept();
    if (!kept) return true;
    const fresh = keys.filter((k) => kept.keys.indexOf(k) < 0);
    const newest = Math.max(kept.at, at);
    if (fresh.length || newest !== kept.at) this.store.set(kept.key, JSON.stringify({ at: newest, keys: kept.keys.concat(fresh).slice(-200) }));
    return fresh.length > 0;
  }

  /**
   * News that came while the app wasn't listening: the profile's
   * unseen requests, of which TVs announce what is ready to watch, each
   * once and only what is newer than the last announced: one title by its
   * words, several as "3 of your requests have news".
   */
  private async catchUp(): Promise<void> {
    const api = this.api.value;
    if (!api || this.catchingUp) return;
    const kept = this.newsKept();
    if (!kept) {
      // Whose news it is comes with who is watching (loadAccount).
      this.catchUpWaiting = true;
      return;
    }
    this.catchUpWaiting = false;
    this.catchingUp = true;
    const generation = this.generation;
    try {
      const list = await api.requests({ unseen: true, limit: 20 });
      if (this.generation !== generation) return;
      this.caughtUp = true;
      const keys = (r: MediaRequest) => newsKeys(r.id, r.stage === "available", r.available_seasons);
      const news = list.requests.filter((r) => readyNews(r) && r.updated_at > kept.at && keys(r).some((k) => kept.keys.indexOf(k) < 0));
      if (!news.length) return;
      this.remember(
        news.reduce((all: string[], r) => all.concat(keys(r)), []),
        Math.max(...news.map((r) => r.updated_at)),
      );
      const one = news[0]!;
      this.notify(
        news.length === 1
          ? { text: readyText(one.title, one.available_seasons, one.stage === "available"), itemId: one.item_id, poster: one.poster_url }
          : { text: newsText(news.length), itemId: null, poster: null },
      );
    } catch {
      // Asked again with the next summary that has news.
    } finally {
      this.catchingUp = false;
    }
  }

  /** The watch state changes of the last 0.3 s: their details go, and Home and the pages ask again once (with a pending library change). */
  private applyStateChanges(): void {
    this.stateTimer = null;
    for (const [id, kept] of Array.from(this.items.entries())) {
      const d = kept.detail;
      const under = (p: number | null | undefined) => p !== null && p !== undefined && this.stateParents.has(p);
      if (this.stateIds.has(id) || under(d.parent_id) || under(d.grandparent_id)) this.items.delete(id);
    }
    this.stateIds.clear();
    this.stateParents.clear();
    if (this.libraryTimer !== null) {
      clearTimeout(this.libraryTimer);
      this.libraryTimer = null;
    }
    this.refresh();
  }
}
