import { afterEach, describe, expect, it } from "vitest";
import { ApiError } from "../src/api/client";
import type { DiscoverDetail, DiscoverSeason, DiscoverTitle, MediaRequest, RequestList, RequestSummary } from "../src/api/types";
import { dayAndTime, relativeTime, shortDate, timeUntil } from "../src/format/format";
import {
  actionRows,
  addTitles,
  allowanceLines,
  askedText,
  askRefusal,
  declinedText,
  hasMoreSeasons,
  hasRequests,
  keepUnchanged,
  mainAction,
  mayAsk,
  pickerButton,
  pickerSeasons,
  preselected,
  problemWords,
  readyText,
  requestNote,
  requestsCount,
  requestsProblem,
  searchText,
  seasonLine,
  seasonsLabel,
  seasonStage,
  stageWords,
  statusFromRequest,
  titleBadge,
  titleRequest,
} from "../src/requests";

/** Friday 30 October 2026, noon on the TV's clock. */
const NOW = new Date(2026, 9, 30, 12, 0, 0).getTime();
const AT = Math.floor(NOW / 1000);
const MIN = 60;
const HOUR = 3600;
const DAY = 86_400;

function summary(over: Partial<RequestSummary> = {}): RequestSummary {
  return {
    enabled: true,
    access: "ask",
    movies_left: null,
    seasons_left: null,
    movies_limit: null,
    seasons_limit: null,
    movies_next_at: null,
    seasons_next_at: null,
    manage: false,
    pending: 0,
    failed: 0,
    unseen: 0,
    own: 0,
    changed_at: null,
    sign_in_again: null,
    ...over,
  };
}

function request(over: Partial<MediaRequest> = {}): MediaRequest {
  return {
    id: 1,
    kind: "movie",
    tmdb_id: 10,
    title: "Arrival",
    year: 2016,
    poster_url: null,
    backdrop_url: null,
    seasons: null,
    available_seasons: [],
    anime: false,
    status: "pending",
    stage: "pending",
    stage_at: null,
    reason: null,
    note: null,
    mine: true,
    watching: false,
    watchers: 0,
    unseen: false,
    actions: [],
    requested_by: null,
    decided_by: null,
    decided_at: null,
    created_at: AT - 3 * DAY,
    updated_at: AT,
    available_at: null,
    retry_at: null,
    sent_to: null,
    item_id: null,
    download: null,
    ...over,
  };
}

function season(number: number, state: DiscoverSeason["state"], over: Partial<DiscoverSeason> = {}): DiscoverSeason {
  return { number, name: number ? `Season ${number}` : "Specials", episodes: 10, aired: 10, air_date: null, poster_url: null, in_library: 0, requested: false, state, declined: false, request_id: null, ...over };
}

function detail(over: Partial<DiscoverDetail> = {}): DiscoverDetail {
  return {
    kind: "movie",
    tmdb_id: 10,
    title: "Arrival",
    year: 2016,
    overview: "",
    poster_url: null,
    backdrop_url: null,
    rating: 7.9,
    status: "none",
    item_id: null,
    request_id: null,
    original_title: null,
    tagline: null,
    genres: [],
    release_date: null,
    runtime_minutes: 116,
    certification: null,
    network: null,
    show_status: null,
    anime: false,
    cast: [],
    seasons: null,
    imdb_id: null,
    declined: null,
    restriction: null,
    watching: false,
    requests: [],
    movies_left: null,
    seasons_left: null,
    movies_next_at: null,
    seasons_next_at: null,
    ...over,
  };
}

function show(seasons: DiscoverSeason[], over: Partial<DiscoverDetail> = {}): DiscoverDetail {
  return detail({ kind: "show", tmdb_id: 20, title: "Severance", runtime_minutes: null, seasons, ...over });
}

