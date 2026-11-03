import { useCallback, useMemo, useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { castControl, castKeys, castPlay, castPosition, useCastTargets, type CastTarget } from "@/api/cast";
import { errorMessage } from "@/api/client";
import type { CastAction } from "@/api/types";
import { useToast } from "@/components/Toast";
import { useLocalStorage } from "@/lib/hooks";
import { CastPicker } from "./CastPicker";
import { CastRemote } from "./CastRemote";
import { CastContext, type CastApi, type CastPending, type CastRequest } from "./context";
import { castSelf } from "./device";
import { useCastCommands } from "./receiver";

/**
 * Casting for the signed-in profile (docs/API.md, "Casting to another
 * device"): from here, the "Play on" picker and a device's remote; to here,
 * where this device may be cast to (the desktop app), the commands sent to
 * it.
 */
export function CastProvider({ children }: { children: ReactNode }) {
  const qc = useQueryClient();
  const toast = useToast();
  const [self] = useState(castSelf);
  const targets = useCastTargets(self?.id ?? null);
  useCastCommands(self);

  const [request, setRequest] = useState<CastRequest | null>(null);
  const [sending, setSending] = useState<string | null>(null);
  const [remote, setRemote] = useState<{ id: string; name: string } | null>(null);
  const [pending, setPending] = useState<CastPending | null>(null);
  // Kept across reloads: the bar shows what still plays there.
  const [last, setLast] = useLocalStorage<string | null>("pley.castLast", null);

  const showRemote = useCallback(
    (deviceId: string) => {
      const device = qc.getQueryData<CastTarget[]>(castKeys.devices)?.find((d) => d.id === deviceId);
      setRemote({ id: deviceId, name: device?.name ?? "" });
      setLast(deviceId);
    },
    [qc, setLast],
  );

  /** A device picked: one that plays something shows its remote; another is sent the title. */
  const pick = (device: CastTarget) => {
    if (!request) return;
    if (device.state) {
      setRequest(null);
      showRemote(device.id);
      return;
    }
    const position = request.position?.();
    const sentAt = Date.now();
    setSending(device.id);
    castPlay(device.id, {
      item_id: request.item.id,
      ...(request.fileId != null ? { file_id: request.fileId } : {}),
      ...(position !== undefined ? { position_ms: Math.max(0, Math.round(position)) } : {}),
    }).then(
      () => {
        setSending(null);
        setRequest(null);
        setPending({ deviceId: device.id, itemId: request.item.id, sentAt });
        setRemote({ id: device.id, name: device.name });
        setLast(device.id);
        request.onSent?.();
      },
      (err: unknown) => {
        setSending(null);
        toast.error(errorMessage(err));
      },
    );
  };

  const steer = useCallback(
    (deviceId: string, action: CastAction, positionMs?: number) => {
      castControl(deviceId, action, positionMs).then(
        () => {
          // The remote follows at once; the device's own word comes after.
          const at = Date.now();
          qc.setQueryData<CastTarget[]>(castKeys.devices, (old) =>
            old?.map((d) => {
              if (d.id !== deviceId || !d.state) return d;
              const now = castPosition(d.state, at);
              switch (action) {
                case "pause":
                  return { ...d, state: { ...d.state, state: "paused", position_ms: now, age_ms: 0, heard_at: at } };
                case "resume":
                  return { ...d, state: { ...d.state, state: "playing", position_ms: now, age_ms: 0, heard_at: at } };
                case "seek":
                  return { ...d, state: { ...d.state, position_ms: positionMs ?? now, age_ms: 0, heard_at: at } };
                case "stop":
                  return d;
              }
            }),
          );
        },
        (err: unknown) => toast.error(errorMessage(err)),
      );
    },
    [qc, toast],
  );

  const api = useMemo<CastApi>(
    () => ({ targets, choose: setRequest, showRemote, steer, last, open: !!request || !!remote }),
    [targets, showRemote, steer, last, request, remote],
  );

  return (
    <CastContext.Provider value={api}>
      {children}
      <CastPicker
        open={!!request}
        devices={targets}
        sending={sending}
        onPick={pick}
        onClose={() => {
          if (!sending) setRequest(null);
        }}
      />
      {remote ? (
        <CastRemote
          deviceId={remote.id}
          name={remote.name}
          pending={pending?.deviceId === remote.id ? pending : null}
          onAnswered={() => setPending(null)}
          onClose={() => {
            setRemote(null);
            setPending(null);
          }}
        />
      ) : null}
    </CastContext.Provider>
  );
}
