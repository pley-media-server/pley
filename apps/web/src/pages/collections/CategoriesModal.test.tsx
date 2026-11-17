import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Category } from "@/api/lists";
import { apiError, mockApi, renderWithProviders } from "@/test/render";
import { CategoriesModal } from "./CategoriesModal";

const highlights: Category = { id: 1, key: "highlights", title: "Highlights", from_label: null, position: 0 };
const seasonal: Category = { id: 3, key: null, title: "Seasonal", from_label: null, position: 1 };

describe("CategoriesModal", () => {
  it("says in the reader's words that a name is taken", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/collection-categories": () => ({ categories: [highlights, seasonal] }),
      "POST /api/collection-categories": () => apiError(409, "conflict", "a category already has that name"),
    });
    renderWithProviders(<CategoriesModal open onClose={vi.fn()} />);

    await user.type(await screen.findByLabelText("New category"), "highlights");
    await user.click(screen.getByRole("button", { name: "Add" }));
    expect(await screen.findByText("Another category already has this name.")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "POST")?.body).toEqual({ title: "highlights" });
    // What was typed stays, to change.
    await waitFor(() => expect(screen.getByLabelText("New category")).toHaveValue("highlights"));
  });
});
