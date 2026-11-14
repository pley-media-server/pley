import { useCallback, useEffect, useEffectEvent, useId, useRef, useState, type ReactNode } from "react";
import { Cast } from "lucide-react";
import { useTranslation } from "react-i18next";
import { isApiError } from "@/api/client";
import { castError, pairingError, useCast, usePairDevice } from "@/api/hooks";
import type { CastKind, CastRequest } from "@/api/types";
import { useAuth } from "@/auth";
import { cn } from "@/lib/cn";
import { useDismiss } from "@/lib/hooks";
import { castTargets, openRemoteOn, type OtherDevice } from "@/lib/remote";
import { fitBeside, Menu } from "./Menu";
import { PinPad, pinError } from "./PinPad";
import { DeviceSummary, useOtherDevices } from "./Remote";
import { useToast } from "./Toast";
import { Button } from "./ui";

/**
 * The devices "Play on…" offers for `kind`: the user's other devices that take it, their household's too, and
 * with `video` (a movie or an episode), the Cast devices the server plays on.
 */
export function useCastTargets(kind: CastKind, video = false): OtherDevice[] {
  return castTargets(useOtherDevices().devices, kind, video);
}

export interface PlayOnProps {
  /** `item` for titles, music and playlists; `live` for a channel. */
  kind: CastKind;
  /** It casts a movie or an episode, which Cast devices play too. */
  video?: boolean;
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
  /** Hears the menu, or the PIN it asks for, open and close (see `Menu`). */
  onOpenChange?: (open: boolean) => void;
  /** Once a device took it: besides saying so, and opening the remote on it. */
  onCast?: () => void;
}

/**
 * A cast waiting for a PIN: the user's, to a device on another profile when
 * theirs has one; or with `pairing`, the one an AirPlay device shows, to pair
 * the server with it first.
 */
interface PinAsk {
  device: OtherDevice;
  request: CastRequest;
  pairing?: boolean;
}

/**
 * "Play on…" beside a page's own Play: the user's other devices that play
 * what the page shows, and their household's, as the remote lists them.
 * Picking one plays it there, as that device's own Play would, says so, and
 * opens the remote on it. A device on another profile switches to the
 * user's first, after the user's PIN when their profile has one, which it
 * asks for in the menu's place. An AirPlay device the server hasn't paired
 * with shows a PIN, which it asks for the same way, once. There is nothing
 * to show while no device takes it.
 */
