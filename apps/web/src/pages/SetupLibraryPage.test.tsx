import { Route, Routes } from "react-router";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { LibraryInput } from "@/api/hooks";
import type { FsListing, Library } from "@/api/types";
import { apiError, LocationDisplay, mockApi, renderWithProviders, testUser } from "@/test/render";
import { insertRow, rowProblems, SetupLibraryPage, startingRows, type LibraryRow } from "./SetupLibraryPage";

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

/** The second of some elements: a row added after the one suggested. */
function second(elements: HTMLElement[]): HTMLElement {
  const element = elements[1];
  if (!element) throw new Error("no second element");
  return element;
}

const name = (kind: string) => ({ movies: "Movies", shows: "TV Shows", music: "Music" })[kind] ?? kind;
const row = (over: Partial<LibraryRow>): LibraryRow => ({ key: 0, kind: "movies", name: "", folders: [""], suggested: true, ...over });

describe("startingRows", () => {
  it("suggests each type not added yet", () => {
    expect(startingRows([], name).map((r) => r.kind)).toEqual(["movies", "shows", "music"]);
    expect(startingRows([movies], name).map((r) => r.kind)).toEqual(["shows", "music"]);
  });
});

describe("rowProblems", () => {
  it("leaves out a suggested row left empty, but not one added", () => {
    expect(rowProblems([row({ key: 1 }), row({ key: 2, suggested: false, name: "Kids Movies" })], [], name)).toEqual({ 2: "noFolder" });
  });

  it("asks a library added for its name", () => {
    expect(rowProblems([row({ key: 1, suggested: false, folders: ["/media/Kids"] })], [], name)).toEqual({ 1: "noName" });
  });

  it("names a suggested row left blank after its type", () => {
    expect(rowProblems([row({ key: 1, folders: ["/media/Movies"] })], [], name)).toEqual({});
  });

  it("says when a name is taken, by a library or another row, whatever its case", () => {
    expect(rowProblems([row({ key: 1, kind: "shows", folders: ["/media/TV"], name: "movies" })], [movies], name)).toEqual({ 1: "nameTaken" });
    expect(
      rowProblems([row({ key: 1, folders: ["/a"] }), row({ key: 2, suggested: false, name: "MOVIES", folders: ["/b"] })], [], name),
    ).toEqual({ 2: "nameTaken" });
  });
});

describe("insertRow", () => {
  const rows = startingRows([], name);
  it("puts a row after the others of its type", () => {
    expect(insertRow(rows, row({ key: 9, kind: "movies" })).map((r) => r.key)).toEqual([0, 9, 1, 2]);
    expect(insertRow(rows, row({ key: 9, kind: "music" })).map((r) => r.key)).toEqual([0, 1, 2, 9]);
  });

  it("puts it where its type comes when there's none of it", () => {
    expect(insertRow(startingRows([movies], name), row({ key: 9, kind: "movies" })).map((r) => r.kind)).toEqual(["movies", "shows", "music"]);
  });
});

