import { memo, useState } from "react";
import { Link } from "react-router";
import type { Id } from "@/api/types";
import { cn } from "@/lib/cn";
import { placeholderGradient, sizedImage } from "@/lib/images";
import { personUrl } from "@/lib/urls";

function initials(name: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean);
  const first = words[0]?.[0] ?? "";
  const last = words.length > 1 ? (words.at(-1)?.[0] ?? "") : "";
  return (first + last).toUpperCase();
}

/** Round photo of a person; their initials until it loads or when there is none. */
export const PersonPhoto = memo(function PersonPhoto({
  name,
  url,
  size,
  eager,
  className,
}: {
  name: string;
  url?: string | null;
  /** Rendered CSS width, to pick the image size. */
  size: number;
  eager?: boolean;
  className?: string;
}) {
  const [state, setState] = useState<{ url: string; loaded: boolean; failed: boolean } | null>(null);
  const current = state && state.url === url ? state : null;
  const src = url && !current?.failed ? sizedImage(url, size) : null;
  const loaded = !!src && !!current?.loaded;

  return (
    <div
      className={cn("relative aspect-square overflow-hidden rounded-full bg-surface-2 ring-1 ring-fg/10", className)}
      style={loaded ? undefined : { backgroundImage: placeholderGradient(name) }}
    >
      {loaded ? null : (
        <span
          className="absolute inset-0 flex items-center justify-center font-semibold tracking-wide text-white/70"
          style={{ fontSize: Math.max(12, Math.round(size * 0.3)) }}
          aria-hidden
        >
          {initials(name)}
        </span>
      )}
      {src && url ? (
        <img
          key={src}
          src={src}
          alt=""
          loading={eager ? "eager" : "lazy"}
          decoding="async"
          draggable={false}
          onLoad={() => setState({ url, loaded: true, failed: false })}
          onError={() => setState({ url, loaded: false, failed: true })}
          className={cn(
            "absolute inset-0 size-full object-cover object-[center_20%] transition-opacity duration-300",
            loaded ? "opacity-100" : "opacity-0",
          )}
        />
      ) : null}
    </div>
  );
});

/** A person with a line under the name (the part played, the job). */
export function PersonCard({
  person,
  subtitle,
  width,
  className,
}: {
  person: { id: Id; name: string; photo_url?: string | null };
  subtitle?: string;
  width: number;
  className?: string;
}) {
  return (
    <Link
      to={personUrl(person.id)}
      className={cn("group/person block rounded-md text-center outline-none", className)}
      style={{ width }}
    >
      <PersonPhoto
        name={person.name}
        url={person.photo_url}
        size={width}
        className="w-full transition duration-200 group-hover/person:ring-2 group-hover/person:ring-accent group-focus-visible/person:ring-2 group-focus-visible/person:ring-accent"
      />
      <div className="mt-2 truncate px-0.5 text-sm font-medium text-fg transition-colors group-hover/person:text-accent" title={person.name}>
        {person.name}
      </div>
      {subtitle ? (
        <div className="truncate px-0.5 text-xs text-muted" title={subtitle}>
          {subtitle}
        </div>
      ) : (
        <div className="h-4" />
      )}
    </Link>
  );
}