describe("who sees what", () => {
  it("shows Discover while requests are on and the profile may ask", () => {
    expect(mayAsk(summary())).toBe(true);
    expect(mayAsk(summary({ access: "auto" }))).toBe(true);
    expect(mayAsk(summary({ access: "off" }))).toBe(false);
    expect(mayAsk(summary({ enabled: false }))).toBe(false);
    expect(mayAsk(null)).toBe(false);
  });

  it("shows Your requests to a profile that may not ask only while it has some, and to none that must sign in again", () => {
    expect(hasRequests(summary({ access: "off" }))).toBe(false);
    expect(hasRequests(summary({ access: "off", own: 2 }))).toBe(true);
    expect(hasRequests(summary({ sign_in_again: "requests" }))).toBe(false);
    expect(hasRequests(summary({ sign_in_again: "approvals" }))).toBe(true);
    expect(hasRequests(summary({ enabled: false, own: 3 }))).toBe(false);
  });

  it("counts what waits for managers, in the danger colour while some failed, and only news for everyone else", () => {
    expect(requestsCount(summary({ manage: true, pending: 2, failed: 1, unseen: 1 }))).toEqual({ count: 4, danger: true });
    expect(requestsCount(summary({ manage: true, pending: 2, unseen: 1 }))).toEqual({ count: 3, danger: false });
    expect(requestsCount(summary({ pending: 5, unseen: 1 }))).toEqual({ count: 1, danger: false });
    expect(requestsCount(summary({ enabled: false, unseen: 4 }))).toEqual({ count: 0, danger: false });
  });
});

describe("a title's badge", () => {
  it("says In library, Partly in library where it can be opened, Requested and Declined; nothing else", () => {
    expect(titleBadge({ status: "library", item_id: 5 })).toBe("In library");
    expect(titleBadge({ status: "partial", item_id: 5 })).toBe("Partly in library");
    expect(titleBadge({ status: "partial", item_id: null })).toBe("Requested");
    expect(titleBadge({ status: "requested", item_id: null })).toBe("Requested");
    expect(titleBadge({ status: "declined", item_id: null })).toBe("Declined");
    expect(titleBadge({ status: "restricted", item_id: null })).toBeNull();
    expect(titleBadge({ status: "none", item_id: null })).toBeNull();
  });

  it("follows the news of a request the profile made or follows: active Requested, declined, a movie available In library, gone none", () => {
    expect(statusFromRequest(request({ status: "pending" }))).toBe("requested");
    expect(statusFromRequest(request({ status: "failed", stage: "failed" }))).toBe("requested");
    expect(statusFromRequest(request({ status: "approved", stage: "downloading" }))).toBe("requested");
    expect(statusFromRequest(request({ status: "declined", stage: "declined" }))).toBe("declined");
    expect(statusFromRequest(request({ status: "available", stage: "available" }))).toBe("library");
    expect(statusFromRequest(request({ status: "available", stage: "gone" }))).toBe("none");
    expect(statusFromRequest(request({ kind: "show", status: "approved", stage: "partial", available_seasons: [1] }))).toBe("partial");
    // Only the title's details can say: a show wholly ready may lack seasons nobody asked for, or have them all;
    // a request no longer seen may leave others' asking for the title.
    expect(statusFromRequest(request({ kind: "show", status: "available", stage: "available" }))).toBe("ask");
    expect(statusFromRequest(null)).toBe("ask");
    // One it follows counts; someone else's, which a manager hears of too, says nothing of the title.
    expect(statusFromRequest(request({ mine: false, watching: true, status: "declined", stage: "declined" }))).toBe("declined");
    expect(statusFromRequest(request({ mine: false, watching: false, status: "declined", stage: "declined" }))).toBeNull();
  });

  it("keeps each title once as pages come", () => {
    const t = (tmdb_id: number, kind: "movie" | "show" = "movie") => ({ kind, tmdb_id }) as DiscoverTitle;
    expect(addTitles([t(1), t(2)], [t(2), t(3), t(2, "show")]).map((x) => `${x.kind}:${x.tmdb_id}`)).toEqual(["movie:1", "movie:2", "movie:3", "show:2"]);
  });

  it("gives TMDB's search the words trimmed and cut to the server's 200 characters, never halfway through one", () => {
    expect(searchText("  dune  ")).toBe("dune");
    expect(searchText("a".repeat(250))).toBe("a".repeat(200));
    // 𝄞 is two of JavaScript's characters but one of the server's.
    expect(searchText(`${"𝄞".repeat(150)}x`)).toBe("𝄞".repeat(150) + "x");
    expect(searchText("𝄞".repeat(250))).toBe("𝄞".repeat(200));
    // What the cut leaves at its end goes too.
    expect(searchText(`${"a".repeat(199)} b`)).toBe("a".repeat(199));
  });
});

