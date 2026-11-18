import { act, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ToastProvider, useToast } from "./Toast";

function Saying({ text }: { text: string }) {
  const toast = useToast();
  return (
    <button type="button" onClick={() => toast.success(text)}>
      Say
    </button>
  );
}

describe("ToastProvider", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("takes a toast away after a while", () => {
    vi.useFakeTimers();
    render(
      <ToastProvider>
        <Saying text="Saved" />
      </ToastProvider>,
    );
    act(() => screen.getByRole("button", { name: "Say" }).click());
    expect(screen.getByText("Saved")).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(3500);
    });
    expect(screen.queryByText("Saved")).not.toBeInTheDocument();
  });

  it("stops its toasts' timers when it goes, so none fires after", () => {
    vi.useFakeTimers();
    const { unmount } = render(
      <ToastProvider>
        <Saying text="Saved" />
      </ToastProvider>,
    );
    act(() => screen.getByRole("button", { name: "Say" }).click());
    expect(vi.getTimerCount()).toBe(1);
    unmount();
    expect(vi.getTimerCount()).toBe(0);
  });
});
