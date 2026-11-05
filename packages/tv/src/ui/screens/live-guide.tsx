import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "preact/hooks";
import type { LiveChannel, LiveProgram } from "../../api/types";
import { t } from "../../i18n";
import { airing, cellAt, channelName, formatGuideDay, formatSpan, formatTime, GUIDE_SPAN, guideCells, programProgress, SLOT, type GuideCell, type LiveCategory } from "../../live/data";
import { GuideStore } from "../../live/guide";
import type { ChannelPages } from "../../live/pages";
import { useUi } from "../context";
import type { Dir } from "../focus";
import { F, G } from "../kit/focusable";
import { Icon } from "../kit/icons";
import { memo } from "../kit/memo";
import type { Entry } from "../shell/navigator";
import { ChannelLogo, ProgramMarks } from "./live-kit";
import type { Jump } from "./livetv";

/** The guide's lines, in canvas px: the times' row, then a row a channel. */
const HEAD_TOP = 132;
const ROWS_TOP = 164;
/** A row is 52 high, 6 apart. */
const PITCH = 58;
/** The channels' column, and the programmes' box right of it (to 912). */
const CHANNEL_W = 184;
const PROGRAMS_LEFT = 96 + CHANNEL_W + 8;
const PROGRAMS_W = 912 - PROGRAMS_LEFT;
/** Four px a minute: two and a half hours show. */
const PX_PER_SEC = 4 / 60;
const VIEW_SPAN = PROGRAMS_W / PX_PER_SEC;
/** Programmes drawn either side of what shows, so the remote moves into them without waiting. */
const DRAW_BEFORE = 2 * 3600;
const DRAW_AFTER = 4 * 3600;
/** The focused row sits this many rows from the top once the guide scrolls. */
const ROWS_ABOVE = 2;
/** Rows drawn above and below what shows. */
const ROWS_DRAWN_ABOVE = 3;
const ROWS_DRAWN_BELOW = 10;
/** How long the time-now pill and a half hour's time are, in seconds of the guide (about 64 and 72 px). */
const NOW_LABEL_SPAN = 16 * 60;
const SLOT_LABEL_SPAN = 18 * 60;
/** The rewind and fast-forward keys, and Right past what's drawn, move the guide this far. */
const JUMP = 4 * SLOT;

/** The half hour `t` is in. */
function slotOf(t: number): number {
  return Math.floor(t / SLOT) * SLOT;
}

/**
 * The guide (the web's GuideGrid on a TV): channels down and time across,
 * now marked, two and a half hours in view. Up and Down keep the time
 * focus is at; Left and Right go programme by programme, the guide moving
 * through time as they leave what shows; rewind and fast-forward move it
 * two hours; Back goes back to now first; Left from the channels opens
 * the categories. Only the rows near the screen are drawn: their page of
 * channels is asked for as they come, and the guide in six-hour chunks
 * for those channels alone.
 */