describe("a title page's main action", () => {
  it("asks to sign in again before anything else", () => {
    expect(mainAction(detail({ status: "library", item_id: 3 }), summary({ sign_in_again: "requests" }))).toEqual({ kind: "signIn" });
  });

  it("says why a restricted title can't be asked for, with no button", () => {
    const d = detail({ status: "restricted", restriction: "Rated R: above this account's age limit" });
    expect(mainAction(d, summary())).toEqual({ kind: "restricted", text: "Rated R: above this account's age limit" });
  });

  it("opens what the library has", () => {
    expect(mainAction(detail({ status: "library", item_id: 3 }), summary())).toEqual({ kind: "open", itemId: 3 });
  });

  it("says when a declined title may be asked for again, with the reason where there is one", () => {
    const d = detail({ status: "declined", declined: { reason: "Not on any service.", at: AT - 18 * DAY, again_at: AT + 12 * DAY } });
    expect(mainAction(d, summary(), NOW)).toEqual({ kind: "declined", text: "Declined: “Not on any service”. You can ask again in 12 days." });
    expect(declinedText({ reason: null, at: AT, again_at: AT + DAY }, NOW)).toBe("Declined. You can ask again tomorrow.");
  });

  it("shows the profile's own request, one it follows, or offers Tell me too with the seasons asked for", () => {
    const mine = request({ id: 7, mine: true });
    expect(mainAction(detail({ status: "requested", request_id: 7, requests: [mine] }), summary())).toEqual({ kind: "requested", request: mine });
    const followed = request({ id: 8, mine: false, watching: true });
    expect(mainAction(detail({ status: "requested", watching: true, requests: [followed] }), summary())).toEqual({ kind: "following", request: followed });
    const theirs = show([season(1, "library"), season(2, "requested"), season(3, "requested")], { status: "requested" });
    expect(mainAction(theirs, summary())).toEqual({ kind: "tellMeToo", seasons: [2, 3] });
    expect(mainAction(detail({ status: "requested" }), summary())).toEqual({ kind: "tellMeToo", seasons: undefined });
    // Hearing costs nothing, but asking needs access.
    expect(mainAction(detail({ status: "requested" }), summary({ access: "off" }))).toEqual({ kind: "noAccess" });
  });

  it("offers Open and Request more seasons for a show partly there", () => {
    const d = show([season(1, "library"), season(2, "missing")], { status: "partial", item_id: 4 });
    expect(mainAction(d, summary())).toEqual({ kind: "more", itemId: 4, more: true, ask: true, disabled: false });
    // Only seasons not out yet are left: nothing more to ask for now.
    const later = show([season(1, "library"), season(2, "unaired")], { status: "partial", item_id: 4 });
    expect(mainAction(later, summary())).toMatchObject({ kind: "more", more: false });
  });

  it("offers Request, disabled once nothing is left to ask for, and none to a profile that may not ask", () => {
    expect(mainAction(detail(), summary())).toEqual({ kind: "request", disabled: false });
    expect(mainAction(detail({ movies_left: 0, movies_next_at: AT + 3 * DAY }), summary())).toEqual({ kind: "request", disabled: true });
    // The seasons' allowance doesn't hold a movie back.
    expect(mainAction(detail({ seasons_left: 0 }), summary())).toEqual({ kind: "request", disabled: false });
    expect(mainAction(detail(), summary({ access: "off" }))).toEqual({ kind: "noAccess" });
  });

  it("follows this profile's request with its stage pill, else one it follows, else the newest", () => {
    const theirs = request({ id: 1, mine: false });
    const followed = request({ id: 2, mine: false, watching: true });
    const declined = request({ id: 3, status: "declined", stage: "declined" });
    expect(titleRequest(detail({ requests: [declined, theirs, followed] }))).toBe(followed);
    expect(titleRequest(detail({ requests: [declined, theirs] }))).toBe(theirs);
    expect(titleRequest(detail({ requests: [declined] }))).toBeNull();
  });
});

