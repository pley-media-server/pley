import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import type { ServerStatus, User } from "@/api/types";
import { i18n, setDeviceLanguage, setLanguage } from "@/i18n";
import { LanguagePicker } from "@/i18n/LanguagePicker";
import { LanguageSync } from "@/i18n/LanguageSync";
import { mockApi, renderWithProviders, testUser } from "@/test/render";
import { LanguageCard } from "./LanguageCard";

const server: ServerStatus = { name: "Den", version: "1.0.0", setup_required: false, language: "en" };

/** The browser in a language Pley doesn't have, so the server language counts. */
function browserIn(...tags: string[]) {
  Object.defineProperty(navigator, "languages", { value: tags, configurable: true });
}

afterEach(async () => {
  setDeviceLanguage(null);
  delete (navigator as { languages?: unknown }).languages;
  await setLanguage("en");
});

describe("LanguageCard", () => {
  it("shows Pley in the language picked, and back in this device's with Automatic", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/server": () => server,
      "PATCH /api/me": (req) => ({ ...testUser, ...(req.body as Partial<User>) }),
    });
    const { client } = renderWithProviders(
      <>
        <LanguageSync />
        <LanguageCard />
      </>,
    );
    client.setQueryData(["me"], testUser);

    const picker = screen.getByLabelText("Language");
    expect(picker).toHaveValue("");
    expect(screen.getByRole("option", { name: "Automatic (English)" })).toBeInTheDocument();

    await user.selectOptions(picker, "Français");
    await waitFor(() => expect(requests.at(-1)?.body).toEqual({ language: "fr" }));
    // Every word, this card's own too, is French once the account has it.
    expect(await screen.findByText("Langue")).toBeInTheDocument();
    expect(i18n.language).toBe("fr");
    expect(document.documentElement.lang).toBe("fr");

    await user.selectOptions(screen.getByLabelText("Langue"), "");
    await waitFor(() => expect(requests.at(-1)?.body).toEqual({ language: null }));
    expect(await screen.findByText("Language")).toBeInTheDocument();
  });

  it("keeps the language of the last visit until the server has answered, where the browser's isn't Pley's", async () => {
    browserIn("de-DE");
    await setLanguage("fr");
    let answer: (status: ServerStatus) => void = () => {};
    mockApi({ "GET /api/server": () => new Promise<ServerStatus>((resolve) => (answer = resolve)) });
    renderWithProviders(<LanguageSync />);
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(i18n.language).toBe("fr");
    // The server language once the server says it.
    answer({ ...server, language: "es" });
    await waitFor(() => expect(i18n.language).toBe("es"));
  });

  it("names the server language as Automatic where the browser's isn't Pley's", async () => {
    browserIn("de-DE");
    mockApi({ "GET /api/server": () => ({ ...server, language: "es" }) });
    renderWithProviders(<LanguageCard />);
    expect(await screen.findByRole("option", { name: "Automatic (Español)" })).toBeInTheDocument();
  });
});

describe("LanguagePicker", () => {
  it("picks this device's language before signing in, and Automatic goes back to the browser's", async () => {
    const user = userEvent.setup();
    mockApi({ "GET /api/server": () => ({ ...server, language: "fr" }) });
    renderWithProviders(
      <>
        <LanguageSync />
        <LanguagePicker />
      </>,
    );
    // The browser's language (English) before the server language.
    await user.click(await screen.findByRole("button", { name: "Choose a language" }));
    expect(screen.getByRole("menuitem", { name: "Automatic (English)" })).toBeInTheDocument();

    await user.click(screen.getByRole("menuitem", { name: "Español" }));
    await waitFor(() => expect(i18n.language).toBe("es"));
    expect(localStorage.getItem("pley.deviceLanguage")).toBe("es");

    await user.click(screen.getByRole("button", { name: "Elegir un idioma" }));
    await user.click(screen.getByRole("menuitem", { name: "Automático (English)" }));
    await waitFor(() => expect(i18n.language).toBe("en"));
    expect(localStorage.getItem("pley.deviceLanguage")).toBeNull();
  });
});