export function LiveGuide(p: {
  entry: Entry;
  pages: ChannelPages;
  category: LiveCategory;
  now: number;
  /** The minute: the pages in view are asked for again as it changes. */
  minute: number;
  jump: Jump | null;
  hours24: boolean;
  onWatch: (c: LiveChannel) => void;
  onDetails: (c: LiveChannel, program: LiveProgram | null) => void;
  onCategories: () => void;
  focusAbove: () => boolean;
  /** Bumped when the guide changed on the server. */
  version: number;
}) {
  const ui = useUi();
  const entry = p.entry;
  const pages = p.pages;
  const total = pages.total.value ?? 0;
  pages.version.value;
  // Where the guide opens: the half hour before the one now is in, so what's on shows whole.
  const origin = useMemo(() => slotOf(p.now) - SLOT, []);
  const maxFrom = origin + GUIDE_SPAN - slotOf(VIEW_SPAN);
  const nowSlot = () => Math.max(origin, slotOf(Date.now() / 1000));
  const store = useMemo(() => new GuideStore(ui.api, origin), []);
  const saved = entry.state.guideFrom as number | undefined;
  const [viewFrom, setViewFromState] = useState(() => Math.max(origin, Math.min(maxFrom, saved ?? nowSlot())));
  const [row, setRowState] = useState(() => Math.max(0, Math.min(Math.max(0, total - 1), (entry.state.guideRow as number | undefined) ?? 0)));
  const viewRef = useRef(viewFrom);
  viewRef.current = viewFrom;
  /** The time focus is at: Up and Down keep it. */
  const cursor = useRef<number>((entry.state.guideCursor as number | undefined) ?? Math.max(viewFrom, p.now));
  /** Focus is on the channels' column: Up and Down stay in it. */
  const inChannels = useRef(false);
  /** After the guide moves, focus goes to the focused row's cell at this time (or after it, `after`). */
  const pending = useRef<{ at: number; after: boolean } | null>(null);
  const setViewFrom = (t: number) => {
    const v = Math.max(origin, Math.min(maxFrom, t));
    entry.state.guideFrom = v;
    setViewFromState(v);
  };
  const setRow = (i: number) => {
    entry.state.guideRow = i;
    setRowState(i);
  };

  const drawFrom = Math.max(origin, viewFrom - DRAW_BEFORE);
  const drawTo = Math.min(origin + GUIDE_SPAN, viewFrom + VIEW_SPAN + DRAW_AFTER);
  // Read for its changes: rows draw again as chunks come.
  store.version.value;

  const maxShift = Math.max(0, total * PITCH - (540 - ROWS_TOP - 12));
  const shift = Math.max(0, Math.min(maxShift, (row - ROWS_ABOVE) * PITCH));
  const firstRow = Math.max(0, Math.floor(shift / PITCH) - ROWS_DRAWN_ABOVE);
  const lastRow = Math.min(total, Math.ceil(shift / PITCH) + ROWS_DRAWN_BELOW);
  // The rows' channels, as their pages come; the guide is asked for those alone.
  useEffect(() => pages.want(firstRow, lastRow), [firstRow, lastRow, pages, p.minute]);
  const ids: number[] = [];
  for (let i = firstRow; i < lastRow; i++) {
    const c = pages.at(i);
    if (c) ids.push(c.id);
  }
  const idsKey = ids.join(",");
  useEffect(() => store.want(drawFrom, drawTo, ids), [drawFrom, drawTo, idsKey]);
  const firstVersion = useRef(p.version);
  useEffect(() => {
    if (p.version === firstVersion.current) return;
    store.forget();
    store.want(drawFrom, drawTo, ids);
  }, [p.version]);

  const rowEl = (i: number) => document.querySelector<HTMLElement>(`.live-guide [data-row="${i}"]`)?.parentElement ?? null;
  /** The cell of a row at time `t` (`after`: the first that starts at it or later). */
  const cellIn = (el: HTMLElement | null, t: number, after = false): HTMLElement | null => {
    if (!el) return null;
    const marks = el.querySelectorAll<HTMLElement>("[data-start]");
    let last: HTMLElement | null = null;
    for (let i = 0; i < marks.length; i++) {
      const m = marks[i]!;
      const start = Number(m.getAttribute("data-start"));
      const stop = Number(m.getAttribute("data-stop"));
      if (after ? start >= t : stop > t) return m.parentElement;
      last = m.parentElement;
    }
    return last;
  };

  // A cell focused: the time it is at is kept, and the guide moves through time when it is off the edge of what shows.
  const onCell = (i: number, cell: GuideCell) => {
    inChannels.current = false;
    const from = viewRef.current;
    const to = from + VIEW_SPAN;
    if (i !== row) setRow(i);
    // Less than a quarter of an hour of it shows (or less than all of a shorter one): the guide moves to it.
    const shows = Math.min(cell.stop, to) - Math.max(cell.start, from);
    const hidden = shows < Math.min(SLOT / 2, cell.stop - cell.start);
    if (ui.focus.byPointer) {
      cursor.current = Math.max(cell.start, from);
    } else if (hidden && cell.start < from) {
      setViewFrom(slotOf(cell.start));
      cursor.current = Math.max(cell.start, slotOf(cell.start));
    } else if (hidden) {
      setViewFrom(slotOf(cell.start) - SLOT);
      cursor.current = cell.start;
    } else {
      cursor.current = Math.max(cell.start, from);
    }
    entry.state.guideCursor = cursor.current;
  };
  const onChannel = (i: number) => {
    inChannels.current = true;
    if (i !== row) setRow(i);
  };
  // Into a row from above or below: the same column, at the same time.
  const onEnterRow = (el: HTMLElement | null, dir: Dir | null): HTMLElement | null => {
    if (!el || (dir !== "up" && dir !== "down")) return null;
    if (inChannels.current) return el.querySelector<HTMLElement>(".guide-channel");
    return cellIn(el, cursor.current);
  };
  // Right past the programmes drawn: the guide moves on, and focus to the next one. Left of the channels: the categories.
  const onRowEdge = (dir: Dir, from: HTMLElement): boolean => {
    if (dir === "left" && from.classList.contains("guide-channel")) {
      p.onCategories();
      return true;
    }
    if (dir !== "right") return false;
    const stop = Number(from.querySelector("[data-stop]")?.getAttribute("data-stop"));
    if (viewRef.current >= maxFrom) return true;
    pending.current = { at: isFinite(stop) ? stop : viewRef.current + VIEW_SPAN, after: true };
    setViewFrom(viewRef.current + JUMP);
    return true;
  };
  const moveTime = (by: number) => {
    const v = Math.max(origin, Math.min(maxFrom, viewRef.current + by));
    if (v === viewRef.current) return;
    cursor.current = Math.max(v, Math.min(v + VIEW_SPAN - 1, cursor.current + by));
    pending.current = { at: cursor.current, after: false };
    inChannels.current = false;
    setViewFrom(v);
  };
  // After the guide moved: focus to the cell asked for, or back onto the row when its cell was drawn anew.
  useLayoutEffect(() => {
    const want = pending.current;
    const current = ui.focus.current;
    const lost = !!current && !current.isConnected && current.classList.contains("guide-cell");
    if (!want && !lost) return;
    pending.current = null;
    const el = cellIn(rowEl(row), want ? want.at : cursor.current, want?.after);
    if (el) ui.focus.focus(el);
  });

  // A channel brought into view (its number typed, or back from the player): its row, at the time focus is at.
  useEffect(() => {
    if (!p.jump) return;
    setRow(Math.max(0, Math.min(total - 1, p.jump.index)));
    pending.current = { at: Math.max(cursor.current, viewRef.current), after: false };
    inChannels.current = false;
  }, [p.jump?.n]);

  // The remote's keys over the guide: two hours either way, a screen of channels, and Back to now.
  useEffect(
    () =>
      ui.input.push((key, e) => {
        const at = ui.focus.current;
        const guide = document.querySelector(".live-guide");
        if (!at || !guide || !guide.contains(at)) return false;
        const down = e.type === "down";
        switch (key) {
          case "fastForward":
          case "next":
            if (down) moveTime(JUMP);
            return true;
          case "rewind":
          case "previous":
            if (down) moveTime(-JUMP);
            return true;
          case "channelUp":
          case "channelDown": {
            if (!down) return true;
            const by = key === "channelDown" ? 6 : -6;
            const to = Math.max(0, Math.min(rowRef.current >= 0 ? totalRef.current - 1 : 0, rowRef.current + by));
            if (to !== rowRef.current) {
              setRow(to);
              pending.current = { at: cursor.current, after: false };
            }
            return true;
          }
          case "back": {
            const n = nowSlot();
            if (viewRef.current === n || !down || e.repeat) return viewRef.current !== n;
            cursor.current = Date.now() / 1000;
            pending.current = { at: cursor.current, after: false };
            inChannels.current = false;
            setViewFrom(n);
            return true;
          }
          default:
            return false;
        }
      }),
    [],
  );
  const rowRef = useRef(row);
  rowRef.current = row;
  const totalRef = useRef(total);
  totalRef.current = total;

  const offset = (viewFrom - origin) * PX_PER_SEC;
  const nowX = (p.now - viewFrom) * PX_PER_SEC;
  const slots: number[] = [];
  for (let t = Math.max(origin, slotOf(drawFrom)); t < drawTo; t += SLOT) slots.push(t);
  const nowMinute = Math.floor(p.now / 60) * 60;
  const rows = [];
  for (let i = firstRow; i < lastRow; i++) {
    const c = pages.at(i) ?? null;
    rows.push(
      <GuideRow
        key={i}
        channel={c}
        name={c ? channelName(c, p.category) : ""}
        index={i}
        programs={c ? store.programs.get(c.id) ?? null : null}
        loading={!c || !store.answered(c.id, viewFrom)}
        drawFrom={drawFrom}
        drawTo={drawTo}
        origin={origin}
        viewFrom={viewFrom}
        now={nowMinute}
        hours24={p.hours24}
        initialAt={i === row ? cursor.current : null}
        onCell={onCell}
        onChannel={onChannel}
        onEnterRow={onEnterRow}
        onRowEdge={onRowEdge}
        onWatch={p.onWatch}
        onDetails={p.onDetails}
      />,
    );
  }
  return (
    <div class="live-guide">
      <div class="guide-head" style={`top:${HEAD_TOP}px`}>
        <div class="guide-day t-label c-fg one-line">{formatGuideDay(Math.max(viewFrom, Math.min(p.now, viewFrom + VIEW_SPAN)), p.now)}</div>
        <div class="guide-times">
          <div class="guide-track" style={`transform:translateX(${-offset}px)`}>
            {slots.map((t) => (
              <div key={t} class="guide-slot t-caption" style={`left:${(t - origin) * PX_PER_SEC}px`}>
                {/* The time now, in its pill, takes the place of a time under it. */}
                {p.now > t - NOW_LABEL_SPAN / 2 && p.now < t + SLOT_LABEL_SPAN + NOW_LABEL_SPAN / 2 ? null : new Date(t * 1000).getHours() === 0 &&
                  new Date(t * 1000).getMinutes() === 0 ? (
                  <span class="c-fg" style="font-weight:700">
                    {formatGuideDay(t, p.now)}
                  </span>
                ) : (
                  formatTime(t, p.hours24)
                )}
              </div>
            ))}
          </div>
        </div>
        {nowX >= 0 && nowX <= PROGRAMS_W && (
          <div class="guide-now-label t-badge" style={`left:${PROGRAMS_LEFT + nowX}px`}>
            {formatTime(p.now, p.hours24)}
          </div>
        )}
      </div>
      <div class="guide-body" style={`top:${ROWS_TOP}px`}>
        <G kind="col" class="guide-rows" style={`transform:translateY(${-shift}px);height:${total * PITCH}px`}>
          {rows}
        </G>
        {nowX >= 0 && nowX <= PROGRAMS_W && (
          <div class="guide-now-line" style={`left:${PROGRAMS_LEFT + nowX}px;height:${Math.max(0, Math.min(total * PITCH - 6 - shift, 540 - ROWS_TOP))}px`} />
        )}
      </div>
    </div>
  );
}