describe("seasons", () => {
  it("are named as the web names them", () => {
    expect(seasonsLabel([2])).toBe("Season 2");
    expect(seasonsLabel([5, 1, 2, 3])).toBe("Seasons 1–3, 5");
    expect(seasonsLabel([0, 1, 2])).toBe("Seasons 1–2 and specials");
    expect(seasonsLabel([0])).toBe("Specials");
    expect(seasonsLabel([])).toBe("");
  });

  it("are all listed by the picker, specials last", () => {
    const d = show([season(0, "missing"), season(2, "missing"), season(1, "library")]);
    expect(pickerSeasons(d).map((x) => x.number)).toEqual([1, 2, 0]);
  });

  it("are ticked when missing or partly had, lowest first, up to what may still be asked for; not unaired ones, specials or declined", () => {
    const d = show([season(0, "missing"), season(1, "library"), season(2, "partial"), season(3, "missing"), season(4, "missing", { declined: true }), season(5, "missing"), season(6, "unaired")]);
    expect(preselected(d, null)).toEqual([2, 3, 5]);
    expect(preselected(d, 2)).toEqual([2, 3]);
    expect(preselected(d, 0)).toEqual([]);
    expect(hasMoreSeasons(d)).toBe(true);
    expect(preselected(show([season(1, "missing")], { status: "restricted" }), null)).toEqual([]);
  });

  it("say their state, or what they hold", () => {
    expect(seasonLine(season(1, "library"))).toBe("In library");
    expect(seasonLine(season(1, "requested"))).toBe("Requested");
    expect(seasonLine(season(1, "missing", { declined: true }))).toBe("Declined");
    expect(seasonLine(season(1, "partial"))).toBe("Partly in library");
    expect(seasonLine(season(1, "missing", { episodes: 1 }))).toBe("1 episode");
    expect(seasonLine(season(3, "unaired", { air_date: "2026-11-04" }), NOW)).toBe("Airs Nov 4");
    expect(seasonLine(season(3, "unaired"), NOW)).toBe("Not aired yet");
  });

  it("are counted on the picker's button", () => {
    expect(pickerButton(3)).toBe("Request 3 seasons");
    expect(pickerButton(1)).toBe("Request 1 season");
    expect(pickerButton(0)).toBe("Pick a season.");
  });
});

