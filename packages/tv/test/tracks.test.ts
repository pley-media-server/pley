import { describe, expect, it } from "vitest";
import { castAction, castBanner, itemPlay, type CastAction, type CastEvent } from "../src/player/cast";
import { qualityAction, trackAction, videoControls, type PlayerCommand, type QualityAction, type TrackAction } from "../src/player/remote";
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

/** A quality command's answer as the fixture writes it. */
function writtenQuality(a: QualityAction): Record<string, unknown> {
  return a.act === "none" ? { act: "none", report: a.report } : { act: a.act, quality: a.id, report: a.report };
}

/** A cast's answer as the fixture writes it, with the banner it shows as it starts, and the switch first when it makes one. */
function writtenCast(a: CastAction, from: string | null): Record<string, unknown> {
  if (a.act === "none") return { act: "none" };
  const switched = a.switchTo ? { switch: { profile: a.switchTo.profile, ticket: a.switchTo.ticket } } : {};
  switch (a.act) {
    case "item":
      return { act: "item", item_id: a.itemId, start_ms: a.startMs, shuffle: a.shuffle, audio_index: a.audioIndex, subtitle_index: a.subtitleIndex, banner: castBanner(from), ...switched };
    case "playlist":
      return { act: "playlist", playlist_id: a.playlistId, shuffle: a.shuffle, banner: castBanner(from), ...switched };
    case "channel":
      return { act: "channel", channel_id: a.channelId, banner: castBanner(from), ...switched };
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
  it("controls: a video declares audio and subtitles when its file has them, and its quality", () => {
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

  it("qualities: what its Quality menu offers", () => {
    for (const c of fixture.qualities) {
      const p = c.in.player;
      const command = { type: "player_command", position_ms: null, offset_ms: null, stream_index: null, ...c.in.command } as PlayerCommand;
      sameFields(c.in, writtenQuality(qualityAction({ session: p.session_id, offered: p.offered, picked: p.picked }, command)), c.out);
    }
  });

  it("casts: this TV's own, while it shows, as the remote's profile", () => {
    for (const c of fixture.casts) {
      const e = { type: "cast", ...c.in.cast } as CastEvent;
      // The profile the TV acts as: none known when the case leaves it out.
      const profile = "profile" in c.in && typeof c.in.profile === "number" ? c.in.profile : null;
      sameFields(c.in, writtenCast(castAction(c.in.receiver_id, profile, c.in.shown, e), e.from), c.out);
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
