import { useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import { t } from "../../i18n";
import type { AppState, RequestNotice } from "../../state";
import { useUi } from "../context";
import { Art } from "../kit/art";
import { TextButton } from "../kit/buttons";
import { G } from "../kit/focusable";
import { Icon } from "../kit/icons";

/** Untouched this long, a notice goes by itself. */
export const NOTICE_SHOWN_MS = 10_000;
/** A notice waits for the remote to rest this long, so it never takes a press meant for the page. */
export const NOTICE_IDLE_MS = 1_500;
/** OK this soon after a notice took focus was meant for what had it before: it does nothing. */
const OK_GUARD_MS = 600;

export interface NoticeApi {
  /** The pointer moved onto the page: an open notice closes, and true when one did. */
  pointedAway(el: HTMLElement): boolean;
}

/** What had focus when a notice took it: its `data-fid`, by which focus finds its place again. */
type Opener = { fid: string | null };

/**
 * Ready-to-watch news over the pages, one at a time (TVs pop up nothing
 * else): a card at the bottom right with [Open] and [Close]. It shows once
 * the remote has rested and nothing else holds focus (`free`): never over
 * a playing video, a panel, a dialog, the open rail or a field that types.
 * It gives focus back to where it was when it closes: by Close, Back, Up or
 * Down, the pointer moving onto the page, or after 10 s untouched.
 */
export function RequestNotices(p: { state: AppState; free: () => boolean; onOpen: (n: RequestNotice) => void; noticeRef: (api: NoticeApi) => void }) {
  const ui = useUi();
  const queue = p.state.notices.value;
  const [shown, setShown] = useState<RequestNotice | null>(null);
  const free = useRef(p.free);
  free.current = p.free;

  // The first in the queue, once the remote has rested and nothing else holds focus.
  const next = queue[0] ?? null;
  useEffect(() => {
    if (shown || !next) return;
    const timer = setInterval(() => {
      if (Date.now() - ui.input.lastKeyAt < NOTICE_IDLE_MS || !free.current()) return;
      clearInterval(timer);
      setShown(next);
    }, 250);
    return () => clearInterval(timer);
  }, [shown?.id ?? null, next?.id ?? null]);

  // Gone from the queue meanwhile (another profile): it goes.
  const gone = !!shown && !queue.some((n) => n.id === shown.id);
  useEffect(() => {
    if (gone) setShown(null);
  }, [gone]);

  // The state keeps the notice on screen in the queue, whatever newer news comes.
  const onScreen = shown && !gone ? shown.id : null;
  useLayoutEffect(() => {
    if (onScreen === null) return;
    p.state.showingNotice(onScreen);
    return () => p.state.showingNotice(null);
  }, [onScreen]);

  // Closed from the remote: once the card has gone, focus goes back to what had it, or to what took its
  // place (its row was drawn again), else to the page's first, so it is never lost.
  const back = useRef<Opener | null>(null);
  useLayoutEffect(() => {
    const to = back.current;
    if (!to || onScreen !== null) return;
    back.current = null;
    const at = ui.focus.current;
    if (at && at.isConnected) return;
    const root = ui.focus.root;
    const same = to.fid && root ? root.querySelector<HTMLElement>(`[data-fid="${to.fid}"]`) : null;
    if (!ui.focus.focus(same)) ui.focus.move("down");
  });

  if (!shown || gone) {
    p.noticeRef({ pointedAway: () => false });
    return null;
  }
  return (
    <NoticeCard
      key={shown.id}
      notice={shown}
      noticeRef={p.noticeRef}
      onDone={(open, opener) => {
        back.current = opener;
        p.state.dropNotice(shown.id);
        setShown(null);
        if (open) p.onOpen(shown);
      }}
    />
  );
}

function NoticeCard(p: { notice: RequestNotice; onDone: (open: boolean, opener: Opener | null) => void; noticeRef: (api: NoticeApi) => void }) {
  const ui = useUi();
  const card = useRef<HTMLDivElement>(null);
  const opener = useRef<Opener>({ fid: null });
  const shownAt = useRef(Date.now());
  const touchedAt = useRef(Date.now());
  const closed = useRef(false);

  /** `restore`: focus goes back (Close, Back, Up, Down, its time up); not when it opens a page or the pointer moved away. */
  const close = (restore: boolean, open = false) => {
    if (closed.current) return;
    closed.current = true;
    if (card.current) ui.focus.release(card.current, restore);
    p.onDone(open, restore ? opener.current : null);
  };
  p.noticeRef({
    pointedAway: (el) => {
      if (closed.current || !card.current || card.current.contains(el)) return false;
      close(false);
      return true;
    },
  });

  // Focus comes here, and goes back when it closes; Back closes it.
  useLayoutEffect(() => {
    const el = card.current;
    if (!el) return;
    opener.current = { fid: ui.focus.current?.getAttribute("data-fid") ?? null };
    ui.focus.trap(el);
    ui.focus.focus(el.querySelector<HTMLElement>("[data-f]"));
    shownAt.current = Date.now();
    touchedAt.current = shownAt.current;
    const off = ui.input.push((key, e) => {
      if (key !== "back") return false;
      if (e.type === "down") close(true);
      return true;
    });
    return () => {
      off();
      // Closed some other way (the page went): the trap goes with it.
      if (!closed.current) ui.focus.release(el, false);
    };
  }, []);

  // Untouched, it goes by itself.
  useEffect(() => {
    let t: ReturnType<typeof setTimeout>;
    const wait = (ms: number) => {
      t = setTimeout(() => {
        const left = touchedAt.current + NOTICE_SHOWN_MS - Date.now();
        if (left > 0) wait(left);
        else close(true);
      }, ms);
    };
    wait(NOTICE_SHOWN_MS);
    return () => clearTimeout(t);
  }, []);

  const touched = () => (touchedAt.current = Date.now());
  const guarded = () => Date.now() - shownAt.current < OK_GUARD_MS;
  return (
    <div ref={card} class="request-notice" role="alertdialog" aria-label={p.notice.text}>
      <div class="request-notice-head">
        {p.notice.poster && (
          <div class="request-notice-art">
            <Art candidates={[p.notice.poster]} width={40} title="" class="tile-clip" />
          </div>
        )}
        <div class="request-notice-text">
          {/* The poster says which title it is; without one, a tick says it's good news. */}
          {!p.notice.poster && <Icon name="circle-check" size={20} color="var(--success)" />}
          <div class="t-label lines-3 c-fg">{p.notice.text}</div>
        </div>
      </div>
      <G
        kind="row"
        class="request-notice-buttons"
        onEdge={(dir) => {
          if (dir === "up" || dir === "down") {
            close(true);
            return true;
          }
          touched();
          return true;
        }}
      >
        <TextButton
          label={t("common.actions.open")}
          onFocus={touched}
          onSelect={() => {
            if (!guarded()) close(false, true);
          }}
        />
        <TextButton
          label={t("common.actions.close")}
          onFocus={touched}
          onSelect={() => {
            if (!guarded()) close(true);
          }}
        />
      </G>
    </div>
  );
}
