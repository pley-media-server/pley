import { describe, expect, it } from "vitest";
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

interface Player {
  session_id: string | null;
  audio: number[];
  subtitles: number[];
  audio_index: number | null;
  subtitle_index: number;
}

describe("tracks, as the shared fixture has them", () => {
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
});
