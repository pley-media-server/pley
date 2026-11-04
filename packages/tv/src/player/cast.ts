import type { Id, ItemDetail, ItemSummary, Playlist, PlaylistEntry, ServerEvent } from "../api/types";
import type { PlayRequest } from "./plan";

/**
 * Casts from the user's remote, on another of their devices (docs/API.md,
 * "Casting"): this TV takes them while it shows, and plays what it is sent
 * as its own Play would, in place of what plays, saying where it came from.
 * The rules are pinned for both Samsung apps by test/fixtures/tracks.json.
 */

export type CastEvent = Extract<ServerEvent, { type: "cast" }>;

/** How long "Playing from …" shows over what starts. */
export const CAST_BANNER_MS = 4_000;

/** Said when a show, an album or a playlist has nothing to play. */
export const NOTHING_TO_PLAY = "Nothing to play";

/**
 * The event stream's query that makes this TV a receiver: its install id,
 * a TV, taking titles and playlists always and channels where Live TV
 * shows. The server names the TV, as it does its players.
 */
export function receiverQuery(id: string, live: boolean): string {
  return `receiver=${encodeURIComponent(id)}&device_kind=tv&casts=${live ? "item,live" : "item"}`;
}

/** What a cast asks of this TV: nothing, or the title, playlist or channel to play. */
export type CastAction =
  | { act: "none" }
  | { act: "item"; itemId: Id; startMs: number | null; shuffle: boolean }
  | { act: "playlist"; playlistId: Id; shuffle: boolean }
  | { act: "channel"; channelId: Id };

const NO_CAST: CastAction = { act: "none" };

/**
 * The casts' rule: one for another receiver is another device's, and one
 * that comes while the app can't take it (hidden, signed out, asking who's
 * watching: `shown` false) does nothing; else it plays what it names.
 */
export function castAction(receiver: string, shown: boolean, e: CastEvent): CastAction {
  if (!shown || e.receiver_id !== receiver) return NO_CAST;
  if (typeof e.item_id === "number") return { act: "item", itemId: e.item_id, startMs: typeof e.start_ms === "number" ? Math.max(0, e.start_ms) : null, shuffle: !!e.shuffle };
  if (typeof e.playlist_id === "number") return { act: "playlist", playlistId: e.playlist_id, shuffle: !!e.shuffle };
  if (typeof e.channel_id === "number") return { act: "channel", channelId: e.channel_id };
  return NO_CAST;
}

/** "Playing from {device}", as the cast starts; "another device" when the remote's has no name. */
export function castBanner(from: string | null): string {
  const name = (from ?? "").trim();
  return `Playing from ${name || "another device"}`;
}

/**
 * What a title cast here plays, as Play on its page does: a movie, an
 * episode or an extra in the player (from `startMs`, else where Play
 * resumes); a show or a season its next episode; an album or an artist
 * its tracks as the music's queue; a track its album from it.
 */
export type ItemPlay =
  | { plays: "video"; startMs: number | null }
  | { plays: "next_episode" }
  | { plays: "album" | "artist"; shuffle: boolean }
  | { plays: "track"; startMs: number | null }
  | { plays: "nothing" };

export function itemPlay(kind: string, startMs: number | null, shuffle: boolean): ItemPlay {
  switch (kind) {
    case "movie":
    case "episode":
    case "extra":
      return { plays: "video", startMs };
    case "show":
    case "season":
      return { plays: "next_episode" };
    case "album":
    case "artist":
      return { plays: kind, shuffle };
    case "track":
      return { plays: "track", startMs };
    default:
      // A kind a newer server knows.
      return { plays: "nothing" };
  }
}

/** What playing a cast reads from the server: the app's existing calls. */
export interface CastServer {
  item(id: Id): Promise<ItemDetail>;
  onDeck(id: Id): Promise<{ item: ItemSummary | null }>;
  children(id: Id): Promise<{ items: ItemSummary[] }>;
  playlist(id: Id): Promise<Playlist & { entries: PlaylistEntry[] }>;
}

/** What a cast needs from the app around it. */
export interface CastEnv {
  /** This TV's receiver id: its install id, which its playbacks are made with. */
  id(): string;
  /** Whether it takes casts now: the app shows, signed in, past "Who's watching?". */
  shown(): boolean;
  server(): CastServer | null;
  /** Plays a video in the player, in place of what plays. */
  video(request: PlayRequest): void;
  /** Plays `tracks` as the music's queue from `at` (`fromMs` into it), shuffled or not, in place of what plays; Now Playing shows. */
  music(tracks: ItemSummary[], at: number, shuffle: boolean, fromMs: number): void;
  /** Tunes channel `id` in the live player, in place of what plays. */
  channel(id: Id): void;
  /** A notice over what plays: where it came from, or that there is nothing to play. */
  say(text: string): void;
  /** Fetching what to play failed: the app says why, as its own Play does. */
  failed(error: unknown): void;
  random?: () => number;
}