describe("a request's stage", () => {
  const opts = { manage: false, hours24: true };

  it("is coloured by where it has got to", () => {
    expect(stageWords(request({ stage: "pending" }), opts, NOW)).toMatchObject({ text: "Waiting for approval", tone: "amber" });
    expect(stageWords(request({ status: "approved", stage: "approved" }), opts, NOW)).toMatchObject({ text: "Approved", tone: "blue" });
    expect(stageWords(request({ status: "approved", stage: "searching" }), opts, NOW)).toMatchObject({ text: "Searching", tone: "blue" });
    expect(stageWords(request({ status: "approved", stage: "importing" }), opts, NOW)).toMatchObject({ text: "Adding to the library", tone: "blue" });
    expect(stageWords(request({ status: "available", stage: "available", item_id: 3 }), opts, NOW)).toMatchObject({ text: "Ready to watch", tone: "green", note: null });
    expect(stageWords(request({ status: "available", stage: "gone" }), opts, NOW)).toMatchObject({ text: "No longer in a library", tone: "gray" });
    expect(stageWords(request({ status: "declined", stage: "declined", reason: "Not here" }), opts, NOW)).toMatchObject({ text: "Declined", tone: "red", note: "“Not here”" });
  });

  it("shows a download's share, when it ends, and its states", () => {
    const downloading = request({ status: "approved", stage: "downloading", download: { progress: 0.423, state: "downloading", eta: AT + 20 * MIN, episodes: 1 } });
    expect(stageWords(downloading, opts, NOW)).toEqual({ text: "Downloading 42% · in 20 min", tone: "blue", progress: 0.423, note: null });
    const queued = request({ status: "approved", stage: "downloading", download: { progress: 0, state: "queued", eta: null, episodes: 1 } });
    expect(stageWords(queued, opts, NOW).text).toBe("Waiting to download");
    const paused = request({ status: "approved", stage: "downloading", download: { progress: 0.5, state: "paused", eta: null, episodes: 1 } });
    expect(stageWords(paused, opts, NOW).text).toBe("Download paused");
    const warning = request({ status: "approved", stage: "downloading", download: { progress: 0.5, state: "warning", eta: null, episodes: 1 } });
    expect(stageWords(warning, opts, NOW).text).toBe("The download has a problem");
  });

  it("says which seasons are ready, the download bar still showing while more come", () => {
    const partial = request({ kind: "show", status: "approved", stage: "partial", seasons: [1, 2, 3], available_seasons: [1, 2], download: { progress: 0.1, state: "downloading", eta: null, episodes: 4 } });
    expect(stageWords(partial, opts, NOW)).toMatchObject({ text: "Seasons 1–2 ready", tone: "green", progress: 0.1 });
  });

  it("gives a release's day, and an episode's time within a day", () => {
    // Radarr knows the day only: 12:00 UTC of 4 November.
    const out = request({ status: "approved", stage: "waiting", stage_at: Date.UTC(2026, 10, 4, 12) / 1000 });
    expect(stageWords(out, opts, NOW).text).toBe("Out Nov 4");
    const airs = request({ kind: "show", status: "approved", stage: "waiting", stage_at: AT + 9 * HOUR + 5 * MIN });
    expect(stageWords(airs, opts, NOW).text).toBe("Airs today at 21:05");
    expect(stageWords(airs, { manage: false, hours24: false }, NOW).text).toBe("Airs today at 9:05 PM");
    const later = request({ kind: "show", status: "approved", stage: "waiting", stage_at: AT + 5 * DAY });
    expect(stageWords(later, opts, NOW).text).toBe("Airs Nov 4");
    expect(stageWords(request({ status: "approved", stage: "waiting" }), opts, NOW).text).toBe("Not out yet");
  });

  it("tells managers why sending failed, and everyone else that an administrator will look", () => {
    const failed = request({ status: "failed", stage: "failed", reason: "Radarr isn't answering" });
    // "Couldn't send: {reason}" in two parts: the pill, and the reason whole under it.
    expect(stageWords(failed, { manage: true, hours24: true }, NOW)).toMatchObject({ text: "Couldn't send", tone: "red", note: "Radarr isn't answering." });
    expect(stageWords({ ...failed, retry_at: AT + 5 * MIN }, { manage: true, hours24: true }, NOW).note).toBe("Radarr isn't answering. Trying again in 5 min.");
    expect(stageWords({ ...failed, reason: null }, opts, NOW)).toMatchObject({
      text: "Not sent yet",
      note: "It couldn't be sent to download yet: an administrator will look at it.",
    });
    expect(stageWords({ ...failed, reason: null, retry_at: AT + 5 * MIN }, opts, NOW).note).toBe("Trying again in 5 min.");
  });

  it("gives managers the server's note as a sentence, as the stage's line beside it is", () => {
    expect(requestNote(request({ note: "In Movies 4K, which Ann can't open" }))).toBe("In Movies 4K, which Ann can't open.");
    expect(requestNote(request({ note: "Radarr has the file, but no library has it: is its folder in one?" }))).toBe("Radarr has the file, but no library has it: is its folder in one?");
    expect(requestNote(request({ note: " " }))).toBeNull();
    expect(requestNote(request())).toBeNull();
  });

  it("says where a follower can't open what is ready", () => {
    expect(stageWords(request({ status: "available", stage: "available", item_id: null, mine: false, watching: true }), opts, NOW).note).toBe(
      "Ready, in a library you can't open: ask an administrator",
    );
  });

  it("is each season's own while a show's request is partly ready", () => {
    const download = { progress: 0.7, state: "downloading" as const, eta: AT + 2 * HOUR, episodes: 8 };
    const partial = request({ kind: "show", status: "approved", stage: "partial", seasons: [3, 4], available_seasons: [3], download });
    expect(seasonStage(partial, 3, opts, NOW)).toEqual({ text: "Ready to watch", tone: "green", progress: null, note: null });
    expect(seasonStage(partial, 4, opts, NOW)).toEqual({ text: "Downloading 70% · in 2 hours", tone: "blue", progress: 0.7, note: null });
    expect(seasonStage({ ...partial, download: null }, 4, opts, NOW)).toBeNull();
    expect(seasonStage(request({ kind: "show", stage: "pending", seasons: [3, 4] }), 4, opts, NOW)).toMatchObject({ text: "Waiting for approval", tone: "amber" });
  });
});

