import { fireEvent, render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import fixture from "../../../../packages/tv/test/fixtures/avatar.json";
import { Avatar, AVATAR_COLORS, avatarColor, avatarInitial, textOn } from "./Avatar";

describe("a profile's avatar, as on every app (fixtures/avatar.json)", () => {
  it("offers the fixture's colors", () => {
    expect(AVATAR_COLORS).toEqual(fixture.colors);
  });

  it.each(fixture.cases)("$name, $avatar_color: $initial on $color in $ink", (c) => {
    const color = avatarColor({ name: c.name, avatar_color: c.avatar_color });
    expect(color).toBe(c.color);
    expect(textOn(color)).toBe(c.ink);
    expect(avatarInitial(c.name)).toBe(c.initial);
  });
});

describe("avatarColor", () => {
  it("uses the chosen color", () => {
    expect(avatarColor({ name: "Sam", avatar_color: "#123456" })).toBe("#123456");
  });

  it("derives a stable color from the name", () => {
    const color = avatarColor({ name: "Sam", avatar_color: null });
    expect(AVATAR_COLORS).toContain(color);
    expect(avatarColor({ name: "Sam", avatar_color: null })).toBe(color);
  });
});

describe("textOn", () => {
  it("picks dark text on light colors and white on dark ones", () => {
    expect(textOn("#ffffff")).toBe("#0b0d10");
    expect(textOn("#facc15")).toBe("#0b0d10");
    expect(textOn("#38bdf8")).toBe("#0b0d10");
    expect(textOn("#000")).toBe("#ffffff");
    expect(textOn("#1e3a8a")).toBe("#ffffff");
  });

  it("reads short and unprefixed colors", () => {
    expect(textOn("fff")).toBe("#0b0d10");
    expect(textOn("#FFF")).toBe("#0b0d10");
  });

  it("falls back to white for anything that isn't a color", () => {
    expect(textOn("red")).toBe("#ffffff");
    expect(textOn("#12345")).toBe("#ffffff");
  });
});

describe("Avatar", () => {
  const user = { id: 1, name: "sam", avatar_url: null, avatar_color: "#000000" };

  it("shows the initial on the profile color", () => {
    const { container } = render(<Avatar user={user} />);
    const avatar = container.firstElementChild as HTMLElement;
    expect(avatar).toHaveTextContent("S");
    expect(avatar).toHaveStyle({ backgroundColor: "#000000", color: "#ffffff" });
  });

  it("shows the picture, and the initial if it fails to load", () => {
    const { container } = render(<Avatar user={{ ...user, avatar_url: "/api/users/1/avatar?v=2" }} />);
    const img = container.querySelector("img") as HTMLImageElement;
    expect(img).toHaveAttribute("src", "/api/users/1/avatar?v=2");

    fireEvent.error(img);
    expect(container.querySelector("img")).toBeNull();
    expect(container.firstElementChild).toHaveTextContent("S");
  });

  it("falls back to a color from the name when the profile's isn't one", () => {
    const { container } = render(<Avatar user={{ ...user, avatar_color: "blue" }} />);
    expect(container.firstElementChild).toHaveStyle({ backgroundColor: avatarColor({ name: "sam", avatar_color: null }) });
  });

  it("has a placeholder for blank names", () => {
    const { container } = render(<Avatar user={{ ...user, name: "  " }} />);
    expect(container.firstElementChild).toHaveTextContent("?");
  });
});
