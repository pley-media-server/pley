import { useId, type ReactNode } from "react";
import { Cast } from "lucide-react";
import { useTranslation } from "react-i18next";
import { castError, useCast } from "@/api/hooks";
import type { CastKind, CastRequest } from "@/api/types";
import { castTargets, openRemoteOn, type OtherDevice } from "@/lib/remote";
import { Menu } from "./Menu";
import { DeviceSummary, useOtherDevices } from "./Remote";
import { useToast } from "./Toast";
import { Button } from "./ui";

/** The devices "Play on…" offers for `kind`: the user's other devices that take it. */
export function useCastTargets(kind: CastKind): OtherDevice[] {
  return castTargets(useOtherDevices().devices, kind);
}

export interface PlayOnProps {
  /** `item` for titles, music and playlists; `live` for a channel. */
  kind: CastKind;
  /** What it sends, as the page's own Play would play it now. */
  cast: () => CastRequest;
  /** "Play on…", "Shuffle on…", "Watch on…". */
  label?: string;
  icon?: ReactNode;
  /** As the page's own Play: a page's header has large buttons, a dialog's footer others. */
  size?: "md" | "lg";
  /** Its words hidden on a phone, where the row it sits in has room for icons only. */
  compact?: boolean;
  /** Another button than the page's secondary one (the live player's). */
  trigger?: (props: { open: boolean; toggle: () => void; label: string }) => ReactNode;
  align?: "left" | "right";
  placement?: "below" | "above";
  /** Hears the menu open and close (see `Menu`). */
  onOpenChange?: (open: boolean) => void;
  /** Once a device took it: besides saying so, and opening the remote on it. */
  onCast?: () => void;
}

/**
 * "Play on…" beside a page's own Play: the user's other devices that play
 * what the page shows, as the remote lists them. Picking one plays it
 * there, as that device's own Play would, says so, and opens the remote on
 * it. There is nothing to show while no device takes it.
 */
export function PlayOn({
  kind,
  cast,
  label: given,
  icon = <Cast className="size-4" />,
  size = "lg",
  compact,
  trigger,
  align = "left",
  placement = "below",
  onOpenChange,
  onCast,
}: PlayOnProps) {
  const { t } = useTranslation("components");
  const targets = useCastTargets(kind);
  const label = given ?? t("playOn.label");
  const send = useCast();
  const toast = useToast();
  if (!targets.length) return null;

  const play = (device: OtherDevice) => {
    if (!device.receiver) return;
    send.mutate(
      { receiverId: device.receiver.receiver_id, cast: cast() },
      {
        onSuccess: () => {
          toast.success(t("playOn.playingOn", { device: device.name }));
          openRemoteOn(device.id);
          onCast?.();
        },
        onError: (err) => toast.error(castError(err, device.name)),
      },
    );
  };

  return (
    <Menu
      label={label}
      align={align}
      placement={placement}
      onOpenChange={onOpenChange}
      className="w-72"
      trigger={({ open, toggle }) =>
        trigger ? (
          trigger({ open, toggle, label })
        ) : (
          <Button
            variant="secondary"
            size={size}
            icon={icon}
            onClick={toggle}
            aria-label={label}
            aria-haspopup="menu"
            aria-expanded={open}
            title={label}
            className={compact ? "max-sm:w-11 max-sm:px-0" : undefined}
          >
            <span className={compact ? "hidden sm:inline" : undefined}>{label}</span>
          </Button>
        )
      }
    >
      {(close) => (
        <>
          <div aria-hidden className="px-3.5 pb-1 pt-1.5 text-xs font-medium text-subtle">
            {label}
          </div>
          {targets.map((d) => (
            <DeviceItem
              key={d.id}
              device={d}
              onClick={() => {
                close();
                play(d);
              }}
            />
          ))}
        </>
      )}
    </Menu>
  );
}

/** A device in the menu, as the remote lists it. */
function DeviceItem({ device, onClick }: { device: OtherDevice; onClick: () => void }) {
  const lineId = useId();
  return (
    <button
      type="button"
      role="menuitem"
      aria-label={device.name}
      aria-describedby={lineId}
      onClick={onClick}
      className="flex w-full items-center gap-3 px-3.5 py-2 text-left outline-none transition-colors hover:bg-white/5 focus-visible:bg-white/10"
    >
      <DeviceSummary device={device} lineId={lineId} />
    </button>
  );
}
