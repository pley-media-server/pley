import type { ComponentChildren } from "preact";
import { useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import { t } from "../../i18n";
import { useUi } from "../context";
import { nudge, topIn } from "../focus";
import { G } from "./focusable";
import { TextButton } from "./buttons";

/**
 * Keeps the focused element of a tall column in view, as the Android
 * app's root scrolling does: its top at 30 % of the box, the column's
 * ends never passed. Moves only by transform. What the pointer focuses
 * leaves the column where it is, but for an element cut off (`nudge`).
 */
export function useScrollToFocus(inner: { current: HTMLElement | null }, height: number) {
  const ui = useUi();
  const [shift, setShift] = useState(0);
  useEffect(
    () =>
      ui.focus.onChange((el) => {
        const b = inner.current;
        if (!el || !b || !b.contains(el)) return;
        const top = topIn(el, b);
        const max = Math.max(0, b.offsetHeight - height);
        if (ui.focus.byPointer) setShift((s) => nudge(s, top, top + el.offsetHeight, 0, height, 0, max));
        else setShift(Math.max(0, Math.min(top - height * 0.3, max)));
      }),
    [height],
  );
  return shift;
}

/**
 * A focus trap for an overlay, set up from its root's ref: refs are set
 * before any layout effect runs, so the overlay's own rows can claim
 * focus inside it as it opens. Released (focus back to the opener) when
 * the overlay goes.
 */
export function useTrap(): (el: HTMLElement | null) => void {
  const ui = useUi();
  const root = useRef<HTMLElement | null>(null);
  useLayoutEffect(
    () => () => {
      if (root.current) ui.focus.release(root.current);
      root.current = null;
    },
    [],
  );
  return (el) => {
    if (el && root.current !== el) {
      root.current = el;
      ui.focus.trap(el);
    }
  };
}

/**
 * A side panel (TvSidePanel): 360 wide at the right, sliding in over a
 * scrim, focus trapped inside; Back, or the pointer's click outside it,
 * closes it and focus goes back to what opened it.
 */
export function SidePanel(p: { onClose: () => void; children: ComponentChildren; wide?: boolean; label?: string }) {
  const ui = useUi();
  const trap = useTrap();
  const inner = useRef<HTMLDivElement>(null);
  const [shown, setShown] = useState(false);
  useLayoutEffect(() => {
    const off = ui.input.push((key, e) => {
      if (key === "back") {
        if (e.type === "down") p.onClose();
        return true;
      }
      return false;
    });
    const t = setTimeout(() => setShown(true), 16);
    return () => {
      clearTimeout(t);
      off();
    };
  }, []);
  const shift = useScrollToFocus(inner, 540 - 54);
  return (
    <div ref={trap} class="panel-root" style="position:absolute;left:0;top:0;right:0;bottom:0;z-index:30" aria-label={p.label}>
      <div class={`scrim${shown ? " in" : ""}`} onClick={p.onClose} />
      <div class={`side-panel${p.wide ? " wide" : ""}${shown ? " in" : ""}`}>
        <div ref={inner} class="panel-scroll" style={`transform:translateY(${-shift}px)`}>
          <G kind="col">{p.children}</G>
        </div>
      </div>
    </div>
  );
}

export function PanelTitle(p: { children: ComponentChildren }) {
  return <div class="panel-title t-title lines-2">{p.children}</div>;
}

export function PanelHeading(p: { children: ComponentChildren }) {
  return <div class="panel-heading t-title">{p.children}</div>;
}

/** A dialog of text with Close (TvTextDialog). */
export function TextDialog(p: { title: string; text: string; onClose: () => void }) {
  const ui = useUi();
  const trap = useTrap();
  useLayoutEffect(
    () =>
      ui.input.push((key, e) => {
        if (key === "back") {
          if (e.type === "down") p.onClose();
          return true;
        }
        return false;
      }),
    [],
  );
  return (
    <div ref={trap} style="position:absolute;left:0;top:0;right:0;bottom:0;z-index:40">
      <div class="scrim in" onClick={p.onClose} />
      <div class="dialog">
        <div class="t-title">{p.title}</div>
        <div class="t-body-large" style="margin-top:16px;color:rgba(233,235,239,0.85);white-space:pre-line">
          {p.text}
        </div>
        <G kind="row" style="display:flex;justify-content:flex-end;margin-top:16px">
          <TextButton label={t("common.actions.close")} initial onSelect={p.onClose} />
        </G>
      </div>
    </div>
  );
}
