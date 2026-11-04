import { describe, expect, it } from "vitest";
import { castAction, castBanner, itemPlay, type CastAction, type CastEvent } from "../src/player/cast";
import { trackAction, videoControls, type PlayerCommand, type TrackAction } from "../src/player/remote";
// Shared with the native Samsung app's tests, which must answer the same.
import fixture from "./fixtures/tracks.json";

/** Each field of `answer` against the fixture's `out`, one by one, so a difference names its field; then no field more or less. */
function sameFields(input: unknown, answer: Record<string, unknown>, out: Record<string, unknown>): void {
  for (const key of Object.keys(out)) expect([input, key, answer[key]]).toEqual([input, key, out[key]]);
  expect([input, Object.keys(answer).sort()]).toEqual([input, Object.keys(out).sort()]);
}

/** A track command's answer as the fixture writes it. */
function writtenTrack(a: TrackAction): Record<string, unknown> {
  return a.act === "none" ? { act: "none", report: a.report } : { act: a.act, stream_index: a.index, report: a.report };
}

/** A cast's answer as the fixture writes it, with the banner it shows as it starts. */
function writtenCast(a: CastAction, from: string | null): Record<string, unknown> {
  switch (a.act) {
    case "none":
      return { act: "none" };
    case "item":
      return { act: "item", item_id: a.itemId, start_ms: a.startMs, shuffle: a.shuffle, banner: castBanner(from) };
    case "playlist":
      return { act: "playlist", playlist_id: a.playlistId, shuffle: a.shuffle, banner: castBanner(from) };
    case "channel":
      return { act: "channel", channel_id: a.channelId, banner: castBanner(from) };
  }
}

interface Player {
  session_id: string | null;
  audio: number[];
  subtitles: number[];
  audio_index: number | null;
  subtitle_index: number;
}

describe("tracks and casts, as the shared fixture has them", () => {
  it("controls: a video declares audio and subtitles when its file has them", () => {
    for (const c of fixture.controls) {
      expect([c.in, videoControls(c.in.has_next, c.in.audio, c.in.subtitles)]).toEqual([c.in, c.out]);
    }
  });

  it("commands: audio and subtitles", () => {
    for (const c of fixture.commands) {
      const p = c.in.player as Player;
      const command = { type: "player_command", position_ms: null, offset_ms: null, ...c.in.command } as PlayerCommand;
      const answer = trackAction({ session: p.session_id, audio: p.audio, subtitles: p.subtitles, audioIndex: p.audio_index, subtitleIndex: p.subtitle_index }, command);
      sameFields(c.in, writtenTrack(answer), c.out);
    }
  });

  it("casts: this TV's own, while it shows", () => {
    for (const c of fixture.casts) {
      const e = { type: "cast", ...c.in.cast } as CastEvent;
      sameFields(c.in, writtenCast(castAction(c.in.receiver_id, c.in.shown, e), e.from), c.out);
    }
  });

  it("items: what a title cast plays, by its kind", () => {
    for (const c of fixture.items) {
      const plan = itemPlay(c.in.kind, c.in.start_ms, c.in.shuffle);
      const written: Record<string, unknown> =
        plan.plays === "video" || plan.plays === "track"
          ? { plays: plan.plays, start_ms: plan.startMs }
          : plan.plays === "album" || plan.plays === "artist"
            ? { plays: plan.plays, shuffle: plan.shuffle }
            : { plays: plan.plays };
      sameFields(c.in, written, c.out);
    }
  });
});
