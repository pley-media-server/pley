import { render, type ComponentChildren } from "preact";
import { act } from "preact/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import type { User } from "../../src/api/types";
import { UiContext, type Ui } from "../../src/ui/context";
import { FocusManager } from "../../src/ui/focus";
import { AVATAR_COLORS, AVATAR_INK, avatarColor, avatarInitial, avatarInk, ProfileAvatar } from "../../src/ui/kit/avatar";
import { Rail, railEntries } from "../../src/ui/shell/rail";
// The rule every app draws a profile's avatar by (docs/THEMES.md, "Profile colors").
import fixture from "../fixtures/avatar.json";

describe("a profile's avatar, as the shared fixture has it", () => {
  it("offers the fixture's eight colors and draws its initial in its two inks", () => {
    expect(AVATAR_COLORS).toEqual(fixture.colors);
    expect(AVATAR_INK).toEqual(fixture.ink);
  });

  it("has the color, ink and initial of every case", () => {
    for (const c of fixture.cases) {
      const color = avatarColor(c.name, c.avatar_color);
      const drawn = { color, ink: avatarInk(color), initial: avatarInitial(c.name) };
      expect([c.name, c.avatar_color, drawn]).toEqual([c.name, c.avatar_color, { color: c.color, ink: c.ink, initial: c.initial }]);
    }
  });
});

let root: HTMLElement | null = null;

afterEach(() => {
  if (root) act(() => render(null, root!));
  root = null;
});

/** A color as the page's styles give it back, to compare with what was drawn. */
function styled(hex: string): string {
  const probe = document.createElement("div");
  probe.style.backgroundColor = hex;
  return probe.style.backgroundColor;
}

/** Pictures that arrive when the test says, or fail. */
function pictures() {
  const waiting = new Map<string, (objectUrl: string | null) => void>();
  return {
    waiting,
    images: {
      url: (path: string) => `http://tv.test${path}`,
      ready: () => null,
      load: (url: string, done: (objectUrl: string | null) => void) => {
        waiting.set(url, done);
        return () => waiting.delete(url);
      },
    },
  };
}

function draw(node: ComponentChildren, images?: unknown) {
  document.body.innerHTML = "";
  root = document.createElement("div");
  document.body.appendChild(root);
  const focus = new FocusManager();
  focus.setPage(root);
  const ui = {
    focus,
    images,
    scale: 2,
    input: { pointerMoved: false },
    claim: () => undefined,
    nav: { stack: { value: [] }, restart: () => undefined },
  } as unknown as Ui;
  act(() => render(<UiContext.Provider value={ui}>{node}</UiContext.Provider>, root!));
  return root;
}

/** The circle drawn: its color, the initial's ink and the initial; null while a picture shows. */
function circle(el: HTMLElement) {
  const avatar = el.querySelector<HTMLElement>(".profile-avatar")!;
  const initial = avatar.querySelector<HTMLElement>(":scope > span:not(.lock-badge)");
  if (!initial) return null;
  return { color: avatar.style.backgroundColor, ink: initial.style.color, initial: initial.textContent };
}

describe("ProfileAvatar", () => {
  it("draws the profile's circle and initial without a picture", () => {
    const el = draw(<ProfileAvatar name="Ben" color="#7c3aed" picture={null} size={48} />);
    expect(circle(el)).toEqual({ color: styled("#7c3aed"), ink: styled("#ffffff"), initial: "B" });
  });

  it("draws the circle while its picture loads, then the picture alone", () => {
    const { waiting, images } = pictures();
    const el = draw(<ProfileAvatar name="Robin" color={null} picture="/api/users/2/avatar?v=1" size={48} />, images);
    expect(circle(el)).toEqual({ color: styled("#f472b6"), ink: styled("#0b0d10"), initial: "R" });
    act(() => waiting.get("http://tv.test/api/users/2/avatar?v=1")!("blob:robin"));
    expect(circle(el)).toBeNull();
    expect(el.querySelector("img")!.getAttribute("src")).toBe("blob:robin");
    expect(el.querySelector<HTMLElement>(".profile-avatar")!.style.backgroundColor).toBe("");
  });

  it("keeps the circle when its picture fails", () => {
    const { waiting, images } = pictures();
    const el = draw(<ProfileAvatar name="Alex" color="nope" picture="/api/users/3/avatar" size={48} />, images);
    act(() => waiting.get("http://tv.test/api/users/3/avatar")!(null));
    expect(el.querySelector("img")).toBeNull();
    expect(circle(el)).toEqual({ color: styled("#2dd4bf"), ink: styled("#0b0d10"), initial: "A" });
  });
});

describe("the rail's avatar", () => {
  const me = (over: Partial<User>) => ({ id: 1, username: "robin", name: "Robin", display_name: null, avatar_url: null, avatar_color: null, ...over }) as User;
  const rail = (user: User | null) => <Rail entries={railEntries([], true, true)} me={user} current="home" railRef={() => undefined} />;

  it("is the profile's, in its colors rather than the theme's", () => {
    expect(circle(draw(rail(me({}))))).toEqual({ color: styled("#f472b6"), ink: styled("#0b0d10"), initial: "R" });
    expect(circle(draw(rail(me({ avatar_color: "#ABC" }))))).toEqual({ color: styled("#aabbcc"), ink: styled("#0b0d10"), initial: "R" });
  });

  it("shows the profile's picture once it has loaded", () => {
    const { waiting, images } = pictures();
    const el = draw(rail(me({ avatar_url: "/api/users/1/avatar" })), images);
    expect(circle(el)?.initial).toBe("R");
    act(() => waiting.get("http://tv.test/api/users/1/avatar")!("blob:me"));
    expect(el.querySelector(".rail-avatar img")!.getAttribute("src")).toBe("blob:me");
  });
});
