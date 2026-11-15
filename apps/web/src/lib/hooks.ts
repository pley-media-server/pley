import { useCallback, useEffect, useEffectEvent, useLayoutEffect, useRef, useState, type RefObject } from "react";

/** The current time in unix seconds, updated every `everyMs`. */
export function useNow(everyMs = 15_000): number {
  const [now, setNow] = useState(() => Date.now() / 1000);
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now() / 1000), everyMs);
    return () => window.clearInterval(id);
  }, [everyMs]);
  return now;
}

export function useDebouncedValue<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const t = window.setTimeout(() => setDebounced(value), delay);
    return () => window.clearTimeout(t);
  }, [value, delay]);
  return debounced;
}

function readStorage<T>(key: string, fallback: T): T {
  try {
    const raw = window.localStorage.getItem(key);
    if (raw == null) return fallback;
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

export function writeStorage<T>(key: string, value: T) {
  try {
    window.localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // storage unavailable (private mode, quota) - ignore
  }
}

export function getStored<T>(key: string, fallback: T): T {
  return readStorage(key, fallback);
}

/** `useState` persisted to localStorage (per-browser convenience only). */
export function useLocalStorage<T>(key: string, initial: T): [T, (v: T | ((prev: T) => T)) => void] {
  const [value, setValue] = useState<T>(() => readStorage(key, initial));
  const set = useCallback(
    (v: T | ((prev: T) => T)) => {
      setValue((prev) => {
        const next = typeof v === "function" ? (v as (p: T) => T)(prev) : v;
        writeStorage(key, next);
        return next;
      });
    },
    [key],
  );
  return [value, set];
}

/** Observe an element's content-box width. */
export function useElementWidth<T extends HTMLElement>(): [RefObject<T | null>, number] {
  const ref = useRef<T | null>(null);
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    setWidth(el.clientWidth);
    const ro = new ResizeObserver((entries) => {
      const w = entries[0]?.contentRect.width;
      if (w != null) setWidth(Math.round(w));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return [ref, width];
}

export function useDocumentTitle(title: string | null | undefined) {
  useEffect(() => {
    if (!title) return;
    let prev = "";
    showTab((tab) => {
      prev = tab.page;
      tab.page = `${title} · Pley`;
    });
    return () => showTab((tab) => void (tab.page = prev));
  }, [title]);
}

/**
 * Puts `note` before the page's title while mounted, for what goes on
 * whatever the page, such as the Plex import: "22% · Importing from Plex ·
 * Home · Pley".
 */
export function useTitleNote(note: string | null) {
  useEffect(() => {
    if (!note) return;
    showTab((tab) => void (tab.note = note));
    return () => showTab((tab) => void (tab.note = null));
  }, [note]);
}

/** The tab's title: the page's, after a note while there's one. */
let tab: { page: string; note: string | null } | null = null;

function showTab(change: (tab: { page: string; note: string | null }) => void) {
  // The document's own title until a page names itself.
  tab ??= { page: document.title, note: null };
  change(tab);
  document.title = tab.note ? `${tab.note} · ${tab.page}` : tab.page;
}

/**
 * A ref to the latest `value`, for callbacks that run after render (event
 * handlers, timers). It is updated after each commit, so reading it during
 * render gives the previous value.
 */
export function useLatest<T>(value: T): RefObject<T> {
  const ref = useRef(value);
  useLayoutEffect(() => {
    ref.current = value;
  });
  return ref;
}

/**
 * Call `onChange` during render when `key` changes (React's "adjusting state
 * when a prop changes"): use it to reset local state, instead of an effect
 * that would render twice.
 */
export function useOnChange<K>(key: K, onChange: (key: K) => void) {
  const [seen, setSeen] = useState(key);
  if (!Object.is(seen, key)) {
    setSeen(key);
    onChange(key);
  }
}

/** Close something when clicking outside of `ref` or pressing Escape. */
export function useDismiss(ref: RefObject<HTMLElement | null>, open: boolean, onClose: () => void) {
  const close = useEffectEvent(onClose);
  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) close();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    document.addEventListener("pointerdown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open, ref]);
}

/** "smooth", unless the system asks for less motion. */
export function smoothScroll(): ScrollBehavior {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth";
}

export function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(() => window.matchMedia(query).matches);
  useEffect(() => {
    const mql = window.matchMedia(query);
    const onChange = () => setMatches(mql.matches);
    onChange();
    mql.addEventListener("change", onChange);
    return () => mql.removeEventListener("change", onChange);
  }, [query]);
  return matches;
}