interface RowProps {
  /** `null` until its page comes: the row holds its place, and focus can rest on it. */
  channel: LiveChannel | null;
  /** Its name as the category shows it (without the region its group says). */
  name: string;
  index: number;
  programs: LiveProgram[] | null;
  loading: boolean;
  drawFrom: number;
  drawTo: number;
  origin: number;
  viewFrom: number;
  now: number;
  hours24: boolean;
  /** The focused row: its cell at this time may take the page's first focus. */
  initialAt: number | null;
  onCell: (index: number, cell: GuideCell) => void;
  onChannel: (index: number) => void;
  onEnterRow: (el: HTMLElement | null, dir: Dir | null) => HTMLElement | null;
  onRowEdge: (dir: Dir, from: HTMLElement) => boolean;
  onWatch: (c: LiveChannel) => void;
  onDetails: (c: LiveChannel, program: LiveProgram | null) => void;
}

/** One channel's row: its logo, number and name, then its programmes, drawn again only when they or the time shown change. */
const GuideRow = memo(function GuideRow(p: RowProps) {
  const c = p.channel;
  const group = useRef<HTMLElement | null>(null);
  const cells = useMemo(() => guideCells(p.programs ?? undefined, p.drawFrom, p.drawTo), [p.programs, p.drawFrom, p.drawTo]);
  const initial = p.initialAt !== null ? cellAt(cells, p.initialAt) : -1;
  const viewLeft = (p.viewFrom - p.origin) * PX_PER_SEC;
  return (
    <G
      kind="row"
      class="guide-row"
      style={`top:${p.index * PITCH}px`}
      elRef={(el) => (group.current = el)}
      onEnter={(dir) => p.onEnterRow(group.current, dir)}
      onEdge={(dir, from) => p.onRowEdge(dir, from)}
    >
      <span data-row={p.index} style="display:none" />
      {c ? (
        <F
          class="guide-channel"
          fid={`guide/${c.id}`}
          label={t("livetv.guide.watch", { name: `${c.number ? `${c.number} ` : ""}${p.name}` })}
          onFocus={() => p.onChannel(p.index)}
          onSelect={() => p.onWatch(c)}
          onLongSelect={() => p.onDetails(c, null)}
          onMenu={() => p.onDetails(c, null)}
        >
          <ChannelLogo channel={c} width={64} height={36} class="guide-logo" />
          <div class="guide-channel-text">
            <div class="guide-channel-number t-caption">
              {c.number ?? ""}
              {c.favorite && <Icon name="star" size={12} class="live-star" />}
            </div>
            <div class="t-label one-line">{p.name}</div>
          </div>
        </F>
      ) : (
        <F class="guide-channel loading" fid={`guide/at${p.index}`} label={t("common.states.loading")} onFocus={() => p.onChannel(p.index)}>
          <div class="skeleton guide-logo" style="width:64px;height:36px" />
        </F>
      )}
      <div class="guide-cells">
        <div class="guide-track" style={`transform:translateX(${-viewLeft}px)`}>
          {cells.map((cell, i) => {
            const left = (cell.start - p.origin) * PX_PER_SEC;
            const width = Math.max(2, (Math.min(cell.stop, p.drawTo) - cell.start) * PX_PER_SEC);
            const prog = cell.program;
            // `now` is the minute: its middle decides what is on.
            const on = prog ? airing(prog, p.now + 30) : false;
            const past = cell.stop <= p.now;
            // A cell begun before the left edge keeps its words in view.
            const indent = Math.max(0, Math.min(viewLeft - left, width - 44));
            const noGuide = !!c && !c.has_guide;
            const label = prog
              ? t(on ? "livetv.guide.cellOnNow" : "livetv.guide.cell", { title: prog.title, time: formatSpan(prog, p.hours24) })
              : !c
                ? t("common.states.loading")
                : noGuide
                  ? t("livetv.guide.noGuideFor", { name: p.name })
                  : t("livetv.guide.noInformationThen");
            return (
              <F
                key={c ? cell.key : `at/${cell.key}`}
                class={`guide-cell${on ? " on" : ""}${past ? " past" : ""}${prog ? "" : " empty"}`}
                fid={c ? `guide/${c.id}/${cell.key}` : `guide/at${p.index}/${cell.key}`}
                initial={i === initial}
                label={label}
                style={`left:${left + 1}px;width:${width - 2}px`}
                onFocus={() => p.onCell(p.index, cell)}
                onSelect={() => c && (prog && !on ? p.onDetails(c, prog) : p.onWatch(c))}
                onLongSelect={() => c && p.onDetails(c, prog)}
                onMenu={() => c && p.onDetails(c, prog)}
              >
                <span data-start={cell.start} data-stop={cell.stop} style="display:none" />
                {width > 30 && (
                  <div class="guide-cell-text" style={indent ? `padding-left:${10 + indent}px` : undefined}>
                    {prog ? (
                      <>
                        <div class="guide-cell-title">
                          <ProgramMarks program={prog} />
                          <span class="t-label one-line">{prog.title}</span>
                        </div>
                        <div class="t-caption one-line guide-cell-sub">{formatSpan(prog, p.hours24)}</div>
                      </>
                    ) : noGuide ? (
                      // A channel without a guide shows its name across it, as the web's does.
                      <>
                        <div class="t-label one-line">{p.name}</div>
                        <div class="t-caption one-line guide-cell-sub">{t("livetv.noGuideHere")}</div>
                      </>
                    ) : (
                      <div class="t-caption one-line guide-cell-sub">{p.loading ? "" : t("livetv.guide.noInformation")}</div>
                    )}
                  </div>
                )}
                {on && prog && <div class="guide-cell-progress" style={`width:${programProgress(prog, p.now) * 100}%`} />}
              </F>
            );
          })}
        </div>
      </div>
    </G>
  );
});