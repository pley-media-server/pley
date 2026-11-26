import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { ACCENT_KEY, startTheme, THEME_KEY } from "@/theme";
import { mockApi, renderWithProviders } from "@/test/render";
import { ThemeCard } from "./ThemeCard";

afterEach(() => {
  localStorage.clear();
  startTheme();
});

const themes = () => screen.getByRole("radiogroup", { name: "Theme" });
const accents = () => screen.getByRole("radiogroup", { name: "Accent" });

describe("ThemeCard", () => {
  it("starts in Midnight on a device that picked none", () => {
    startTheme();
    renderWithProviders(<ThemeCard />);
    expect(within(themes()).getAllByRole("radio")).toHaveLength(9);
    expect(screen.getByRole("radio", { name: /^Midnight/ })).toBeChecked();
  });

  it("shows the theme picked at once and keeps it on this device, asking the server nothing", async () => {
    startTheme();
    const requests = mockApi({});
    renderWithProviders(<ThemeCard />);

    await userEvent.click(screen.getByRole("radio", { name: /^Starlight/ }));
    expect(document.documentElement.dataset.theme).toBe("starlight");
    expect(screen.getByRole("radio", { name: /^Starlight/ })).toBeChecked();
    expect(localStorage.getItem(THEME_KEY)).toBe("starlight");
    expect(requests).toEqual([]);
  });

  it("shows this device's last pick when it comes back", () => {
    localStorage.setItem(THEME_KEY, "twilight");
    localStorage.setItem(ACCENT_KEY, "gold");
    startTheme();
    renderWithProviders(<ThemeCard />);
    expect(document.documentElement.dataset.theme).toBe("twilight");
    expect(screen.getByRole("radio", { name: /^Twilight/ })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Gold" })).toBeChecked();
  });

  it("offers the theme's own and five accents, the theme's own on a device that picked none", () => {
    startTheme();
    renderWithProviders(<ThemeCard />);
    expect(within(accents()).getAllByRole("radio").map((radio) => radio.closest("label")?.textContent)).toEqual([
      "The theme's own",
      "Sky",
      "Violet",
      "Pink",
      "Gold",
      "Mint",
    ]);
    expect(screen.getByRole("radio", { name: "The theme's own" })).toBeChecked();
  });

  it("shows the accent picked at once over the theme, keeps it on this device, and goes back to the theme's own", async () => {
    startTheme();
    const requests = mockApi({});
    renderWithProviders(<ThemeCard />);

    await userEvent.click(screen.getByRole("radio", { name: "Violet" }));
    expect(document.documentElement.dataset.accent).toBe("violet");
    expect(screen.getByRole("radio", { name: "Violet" })).toBeChecked();
    expect(localStorage.getItem(ACCENT_KEY)).toBe("violet");

    // Another theme keeps it.
    await userEvent.click(screen.getByRole("radio", { name: /^Sunshine/ }));
    expect(document.documentElement.dataset.accent).toBe("violet");

    await userEvent.click(screen.getByRole("radio", { name: "The theme's own" }));
    expect(document.documentElement.dataset.accent).toBeUndefined();
    expect(localStorage.getItem(ACCENT_KEY)).toBeNull();
    expect(requests).toEqual([]);
  });

  it("can't change Blacklight's white, and says why", async () => {
    startTheme();
    renderWithProviders(<ThemeCard />);
    await userEvent.click(screen.getByRole("radio", { name: /^Blacklight/ }));
    for (const radio of within(accents()).getAllByRole("radio")) expect(radio).toBeDisabled();
    expect(screen.getByText("Blacklight is black and white only, so it keeps its white.")).toBeInTheDocument();
  });
});
