import { ListMusic, ListVideo } from "lucide-react";
import type { Playlist } from "@/api/lists";
import { Artwork } from "@/components/Artwork";
import { cn } from "@/lib/cn";

/** Square cover of a playlist: a 2×2 mosaic of its first entries, or the first one. */
export function PlaylistArt({
  playlist,
  width,
  className,
}: {
  playlist: Pick<Playlist, "title" | "kind" | "artwork">;
  width: number;
  className?: string;
}) {
  const art = playlist.artwork;
  const cover = art[0];
  const Icon = playlist.kind === "audio" ? ListMusic : ListVideo;
  return (
    <div className={cn("relative aspect-square overflow-hidden rounded-md bg-surface-2 ring-1 ring-white/5", className)}>
      {art.length >= 4 ? (
        <div className="grid size-full grid-cols-2 grid-rows-2">
          {art.slice(0, 4).map((url) => (
            <Artwork key={url} candidates={[url]} width={width / 2} alt="" title="" className="size-full" />
          ))}
        </div>
      ) : cover ? (
        <Artwork candidates={[cover]} width={width} alt="" title={playlist.title} mark={playlist.kind === "audio" ? "music" : "play"} className="size-full" />
      ) : (
        <div className="flex size-full items-center justify-center bg-linear-to-br from-surface-3 to-surface-2 text-subtle">
          <Icon className="size-1/3" strokeWidth={1.5} />
        </div>
      )}
    </div>
  );
}
