import type { Chapter, Credit, HistoryEntry, ItemSummary, Marker, MediaStream, PlayingSession, RemoteDevice } from "@/api/types";

let nextId = 1000;

/** An item summary: a movie unless `overrides` say otherwise. */
export function makeItem(overrides: Partial<ItemSummary> = {}): ItemSummary {
  const id = overrides.id ?? nextId++;
  return {
    id,
    library_id: 1,
    kind: "movie",
    title: `Item ${id}`,
    sort_title: `item ${id}`,
    added_at: 1_700_000_000,
    user_state: { position_ms: 0, watched: false, play_count: 0 },
    ...overrides,
  };
}

export function makeStream(overrides: Partial<MediaStream> = {}): MediaStream {
  return {
    index: 0,
    kind: "video",
    codec: "h264",
    language: null,
    title: null,
    default: true,
    forced: false,
    ...overrides,
  };
}

/** One of the user's sessions as `GET /api/sessions` lists it: a movie playing on a TV unless `overrides` say otherwise. */
export function makeSession(overrides: Partial<PlayingSession> = {}): PlayingSession {
  return {
    session_id: "s1",
    kind: "item",
    device: "Living room TV",
    device_id: "app-1",
    device_kind: "tv",
    item: makeItem({ id: 7 }),
    channel: null,
    state: "playing",
    position_ms: 0,
    duration_ms: 6_000_000,
    updated_at: 1_700_000_000,
    age_ms: 0,
    controls: [],
    audio_index: null,
    subtitle_index: null,
    tracks: { audio: [], subtitles: [] },
    ...overrides,
  };
}

/** One of the user's devices that takes casts, as `GET /api/remote/devices` lists it: a TV that takes both kinds. */
export function makeReceiver(overrides: Partial<RemoteDevice> = {}): RemoteDevice {
  return { receiver_id: "tv-1", device_id: "app-1", name: "Living room TV", kind: "tv", casts: ["item", "live"], ...overrides };
}

export function makeChapter(startMs: number, title: string | null = null, thumbUrl?: string): Chapter {
  return { start_ms: startMs, end_ms: startMs + 60_000, title, thumb_url: thumbUrl };
}

export function makeMarker(overrides: Partial<Marker> = {}): Marker {
  return { file_id: 1, kind: "intro", start_ms: 10_000, end_ms: 70_000, source: "fingerprint", ...overrides };
}

export function makeCredit(id: number, name: string, role: Credit["role"], character?: string): Credit {
  return { id, name, role, character };
}

export function makeHistoryEntry(id: number, item: ItemSummary, overrides: Partial<HistoryEntry> = {}): HistoryEntry {
  return { id, viewed_at: 1_700_000_000, user: { id: 1, name: "Admin" }, item, ...overrides };
}

/** A box at `left` (jsdom lays nothing out: every box is empty there). */
export function layoutBox(left: number, width: number, height = 20): DOMRect {
  return { left, width, height, x: left, y: 0, top: 0, right: left + width, bottom: height, toJSON: () => ({}) };
}