describe("SetupLibraryPage", () => {
  it("adds each library given a folder, one of each type and more, then goes home", async () => {
    const user = userEvent.setup();
    const requests = renderStep();
    await user.type(await screen.findByLabelText("Folder for Movies"), "/media/Movies");
    await user.type(screen.getByLabelText("Folder for TV Shows"), "/media/TV");
    // Another movie library, named first.
    await user.click(within(screen.getByRole("group", { name: "Add another library:" })).getByRole("button", { name: "Movies" }));
    const kids = second(screen.getAllByLabelText("Name of this Movies library"));
    expect(kids).toHaveFocus();
    await user.type(kids, "Kids Movies");
    await user.type(screen.getByLabelText("Folder for Kids Movies"), "/media/Kids");
    // Two folders in one library.
    await user.click(second(screen.getAllByRole("button", { name: "Add another folder" })));
    await user.type(second(screen.getAllByLabelText("Folder for Kids Movies")), "/media/Cartoons");

    await user.click(screen.getByRole("button", { name: "Start watching" }));
    expect(await screen.findByTestId("location")).toHaveTextContent(/^\/$/);
    expect(created(requests)).toEqual([
      { name: "Movies", kind: "movies", paths: ["/media/Movies"] },
      { name: "Kids Movies", kind: "movies", paths: ["/media/Kids", "/media/Cartoons"] },
      { name: "TV Shows", kind: "shows", paths: ["/media/TV"] },
    ]);
  });

  it("asks for a folder before going on", async () => {
    const user = userEvent.setup();
    const requests = renderStep();
    await user.click(await screen.findByRole("button", { name: "Start watching" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Choose a folder for at least one library, or press Later.");
    expect(created(requests)).toHaveLength(0);
  });

  it("doesn't drop a library added and left without a folder", async () => {
    const user = userEvent.setup();
    const requests = renderStep();
    await user.type(await screen.findByLabelText("Folder for Movies"), "/media/Movies");
    await user.click(within(screen.getByRole("group", { name: "Add another library:" })).getByRole("button", { name: "Music" }));
    await user.type(second(screen.getAllByLabelText("Name of this Music library")), "Audiobooks");
    await user.click(screen.getByRole("button", { name: "Start watching" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Choose a folder for this library, or remove it.");
    expect(created(requests)).toHaveLength(0);

    // Removed, it's no longer asked for.
    await user.click(screen.getByRole("button", { name: "Remove Audiobooks" }));
    await user.click(screen.getByRole("button", { name: "Start watching" }));
    expect(await screen.findByTestId("location")).toHaveTextContent(/^\/$/);
    expect(created(requests)).toEqual([{ name: "Movies", kind: "movies", paths: ["/media/Movies"] }]);
  });

  it("says why the server didn't add one, and doesn't add the others twice", async () => {
    const user = userEvent.setup();
    const list: Library[] = [];
    const requests = mockApi({
      "GET /api/libraries": () => list,
      "POST /api/fs": () => ROOTS,
      "POST /api/libraries": (req) => {
        const input = req.body as LibraryInput;
        if (input.paths[0] === "/nowhere") return apiError(400, "bad_path", "That folder doesn't exist.");
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
    await user.type(await screen.findByLabelText("Folder for Movies"), "/media/Movies");
    await user.type(screen.getByLabelText("Folder for TV Shows"), "/nowhere");
    await user.click(screen.getByRole("button", { name: "Start watching" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("That folder doesn't exist.");
    expect(within(await screen.findByRole("list", { name: "Added" })).getByText("Movies")).toBeInTheDocument();

    await user.clear(screen.getByLabelText("Folder for TV Shows"));
    await user.type(screen.getByLabelText("Folder for TV Shows"), "/media/TV");
    await user.click(screen.getByRole("button", { name: "Start watching" }));
    expect(await screen.findByTestId("location")).toHaveTextContent(/^\/$/);
    expect(created(requests).map((b) => (b as LibraryInput).name)).toEqual(["Movies", "TV Shows", "TV Shows"]);
    expect(list.map((l) => l.name)).toEqual(["Movies", "TV Shows"]);
  });

  it("takes away a suggested library that isn't wanted", async () => {
    const user = userEvent.setup();
    const requests = renderStep();
    await user.type(await screen.findByLabelText("Folder for Music"), "/media/Music");
    await user.click(screen.getByRole("button", { name: "Remove Music" }));
    expect(screen.queryByLabelText("Folder for Music")).not.toBeInTheDocument();
    await user.type(screen.getByLabelText("Folder for Movies"), "/media/Movies");
    await user.click(screen.getByRole("button", { name: "Start watching" }));
    expect(await screen.findByTestId("location")).toHaveTextContent(/^\/$/);
    expect(created(requests)).toEqual([{ name: "Movies", kind: "movies", paths: ["/media/Movies"] }]);
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

  it("goes on from libraries added before, suggesting the types not added", async () => {
    const user = userEvent.setup();
    renderStep({ libraries: [movies] });
    expect(within(await screen.findByRole("list", { name: "Added" })).getByText("Movies")).toBeInTheDocument();
    expect(screen.queryByLabelText("Folder for Movies")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Folder for TV Shows")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Later" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Back" })).not.toBeInTheDocument();
    // With one there already, Start watching needs no other.
    await user.click(screen.getByRole("button", { name: "Start watching" }));
    expect(await screen.findByTestId("location")).toHaveTextContent(/^\/$/);
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
