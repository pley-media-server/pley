import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import type { ServerStatus, User } from "@/api/types";
import { i18n, setLanguage } from "@/i18n";
import { LanguageSync } from "@/i18n/LanguageSync";
import { mockApi, renderWithProviders, testUser } from "@/test/render";
import { LanguageCard } from "./LanguageCard";

const server: ServerStatus = { name: "Den", version: "1.0.0", setup_required: false, language: null };

afterEach(() => setLanguage("en"));

describe("LanguageCard", () => {
  it("shows Pley in the language picked, and back in the server's with the default", async () => {
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
    expect(screen.getByRole("option", { name: "Server default (this device's language, English)" })).toBeInTheDocument();

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

  it("keeps the language of the last visit until the server has answered", async () => {
    await setLanguage("fr");
    let answer: (status: ServerStatus) => void = () => {};
    mockApi({ "GET /api/server": () => new Promise<ServerStatus>((resolve) => (answer = resolve)) });
    renderWithProviders(<LanguageSync />);
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(i18n.language).toBe("fr");
    // The browser's (English here) once the server says it has no language of its own.
    answer(server);
    await waitFor(() => expect(i18n.language).toBe("en"));
  });

  it("names the server's language as the default when it has one", async () => {
    mockApi({ "GET /api/server": () => ({ ...server, language: "es" }) });
    renderWithProviders(<LanguageCard />);
    expect(await screen.findByRole("option", { name: "Server default (Español)" })).toBeInTheDocument();
  });
});
