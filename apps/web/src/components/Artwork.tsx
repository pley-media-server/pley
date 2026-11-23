import { memo, useState } from "react";
import { markLayers, type MarkGlyph } from "pley-placeholder-mark";
import { cn } from "@/lib/cn";
import { placeholderGradient, sizedImage } from "@/lib/images";

export interface ArtworkProps {
  /** Unsized image urls in priority order (the first that loads wins). */
  candidates: string[];
  /** Rendered CSS width used to pick the image bucket. */
  width: number;
  alt: string;
  /** The title the placeholder's gradient is made from. */
  title: string;
  className?: string;
  imgClassName?: string;
  eager?: boolean;
  /**
   * Pley's mark with this glyph (`markGlyph(item.kind)`) once no picture will
   * come. Only where the title is named beside or under the picture (a card's
   * caption, a row's line): elsewhere the mark would say nothing about what it
   * stands for. Backdrops, mosaics and thumbnails too small for it leave it out.
   */
  mark?: MarkGlyph;
  /** The title on the placeholder, for a picture nothing beside it names. */
  titled?: boolean;
}

/**
 * Lazy-loaded image with fade-in, fallback chain and a gradient placeholder.
 * Once no picture will come (none, or none of the candidates loaded) the
 * gradient has the title on it when `titled`, else Pley's mark when given
 * one; not while one loads, so a page of tiles scrolling in doesn't flash
 * marks. The parent controls the box size (aspect ratio) through `className`.
 */
export const Artwork = memo(function Artwork({
  candidates,
  width,
  alt,
  title,
  className,
  imgClassName,
  eager,
  mark,
  titled,
}: ArtworkProps) {
  const key = candidates.join("|");
  const [state, setState] = useState({ key, index: 0, loaded: false });
  const current = state.key === key ? state : { key, index: 0, loaded: false };
  const url = candidates[current.index];
  const src = url ? sizedImage(url, width) : null;
  const failed = !src;

  return (
    <div
      className={cn("relative overflow-hidden bg-surface-2", className)}
      style={!current.loaded ? { backgroundImage: placeholderGradient(title || alt) } : undefined}
    >
      {failed && titled ? (
        <div className="absolute inset-0 flex items-center justify-center p-3 text-center">
          <span className="line-clamp-4 text-sm font-semibold leading-snug text-white/70 [text-wrap:balance]">{title}</span>
        </div>
      ) : failed && mark ? (
        <PlaceholderMark glyph={mark} />
      ) : null}
      {src ? (
        <img
          key={src}
          src={src}
          alt={alt}
          loading={eager ? "eager" : "lazy"}
          decoding="async"
          draggable={false}
          onLoad={() => setState({ key, index: current.index, loaded: true })}
          onError={() => setState({ key, index: current.index + 1, loaded: false })}
          className={cn(
            "absolute inset-0 size-full object-cover transition-opacity duration-300",
            current.loaded ? "opacity-100" : "opacity-0",
            imgClassName,
          )}
        />
      ) : null}
    </div>
  );
});

/**
 * Pley's mark on a picture that won't come (packages/placeholder-mark): white
 * posters, each lighter than the one behind, `glyph` cut out of the front one,
 * centered at 40 % of the box's shorter side and 120 px at most. The SVG's box
 * is 40 % by 40 %; its square viewBox keeps the mark the shorter of the two.
 */
function PlaceholderMark({ glyph }: { glyph: MarkGlyph }) {
  return (
    <div className="pointer-events-none absolute inset-0 flex items-center justify-center" aria-hidden>
      <svg viewBox="0 0 64 64" className="size-[40%] max-h-[120px] max-w-[120px]">
        {markLayers(glyph).map((layer) => (
          <path key={layer.alpha} fill="#fff" fillOpacity={layer.alpha} fillRule={layer.evenOdd ? "evenodd" : undefined} d={layer.path} />
        ))}
      </svg>
    </div>
  );
}
