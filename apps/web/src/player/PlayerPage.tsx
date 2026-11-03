import { useEffect, useMemo, useState } from "react";
import { Navigate, useLocation, useNavigate, useParams, useSearchParams } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import { LoaderCircle, TriangleAlert } from "lucide-react";
import { errorMessage } from "@/api/client";
import { keys, useItem } from "@/api/hooks";
import type { Id } from "@/api/types";
import { castPlayKey } from "@/cast/receiver";
import { decodeId, idParam } from "@/lib/ids";
import { androidPlayer } from "@/lib/shell";
import { itemUrl } from "@/lib/urls";
import { loadQueue } from "@/lib/videoQueue";
import { VideoPlayer } from "./VideoPlayer";

function num(v: string | null): number | null {
  if (v == null || v === "") return null;
  const n = Number(v);
  return Number.isFinite(n) ? n : null;
}

function PlayerMessage({ title, message, onBack }: { title: string; message: string; onBack: () => void }) {
  return (
    <div className="fixed inset-0 z-50 flex flex-col items-center justify-center gap-4 bg-black p-6 text-center text-white">
      <TriangleAlert className="size-12 text-accent" />
      <div>
        <div className="text-lg font-semibold">{title}</div>
        <p className="mt-1 text-sm text-white/70">{message}</p>
      </div>
      <button type="button" onClick={onBack} className="rounded-md bg-white/10 px-5 py-2 text-sm font-semibold hover:bg-white/20">
        Go back
      </button>
    </div>
  );
}

function Loading() {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black">
      <LoaderCircle className="size-12 animate-spin text-white/70" />
    </div>
  );
}

const PLAYABLE = new Set(["movie", "episode", "extra"]);

export function PlayerPage() {
  const { id } = useParams();
  const [params] = useSearchParams();
  const navigate = useNavigate();
  // A title sent by cast plays afresh, also the one playing now.
  const castPlay = castPlayKey(useLocation().state);
  const itemId = idParam(id);
  const item = useItem(Number.isFinite(itemId) ? itemId : null);
  // Playing through a playlist or collection (`?queue=&qi=`).
  const queueKey = params.get("queue");
  const queueIndex = num(params.get("qi"));
  const queue = useMemo(() => {
    const q = loadQueue(queueKey);
    return q && queueIndex != null && q.ids[queueIndex] === itemId ? { queue: q, index: queueIndex } : null;
  }, [queueKey, queueIndex, itemId]);
  const startMs = num(params.get("t")) ?? 0;
  const fileId = decodeId(params.get("file"));
  const audio = num(params.get("audio"));
  const sub = num(params.get("sub"));

  // In the Android app its own player takes over, and this page steps back
  // (`?web=1`: it couldn't, the web player plays).
  const native = useMemo(() => androidPlayer(), []);
  const data = item.data;
  // A copy known to be out of date (tracks were picked for the show, or
  // the profile's defaults changed) would start with the tracks it names:
  // the player waits for the new one, unless asking fails. Only before the
  // item first plays: a refetch while it plays changes nothing shown.
  const qc = useQueryClient();
  const [shown, setShown] = useState<Id | null>(null);
  const outdated =
    shown !== itemId && !!data && item.isFetching && !!qc.getQueryState(keys.item(itemId))?.isInvalidated;
  if (data && !outdated && shown !== itemId) setShown(itemId);
  /** Back where the player was opened from; opened straight (a bookmark, a new tab), to `fallback`. */
  const back = (fallback: string) => {
    const idx = (window.history.state as { idx?: number } | null)?.idx ?? 0;
    if (idx > 0) void navigate(-1);
    else void navigate(fallback, { replace: true });
  };
  const handOff = !!native && !params.has("web") && !!data && PLAYABLE.has(data.kind) && data.files.length > 0;
  useEffect(() => {
    if (!handOff || !native || !data) return;
    const title = data.kind === "episode" && data.grandparent_title ? `${data.grandparent_title} · ${data.title}` : data.title;
    if (native({ item_id: data.id, start_ms: startMs, file_id: fileId, audio, sub, title })) {
      back(itemUrl(data.kind === "extra" && data.parent_id ? data.parent_id : data.id));
    } else {
      const web = new URLSearchParams(params);
      web.set("web", "1");
      void navigate({ search: web.toString() }, { replace: true });
    }
    // Once per item: the Android player reports progress itself.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [handOff, data?.id]);

  if (!Number.isFinite(itemId) || itemId <= 0) return <Navigate to="/" replace />;
  if (item.isPending || handOff || outdated) return <Loading />;
  // A failed background refetch (e.g. after watch state changed) keeps the
  // previous data: don't tear down a playing video because of it.
  if (!data) {
    return <PlayerMessage title="Can't play this item" message={errorMessage(item.error)} onBack={() => back("/")} />;
  }
  if (!PLAYABLE.has(data.kind)) return <Navigate to={itemUrl(data.id)} replace />;
  if (!data.files.length) {
    return (
      <PlayerMessage title="Nothing to play" message="This item has no media files." onBack={() => back(itemUrl(data.id))} />
    );
  }

  return (
    <VideoPlayer
      key={`${data.id}:${queue?.index ?? ""}:${castPlay ?? ""}`}
      item={data}
      startMs={startMs}
      fileId={fileId}
      audio={audio}
      sub={sub}
      queue={queue}
    />
  );
}
