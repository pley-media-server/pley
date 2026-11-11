import { describe, expect, it } from "vitest";
import type { ImportPlan, PlanLibrary } from "@/api/plexImport";
import { choicesFor, fillIn, firstChoices, libraryProblems, swapFills, swapOf } from "./choices";

function library(key: string, pairing: Partial<PlanLibrary["pairing"]>, locations = [`/plex/${key}`]): PlanLibrary {
  return {
    key,
    title: `Library ${key}`,
    kind: "movies",
    locations,
    pairing: { status: "not_in_pley", library_id: null, folders: [], folders_here: [], folders_found: [], ...pairing },
  };
}

const plan: ImportPlan = {
  server: "Den",
  libraries: [
    // Its files are in Pley's library 7, under another path.
    library("1", { status: "matched", library_id: 7, folders: [{ plex: "/plex/1", pley: "/media/films" }] }),
    // Not in Pley, and Pley opens its folder where Plex has it.
    library("2", { folders_here: ["/plex/2"] }),
    // Not in Pley, and Pley can't open its folder.
    library("3", {}),
  ],
  users: [],
};

describe("Plex import choices", () => {
  it("start where the plan says each library goes", () => {
    expect(firstChoices(plan)).toEqual({
      "1": { target: 7, name: "Library 1", folders: { "/plex/1": "/media/films" }, left: false },
      "2": { target: "new", name: "Library 2", folders: { "/plex/2": "/plex/2" }, left: false },
      "3": { target: "leave", name: "Library 3", folders: { "/plex/3": "" }, left: false },
    });
  });

  it("send the libraries brought and where their folders are", () => {
    const choices = firstChoices(plan);
    choices["2"] = { ...choices["2"]!, name: "  " };
    expect(choicesFor(plan, choices)).toEqual({
      libraries: [
        { key: "1", library_id: 7 },
        { key: "2", new: { name: "Library 2", paths: ["/plex/2"] } },
      ],
      folders: [
        { plex: "/plex/1", pley: "/media/films" },
        { plex: "/plex/2", pley: "/plex/2" },
      ],
    });
  });

  it("need a folder for a new library", () => {
    const choices = firstChoices(plan);
    choices["3"] = { ...choices["3"]!, target: "new" };
    expect(libraryProblems(plan, choices)).toEqual({ "3": "no_folders" });
    choices["3"] = { ...choices["3"], folders: { "/plex/3": " /mnt/films " } };
    expect(libraryProblems(plan, choices)).toEqual({});
    expect(choicesFor(plan, choices).libraries.at(-1)).toEqual({ key: "3", new: { name: "Library 3", paths: ["/mnt/films"] } });
  });

  it("map Plex's folders through a parent folder, and send it", () => {
    const parent: ImportPlan = {
      ...plan,
      libraries: [library("9", { status: "matched", library_id: 7, folders: [{ plex: "/data", pley: "/mnt/" }] }, ["/data/films/"])],
    };
    const choices = firstChoices(parent);
    expect(choices["9"]?.folders).toEqual({ "/data/films/": "/mnt/films/" });
    expect(choicesFor(parent, choices).folders).toEqual([
      { plex: "/data/films/", pley: "/mnt/films/" },
      { plex: "/data", pley: "/mnt/" },
    ]);
  });

  it("start from the folders Pley found, to be checked", () => {
    const found: ImportPlan = { ...plan, libraries: [library("4", { folders_found: [{ plex: "/plex/4", pley: "/media/Movies" }] })] };
    expect(firstChoices(found)["4"]).toEqual({ target: "new", name: "Library 4", folders: { "/plex/4": "/media/Movies" }, left: false });
  });

  it("read what one folder given says of the others", () => {
    expect(swapOf("/data/movies", "/mnt/user/Movies")).toEqual({ plex: "/data", pley: "/mnt/user" });
    // All they share is taken off: `/data` is Pley's `/`.
    expect(swapOf("/data/media/movies", "/media/Movies")).toEqual({ plex: "/data", pley: "" });
    expect(swapOf("/movies", "/media/movies")).toEqual({ plex: "", pley: "/media" });
    expect(swapOf("D:\\Plex\\Films", "/mnt/nas/films")).toEqual({ plex: "D:\\Plex", pley: "/mnt/nas" });
    // Nothing shared: the folder itself, for the folders inside it.
    expect(swapOf("/data/films", "/volume1/video")).toEqual({ plex: "/data/films", pley: "/volume1/video" });
  });

  it("fill in only empty folders, and bring a library left out for want of them", () => {
    const docker: ImportPlan = {
      server: "Den",
      libraries: [
        library("1", {}, ["/data/media/movies"]),
        library("2", {}, ["/data/media/tv", "/data/other/kids"]),
        library("3", {}, ["/data/media/music"]),
      ],
      users: [],
    };
    let choices = firstChoices(docker);
    choices["1"] = { ...choices["1"]!, target: "new", folders: { "/data/media/movies": "/media/movies" } };
    choices["3"] = { ...choices["3"]!, left: true };
    const fills = swapFills(docker, choices, swapOf("/data/media/movies", "/media/movies"));
    expect(fills).toEqual([
      { key: "2", location: "/data/media/tv", path: "/media/tv" },
      { key: "2", location: "/data/other/kids", path: "/other/kids" },
      { key: "3", location: "/data/media/music", path: "/media/music" },
    ]);
    // The step fills in those that are there.
    choices = fillIn(
      choices,
      fills.filter((f) => f.path !== "/other/kids"),
    );
    expect(choices["2"]).toMatchObject({ target: "new", folders: { "/data/media/tv": "/media/tv", "/data/other/kids": "" } });
    // Left out by the person: filled in, but still left out.
    expect(choices["3"]).toMatchObject({ target: "leave", folders: { "/data/media/music": "/media/music" } });
    // A folder said already isn't changed.
    expect(fillIn(choices, [{ key: "2", location: "/data/media/tv", path: "/elsewhere" }])["2"]?.folders["/data/media/tv"]).toBe("/media/tv");
  });
});