describe("a list of requests asked for again", () => {
  it("keeps each request answered as before as the object it was, so only the cards that changed draw again", () => {
    const download = { progress: 0.2, state: "downloading" as const, eta: null, episodes: 1 };
    const before: RequestList = {
      total: 2,
      counts: { pending: 0, approved: 1, declined: 0, failed: 0, available: 1 },
      requests: [request({ id: 1, status: "available", stage: "available", requested_by: { id: 2, name: "Ann", avatar_url: null, avatar_color: null } }), request({ id: 2, status: "approved", stage: "downloading", download })],
      people: [],
    };
    // A new answer is new objects throughout, the same as before but for the download's progress, and one more.
    const next: RequestList = JSON.parse(JSON.stringify(before)) as RequestList;
    next.requests[1]!.download!.progress = 0.4;
    next.requests.push(request({ id: 3 }));
    const kept = keepUnchanged(before, next);
    expect(kept.requests[0]).toBe(before.requests[0]);
    expect(kept.requests[1]).toBe(next.requests[1]);
    expect(kept.requests[2]).toBe(next.requests[2]);
    expect(kept.total).toBe(next.total);
    expect(keepUnchanged(undefined, next)).toBe(next);
  });
});

describe("a request's actions", () => {
  it("are the server's, in its order, with Decline with a reason… beside Decline", () => {
    const r = request({ actions: ["approve", "decline", "handled", "delete"] });
    expect(actionRows(r).map((a) => a.label)).toEqual(["Approve", "Decline", "Decline with a reason…", "Mark as added by hand", "Delete"]);
    expect(actionRows(request({ actions: ["retry", "decline"] })).map((a) => a.id)).toEqual(["retry", "decline", "decline_reason"]);
  });

  it("ask first for what can't be undone", () => {
    const [withdraw] = actionRows(request({ actions: ["withdraw"] }));
    expect(withdraw).toMatchObject({ label: "Withdraw", confirm: { title: "Withdraw your request for “Arrival”?" } });
    const [remove] = actionRows(request({ status: "declined", stage: "declined", actions: ["withdraw"] }));
    expect(remove).toMatchObject({ label: "Remove", confirm: { title: "Remove “Arrival” from your requests?" } });
    const [del] = actionRows(request({ actions: ["delete"] }));
    expect(del).toMatchObject({ danger: true, confirm: { title: "Delete this request?", body: "Radarr and Sonarr keep what they have." } });
  });

  it("leave out what this app doesn't know", () => {
    const r = request({ actions: ["unwatch", "frobnicate" as never, "ask_again"] });
    expect(actionRows(r).map((a) => a.label)).toEqual(["Stop following", "Request again"]);
  });
});

