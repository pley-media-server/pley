import type { ComponentChildren } from "preact";
import { useLayoutEffect, useRef, useState } from "preact/hooks";
import { useUi } from "../context";
import { nudge, type Dir } from "../focus";
import { FocusWatch, G } from "./focusable";
import { GUTTER } from "./row";
import { TILES, type TileKind } from "./tile";

/** Between a caption and the next row's wells (the grid's own gap, tighter than a page's rows). */
export const GRID_ROW_GAP = 20;
const CAPTION_BLOCK = 14 + 34;
/** The grid's right edge leaves room for the A–Z jump: 960 − 48 − 40 − 14. */
export const GRID_END = 858;

/** How many columns of `kind` fit between x 96 and the grid's end (7 posters, 5 squares). */
export function gridColumns(kind: TileKind): number {
  const { w } = TILES[kind];
  return Math.max(1, Math.floor((GRID_END - GUTTER + 14) / (w + 14)));
}

export function gridPitch(kind: TileKind): number {
  return TILES[kind].h + CAPTION_BLOCK + GRID_ROW_GAP;
}

export interface GridProps {
  kind: TileKind;
  /** Cells: one per position, loaded or not. */
  count: number;
  columns?: number;
  /** The y the focused row's wells sit at. */
  top: number;
  /** The y the grid's content may draw from (above it, the page's header). */
  clipTop: number;
  /** One cell; `place.onFocus` must reach its focusable element. */
  cell: (index: number, place: { onFocus: (el: HTMLElement) => void; priority: number }) => ComponentChildren;
  /** The position to focus when the grid is entered and has nothing to come back to. */
  enterAt?: number;
  onFocusIndex?: (index: number) => void;
  /** Right from the last column (the A–Z jump). */
  onRightEdge?: (index: number) => boolean;
  /** Focus moved to this position by the page (a letter jump): the grid scrolls there. */
  focusTo?: { index: number; n: number } | null;
  /** Rows the grid starts scrolled to (Back to the page). */
  startIndex?: number;
  bottomPad?: number;
  /** A hero spanning the full first row, `heroHeight` tall (a collection's page). */
  hero?: ComponentChildren;
  heroHeight?: number;
}

/**
 * A grid of tiles (TvPosterGrid): rows of 7 posters or 5 squares from x 96;
 * the focused row's wells stay at the grid's top line and the rows slide
 * under it. Only the rows near the screen are drawn, so a library of
 * thousands moves as lightly as one of ten. What the pointer focuses
 * leaves the grid where it is, but for a row cut off at an edge (`nudge`).
 */
