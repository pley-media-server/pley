import { describe, expect, it } from "vitest";
import type { LiveChannel } from "../src/api/types";
import { ChannelPages, type LiveBrowser } from "../src/live/pages";
import { liveControls, MUSIC_CONTROLS, remoteAction, videoControls, type PlayerCommand, type RemoteAction } from "../src/player/remote";
// Shared with the other TV apps' tests, which must answer the same.
import fixture from "./fixtures/remote.json";

interface Player {
  session_id: string;
  state: "playing" | "paused";
  position_ms: number;
  duration_ms: number | null;
  has_next: boolean;
  index?: number;
}

const command = (c: Omit<PlayerCommand, "type">): PlayerCommand => ({ type: "player_command", ...c }) as PlayerCommand;

/**
 * An action as the fixture writes it: what the player does, and whether
 * its session reports at once (a play, pause or seek does, and a play or
 * pause that finds it so already); a channel as the one it tunes.
 */
function written(a: RemoteAction, channel: number | null = null): Record<string, unknown> {
  switch (a.act) {
    case "none":
      return { act: "none", report: false };
    case "report":
      return { act: "none", report: true };
    case "seek":
      return { act: "seek", position_ms: a.positionMs, report: true };
    case "channel":
      return channel === null ? { act: "none", report: false } : { act: "channel", channel, report: false };
    default:
      return { act: a.act, report: a.act === "play" || a.act === "pause" };
  }
}

/** A server whose every channel is `order`'s, in that order. */
function lineup(order: number[]): LiveBrowser {
  const channels = order.map((id) => ({ id, name: `Ch ${id}`, number: String(id) }) as LiveChannel);
  return { liveBrowse: () => Promise.resolve({ total: channels.length, offset: 0, channels }) };
}

function played(kind: "video" | "music", p: Player, c: PlayerCommand) {
  return written(
    remoteAction(
      { kind, session: p.session_id, playing: p.state === "playing", positionMs: p.position_ms, durationMs: p.duration_ms, hasNext: p.has_next, index: p.index },
      c,
    ),
  );
}

describe("the remote, as the shared fixture has it", () => {
  it("controls", () => {
    for (const c of fixture.controls) {
      const declared = c.in.kind === "video" ? videoControls(!!c.in.has_next) : c.in.kind === "music" ? MUSIC_CONTROLS : liveControls(!!c.in.has_order);
      expect([c.in, declared]).toEqual([c.in, c.out]);
    }
  });

  it("video", () => {
    for (const c of fixture.video) {
      expect([c.in, played("video", c.in.player as Player, command(c.in.command as PlayerCommand))]).toEqual([c.in, c.out]);
    }
  });

  it("music", () => {
    for (const c of fixture.music) {
      expect([c.in, played("music", c.in.player as Player, command(c.in.command as PlayerCommand))]).toEqual([c.in, c.out]);
    }
  });

  // Through the pages channel up and down go through, as the player's remote step does: a
  // channel the remote would tune again is left alone.
  it("live", async () => {
    for (const c of fixture.live) {
      const p = c.in.player as { session_id: string; channel: number; channels: number[] | null };
      const action = remoteAction({ kind: "live", session: p.session_id, hasOrder: p.channels !== null }, command(c.in.command as PlayerCommand));
      let to: number | null = null;
      if (action.act === "channel") {
        const next = await new ChannelPages(lineup(p.channels ?? []), { kind: "all" }, { frozen: true }).neighbor(p.channel, action.by);
        to = next && next.id !== p.channel ? next.id : null;
      }
      expect([c.in, written(action, to)]).toEqual([c.in, c.out]);
    }
  });
});
