import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ConfirmModal, Modal } from "./Modal";

function renderModal(props: Partial<Parameters<typeof Modal>[0]> = {}) {
  const onClose = vi.fn();
  const result = render(
    <Modal open title="Edit library" onClose={onClose} footer={<button type="button">Save</button>} {...props}>
      <input aria-label="Name" />
    </Modal>,
  );
  return { ...result, onClose };
}

describe("Modal", () => {
  it("renders nothing while closed", () => {
    renderModal({ open: false });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("is a dialog named by its title that focuses the first field", async () => {
    renderModal();
    expect(screen.getByRole("dialog", { name: "Edit library" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByLabelText("Name")).toHaveFocus());
  });

  it("closes on Escape, on the backdrop and with the close button", async () => {
    const user = userEvent.setup();
    const { onClose, baseElement } = renderModal();

    await user.keyboard("{Escape}");
    fireEvent.click(baseElement.querySelector(".backdrop-blur-sm") as HTMLElement);
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledTimes(3);
  });

  it("closes only the dialog on top on Escape", async () => {
    const user = userEvent.setup();
    const outer = vi.fn();
    const inner = vi.fn();
    const both = (confirming: boolean) => (
      <Modal open onClose={outer} title="Import">
        <ConfirmModal open={confirming} onClose={inner} onConfirm={() => {}} title="Stop?" message="Sure?" />
      </Modal>
    );
    // The confirmation opens over the dialog, as a button in it opens it.
    const { rerender } = render(both(false));
    rerender(both(true));
    await user.keyboard("{Escape}");
    expect(inner).toHaveBeenCalledTimes(1);
    expect(outer).not.toHaveBeenCalled();
  });

  it("stays open while locked", async () => {
    const user = userEvent.setup();
    const { onClose, baseElement } = renderModal({ locked: true });

    await user.keyboard("{Escape}");
    fireEvent.click(baseElement.querySelector(".backdrop-blur-sm") as HTMLElement);
    expect(screen.getByRole("button", { name: "Close" })).toBeDisabled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("keeps the focus inside", async () => {
    const user = userEvent.setup();
    renderModal();
    const name = screen.getByLabelText("Name");
    await waitFor(() => expect(name).toHaveFocus());

    screen.getByRole("button", { name: "Save" }).focus();
    await user.tab();
    expect(screen.getByRole("button", { name: "Close" })).toHaveFocus();
    await user.tab({ shift: true });
    expect(screen.getByRole("button", { name: "Save" })).toHaveFocus();
  });

  it("locks page scrolling and gives the focus back when it closes", () => {
    const opener = document.createElement("button");
    document.body.append(opener);
    opener.focus();

    const { rerender } = renderModal();
    expect(document.body.style.overflow).toBe("hidden");

    rerender(
      <Modal open={false} title="Edit library" onClose={() => {}}>
        body
      </Modal>,
    );
    expect(document.body.style.overflow).toBe("");
    expect(opener).toHaveFocus();
    opener.remove();
  });
});

describe("ConfirmModal", () => {
  it("asks before acting", async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();
    const onClose = vi.fn();
    render(
      <ConfirmModal open title="Delete user?" message="Their history goes too." confirmLabel="Delete" danger onConfirm={onConfirm} onClose={onClose} />,
    );

    const confirm = screen.getByRole("button", { name: "Delete" });
    await waitFor(() => expect(confirm).toHaveFocus());
    await user.click(confirm);
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("can't be dismissed while working", () => {
    render(<ConfirmModal open loading title="Delete user?" message="…" onConfirm={vi.fn()} onClose={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Close" })).toBeDisabled();
  });
});