describe("what may still be asked for", () => {
  it("says how many, and the limits per 7 days", () => {
    const a = { movies_left: 2, seasons_left: 5, movies_next_at: null, seasons_next_at: null, movies_limit: 3, seasons_limit: 10 };
    expect(allowanceLines(a, NOW)).toEqual(["You can ask for 2 more movies and 5 more seasons.", "Limits: 3 movies and 10 seasons per 7 days."]);
  });

  it("says when more may be asked for once one kind is used up", () => {
    const a = { movies_left: 0, seasons_left: null, movies_next_at: AT + 3 * DAY, seasons_next_at: null };
    expect(allowanceLines(a, NOW)).toEqual(["You've asked for all the movies you can for now: you can ask for more in 3 days."]);
  });

  it("says when for each once both are", () => {
    const a = { movies_left: 0, seasons_left: 0, movies_next_at: AT + 3 * DAY, seasons_next_at: AT + 5 * DAY };
    expect(allowanceLines(a, NOW)).toEqual(["You've asked for all you can for now: more movies in 3 days, more seasons in 5 days."]);
  });

  it("says only what a title's kind is asked within", () => {
    const a = { movies_left: 0, seasons_left: 0, movies_next_at: AT + 3 * DAY, seasons_next_at: AT + 5 * DAY, movies_limit: 3, seasons_limit: 10 };
    expect(allowanceLines({ ...a, kind: "movie" }, NOW)).toEqual([
      "You've asked for all the movies you can for now: you can ask for more in 3 days.",
      "Limits: 3 movies per 7 days.",
    ]);
    expect(allowanceLines({ ...a, kind: "show" }, NOW)).toEqual([
      "You've asked for all the seasons you can for now: you can ask for more in 5 days.",
      "Limits: 10 seasons per 7 days.",
    ]);
    expect(allowanceLines({ ...a, movies_left: null, movies_next_at: null, kind: "movie" }, NOW)).toBeNull();
  });

  it("is nothing without limits", () => {
    expect(allowanceLines({ movies_left: null, seasons_left: null, movies_next_at: null, seasons_next_at: null }, NOW)).toBeNull();
  });
});

