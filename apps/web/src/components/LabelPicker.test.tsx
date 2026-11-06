import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { mockApi, renderWithProviders } from "@/test/render";
import { LabelPicker } from "./LabelPicker";
import { Modal } from "./Modal";

function Picker({ start = [] as string[], onClose = () => {} }) {
  const [value, setValue] = useState(start);
  return (
    <Modal open title="Labels" onClose={onClose}>
      <label htmlFor="labels">Labels</label>
      <LabelPicker id="labels" value={value} onChange={setValue} removeLabel={(l) => `Remove ${l}`} />
      <p data-testid="value">{value.join("|")}</p>
    </Modal>
  );
}

function renderPicker(start: string[] = [], onClose = vi.fn()) {
  mockApi({
    "GET /api/labels": () => [
      { label: "Kids OK", items: 4 },
      { label: "Kometa", items: 12 },
      { label: "Scary", items: 1 },
    ],
  });
  renderWithProviders(<Picker start={start} onClose={onClose} />);
  return { onClose, field: screen.getByRole("combobox", { name: "Labels" }) };
}

const value = () => screen.getByTestId("value").textContent;

describe("LabelPicker", () => {
  it("suggests labels as they're typed, and Enter makes the highlighted one a chip", async () => {
    const user = userEvent.setup();
    const { field } = renderPicker();
    await user.type(field, "k");
    const options = await screen.findAllByRole("option");
    // Those starting with the text come first.
    expect(options.map((o) => o.textContent)).toEqual(["Kids OK4 titles", "Kometa12 titles", "Add “k”"]);
    await user.keyboard("{ArrowDown}{Enter}");
    expect(value()).toBe("Kometa");
    expect(field).toHaveValue("");
    // A label chosen isn't offered again.
    await user.keyboard("k");
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(["Kids OK4 titles", "Add “k”"]);
  });

  it("adds a label no title has yet, and a click picks a suggestion", async () => {
    const user = userEvent.setup();
    const { field } = renderPicker();
    await user.type(field, "Override: Kids{Enter}");
    expect(value()).toBe("Override: Kids");
    await user.type(field, "sca");
    await user.click(await screen.findByRole("option", { name: /Scary/ }));
    expect(value()).toBe("Override: Kids|Scary");
    expect(field).toHaveFocus();
  });

  it("closes its list on Escape before the dialog, and Backspace removes the last chip", async () => {
    const user = userEvent.setup();
    const { field, onClose } = renderPicker(["Kometa", "Scary"]);
    await user.type(field, "ki");
    expect(await screen.findByRole("listbox")).toBeVisible();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
    await user.clear(field);
    await user.keyboard("{Backspace}");
    expect(value()).toBe("Kometa");
    // Clearing the field opened the list again: one Escape for it, one for the dialog.
    await user.keyboard("{Escape}");
    expect(onClose).not.toHaveBeenCalled();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });
});
