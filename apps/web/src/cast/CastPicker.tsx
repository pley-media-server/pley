import { Globe, Laptop, LoaderCircle, Smartphone, Tv, type LucideIcon } from "lucide-react";
import type { CastTarget } from "@/api/cast";
import type { CastPlatform } from "@/api/types";
import { Modal } from "@/components/Modal";
import { fullTitle } from "@/lib/format";

const PLATFORM_ICONS: Record<CastPlatform, LucideIcon> = {
  web: Globe,
  desktop: Laptop,
  android: Smartphone,
  android_tv: Tv,
  kodi: Tv,
  roku: Tv,
  samsung: Tv,
  lg: Tv,
};

export function PlatformIcon({ platform, className }: { platform: CastPlatform; className?: string }) {
  const Icon = PLATFORM_ICONS[platform] ?? Tv;
  return <Icon className={className} aria-hidden />;
}

/** "Play on": the devices to cast to, each with what it plays now. */
export function CastPicker({
  open,
  devices,
  sending,
  onPick,
  onClose,
}: {
  open: boolean;
  devices: CastTarget[];
  /** The device being sent the title. */
  sending: string | null;
  onPick: (device: CastTarget) => void;
  onClose: () => void;
}) {
  return (
    <Modal open={open} onClose={onClose} title="Play on" size="sm" locked={sending !== null}>
      {devices.length ? (
        <ul className="-mx-2 space-y-0.5">
          {devices.map((device) => (
            <li key={device.id}>
              <button
                type="button"
                disabled={sending !== null}
                onClick={() => onPick(device)}
                className="flex w-full items-center gap-3 rounded-md px-2 py-2.5 text-left transition-colors hover:bg-white/5 disabled:opacity-60"
              >
                <PlatformIcon platform={device.platform} className="size-5 shrink-0 text-muted" />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-medium">{device.name}</span>
                  {device.state ? (
                    <span className="block truncate text-xs text-accent">Playing {fullTitle(device.state.item)}</span>
                  ) : null}
                </span>
                {sending === device.id ? <LoaderCircle className="size-4 shrink-0 animate-spin text-muted" aria-label="Sending" /> : null}
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-sm text-muted">No other device can play this now.</p>
      )}
    </Modal>
  );
}