describe("asking", () => {
  it("says what became of it", () => {
    expect(askedText(request({ status: "pending" }), true)).toBe("Asked for “Arrival”: an administrator decides.");
    expect(askedText(request({ status: "approved", stage: "approved" }), true)).toBe("“Arrival” is approved and on its way.");
    expect(askedText(request({ status: "failed", stage: "failed" }), true)).toBe("Asked for “Arrival”, but it couldn't be sent to download yet.");
    expect(askedText(request({ mine: false, watching: true }), false)).toBe("You'll hear when “Arrival” is ready.");
  });

  it("says when one may ask again from retry_at, never the message's date", () => {
    const declined = requestsProblem(new ApiError(409, "declined", "declined until 12 Nov", { code: "declined", message: "declined until 12 Nov", retry_at: AT + 12 * DAY }));
    expect(askRefusal(declined, "movie", null, NOW)).toBe("You can ask again in 12 days.");
    const limit = requestsProblem(new ApiError(429, "too_many_requests", "limit", { code: "too_many_requests", message: "limit", retry_at: AT + 3 * DAY }));
    expect(askRefusal(limit, "show", null, NOW)).toBe("You can ask for more in 3 days.");
    const tooMany = requestsProblem(new ApiError(429, "too_many_requests", "limit", { code: "too_many_requests", message: "limit", retry_at: null }));
    expect(askRefusal(tooMany, "show", { movies_limit: 3, seasons_limit: 5 }, NOW)).toBe("You can ask for at most 5 seasons per 7 days: pick fewer.");
  });

  it("reads why requests can't be used", () => {
    const off = requestsProblem(new ApiError(503, "unavailable", "requests are off", { code: "unavailable", setup: false }));
    expect(problemWords(off, false)).toEqual({ title: "Requests are off", body: "This server doesn't take requests for movies and shows.", retry: false });
    const down = requestsProblem(new ApiError(503, "tmdb_down", "TMDB isn't answering", { code: "tmdb_down", setup: true }));
    expect(problemWords(down, false)).toMatchObject({ title: "TMDB isn't answering", retry: true });
    const key = requestsProblem(new ApiError(503, "tmdb_key", "refused", { code: "tmdb_key", setup: true, retry: false }));
    expect(problemWords(key, true)).toMatchObject({ title: "TMDB refused this server's key", retry: false });
    expect(problemWords(key, false)?.body).toBe("An administrator needs to check it.");
    expect(requestsProblem(new ApiError(403, "no_access", "no", { code: "no_access" }))).toEqual({ kind: "noAccess" });
    expect(requestsProblem(new ApiError(403, "insufficient_scope", "scope", { code: "insufficient_scope" }))).toEqual({ kind: "signIn" });
    expect(requestsProblem(new ApiError(403, "restricted", "rated R: above this account's age limit", { code: "restricted" }))).toEqual({
      kind: "restricted",
      text: "Rated R: above this account's age limit.",
    });
  });

  it("hands back the request a race left behind", () => {
    const current = request({ status: "approved", stage: "approved" });
    const p = requestsProblem(new ApiError(409, "conflict", "it was approved already", { code: "conflict", message: "it was approved already", request: current }));
    expect(p).toEqual({ kind: "conflict", text: "It was approved already.", request: current });
  });
});

describe("news", () => {
  it("names what is ready to watch", () => {
    expect(readyText("Arrival", null, true)).toBe("“Arrival” is ready to watch.");
    expect(readyText("Severance", [2], false)).toBe("Season 2 of “Severance” is ready to watch.");
    expect(readyText("Severance", [1, 2], false)).toBe("Seasons 1–2 of “Severance” are ready to watch.");
  });
});

describe("relative times", () => {
  const original = (Intl as { RelativeTimeFormat?: unknown }).RelativeTimeFormat;
  afterEach(() => {
    (Intl as { RelativeTimeFormat?: unknown }).RelativeTimeFormat = original;
  });

  it("are written by the app, as the TVs' engines have no Intl.RelativeTimeFormat", () => {
    delete (Intl as { RelativeTimeFormat?: unknown }).RelativeTimeFormat;
    expect(timeUntil(AT + 10, NOW)).toBe("now");
    expect(timeUntil(AT + 5 * MIN, NOW)).toBe("in 5 min");
    expect(timeUntil(AT + HOUR, NOW)).toBe("in 1 hour");
    expect(timeUntil(AT + 3 * HOUR, NOW)).toBe("in 3 hours");
    expect(timeUntil(AT + DAY, NOW)).toBe("tomorrow");
    expect(timeUntil(AT + 12 * DAY, NOW)).toBe("in 12 days");
    expect(relativeTime(AT - 30, NOW)).toBe("now");
    expect(relativeTime(AT - 3 * HOUR, NOW)).toBe("3 hours ago");
    expect(relativeTime(AT - DAY, NOW)).toBe("yesterday");
    expect(relativeTime(AT - 3 * DAY, NOW)).toBe("3 days ago");
    expect(relativeTime(AT - 40 * DAY, NOW)).toBe("Sep 20, 2026");
    expect(shortDate(AT + 5 * DAY, NOW)).toBe("Nov 4");
    expect(shortDate(AT + 90 * DAY, NOW)).toBe("Jan 28, 2027");
    expect(dayAndTime(AT + 23 * HOUR, { time: true, hours24: true }, NOW)).toBe("tomorrow at 11:00");
    expect(dayAndTime(AT + 2 * DAY, { time: true, hours24: true }, NOW)).toBe("Nov 1");
  });
});
