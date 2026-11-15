import type { ImportLibraryRow, ImportProgress, ImportStep } from "@/api/plexImport";
import { currentLocale } from "@/i18n";

/**
 * About how much of an import's time each step takes, for the bar of the
 * whole import: scanning the libraries and fetching their details are most
 * of it, the steps that only write what Plex said take seconds. Rough, and
 * only ever compared with each other.
 */
const STEP_WEIGHTS: Record<ImportStep, number> = {
  profiles: 1,
  scanning: 40,
  matching: 0,
  labels: 2,
  edits: 2,
  collections: 1,
  watch_state: 3,
  history: 3,
  playlists: 1,
  fetching: 35,
  matching_again: 12,
};

/**
 * How far the whole import has come: its steps done and the part of the
 * running one that is, each weighed by how long it takes.
 */
export function importShare(progress: ImportProgress): number {
  const steps = progress.steps ?? [];
  const total = steps.reduce((sum, row) => sum + STEP_WEIGHTS[row.step], 0);
  if (total === 0) return 0;
  const done = steps.reduce((sum, row) => {
    const weight = STEP_WEIGHTS[row.step];
    if (row.state === "done") return sum + weight;
    if (row.state === "running") return sum + weight * runningPart(progress, row.step);
    return sum;
  }, 0);
  return done / total;
}

/** The part of the running step done: for the scans, the libraries' files read; otherwise its count. */
function runningPart(progress: ImportProgress, step: ImportStep): number {
  const libraries = progress.libraries ?? [];
  if (step === "scanning" && libraries.length > 0) {
    return libraries.reduce((sum, library) => sum + libraryPart(library), 0) / libraries.length;
  }
  return progress.total > 0 ? Math.min(progress.done / progress.total, 1) : 0;
}

/** How far a library is in the scanning step: its files read, then Plex's titles found in it. */
function libraryPart(library: ImportLibraryRow): number {
  switch (library.phase) {
    case "done":
    case "matching":
      return 1;
    case "probing":
      return library.total > 0 ? Math.min(library.done / library.total, 1) : 0;
    default:
      return 0;
  }
}

/** How far, as a percentage of the language shown, never rounded up to what isn't done: "22%". */
export function formatShare(share: number): string {
  return new Intl.NumberFormat(currentLocale(), { style: "percent", maximumFractionDigits: 0 }).format(Math.floor(share * 100) / 100);
}
