import { memo, useState } from "react";
import type { Channel } from "@/api/livetv";
import { cn } from "@/lib/cn";
import { placeholderGradient, sizedImage } from "@/lib/images";

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
