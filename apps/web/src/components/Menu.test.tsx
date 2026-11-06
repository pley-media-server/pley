import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Menu, MenuItem, MenuSeparator } from "./Menu";

function renderMenu(onRefresh = vi.fn()) {
  render(
    <div>
      <p>elsewhere</p>
      <Menu
        trigger={({ open, toggle }) => (
          <button type="button" aria-expanded={open} onClick={toggle}>
            More
          </button>
        )}
      >
        {(close) => (
          <>
            <MenuItem
              onClick={() => {
                onRefresh();
                close();
              }}
            >
              Refresh metadata
            </MenuItem>
            <MenuSeparator />
            <MenuItem onClick={vi.fn()} danger disabled>
              Delete
            </MenuItem>
          </>
        )}
      </Menu>
    </div>,
  );
  return onRefresh;
}

describe("Menu", () => {
  it("opens above its trigger where the window has no room below, and below again where it has", async () => {
    const user = userEvent.setup();
    renderMenu();
    const trigger = screen.getByRole("button", { name: "More" });
    const box = (top: number, height: number) => ({ top, bottom: top + height, height, left: 0, right: 100, width: 100, x: 0, y: top, toJSON: () => ({}) }) as DOMRect;
    const rect = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      return this.getAttribute("role") === "menu" ? box(0, 120) : box(window.innerHeight - 60, 40);
    });
    try {
      await user.click(trigger);
      expect(screen.getByRole("menu")).toHaveClass("bottom-full");
      await user.click(trigger);
      rect.mockImplementation(function (this: HTMLElement) {
        return this.getAttribute("role") === "menu" ? box(0, 120) : box(20, 40);
      });
      await user.click(trigger);
      expect(screen.getByRole("menu")).toHaveClass("top-full");
    } finally {
      rect.mockRestore();
    }
  });

  it("opens from its trigger and closes after a choice", async () => {
    const user = userEvent.setup();
    const onRefresh = renderMenu();

    await user.click(screen.getByRole("button", { name: "More" }));
    expect(screen.getByRole("button", { name: "More" })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("menuitem", { name: "Delete" })).toBeDisabled();

    await user.click(screen.getByRole("menuitem", { name: "Refresh metadata" }));
    expect(onRefresh).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("closes on a click elsewhere, on Escape and on its trigger", async () => {
    const user = userEvent.setup();
    renderMenu();
    const trigger = screen.getByRole("button", { name: "More" });

    await user.click(trigger);
    await user.click(screen.getByText("elsewhere"));
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    await user.click(trigger);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    await user.click(trigger);
    await user.click(trigger);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("is used from the keyboard: focus on its items, arrows, Escape back to the trigger", async () => {
    const user = userEvent.setup();
    const onRefresh = renderMenu();
    const trigger = screen.getByRole("button", { name: "More" });

    trigger.focus();
    await user.keyboard("{Enter}");
    const refresh = screen.getByRole("menuitem", { name: "Refresh metadata" });
    expect(refresh).toHaveFocus();
    // The disabled item is skipped: the only other one wraps around to it.
    await user.keyboard("{ArrowDown}");
    expect(refresh).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();

    await user.keyboard("{Enter}");
    await user.keyboard("{Enter}");
    expect(onRefresh).toHaveBeenCalledTimes(1);
  });

  it("tells its owner it opened and closed, and closed when it went while open", async () => {
    const user = userEvent.setup();
    const heard: boolean[] = [];
    const menu = (
      <Menu
        onOpenChange={(open) => heard.push(open)}
        trigger={({ toggle }) => (
          <button type="button" onClick={toggle}>
            More
          </button>
        )}
      >
        {(close) => <MenuItem onClick={close}>Refresh metadata</MenuItem>}
      </Menu>
    );
    const { rerender } = render(menu);

    await user.click(screen.getByRole("button", { name: "More" }));
    await user.click(screen.getByRole("menuitem", { name: "Refresh metadata" }));
    await user.click(screen.getByRole("button", { name: "More" }));
    rerender(<p>gone</p>);
    expect(heard).toEqual([true, false, true, false]);
  });

  it("keeps the keys it takes from the page's own shortcuts", async () => {
    const user = userEvent.setup();
    renderMenu();
    const heard: string[] = [];
    const onKey = (e: KeyboardEvent) => heard.push(e.key);
    window.addEventListener("keydown", onKey);
    try {
      await user.click(screen.getByRole("button", { name: "More" }));
      await user.keyboard("{ArrowDown}{End}{m}{Escape}");
      expect(screen.queryByRole("menu")).not.toBeInTheDocument();
      // Only what the menu doesn't take goes on to the page.
      expect(heard).toEqual(["m"]);
    } finally {
      window.removeEventListener("keydown", onKey);
    }
  });
});