export function PlayOn({
  kind,
  video,
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
  const { t } = useTranslation(["components", "common"]);
  const targets = useCastTargets(kind, video);
  const label = given ?? t("playOn.label");
  const send = useCast();
  const pair = usePairDevice();
  const toast = useToast();
  const me = useAuth();
  const [menuOpen, setMenuOpen] = useState(false);
  // The device a cast is starting on: a Cast device takes seconds to answer.
  const [starting, setStarting] = useState<string | null>(null);
  const [asking, setAsking] = useState<PinAsk | null>(null);
  const [pinMessage, setPinMessage] = useState<string | null>(null);
  const [pinLocked, setPinLocked] = useState(false);
  // Every PIN not taken starts the pad over (clears the dots).
  const [failures, setFailures] = useState(0);
  const ref = useRef<HTMLDivElement>(null);
  // Whether Play on… is still on the page: a cast outlives it, but its PIN
  // can't be asked for once it's gone.
  const mounted = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const askRef = useRef<HTMLDivElement | null>(null);
  // The PIN opens on the side with room for it, as the menu does, measured before it shows.
  const [askSide, setAskSide] = useState(placement);
  const placeAsk = useCallback(
    (box: HTMLDivElement | null) => {
      askRef.current = box;
      const trigger = ref.current?.getBoundingClientRect();
      if (box && trigger) setAskSide(fitBeside(box, trigger, placement));
    },
    [placement],
  );
  const titleId = useId();
  const whyId = useId();

  // The menu, then the PIN asked for in its place, are one to the page: its controls stay while either shows.
  const shown = menuOpen || asking !== null;
  const heard = useEffectEvent((now: boolean) => onOpenChange?.(now));
  useEffect(() => {
    if (!shown) return;
    heard(true);
    return () => heard(false);
  }, [shown]);

  const stopAsking = (refocus: boolean) => {
    setAsking(null);
    setPinMessage(null);
    setPinLocked(false);
    setFailures(0);
    // Back on the button that opened the menu.
    if (refocus) ref.current?.querySelector<HTMLElement>("button")?.focus();
  };
  useDismiss(ref, asking !== null, () => stopAsking(false));
  // Asked, focus goes to the question, for a screen reader to say what it is.
  useEffect(() => {
    if (asking) askRef.current?.focus();
  }, [asking]);

  const ask = (device: OtherDevice, request: CastRequest, pairing = false) => {
    setPinMessage(null);
    setPinLocked(false);
    setAsking({ device, request, pairing });
  };

  /** Has `device` show a PIN, and asks for it: the button stays busy meanwhile. */
  const startPairing = (device: OtherDevice, request: CastRequest) => {
    if (!device.receiver) return;
    setStarting(device.name);
    pair
      .mutateAsync({ receiverId: device.receiver.receiver_id })
      .then(
        () => ask(device, request, true),
        (err: unknown) => toast.error(pairingError(err, device.name)),
      )
      .finally(() => {
        setStarting(null);
        pair.reset();
      });
  };

  /** Pairs the server with `device` with the `pin` it shows, then sends it `request`. */
  const pairWith = (device: OtherDevice, request: CastRequest, pin: string) => {
    if (!device.receiver) return;
    setPinMessage(null);
    pair
      .mutateAsync({ receiverId: device.receiver.receiver_id, pin })
      .then(
        () => {
          stopAsking(false);
          sendTo(device, request);
        },
        (err: unknown) => {
          // A wrong PIN, or tries to wait for: the pad says so, to try again.
          if ((isApiError(err, 403) && err.code === "wrong_pin") || isApiError(err, 429)) {
            setPinMessage(pairingError(err, device.name));
            setFailures((n) => n + 1);
            return;
          }
          stopAsking(true);
          toast.error(pairingError(err, device.name));
        },
      )
      .finally(() => pair.reset());
  };

  /** Sends `request` to `device`: to one on another profile, with that profile and the user's `pin` once given. */
  const sendTo = (device: OtherDevice, request: CastRequest, pin?: string) => {
    if (!device.receiver) return;
    // Another profile's device goes as the one listed, or not at all.
    const body = device.profile
      ? { ...request, profile_id: device.profile.id, device_id: device.receiver.device_id, ...(pin === undefined ? {} : { pin }) }
      : request;
    if (pin !== undefined) setPinMessage(null);
    setStarting(device.name);
    // Busy on through the pairing it may need.
    let pairing = false;
    // Its promise, not the mutation's callbacks, which the page dropping
    // the button (leaving it, or a player closing) would lose.
    send
      .mutateAsync({ receiverId: device.receiver.receiver_id, cast: body })
      .then(
        () => {
          if (pin !== undefined) stopAsking(false);
          toast.success(t("playOn.playingOn", { device: device.name }));
          openRemoteOn(device.id);
          onCast?.();
        },
        (err: unknown) => {
          // An AirPlay device the server hasn't paired with: its PIN first.
          if (pin === undefined && isApiError(err, 409) && err.code === "pairing_needed" && mounted.current) {
            pairing = true;
            startPairing(device, request);
            return;
          }
          // The user's profile has a PIN since it was loaded: the pad.
          if (pin === undefined && isApiError(err, 403) && err.code === "pin_needed") {
            ask(device, request);
            return;
          }
          // The PIN's own refusals (wrong, locked, too many tries): the pad says so, to try again.
          if (pin !== undefined && refusesPin(err)) {
            setPinMessage(pinError(err));
            setFailures((n) => n + 1);
            if (isApiError(err) && err.code === "pin_locked") setPinLocked(true);
            return;
          }
          // Anything else (not allowed, the device gone, no answer) isn't the PIN's: said as the menu says it.
          if (pin !== undefined) stopAsking(true);
          toast.error(castError(err, device.name));
        },
      )
      .finally(() => {
        if (!pairing) setStarting(null);
        // The cast is done with: its PIN goes from the mutation's cache (`useCast`) with it.
        send.reset();
      });
  };

  const pick = (device: OtherDevice) => {
    const request = cast();
    if (device.profile && me.has_pin) ask(device, request);
    else sendTo(device, request);
  };

  if (!targets.length && !asking) return null;

  const title = !asking
    ? ""
    : asking.pairing
      ? t("playOn.pairWith", { device: asking.device.name })
      : `${label.replace(/…$/, "")} ${asking.device.name}`;
  // One cast at a time: the button says where it is starting meanwhile.
  const busy = starting !== null && !asking;
  const shownLabel = busy ? t("playOn.starting", { device: starting }) : label;
  return (
    <div ref={ref} className="relative">
      <Menu
        label={label}
        align={align}
        placement={placement}
        onOpenChange={(open) => {
          setMenuOpen(open);
          // Opened again, the menu takes the PIN's place.
          if (open && asking) stopAsking(false);
        }}
        className="w-72"
        trigger={({ open, toggle }) =>
          trigger ? (
            trigger({ open, toggle: busy ? () => {} : toggle, label: shownLabel })
          ) : (
            <Button
              variant="secondary"
              size={size}
              icon={icon}
              loading={busy}
              onClick={toggle}
              aria-label={shownLabel}
              aria-haspopup="menu"
              aria-expanded={open}
              title={shownLabel}
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
                  pick(d);
                }}
              />
            ))}
          </>
        )}
      </Menu>
      {asking ? (
        <div
          ref={placeAsk}
          role="dialog"
          aria-labelledby={titleId}
          aria-describedby={whyId}
          tabIndex={-1}
          className={cn(
            "absolute z-40 max-h-[min(36rem,var(--room,calc(100dvh-6rem)))] w-72 animate-pop-in overflow-y-auto overscroll-contain rounded-lg border border-border bg-surface-2 p-4 shadow-xl shadow-black/50 outline-none",
            askSide === "below" ? "top-full mt-2" : "bottom-full mb-2",
            align === "right" ? "right-0" : "left-0",
          )}
        >
          <h2 id={titleId} className="text-sm font-semibold text-fg">
            {title}
          </h2>
          <p id={whyId} className="mb-4 mt-1 text-xs text-muted">
            {asking.pairing
              ? t("playOn.pairWhy", { device: asking.device.name })
              : t("playOn.otherProfile", { device: asking.device.name, profile: asking.device.profile?.name })}
          </p>
          <PinPad
            key={failures}
            small
            busy={send.isPending || pair.isPending}
            locked={pinLocked}
            error={pinMessage}
            prompt={asking.pairing ? t("playOn.enterDevicePin", { device: asking.device.name }) : t("playOn.enterPin")}
            submitLabel={title}
            cancelLabel={t("common:actions.cancel")}
            cancelIcon={null}
            onSubmit={(pin) =>
              asking.pairing ? pairWith(asking.device, asking.request, pin) : sendTo(asking.device, asking.request, pin)
            }
            onCancel={() => stopAsking(true)}
          />
        </div>
      ) : null}
    </div>
  );
}

/** Whether the server refused the PIN a cast was sent with, which the pad says, rather than the cast. */
function refusesPin(err: unknown): boolean {
  if (isApiError(err, 429)) return true;
  return isApiError(err, 403) && (err.code === "pin_needed" || err.code === "wrong_pin" || err.code === "pin_locked");
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
