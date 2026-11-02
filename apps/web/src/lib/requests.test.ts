import { describe, expect, it } from "vitest";
import { makeDetail, makeRequest, makeSeason, makeShowDetail, makeSummary, makeTitle } from "@/test/requests";
import {
  actionLabel,
  actionQuestion,
  allowanceText,
  askedText,
  canAskMoreSeasons,
  createdText,
  dayText,
  decidedText,
  defaultPicks,
  failedText,
  freshNews,
  mainAction,
  markAnnounced,
  markTold,
  newsCountText,
  newsText,
  NO_NEWS,
  pickerButton,
  pickerSeasons,
  problemText,
  problemWords,
  readyText,
  requestsBadge,
  seasonAskable,
  seasonsLabel,
  seasonStateText,
  stageWords,
  titleAfter,
  titleBadge,
  titleName,
  untilText,
} from "./requests";

const NOW = 1_800_000_000;
const DAY = 86400;

describe("words", () => {
  it("names seasons in runs", () => {
    expect(seasonsLabel(null)).toBe("");
    expect(seasonsLabel([2])).toBe("Season 2");
    expect(seasonsLabel([3, 1, 2, 5])).toBe("Seasons 1–3, 5");
    expect(seasonsLabel([0, 1, 2])).toBe("Seasons 1–2 and specials");
    expect(seasonsLabel([0])).toBe("Specials");
  });

  it("says how long until a time, never sooner than it is", () => {
    expect(untilText(NOW + 4 * 60 + 10, NOW)).toBe("in 5 min");
    expect(untilText(NOW + 3 * 3600, NOW)).toBe("in 3 hours");
    expect(untilText(NOW + 11.2 * DAY, NOW)).toBe("in 12 days");
    expect(untilText(NOW - 10, NOW)).toBe("soon");
  });

  it("says a day, and the time within the next 24 hours", () => {
    const day = new Date(2027, 10, 4, 12).getTime() / 1000;
    expect(dayText(day, day - 20 * DAY)).toBe(new Date(day * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" }));
    expect(dayText(day, day - 400 * DAY)).toContain("2027");
    expect(dayText(day, day - 3600)).toMatch(/^today at /);
  });
});

describe("badges", () => {
  it("says where a title stands, in Discover's words", () => {
    expect(titleBadge(makeTitle({ status: "library", item_id: 5 }))).toBe("In library");
    expect(titleBadge(makeTitle({ status: "partial", item_id: 5 }))).toBe("Partly in library");
    expect(titleBadge(makeTitle({ status: "partial" }))).toBe("Requested");
    expect(titleBadge(makeTitle({ status: "requested" }))).toBe("Requested");
    expect(titleBadge(makeTitle({ status: "declined" }))).toBe("Declined");
    expect(titleBadge(makeTitle({ status: "restricted" }))).toBeNull();
    expect(titleBadge(makeTitle({ status: "none" }))).toBeNull();
  });

  it("puts it in the title's accessible name", () => {
    expect(titleName(makeTitle({ status: "library", item_id: 5 }))).toBe("The Shawshank Redemption, 1994, Movie, In library");
    expect(titleName(makeTitle({ year: null, kind: "show" }))).toBe("The Shawshank Redemption, Show");
  });

  it("follows a request's change in rows already shown", () => {
    const title = makeTitle();
    const asked = makeRequest({ id: 7, status: "pending", mine: true });
    expect(titleAfter(title, 7, asked)).toMatchObject({ status: "requested", request_id: 7 });
    expect(titleAfter(makeTitle({ status: "requested", request_id: 7 }), 7, makeRequest({ id: 7, status: "declined", mine: true }))).toMatchObject({
      status: "declined",
      request_id: null,
    });
    // Someone else's declined request (a manager's view) leaves no badge.
    expect(titleAfter(makeTitle({ status: "requested", request_id: 7 }), 7, makeRequest({ id: 7, status: "declined" })).status).toBe("none");
    expect(titleAfter(makeTitle({ status: "requested", request_id: 7 }), 7, makeRequest({ id: 7, status: "available", item_id: 9 }))).toMatchObject({
      status: "library",
      item_id: 9,
      request_id: null,
    });
    // A show's may leave aired seasons it didn't ask for missing: partly there, until its details say more.
    const show = makeTitle({ kind: "show", tmdb_id: 70523, status: "requested", request_id: 7 });
    expect(titleAfter(show, 7, makeRequest({ id: 7, kind: "show", tmdb_id: 70523, status: "available", item_id: 9 }))).toMatchObject({
      status: "partial",
      item_id: 9,
      request_id: null,
    });
    // Gone, deleted or withdrawn: no badge, but what a library has of a show stays.
    expect(titleAfter(makeTitle({ status: "requested", request_id: 7 }), 7, null).status).toBe("none");
    expect(titleAfter(makeTitle({ status: "requested", request_id: 7 }), 7, makeRequest({ id: 7, status: "available", stage: "gone" })).status).toBe("none");
    expect(titleAfter(makeTitle({ status: "partial", item_id: 3, request_id: 7 }), 7, null).status).toBe("partial");
    expect(titleAfter(makeTitle({ status: "library", item_id: 3 }), 7, asked).status).toBe("library");
    // Another active request for it keeps it Requested.
    expect(titleAfter(makeTitle({ status: "requested", request_id: 8 }), 7, null)).toMatchObject({ status: "requested", request_id: 8 });
  });
});

describe("allowance", () => {
  it("says nothing without a limit", () => {
    expect(allowanceText(makeSummary())).toBeNull();
    expect(allowanceText(makeSummary({ seasons_left: 3 }), "movie")).toBeNull();
  });

  it("says what's left, and the limits", () => {
    expect(allowanceText(makeSummary({ movies_left: 2, seasons_left: 5, movies_limit: 3, seasons_limit: 10 }))).toEqual({
      text: "You can ask for 2 more movies and 5 more seasons.",
      limits: "Limits: 3 movies and 10 seasons per 7 days.",
    });
    expect(allowanceText(makeSummary({ movies_left: 1, seasons_left: 5 }), "movie")).toEqual({ text: "You can ask for 1 more movie.", limits: null });
  });

  it("says when more of what's used up may be asked for", () => {
    const summary = makeSummary({ movies_left: 0, seasons_left: 0, movies_next_at: NOW + 3 * DAY, seasons_next_at: NOW + 5 * DAY });
    expect(allowanceText(summary, "movie", NOW)?.text).toBe("You've asked for all the movies you can for now: you can ask for more in 3 days.");
    expect(allowanceText(summary, undefined, NOW)?.text).toBe("You've asked for all you can for now: more movies in 3 days, more seasons in 5 days.");
    expect(allowanceText(makeSummary({ movies_left: 0, seasons_left: 2 }), undefined, NOW)?.text).toBe(
      "You can ask for 2 more seasons. You've asked for all the movies you can for now.",
    );
  });
});

describe("the season picker", () => {
  const show = makeShowDetail({
    seasons: [
      makeSeason(0),
      makeSeason(1, { state: "library" }),
      makeSeason(2, { state: "partial" }),
      makeSeason(3),
      makeSeason(4, { state: "requested" }),
      makeSeason(5, { declined: true }),
      makeSeason(6, { state: "unaired", aired: 0 }),
      makeSeason(7),
    ],
  });

  it("lists every season, specials last", () => {
    expect(pickerSeasons(show.seasons ?? []).map((s) => s.number)).toEqual([1, 2, 3, 4, 5, 6, 7, 0]);
  });

  it("ticks the missing and partial seasons, lowest first, up to what's left", () => {
    expect(defaultPicks(show, null)).toEqual([2, 3, 7]);
    expect(defaultPicks(show, 2)).toEqual([2, 3]);
    expect(defaultPicks(show, 0)).toEqual([]);
  });

  it("offers unaired seasons and specials, and not what's in a library, asked for or declined", () => {
    const askable = (show.seasons ?? []).filter((s) => seasonAskable(s, show)).map((s) => s.number);
    expect(askable).toEqual([0, 2, 3, 6, 7]);
    expect(seasonAskable(makeSeason(1), { status: "restricted" })).toBe(false);
  });

  it("shows Request more seasons only with aired seasons to ask for", () => {
    expect(canAskMoreSeasons(show)).toBe(true);
    const unaired = makeShowDetail({ seasons: [makeSeason(0), makeSeason(1, { state: "library" }), makeSeason(2, { state: "unaired" })] });
    expect(canAskMoreSeasons(unaired)).toBe(false);
  });

  it("says each season's state", () => {
    expect(seasonStateText(makeSeason(1, { state: "library" }))).toBe("In library");
    expect(seasonStateText(makeSeason(1, { state: "requested" }))).toBe("Requested");
    expect(seasonStateText(makeSeason(1, { declined: true }))).toBe("Declined");
    expect(seasonStateText(makeSeason(1, { state: "partial" }))).toBe("Partly in library");
    expect(seasonStateText(makeSeason(1, { state: "unaired", air_date: null }))).toBe("Not aired yet");
    expect(seasonStateText(makeSeason(1))).toBeNull();
    expect(pickerButton(3)).toBe("Request 3 seasons");
    expect(pickerButton(1)).toBe("Request 1 season");
  });
});

describe("the main action", () => {
  const summary = makeSummary();

  it("is the first that applies", () => {
    expect(mainAction(makeDetail(), makeSummary({ sign_in_again: "requests" }))).toEqual({ kind: "sign_in" });
    expect(mainAction(makeDetail({ status: "restricted", restriction: "Rated R: above this account's age limit" }), summary)).toEqual({
      kind: "restricted",
      text: "Rated R: above this account's age limit",
    });
    expect(mainAction(makeDetail({ status: "library", item_id: 4 }), summary)).toEqual({ kind: "open", itemId: 4 });
    expect(mainAction(makeDetail(), summary)).toEqual({ kind: "request" });
  });

  it("says when a declined title may be asked for again", () => {
    const declined = makeDetail({ status: "declined", declined: { reason: "Not on this server", at: NOW - DAY, again_at: NOW + 11.5 * DAY } });
    expect(mainAction(declined, summary, NOW)).toEqual({
      kind: "declined",
      text: "Declined: “Not on this server”. You can ask again in 12 days.",
      more: false,
    });
    const noReason = makeDetail({ status: "declined", declined: { reason: null, at: NOW - DAY, again_at: NOW + 3 * DAY } });
    expect(mainAction(noReason, summary, NOW)).toMatchObject({ text: "Declined. You can ask again in 3 days." });
    // A show's other seasons may still be asked for.
    const show = makeShowDetail({ status: "declined", seasons: [makeSeason(1, { declined: true }), makeSeason(2)] });
    expect(mainAction(show, summary, NOW)).toMatchObject({ kind: "declined", more: true });
  });

  it("tells a request this account made from one it follows, and one it doesn't", () => {
    const mine = makeRequest({ mine: true });
    expect(mainAction(makeDetail({ status: "requested", requests: [mine] }), summary)).toEqual({ kind: "requested", request: mine });
    const followed = makeRequest({ watching: true });
    expect(mainAction(makeDetail({ status: "requested", watching: true, requests: [followed] }), summary)).toEqual({
      kind: "following",
      request: followed,
    });
    expect(mainAction(makeDetail({ status: "requested" }), summary)).toEqual({ kind: "tell_me_too", seasons: null });
    const show = makeShowDetail({ status: "requested", seasons: [makeSeason(1, { state: "library" }), makeSeason(2, { state: "requested" })] });
    expect(mainAction(show, summary)).toEqual({ kind: "tell_me_too", seasons: [2] });
  });

  it("opens what a library has of a show, and offers the rest", () => {
    const show = makeShowDetail({ status: "partial", item_id: 8, seasons: [makeSeason(1, { state: "library" }), makeSeason(2)] });
    expect(mainAction(show, summary)).toEqual({ kind: "partial", itemId: 8, more: true });
  });
});

describe("stage pills", () => {
  it("says where a request is, in its colour", () => {
    expect(stageWords(makeRequest({ stage: "pending" }), false)).toMatchObject({ label: "Waiting for approval", tone: "warning" });
    expect(stageWords(makeRequest({ status: "approved", stage: "approved" }), false)).toMatchObject({ label: "Approved", tone: "info" });
    expect(stageWords(makeRequest({ status: "approved", stage: "searching" }), false)).toMatchObject({ label: "Searching", tone: "info" });
    expect(stageWords(makeRequest({ status: "approved", stage: "waiting" }), false).label).toBe("Not out yet");
    expect(stageWords(makeRequest({ status: "approved", stage: "importing" }), false).label).toBe("Adding to the library");
    expect(stageWords(makeRequest({ status: "available", stage: "available" }), false)).toMatchObject({ label: "Ready to watch", tone: "success" });
    expect(stageWords(makeRequest({ status: "available", stage: "gone" }), false)).toMatchObject({ label: "No longer in a library", tone: "neutral" });
    expect(stageWords(makeRequest({ status: "declined" }), false)).toMatchObject({ label: "Declined", tone: "danger" });
  });

  it("says when a movie is out and a show airs", () => {
    const at = NOW + 20 * DAY;
    const day = dayText(at, NOW);
    expect(stageWords(makeRequest({ status: "approved", stage: "waiting", stage_at: at }), false, NOW).label).toBe(`Out ${day}`);
    expect(stageWords(makeRequest({ kind: "show", status: "approved", stage: "waiting", stage_at: at }), false, NOW).label).toBe(`Airs ${day}`);
  });

  it("shows a download's progress and the queue's estimate", () => {
    const download = { progress: 0.42, state: "downloading" as const, eta: NOW + 20 * 60, episodes: 1 };
    const words = stageWords(makeRequest({ status: "approved", stage: "downloading", download }), false, NOW);
    expect(words).toMatchObject({ label: "Downloading 42% · in 20 min", tone: "info", download });
    const paused = { ...download, state: "paused" as const };
    expect(stageWords(makeRequest({ status: "approved", stage: "downloading", download: paused }), false, NOW).label).toBe("Download paused");
    const partial = makeRequest({ kind: "show", seasons: [1, 2, 3], available_seasons: [1, 2], status: "approved", stage: "partial", download });
    expect(stageWords(partial, false, NOW)).toMatchObject({ label: "Seasons 1–2 ready", tone: "success", download });
  });

  it("tells managers why a request failed, and others that it will be looked at", () => {
    const failed = makeRequest({ status: "failed", reason: "Radarr isn't answering" });
    expect(stageWords(failed, true)).toMatchObject({ label: "Couldn't send", tone: "danger", line: "Radarr isn't answering" });
    expect(stageWords({ ...failed, retry_at: NOW + 5 * 60 }, true, NOW).line).toBe("Radarr isn't answering. Trying again in 5 min.");
    expect(stageWords({ ...failed, reason: null }, false)).toMatchObject({
      label: "Not sent yet",
      line: "It couldn't be sent to download yet: an administrator will look at it.",
    });
    expect(stageWords({ ...failed, retry_at: NOW + 5 * 60 }, false, NOW).line).toBe("Trying again in 5 min.");
  });
});

describe("asking and actions", () => {
  it("says what asking did", () => {
    expect(askedText({ request: makeRequest(), following: false })).toBe("Asked for “The Shawshank Redemption”: an administrator decides.");
    expect(askedText({ request: makeRequest({ status: "approved" }), following: false })).toBe("“The Shawshank Redemption” is approved and on its way.");
    expect(askedText({ request: makeRequest({ status: "failed" }), following: false })).toBe(
      "Asked for “The Shawshank Redemption”, but it couldn't be sent to download yet.",
    );
    expect(askedText({ request: makeRequest({ watching: true }), following: true })).toBe("You'll hear when “The Shawshank Redemption” is ready.");
  });

  it("names each action, and asks before those that can't be taken back", () => {
    const pending = makeRequest();
    const declined = makeRequest({ status: "declined" });
    expect(actionLabel("withdraw", pending)).toBe("Withdraw");
    expect(actionLabel("withdraw", declined)).toBe("Remove");
    expect(actionLabel("unwatch", pending)).toBe("Stop following");
    expect(actionLabel("retry", pending)).toBe("Try again");
    expect(actionLabel("handled", pending)).toBe("Mark as added by hand");
    expect(actionLabel("ask_again", pending)).toBe("Request again");
    expect(actionQuestion("withdraw", pending)).toBe("Withdraw your request for “The Shawshank Redemption”?");
    expect(actionQuestion("withdraw", declined)).toBe("Remove “The Shawshank Redemption” from your requests?");
    expect(actionQuestion("delete", pending)).toBe("Delete this request?");
    expect(actionQuestion("approve", pending)).toBeNull();
  });
});

describe("notices", () => {
  it("says what happened, in the notices' words", () => {
    expect(readyText("Jaws", null, true)).toBe("“Jaws” is ready to watch.");
    expect(readyText("Dark", [2], false)).toBe("Season 2 of “Dark” is ready to watch.");
    expect(readyText("Dark", [1, 2], false)).toBe("Seasons 1–2 of “Dark” are ready to watch.");
    expect(decidedText("Jaws", "approved", null)).toBe("“Jaws” was approved.");
    expect(decidedText("Jaws", "declined", "Not here")).toBe("“Jaws” was declined: “Not here”.");
    expect(decidedText("Jaws", "declined", null)).toBe("“Jaws” was declined.");
    expect(createdText("Ann", "Dark", [1, 2, 3])).toBe("Ann asked for “Dark” (Seasons 1–3).");
    expect(createdText("Ann", "Jaws", null)).toBe("Ann asked for “Jaws”.");
    expect(failedText("Jaws", "movie", "Radarr refused the key")).toBe("“Jaws” couldn't be sent to Radarr: Radarr refused the key.");
    expect(failedText("Dark", "show", null)).toBe("“Dark” couldn't be sent to Sonarr.");
    expect(newsCountText(3)).toBe("3 of your requests have news");
  });

  it("tells each request's news once", () => {
    const ready = makeRequest({ id: 1, status: "available", stage: "available", updated_at: 100 });
    const declined = makeRequest({ id: 2, status: "declined", updated_at: 200 });
    const pending = makeRequest({ id: 3, updated_at: 300 });
    expect(newsText(ready)).toBe("“The Shawshank Redemption” is ready to watch.");
    expect(newsText(pending)).toBeNull();
    expect(freshNews([ready, declined, pending], NO_NEWS)).toEqual([ready, declined]);
    const announced = markAnnounced(NO_NEWS, [ready, declined]);
    expect(announced.at).toBe(200);
    expect(freshNews([ready, declined], announced)).toEqual([]);
    // Told as it came: its news isn't told again by a catch-up, though the request changed since.
    const later = makeRequest({ id: 4, status: "available", stage: "available", updated_at: 400 });
    expect(freshNews([later], markTold(announced, 4, "available", []))).toEqual([]);
    expect(freshNews([later], markTold(announced, 4, "approved", [1]))).toEqual([later]);
    // Gone from the libraries: nothing to tell.
    expect(newsText({ ...ready, stage: "gone" })).toBeNull();
  });

  it("tells of a show's seasons as more are ready, while the rest downloads too, and never twice", () => {
    const dark = makeRequest({
      id: 5,
      kind: "show",
      title: "Dark",
      seasons: [1, 2, 3, 4],
      status: "approved",
      stage: "downloading",
      available_seasons: [1, 2],
      updated_at: 500,
    });
    expect(newsText(dark)).toBe("Seasons 1–2 of “Dark” are ready to watch.");
    expect(newsText({ ...dark, available_seasons: [] })).toBeNull();
    // Season 2 told as it came, then the request read with Season 1, ready before it.
    const heard = markTold(markTold(NO_NEWS, 5, "approved", [2]), 5, "approved", [1, 2]);
    expect(heard.told[5]).toEqual({ status: "approved", seasons: [1, 2] });
    expect(freshNews([dark], heard)).toEqual([]);
    const more = { ...dark, available_seasons: [1, 2, 3], updated_at: 600 };
    expect(freshNews([more], heard)).toEqual([more]);
    expect(freshNews([more], markAnnounced(heard, [more]))).toEqual([]);
  });

  it("counts what waits on the entry to requests", () => {
    expect(requestsBadge(undefined)).toEqual({ count: 0, danger: false });
    expect(requestsBadge(makeSummary({ unseen: 2 }))).toEqual({ count: 2, danger: false });
    expect(requestsBadge(makeSummary({ manage: true, pending: 3, failed: 1, unseen: 2 }))).toEqual({ count: 6, danger: true });
    expect(requestsBadge(makeSummary({ manage: true, pending: 3 }))).toEqual({ count: 3, danger: false });
    expect(requestsBadge(makeSummary({ enabled: false, unseen: 2 }))).toEqual({ count: 0, danger: false });
  });
});

describe("problems", () => {
  const anyone = { manage: false };

  it("says why requests can't be used", () => {
    expect(problemWords({ kind: "off" }, anyone)).toMatchObject({ title: "Requests are off", text: "This server doesn't take requests for movies and shows." });
    expect(problemWords({ kind: "tmdb_down" }, anyone)).toMatchObject({ title: "TMDB isn't answering", retry: true });
    expect(problemWords({ kind: "tmdb_key" }, anyone)).toMatchObject({ text: "An administrator needs to check it.", retry: false, settings: false });
    expect(problemWords({ kind: "tmdb_key" }, { manage: true })).toMatchObject({ title: "TMDB refused this server's key", settings: true, retry: false });
    expect(problemWords({ kind: "no_access" }, anyone).title).toBe("Requests aren't open to this account");
  });

  it("says when more may be asked for, from retry_at", () => {
    expect(problemText({ kind: "declined", retry_at: NOW + 12 * DAY }, anyone, NOW)).toBe("You can ask again in 12 days.");
    expect(problemText({ kind: "too_many", retry_at: NOW + 3 * DAY }, anyone, NOW)).toBe("You can ask for more in 3 days.");
    expect(problemText({ kind: "too_many", retry_at: null }, { manage: false, kind: "show", limits: makeSummary({ seasons_limit: 5 }) })).toBe(
      "You can ask for at most 5 seasons per 7 days: pick fewer.",
    );
    expect(problemText({ kind: "conflict", message: "it's in a library already", request: null, retry_at: null }, anyone)).toBe("It's in a library already");
    // When, from retry_at rather than the date in the message.
    const early = { kind: "conflict", message: "a declined request stays for 30 days: it may be removed from Nov 30", retry_at: NOW + 3 * DAY } as const;
    expect(problemText({ ...early, request: makeRequest({ status: "declined", mine: true }) }, anyone, NOW)).toBe("You can remove it in 3 days.");
    expect(problemText({ ...early, request: null }, anyone, NOW)).toBe("You can try again in 3 days.");
    expect(problemText({ kind: "no_access" }, anyone)).toBe("Requests aren't open to this account. An administrator can let it ask for movies and shows.");
  });
});
