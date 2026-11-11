import { Route, Routes } from "react-router";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { LibraryInput } from "@/api/hooks";
import type { FsListing, Library } from "@/api/types";
import { LocationDisplay, mockApi, renderWithProviders, testUser } from "@/test/render";
import { nextKind, SetupLibraryPage } from "./SetupLibraryPage";

const ROOTS: FsListing = { path: null, parent: null, entries: [{ name: "/", path: "/" }], container: false, mounts: [] };

function renderStep({ roots = ROOTS, libraries = [] as Library[] } = {}) {
  const list = [...libraries];
  const requests = mockApi({
    "GET /api/libraries": () => list,
    "POST /api/fs": () => roots,
    "POST /api/libraries": (req) => {
      const input = req.body as LibraryInput;
      const library: Library = { ...input, id: list.length + 1, item_count: 0, created_at: 0, scanned_at: null, scanning: true };
      list.push(library);
      return library;
    },
  });
  renderWithProviders(
    <Routes>
      <Route path="/setup/library" element={<SetupLibraryPage />} />
      <Route path="*" element={<LocationDisplay />} />
    </Routes>,
    { route: "/setup/library" },
  );
  return requests;
}

const created = (requests: ReturnType<typeof renderStep>) =>
  requests.filter((r) => r.method === "POST" && r.path === "/api/libraries").map((r) => r.body);

const movies: Library = {
  id: 1,
  name: "Movies",
  kind: "movies",
  paths: ["/media/Movies"],
  item_count: 0,
  created_at: 0,
  scanned_at: null,
  scanning: true,
};

describe("nextKind", () => {
  it("is the first type not added yet", () => {
    expect(nextKind([])).toBe("movies");
    expect(nextKind([movies])).toBe("shows");
    expect(nextKind([movies, { ...movies, kind: "shows" }])).toBe("music");
    expect(nextKind([{ ...movies, kind: "music" }, { ...movies, kind: "shows" }, movies])).toBe("movies");
  });
});

describe("SetupLibraryPage", () => {
  it("adds libraries one after another, then goes home", async () => {
    const user = userEvent.setup();
    const requests = renderStep();
    await user.type(await screen.findByLabelText("Folder path"), "/media/Movies{Enter}");
    await user.click(screen.getByRole("button", { name: "Add library" }));

    const added = await screen.findByRole("list", { name: "Added" });
    expect(within(added).getByText("Movies")).toBeInTheDocument();
    expect(within(added).getByText("/media/Movies")).toBeInTheDocument();
    // The form starts again, for shows.
    expect(screen.getByRole("button", { name: "TV Shows" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByLabelText("Folder path")).toHaveValue("");

    // Folders typed in when Start watching is pressed are added first.
    await user.type(screen.getByLabelText("Folder path"), "/media/TV");
    await user.click(screen.getByRole("button", { name: "Start watching" }));
    expect(await screen.findByTestId("location")).toHaveTextContent(/^\/$/);
    expect(created(requests)).toEqual([
      { name: "Movies", kind: "movies", paths: ["/media/Movies"] },
      { name: "TV Shows", kind: "shows", paths: ["/media/TV"] },
    ]);
  });

  it("asks for a folder before adding", async () => {
    const user = userEvent.setup();
    const requests = renderStep();
    await user.click(await screen.findByRole("button", { name: "Add library" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Add at least one folder.");
    expect(created(requests)).toHaveLength(0);
  });

  it("goes home without a library when it's for later", async () => {
    const user = userEvent.setup();
    const requests = renderStep();
    await user.click(await screen.findByRole("button", { name: "Later" }));
    expect(await screen.findByTestId("location")).toHaveTextContent(/^\/$/);
    expect(created(requests)).toHaveLength(0);
  });

  it("goes back to how to start until a library is added", async () => {
    const user = userEvent.setup();
    renderStep();
    await user.click(await screen.findByRole("button", { name: "Back" }));
    expect(await screen.findByTestId("location")).toHaveTextContent(/^\/setup\/start$/);
  });

  it("goes on from libraries added before, starting with the next type", async () => {
    renderStep({ libraries: [movies] });
    expect(await screen.findByRole("button", { name: "Start watching" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "TV Shows" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.queryByRole("button", { name: "Later" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Back" })).not.toBeInTheDocument();
  });

  it("says when a container has no media mounted", async () => {
    renderStep({ roots: { ...ROOTS, container: true, mounts: [] } });
    expect(await screen.findByText(/no media folder is mounted/)).toBeInTheDocument();
  });

  it.each([
    ["outside a container", ROOTS],
    ["with media mounted", { ...ROOTS, container: true, mounts: ["/media"] }],
  ])("says nothing of mounts %s", async (_, roots) => {
    const requests = renderStep({ roots });
    await waitFor(() => expect(requests.some((r) => r.path === "/api/fs")).toBe(true));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.queryByText(/no media folder is mounted/)).not.toBeInTheDocument();
  });

  it("is for administrators only", async () => {
    mockApi({});
    renderWithProviders(<SetupLibraryPage />, { route: "/setup/library", user: { ...testUser, is_admin: false } });
    expect(await screen.findByText("Admins only")).toBeInTheDocument();
  });
});
