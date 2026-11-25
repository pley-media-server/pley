import { List, RefreshCw, TriangleAlert, Tv2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ApiError, errorMessage, isApiError } from "@/api/client";
import { busyConnections, type BusyConnection, type Channel } from "@/api/livetv";
import { t } from "@/i18n";
import { cn } from "@/lib/cn";
import { ChannelLogo } from "@/pages/livetv/common";

/** What stopped Live TV playing, as the player says it. */
export type Trouble = {
  kind: "busy" | "unavailable" | "stopped" | "missing" | "other";
  title: string;
  message: string;
  /** All connections in use: what holds them. */
  connections?: BusyConnection[];
};

export function trouble(err: unknown, channel: string): Trouble {
  if (isApiError(err, 409)) {
    return { kind: "busy", title: t("player.busy", { ns: "livetv" }), message: errorMessage(err), connections: busyConnections(err) };
  }
  if (isApiError(err, 410)) return stopped(errorMessage(err));
  if (isApiError(err, 404)) return { kind: "missing", title: t("player.gone", { ns: "livetv" }), message: errorMessage(err) };
  if (isApiError(err, 403)) return { kind: "other", title: t("notOpen", { ns: "livetv" }), message: errorMessage(err) };
  if (isApiError(err, 503)) return { kind: "unavailable", title: t("player.notPlaying", { ns: "livetv", channel }), message: errorMessage(err) };
  return { kind: "other", title: t("player.didntStart", { ns: "livetv", channel }), message: errorMessage(err) };
}

/** The channel stopped being watched on the server (its connection taken over, say): why, as it says. */
export function stopped(message: string | null | undefined): Trouble {
  return { kind: "stopped", title: t("player.stopped", { ns: "livetv" }), message: message || t("player.notWatched", { ns: "livetv" }) };
}

/** The stream broke off for good: what hls.js says of it. */
export function brokeOff(channel: string, details: string): Trouble {
  return { kind: "other", title: t("player.channelStopped", { ns: "livetv", channel }), message: details };
}

/** Neither hls.js nor the browser itself plays the stream. */
export function cantPlay(): Trouble {
  return { kind: "other", title: t("player.cantPlay", { ns: "livetv" }), message: t("player.cantPlayHint", { ns: "livetv" }) };
}

/** An HLS error answered by the server: its status and its own words. */
export function hlsTrouble(response: { code?: number; text?: string } | undefined, channel: string): Trouble | null {
  if (!response?.code || response.code < 400) return null;
  let message = "";
  try {
    message = (JSON.parse(response.text ?? "") as { message?: string }).message ?? "";
  } catch {
    // not the server's JSON
  }
  const said = message || t("player.serverAnswered", { ns: "livetv", code: response.code });
  return trouble(new ApiError(response.code, "hls", said), channel);
}

/** Who holds a connection: "You, on another device", their names, or how many. */
function holders(c: BusyConnection): string {
  if (c.yours) return t("player.you", { ns: "livetv" });
  if (c.watchers.length) return c.watchers.join(", ");
  return c.people === 1 ? t("player.someoneElse", { ns: "livetv" }) : t("player.people", { ns: "livetv", count: c.people });
}

export function TroubleCard({
  trouble,
  channel,
  onRetry,
  onTakeOver,
  onBack,
  onChannels,
}: {
  trouble: Trouble;
  channel?: Channel;
  onRetry?: () => void;
  onTakeOver?: (channelId: number) => void;
  onBack: () => void;
  onChannels?: () => void;
}) {
  const { t } = useTranslation(["livetv", "common"]);
  const Icon = trouble.kind === "busy" ? Tv2 : TriangleAlert;
  const connections = trouble.connections ?? [];
  const anyToTake = connections.some((c) => c.can_take_over);
  return (
    <div className="w-full max-w-md rounded-2xl border border-white/10 bg-surface/95 p-6 text-center shadow-2xl animate-pop-in" role="alert">
      {channel ? (
        <ChannelLogo channel={channel} width={120} className="mx-auto mb-4 h-14 w-24 rounded-lg bg-white/[0.06]" />
      ) : (
        <Icon className={cn("mx-auto mb-4 size-10", trouble.kind === "busy" ? "text-accent" : "text-danger")} />
      )}
      <h2 className="text-lg font-semibold">{trouble.title}</h2>
      {connections.length ? (
        <>
          <p className="mt-2 text-sm text-muted [text-wrap:pretty]">
            {t("player.connectionsWatching", { count: connections.length })}
          </p>
          <ul className="mt-4 space-y-2 text-left">
            {connections.map((c) => (
              <li key={c.channel_id} className="flex items-center gap-3 rounded-lg border border-white/10 bg-white/[0.04] px-3 py-2.5">
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-medium">{c.channel_name}</div>
                  <div className="truncate text-xs text-muted">{holders(c)}</div>
                </div>
                {c.can_take_over && onTakeOver ? (
                  <button
                    type="button"
                    onClick={() => onTakeOver(c.channel_id)}
                    className="h-8 shrink-0 rounded-md bg-accent px-3 text-xs font-semibold text-accent-fg hover:bg-accent-hover"
                  >
                    {t("player.watchHere")}
                  </button>
                ) : null}
              </li>
            ))}
          </ul>
          {!anyToTake ? (
            <p className="mt-3 text-xs text-subtle [text-wrap:pretty]">
              {t("player.joinHint")}
            </p>
          ) : null}
        </>
      ) : trouble.message ? (
        <p className="mt-2 text-sm text-muted [text-wrap:pretty]">{trouble.message}</p>
      ) : null}
      <div className="mt-6 flex flex-wrap justify-center gap-2">
        {onRetry ? (
          <button type="button" onClick={onRetry} className="flex h-10 items-center gap-2 rounded-md bg-accent px-4 text-sm font-medium text-accent-fg hover:bg-accent-hover">
            <RefreshCw className="size-4" /> {trouble.kind === "stopped" ? t("player.watchAgain") : t("common:actions.tryAgain")}
          </button>
        ) : null}
        {onChannels ? (
          <button type="button" onClick={onChannels} className="flex h-10 items-center gap-2 rounded-md bg-surface-3 px-4 text-sm font-medium hover:bg-border">
            <List className="size-4" /> {t("player.otherChannels")}
          </button>
        ) : null}
        <button type="button" onClick={onBack} className="flex h-10 items-center rounded-md px-4 text-sm font-medium text-muted hover:bg-white/5 hover:text-fg">
          {t("player.backToGuide")}
        </button>
      </div>
    </div>
  );
}
