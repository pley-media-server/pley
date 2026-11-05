import { useEffect, useRef, useState } from "preact/hooks";
import type { LiveChannel, LiveProgram } from "../../api/types";
import { t } from "../../i18n";
import { nowSeconds } from "../../live/data";
import { useUi } from "../context";
import { placeholderGradient } from "../images";

// What Live TV's pages and its player share: the time, a typed channel number, logos, progress and marks.

/** Digits typed for a channel number wait this long for more. */
export const NUMBER_WAIT_MS = 1_400;

/** The time now in unix seconds, again every `everyMs`. */
export function useNow(everyMs = 15_000): number {
  const [now, setNow] = useState(nowSeconds);
  useEffect(() => {
    const t = setInterval(() => setNow(nowSeconds()), everyMs);
    return () => clearInterval(t);
  }, [everyMs]);
  return now;
}

/**
 * A channel number typed on the remote's digits: shown as it is typed,
 * handed to `onNumber` once no digit came for 1.4 s. Returns the digits
 * typed so far.
 */
export function useNumberEntry(onNumber: (digits: string) => void, enabled = true): string {
  const ui = useUi();
  const [digits, setDigits] = useState("");
  const typed = useRef("");
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const handler = useRef(onNumber);
  handler.current = onNumber;
  useEffect(() => {
    if (!enabled) return;
    const off = ui.input.push((key, e) => {
      if (key.indexOf("digit") !== 0) return false;
      if (e.type !== "down") return true;
      typed.current = (typed.current + key.slice(5)).slice(-5);
      setDigits(typed.current);
      if (timer.current !== null) clearTimeout(timer.current);
      timer.current = setTimeout(() => {
        timer.current = null;
        const d = typed.current;
        typed.current = "";
        setDigits("");
        if (d) handler.current(d);
      }, NUMBER_WAIT_MS);
      return true;
    });
    return () => {
      off();
      if (timer.current !== null) clearTimeout(timer.current);
      timer.current = null;
      typed.current = "";
    };
  }, [enabled]);
  return digits;
}

/** The digits being typed, or "No channel 123" for a moment once nothing has that number. */
export function NumberBox(p: { digits: string; missing: string | null }) {
  if (!p.digits && !p.missing) return null;
  return (
    <div class="live-digits" role="status" aria-live="polite">
      {p.digits ? <span class="live-digits-n">{p.digits}</span> : <span class="t-label c-muted">{t("livetv.noChannel", { number: p.missing ?? "" })}</span>}
    </div>
  );
}

/** A channel's logo on a dark well, or its number (or name) on a gradient when it has none. */
export function ChannelLogo(p: { channel: Pick<LiveChannel, "logo" | "name" | "number">; width: number; height: number; class?: string; showName?: boolean }) {
  const ui = useUi();
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(!p.channel.logo);
  useEffect(() => {
    const logo = p.channel.logo;
    setSrc(null);
    setFailed(!logo);
    if (!logo || !ui.images) return;
    const url = ui.images.url(logo, p.width * ui.scale);
    const ready = ui.images.ready(url);
    if (ready) {
      setSrc(ready);
      return;
    }
    return ui.images.load(url, (u) => (u ? setSrc(u) : setFailed(true)));
  }, [p.channel.logo]);
  const style = `width:${p.width}px;height:${p.height}px${failed ? `;background-image:${placeholderGradient(p.channel.name)}` : ""}`;
  return (
    <div class={`live-logo ${p.class ?? ""}`} style={style}>
      {src ? (
        <img src={src} alt="" />
      ) : failed ? (
        <span class={`live-logo-text ${p.showName ? "t-label lines-2" : "t-title"}`}>{p.showName ? p.channel.name : p.channel.number ?? p.channel.name.slice(0, 3)}</span>
      ) : null}
    </div>
  );
}

/** A programme's progress, as a thin line. */
export function ProgressLine(p: { fraction: number; class?: string }) {
  return (
    <div class={`live-progress ${p.class ?? ""}`}>
      <div style={`width:${Math.round(p.fraction * 1000) / 10}%`} />
    </div>
  );
}

/** The Live and New marks a guide gives a programme. */
export function ProgramMarks(p: { program: LiveProgram }) {
  return (
    <>
      {p.program.is_live && <span class="live-mark live t-badge">{t("livetv.marks.live")}</span>}
      {!p.program.is_live && p.program.is_new && <span class="live-mark new t-badge">{t("livetv.marks.new")}</span>}
    </>
  );
}