export function PosterGrid(p: GridProps) {
  const ui = useUi();
  const spec = TILES[p.kind];
  const cols = p.columns ?? gridColumns(p.kind);
  const pitch = gridPitch(p.kind);
  const rows = Math.ceil(p.count / cols);
  const heroH = p.hero ? p.heroHeight ?? 0 : 0;
  const [focused, setFocused] = useState(p.startIndex ?? p.enterAt ?? 0);
  const [inHero, setInHero] = useState(!!p.hero && p.startIndex === undefined);
  /** Where the pointer left the grid, until a key moves focus in it. */
  const [held, setHeld] = useState<number | null>(null);
  const row = Math.floor(Math.min(focused, Math.max(0, p.count - 1)) / cols);
  // Rows' wells in the content, from its top; the content slides by `t`.
  const rowY = (r: number) => (heroH ? heroH + GRID_ROW_GAP : 0) + r * pitch;
  const contentH = rows ? rowY(rows - 1) + spec.h + CAPTION_BLOCK + (p.bottomPad ?? 41) : heroH;
  const restT = heroH ? 0 : p.top - p.clipTop;
  const minT = Math.min(restT, 540 - p.clipTop - contentH);
  const t = held ?? (inHero ? 0 : Math.max(minT, Math.min(restT, p.top - p.clipTop - rowY(row))));
  const shown = useRef(t);
  shown.current = t;
  const visible = Math.ceil((540 - p.clipTop) / pitch) + 1;
  const firstRow = Math.max(0, Math.floor((-t - (heroH ? heroH + GRID_ROW_GAP : 0)) / pitch) - 1);
  const lastRow = Math.min(rows, firstRow + visible + 2);
  const group = useRef<HTMLElement | null>(null);
  const focusedEl = useRef<HTMLElement | null>(null);

  const place = (i: number) => ({
    onFocus: (el: HTMLElement) => {
      focusedEl.current = el;
      setFocused(i);
      setInHero(false);
      if (ui.focus.byPointer) {
        // The content shows from -t to -t + (540 − clipTop): the cell's row comes in whole, if it is cut off.
        const y = rowY(Math.floor(i / cols));
        setHeld(-nudge(-shown.current, y, y + spec.h + CAPTION_BLOCK, 0, 540 - p.clipTop, -restT, -minT));
      } else setHeld(null);
      p.onFocusIndex?.(i);
    },
    priority: Math.abs(Math.floor(i / cols) - row),
  });

  const cells: ComponentChildren[] = [];
  for (let r = firstRow; r < lastRow; r++) {
    for (let c = 0; c < cols; c++) {
      const i = r * cols + c;
      if (i >= p.count) break;
      cells.push(
        // The cells' box comes after the hero in the content, so a slot's top is counted from under it.
        <div key={i} class="grid-slot" data-index={i} style={`left:${GUTTER + c * (spec.w + 14)}px;top:${rowY(r) - heroH}px`}>
          {p.cell(i, place(i))}
        </div>,
      );
    }
  }

  // A cell that changed under focus (its page arrived, its item went): the one now at that position.
  useLayoutEffect(() => {
    const was = focusedEl.current;
    if (!was || was.isConnected || ui.focus.current !== was || !group.current || !p.count) return;
    const at = Math.min(focused, p.count - 1);
    const el = group.current.querySelector<HTMLElement>(`[data-index="${at}"] [data-f]`);
    if (el) ui.focus.focus(el, ui.focus.byPointer);
  });

  // A jump: the target's row comes up, then the target takes focus.
  useLayoutEffect(() => {
    if (!p.focusTo) return;
    setFocused(p.focusTo.index);
    setInHero(false);
    setHeld(null);
  }, [p.focusTo?.n]);
  useLayoutEffect(() => {
    if (!p.focusTo || !group.current) return;
    const el = group.current.querySelector<HTMLElement>(`[data-index="${p.focusTo.index}"] [data-f]`);
    if (el) ui.focus.focus(el);
  }, [p.focusTo?.n, focused]);

  const onEdge = (dir: Dir, from: HTMLElement) => {
    if (dir === "right" && p.onRightEdge) {
      const slot = from.closest("[data-index]");
      const i = slot ? Number(slot.getAttribute("data-index")) : focused;
      if (i % cols === cols - 1 || i === p.count - 1) return p.onRightEdge(i);
    }
    return dir === "right";
  };

  return (
    <div class="grid-viewport" style={`top:${p.clipTop}px`}>
      <div class="grid-content" style={`transform:translateY(${t}px)`}>
        {p.hero && (
          <div class="grid-hero" style={`height:${heroH}px`}>
            <FocusWatch
              onFocus={() => {
                setInHero(true);
                if (!ui.focus.byPointer) setHeld(null);
              }}
            >
              {p.hero}
            </FocusWatch>
          </div>
        )}
        <G
          kind="grid"
          columns={cols}
          restore
          class="grid-cells"
          onEdge={onEdge}
          onEnter={(_dir, from) => {
            const g = group.current;
            if (!g || ui.focus.rememberedIn(g)) return null;
            if (p.enterAt !== undefined) return g.querySelector<HTMLElement>(`[data-index="${p.enterAt}"] [data-f]`);
            return from ? null : g.querySelector<HTMLElement>("[data-f]");
          }}
          elRef={(el) => (group.current = el)}
        >
          {cells}
        </G>
      </div>
    </div>
  );
}
