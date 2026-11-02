import { useEffect, useRef, useState } from "preact/hooks";
import { useUi } from "../context";
import { F } from "./focusable";
import { Icon, type IconName } from "./icons";

/**
 * A text field (TvTextField): the remote's focus lands on it without
 * opening the TV's keyboard, unless `typeNow`; OK opens it (the TV's own,
 * with its microphone), and its Done key, Back, Up or Down close it again,
 * leaving focus on the field.
 */
export function TextField(p: {
  value: string;
  placeholder: string;
  onInput: (value: string) => void;
  /** The keyboard's action (Search, Done). */
  onSubmit?: (value: string) => void;
  icon?: IconName;
  fid?: string;
  initial?: boolean;
  width?: number;
  type?: "text" | "url" | "search" | "number";
  onFocus?: (el: HTMLElement) => void;
  onTyping?: (typing: boolean) => void;
  /** The TV's keyboard opens as the field shows (a screen or dialog that only asks for words). */
  typeNow?: boolean;
}) {
  const ui = useUi();
  const input = useRef<HTMLInputElement>(null);
  const box = useRef<HTMLElement | null>(null);
  const [typing, setTyping] = useState(false);

  const stop = (moveTo?: "up" | "down") => {
    if (ui.input.typing !== input.current) return;
    ui.input.typing = null;
    input.current?.blur();
    setTyping(false);
    p.onTyping?.(false);
    if (moveTo) ui.focus.move(moveTo);
    else ui.focus.syncDom();
  };
  const start = () => {
    const el = input.current;
    if (!el) return;
    ui.input.typing = el;
    setTyping(true);
    p.onTyping?.(true);
    el.focus();
    const end = el.value.length;
    try {
      el.setSelectionRange(end, end);
    } catch {
      // Number fields have no selection.
    }
  };
  useEffect(() => {
    // Once it has the app's focus (its `initial`), so that the keyboard closing leaves focus on it.
    if (p.typeNow && box.current && ui.focus.current === box.current) start();
    return () => stop();
  }, []);

  return (
    <F
      class={`field${typing ? " typing" : ""}`}
      style={p.width ? `width:${p.width}px` : undefined}
      fid={p.fid}
      initial={p.initial}
      label={p.placeholder}
      elRef={(el) => (box.current = el)}
      onFocus={p.onFocus}
      onSelect={() => (typing ? undefined : start())}
      onKey={(key) => {
        if (ui.input.typing !== input.current) return false;
        if (key === "back" || key === "ok") {
          if (key === "ok") p.onSubmit?.(input.current?.value ?? "");
          stop();
          return true;
        }
        if (key === "up" || key === "down") {
          stop(key);
          return true;
        }
        return key.indexOf(":up") >= 0;
      }}
    >
      {p.icon && <Icon name={p.icon} size={20} color={typing ? "var(--fg)" : "var(--muted)"} />}
      <input
        ref={input}
        // Preact types an input's attributes by its type, one type at a time.
        type={(p.type ?? "text") as "text"}
        value={p.value}
        style={p.icon ? "margin-left:12px" : undefined}
        placeholder={p.placeholder}
        tabIndex={-1}
        onInput={(e) => p.onInput((e.target as HTMLInputElement).value)}
        onBlur={() => stop()}
        autoComplete="off"
        spellcheck={false}
      />
    </F>
  );
}
