import { describe, expect, it } from "vitest";
import type { ImportPlan, PlanLibrary } from "@/api/plexImport";
import { choicesFor, firstChoices, libraryProblems } from "./choices";

function library(key: string, pairing: Partial<PlanLibrary["pairing"]>, locations = [`/plex/${key}`]): PlanLibrary {
  return {
    key,
    title: `Library ${key}`,
    kind: "movies",
    locations,
    pairing: { status: "not_in_pley", library_id: null, folders: [], folders_here: [], ...pairing },
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
      "1": { target: 7, name: "Library 1", folders: { "/plex/1": "/media/films" } },
      "2": { target: "new", name: "Library 2", folders: { "/plex/2": "/plex/2" } },
      "3": { target: "leave", name: "Library 3", folders: { "/plex/3": "" } },
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
});