/**
 * This TV's side of casting: a cast for it plays (the latest one, when
 * another comes while the first is still fetched), and says where it came
 * from as it starts.
 */
export class CastReceiver {
  private readonly env: CastEnv;
  private casts = 0;

  constructor(env: CastEnv) {
    this.env = env;
  }

  /** A cast heard on the event stream. */
  take(e: CastEvent): void {
    const action = castAction(this.env.id(), this.env.shown(), e);
    const server = this.env.server();
    if (action.act === "none" || !server) return;
    const mine = ++this.casts;
    // Still the cast to play once what it needs comes, and the app still shows.
    const current = () => mine === this.casts && this.env.shown();
    const banner = castBanner(e.from);
    this.play(server, action, current).then(
      (outcome) => {
        if (outcome === "played") this.env.say(banner);
        else if (outcome === "nothing") this.env.say(NOTHING_TO_PLAY);
      },
      (error: unknown) => {
        if (current()) this.env.failed(error);
      },
    );
  }

  /** Plays what `action` names: "dropped" when it no longer may (`current`), "nothing" when it has nothing to play. */
  private async play(server: CastServer, action: Exclude<CastAction, { act: "none" }>, current: () => boolean): Promise<"played" | "nothing" | "dropped"> {
    const env = this.env;
    if (action.act === "channel") {
      env.channel(action.channelId);
      return "played";
    }
    if (action.act === "playlist") {
      const playlist = await server.playlist(action.playlistId);
      if (!current()) return "dropped";
      const items = playlist.entries.map((e) => e.item);
      if (!items.length) return "nothing";
      const at = action.shuffle ? this.pick(items.length) : 0;
      // As the playlist's page plays it: music as the queue, video in the player through the playlist.
      if (playlist.kind === "audio") env.music(items, at, action.shuffle, 0);
      else env.video({ itemId: items[at]!.id, queue: { kind: "playlist", id: playlist.id } });
      return "played";
    }
    const detail = await server.item(action.itemId);
    if (!current()) return "dropped";
    const plan = itemPlay(detail.kind, action.startMs, action.shuffle);
    switch (plan.plays) {
      case "video":
        env.video(plan.startMs !== null ? { itemId: detail.id, startMs: plan.startMs } : { itemId: detail.id, fromStart: false });
        return "played";
      case "next_episode": {
        const { item } = await server.onDeck(detail.id);
        if (!current()) return "dropped";
        if (!item) return "nothing";
        env.video({ itemId: item.id, fromStart: false });
        return "played";
      }
      case "album":
      case "artist": {
        const tracks = plan.plays === "album" ? await this.tracks(server, detail.id) : await this.artistTracks(server, detail.id);
        if (!current()) return "dropped";
        if (!tracks.length) return "nothing";
        env.music(tracks, plan.shuffle ? this.pick(tracks.length) : 0, plan.shuffle, 0);
        return "played";
      }
      case "track": {
        // Its album from it, as the album's page plays a track; alone when it has none.
        const album = typeof detail.parent_id === "number" ? await this.tracks(server, detail.parent_id) : [];
        if (!current()) return "dropped";
        const at = album.findIndex((t) => t.id === detail.id);
        if (at >= 0) env.music(album, at, false, plan.startMs ?? 0);
        else env.music([detail], 0, false, plan.startMs ?? 0);
        return "played";
      }
      default:
        return "nothing";
    }
  }

  /** An album's tracks, in order. */
  private async tracks(server: CastServer, albumId: Id): Promise<ItemSummary[]> {
    return (await server.children(albumId)).items.filter((i) => i.kind === "track");
  }

  /** An artist's tracks, album by album as the server lists the albums: the artist page's Play. */
  private async artistTracks(server: CastServer, artistId: Id): Promise<ItemSummary[]> {
    const albums = (await server.children(artistId)).items.filter((i) => i.kind === "album");
    const lists = await Promise.all(albums.map((a) => this.tracks(server, a.id)));
    return ([] as ItemSummary[]).concat(...lists);
  }

  private pick(count: number): number {
    return Math.min(count - 1, Math.floor((this.env.random ?? Math.random)() * count));
  }
}
