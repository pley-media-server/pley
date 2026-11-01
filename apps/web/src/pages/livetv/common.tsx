import { memo, useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { categoryKey, liveQueries, type Channel, type LiveCategory, type Program } from "@/api/livetv";
import { cn } from "@/lib/cn";
import { placeholderGradient, sizedImage } from "@/lib/images";

/** The current time in unix seconds, updated every `everyMs`. */
export function useNow(everyMs = 15_000): number {
  const [now, setNow] = useState(() => Date.now() / 1000);
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now() / 1000), everyMs);
    return () => window.clearInterval(id);
  }, [everyMs]);
  return now;
}

const timeFormat = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });
const dayFormat = new Intl.DateTimeFormat(undefined, { weekday: "short", day: "numeric", month: "short" });

/** "20:00" (or "8:00 PM"), as the browser's locale writes times. */
export function formatTime(ts: number): string {
  return timeFormat.format(new Date(ts * 1000));
}

/** "20:00 – 21:00". */
export function formatSpan(p: Pick<Program, "start" | "stop">): string {
  return `${formatTime(p.start)} – ${formatTime(p.stop)}`;
}

/** "Today", "Tomorrow", "Yesterday", or "Fri 30 Oct". */
export function formatGuideDay(ts: number, now: number): string {
  // Dates apart, not 24-hour steps: a day is 23 or 25 hours when daylight saving starts or ends.
  const date = (t: number) => {
    const d = new Date(t * 1000);
    return Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()) / 86_400_000;
  };
  const apart = date(ts) - date(now);
  if (apart === 0) return "Today";
  if (apart === 1) return "Tomorrow";
  if (apart === -1) return "Yesterday";
  return dayFormat.format(new Date(ts * 1000));
}

/** Minutes left of a programme: "12 min left". */
export function timeLeft(p: Pick<Program, "stop">, now: number): string {
  const minutes = Math.max(1, Math.round((p.stop - now) / 60));
  if (minutes >= 60) {
    const h = Math.floor(minutes / 60);
    const m = minutes % 60;
    return m ? `${h} h ${m} min left` : `${h} h left`;
  }
  return `${minutes} min left`;
}

/** A channel's logo on a dark tile, or its number and name when it has none. */
export const ChannelLogo = memo(function ChannelLogo({
  channel,
  width,
  className,
  showName = true,
}: {
  channel: Pick<Channel, "logo" | "name" | "number">;
  width: number;
  className?: string;
  /** The name on the placeholder (small tiles show the number only). */
  showName?: boolean;
}) {
  const [failed, setFailed] = useState(false);
  const src = !failed ? sizedImage(channel.logo, width) : null;
  return (
    <div
      className={cn("relative flex items-center justify-center overflow-hidden", className)}
      style={src ? undefined : { backgroundImage: placeholderGradient(channel.name) }}
    >
      {src ? (
        <img
          src={src}
          alt=""
          loading="lazy"
          decoding="async"
          draggable={false}
          onError={() => setFailed(true)}
          className="max-h-[78%] max-w-[82%] object-contain drop-shadow-[0_1px_6px_rgba(0,0,0,0.45)]"
        />
      ) : (
        <span className="line-clamp-2 px-1.5 text-center text-xs font-bold leading-tight text-white/80 [text-wrap:balance]">
          {showName ? channel.name : (channel.number ?? channel.name.slice(0, 3))}
        </span>
      )}
    </div>
  );
});

/**
 * A name's region and the rest, when it begins with one the way providers
 * write them ("UK| Sport", "[UK] Sport", "UK: Sport"), as the server splits
 * group names.
 */
export function splitRegion(name: string): [string, string] | null {
  const n = name.trim().replace(/^\|+\s*/, "");
  const m = /^\[([^\]]{1,16})\]\s*(.+)$/.exec(n) ?? /^([^|]{1,16})\|\s*(.+)$/.exec(n) ?? /^([A-Z]{1,4}):\s*(.+)$/.exec(n);
  if (!m) return null;
  const rest = m[2]!.replace(/^[|:\-\s]+/, "").trim();
  const region = m[1]!.trim();
  return region && rest ? [region, rest] : null;
}

/**
 * A channel's name as a list shows it: in a group, without the region its
 * group already says ("Sky Sports 1" in UK · Sport, not "UK| Sky Sports 1").
 */
export function channelName(c: Pick<Channel, "name" | "group">, category?: LiveCategory): string {
  if (category?.kind !== "group") return c.name;
  const name = splitRegion(c.name);
  const group = splitRegion(c.group);
  return name && group && name[0].toLowerCase() === group[0].toLowerCase() ? name[1] : c.name;
}

const LAST_WATCHED = "pley.live.lastWatched";

/** The channel last watched in this tab: the guide opens back at it. */
export function lastWatched(): number | null {
  try {
    const id = Number(sessionStorage.getItem(LAST_WATCHED));
    return Number.isFinite(id) && id > 0 ? id : null;
  } catch {
    return null;
  }
}

export function setLastWatched(id: number) {
  try {
    sessionStorage.setItem(LAST_WATCHED, String(id));
  } catch {
    // Storage refused: the guide opens at its top.
  }
}

/**
 * Index of channel `id` in `category`, once known (null: not in it, or no
 * channel asked): where a list opens when coming back from the player.
 */
export function useIndexOf(category: LiveCategory, id: number | null): number | null | undefined {
  const around = useQuery({ ...liveQueries.around(category, id ?? 0), enabled: id != null });
  if (id == null) return null;
  if (!around.data) return around.isError ? null : undefined;
  const at = around.data.channels.findIndex((c) => c.id === id);
  return at < 0 ? null : around.data.offset + at;
}

/**
 * `/live/{source}/{channel}`: the channel, named with its source's short id
 * (two sources' channels share numbers), and the category it was chosen in
 * (channel up and down stay in it).
 */
export function liveUrl(channel: Pick<Channel, "id" | "source_uid">, category?: LiveCategory): string {
  const key = category ? categoryKey(category) : "all";
  const path = channel.source_uid ? `/live/${channel.source_uid}/${channel.id}` : `/live/${channel.id}`;
  return key === "all" ? path : `${path}?in=${encodeURIComponent(key)}`;
}
