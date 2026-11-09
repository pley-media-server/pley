import type { Id } from "@/api/types";
import type { ImportChoices, ImportPlan, PlanLibrary, PlexFolder } from "@/api/plexImport";

/** Where a Plex library goes: a Pley library, one made for it, or nowhere. */
export type Target = Id | "new" | "leave";

export interface LibraryChoice {
  target: Target;
  /** The new library's name. */
  name: string;
  /** Each of Plex's folders, where it is for Pley ("" not known). */
  folders: Record<string, string>;
  /** Plex's folders whose place Pley filled in, until they're changed: to check. */
  found: string[];
  /** Left out by the person, rather than for want of its folders. */
  left: boolean;
}

/** By Plex library's key. */
export type LibraryChoices = Record<string, LibraryChoice>;

/** What's wrong with a library's choice, to fix before going on. */
export type LibraryProblem = "no_folders";

/** A path without its trailing separators, as the server compares them. */
const bare = (path: string) => path.replace(/[/\\]+$/, "");

/**
 * Where `path` is for Pley through `folder`: the same path under its Pley
 * folder when it's in its Plex one, else null.
 */
function through(folder: PlexFolder, path: string): string | null {
  const plex = bare(folder.plex);
  const pley = bare(folder.pley);
  if (bare(path) === plex) return pley || folder.pley;
  const rest = path.slice(plex.length);
  if (!path.startsWith(plex) || !/^[/\\]/.test(rest)) return null;
  // A Windows Plex's `\` are `/` for a Pley that writes `/`.
  const windows = pley.includes("\\") && !pley.includes("/");
  return pley + (windows ? rest.replace(/\//g, "\\") : rest.replace(/\\/g, "/"));
}

/** Where each of Plex's folders is for Pley, as far as the plan knows. */
export function knownFolders(library: PlanLibrary): Record<string, string> {
  const { folders, folders_here } = library.pairing;
  return Object.fromEntries(
    library.locations.map((location) => {
      const known = folders.map((f) => through(f, location)).find((p) => p !== null);
      return [location, known ?? (folders_here.some((h) => bare(h) === bare(location)) ? location : foundFolder(library, location))];
    }),
  );
}

/** Where Pley's search found Plex's `location`, else "". */
function foundFolder(library: PlanLibrary, location: string): string {
  return library.pairing.folders_found.find((f) => bare(f.plex) === bare(location))?.pley ?? "";
}

/**
 * The choices the plan suggests: a library goes where its files are, one
 * that isn't in Pley gets a library of its own when Pley can open its
 * folders, and is left out when it can't (until they're said).
 */
export function firstChoices(plan: ImportPlan): LibraryChoices {
  return Object.fromEntries(
    plan.libraries.map((library) => {
      const folders = knownFolders(library);
      const reachable = Object.values(folders).some(Boolean);
      const id = library.pairing.library_id;
      const target: Target = library.pairing.status !== "not_in_pley" && id != null ? id : reachable ? "new" : "leave";
      const found = library.locations.filter((l) => folders[l] !== "" && folders[l] === foundFolder(library, l));
      return [library.key, { target, name: library.title, folders, found, left: false }];
    }),
  );
}

/** A path's parts, `/` and `\\` alike. */
const partsOf = (path: string) => path.split(/[/\\]+/).filter(Boolean);

/** `path` without its last `n` parts ("" when that's all of it). */
function withoutLast(path: string, n: number): string {
  let rest = bare(path);
  for (let i = 0; i < n; i++) {
    const at = Math.max(rest.lastIndexOf("/"), rest.lastIndexOf("\\"));
    if (at < 0) return "";
    rest = rest.slice(0, at);
  }
  return rest;
}

/**
 * What one answer says of Plex's other folders: its Plex folder and Pley's
 * without the end they share, ignoring case. `/data/movies` at
 * `/mnt/user/Movies` says `/data` is `/mnt/user`; with no end shared, it's
 * the folder itself, which still says where the folders inside it are.
 */
export function swapOf(plex: string, pley: string): PlexFolder {
  const a = partsOf(plex);
  const b = partsOf(pley);
  let shared = 0;
  while (shared < a.length && shared < b.length && a[a.length - 1 - shared]!.toLowerCase() === b[b.length - 1 - shared]!.toLowerCase()) {
    shared++;
  }
  return { plex: withoutLast(plex, shared), pley: withoutLast(pley, shared) };
}

/** A folder `swap` may fill in: library `key`'s Plex `location` at Pley's `path`. */
export interface Filled {
  key: string;
  location: string;
  path: string;
}

/** The empty folders of the plan's libraries that `swap` would fill in, for the step to check they're there. */
export function swapFills(plan: ImportPlan, choices: LibraryChoices, swap: PlexFolder): Filled[] {
  return plan.libraries.flatMap((library) => {
    const choice = choices[library.key];
    if (!choice) return [];
    return library.locations.flatMap((location) => {
      if (choice.folders[location]?.trim()) return [];
      const path = through(swap, location);
      return path ? [{ key: library.key, location, path }] : [];
    });
  });
}

/**
 * The choices with `filled` folders, those still empty: each marked as
 * Pley's to check, and a library left out for want of them made new.
 */
export function fillIn(choices: LibraryChoices, filled: Filled[]): LibraryChoices {
  const next = { ...choices };
  for (const { key, location, path } of filled) {
    const choice = next[key];
    if (!choice || choice.folders[location]?.trim()) continue;
    next[key] = {
      ...choice,
      target: choice.target === "leave" && !choice.left ? "new" : choice.target,
      folders: { ...choice.folders, [location]: path },
      found: [...choice.found, location],
    };
  }
  return next;
}

/** The new library's folders: Plex's, where they are for Pley. */
export function newPaths(choice: LibraryChoice): string[] {
  return [...new Set(Object.values(choice.folders).map((f) => f.trim()).filter(Boolean))];
}

/** The libraries whose choice can't be made as it is, by key. */
export function libraryProblems(plan: ImportPlan, choices: LibraryChoices): Record<string, LibraryProblem> {
  const problems: Record<string, LibraryProblem> = {};
  for (const library of plan.libraries) {
    const choice = choices[library.key];
    if (choice?.target === "new" && newPaths(choice).length === 0) problems[library.key] = "no_folders";
  }
  return problems;
}

/** Whether any library is brought at all. */
export function bringsAny(choices: LibraryChoices): boolean {
  return Object.values(choices).some((c) => c.target !== "leave");
}

/** The libraries and folders of the import's choices (docs/API.md, `ImportChoices`). */
export function choicesFor(plan: ImportPlan, choices: LibraryChoices): Pick<ImportChoices, "libraries" | "folders"> {
  const libraries: ImportChoices["libraries"] = [];
  const folders = new Map<string, string>();
  for (const library of plan.libraries) {
    const choice = choices[library.key];
    if (!choice || choice.target === "leave") continue;
    if (choice.target === "new") {
      libraries.push({ key: library.key, new: { name: choice.name.trim() || library.title, paths: newPaths(choice) } });
    } else {
      libraries.push({ key: library.key, library_id: choice.target });
    }
    for (const [plex, pley] of Object.entries(choice.folders)) {
      if (pley.trim() && !folders.has(plex)) folders.set(plex, pley.trim());
    }
    // The plan's other folders (a parent of Plex's, `/data` for `/mnt`),
    // which say where the files found are too.
    const locations = new Set(library.locations.map(bare));
    for (const f of library.pairing.folders) {
      if (!locations.has(bare(f.plex)) && !folders.has(f.plex)) folders.set(f.plex, f.pley);
    }
  }
  return { libraries, folders: [...folders].map(([plex, pley]) => ({ plex, pley })) };
}
