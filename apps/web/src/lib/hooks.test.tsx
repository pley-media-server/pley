import { useRef, useState } from "react";
import { act, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  getStored,
  useDebouncedValue,
  useDismiss,
  useDocumentTitle,
  useTitleNote,
  useElementWidth,
  useLatest,
  useLocalStorage,
  useMediaQuery,
  useOnChange,
  writeStorage,
} from "./hooks";

afterEach(() => {
  vi.useRealTimers();
});

describe("useDebouncedValue", () => {
  it("follows the value once it settles", () => {
    vi.useFakeTimers();
    const { result, rerender } = renderHook(({ value }) => useDebouncedValue(value, 300), { initialProps: { value: "a" } });

    rerender({ value: "ab" });
    act(() => {
      vi.advanceTimersByTime(200);
    });
    rerender({ value: "abc" });
    act(() => {
      vi.advanceTimersByTime(299);
    });
    expect(result.current).toBe("a");

    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(result.current).toBe("abc");
  });
});

describe("local storage", () => {
  it("persists state as JSON", () => {
    window.localStorage.setItem("pley.test", JSON.stringify({ volume: 0.5 }));
    const { result } = renderHook(() => useLocalStorage("pley.test", { volume: 1 }));
    expect(result.current[0]).toEqual({ volume: 0.5 });

    act(() => result.current[1]((prev) => ({ volume: prev.volume / 2 })));
    expect(result.current[0]).toEqual({ volume: 0.25 });
    expect(getStored("pley.test", null)).toEqual({ volume: 0.25 });
  });

  it("falls back when storage is unusable", () => {
    window.localStorage.setItem("pley.broken", "{not json");
    expect(getStored("pley.broken", "fallback")).toBe("fallback");

    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("full", "QuotaExceededError");
    });
    expect(() => writeStorage("pley.test", 1)).not.toThrow();
    const { result } = renderHook(() => useLocalStorage("pley.other", 1));
    act(() => result.current[1](2));
    expect(result.current[0]).toBe(2);
  });
});

describe("useOnChange", () => {
  function Draft({ id }: { id: number }) {
    const [text, setText] = useState("");
    useOnChange(id, () => setText(""));
    return (
      <div>
        <input aria-label="draft" value={text} onChange={(e) => setText(e.target.value)} />
        <span>{text ? "dirty" : "clean"}</span>
      </div>
    );
  }

  it("resets state when the key changes, not otherwise", () => {
    const { rerender } = render(<Draft id={1} />);
    fireEvent.change(screen.getByLabelText("draft"), { target: { value: "hello" } });
    rerender(<Draft id={1} />);
    expect(screen.getByText("dirty")).toBeInTheDocument();

    rerender(<Draft id={2} />);
    expect(screen.getByText("clean")).toBeInTheDocument();
  });

  it("calls back once per change, with the new key", () => {
    const onChange = vi.fn();
    const { rerender } = renderHook(({ key }) => useOnChange(key, onChange), { initialProps: { key: "a" } });
    rerender({ key: "a" });
    expect(onChange).not.toHaveBeenCalled();
    rerender({ key: "b" });
    rerender({ key: "b" });
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith("b");
  });
});

describe("useLatest", () => {
  it("holds the value of the last render", () => {
    const { result, rerender } = renderHook(({ value }) => useLatest(value), { initialProps: { value: 1 } });
    const ref = result.current;
    rerender({ value: 2 });
    expect(result.current).toBe(ref);
    expect(ref.current).toBe(2);
  });
});

describe("useDismiss", () => {
  function Popover({ open, onClose }: { open: boolean; onClose: () => void }) {
    const ref = useRef<HTMLDivElement>(null);
    useDismiss(ref, open, onClose);
    return (
      <div>
        <div ref={ref}>inside</div>
        <div>outside</div>
      </div>
    );
  }

  it("closes on a click outside and on Escape", () => {
    const onClose = vi.fn();
    render(<Popover open onClose={onClose} />);

    fireEvent.pointerDown(screen.getByText("inside"));
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.pointerDown(screen.getByText("outside"));
    expect(onClose).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("ignores everything while closed", () => {
    const onClose = vi.fn();
    render(<Popover open={false} onClose={onClose} />);
    fireEvent.pointerDown(screen.getByText("outside"));
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
  });
});

describe("useDocumentTitle", () => {
  it("names the page while mounted", () => {
    document.title = "Pley";
    const { unmount } = renderHook(() => useDocumentTitle("Heat"));
    expect(document.title).toBe("Heat · Pley");
    unmount();
    expect(document.title).toBe("Pley");
  });

  it("comes after a note, which outlasts the pages", () => {
    document.title = "Pley";
    const note = renderHook(({ text }) => useTitleNote(text), { initialProps: { text: "22% · Importing from Plex" } });
    const page = renderHook(() => useDocumentTitle("Heat"));
    expect(document.title).toBe("22% · Importing from Plex · Heat · Pley");
    note.rerender({ text: "23% · Importing from Plex" });
    expect(document.title).toBe("23% · Importing from Plex · Heat · Pley");
    page.unmount();
    expect(document.title).toBe("23% · Importing from Plex · Pley");
    note.unmount();
    expect(document.title).toBe("Pley");
  });
});

describe("useMediaQuery", () => {
  it("follows the query", () => {
    let listener: (() => void) | undefined;
    const mql = {
      matches: false,
      addEventListener: (_: string, cb: () => void) => (listener = cb),
      removeEventListener: vi.fn(),
    };
    vi.stubGlobal("matchMedia", vi.fn(() => mql));

    const { result, unmount } = renderHook(() => useMediaQuery("(min-width: 640px)"));
    expect(result.current).toBe(false);

    mql.matches = true;
    act(() => listener?.());
    expect(result.current).toBe(true);
    unmount();
    expect(mql.removeEventListener).toHaveBeenCalled();
  });
});

describe("useElementWidth", () => {
  it("reports the observed width", () => {
    const callbacks: ResizeObserverCallback[] = [];
    vi.stubGlobal(
      "ResizeObserver",
      class {
        constructor(cb: ResizeObserverCallback) {
          callbacks.push(cb);
        }
        observe() {}
        disconnect() {}
      },
    );
    function Box() {
      const [ref, width] = useElementWidth<HTMLDivElement>();
      return <div ref={ref}>{width}px</div>;
    }
    render(<Box />);
    expect(screen.getByText("0px")).toBeInTheDocument();

    act(() => callbacks[0]?.([{ contentRect: { width: 321.4 } } as ResizeObserverEntry], {} as ResizeObserver));
    expect(screen.getByText("321px")).toBeInTheDocument();
  });
});
