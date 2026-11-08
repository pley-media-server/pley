/** Longest label the server keeps, in characters. */
export const MAX_LABEL = 120;

/**
 * A label as the server matches it: the case of its letters A to Z doesn't
 * count, as in its database ("Kids" is "kids", "École" isn't "école").
 */
export function labelKey(label: string): string {
  return label.replace(/[A-Z]/g, (c) => c.toLowerCase());
}

/**
 * `labels` with `text` added, trimmed: unchanged when it's empty or there
 * already, as the server matches labels.
 */
export function addLabel(labels: string[], text: string): string[] {
  const label = text.trim().slice(0, MAX_LABEL);
  if (!label || labels.some((l) => labelKey(l) === labelKey(label))) return labels;
  return [...labels, label];
}

/** Labels in both lists, as the server matches them: always and never shown at once, which it refuses. */
export function inBoth(show: string[], hide: string[]): string[] {
  return show.filter((s) => hide.some((h) => labelKey(h) === labelKey(s)));
}

/** Whether two lists hold the same labels, whatever their order. */
export function sameLabels(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((label) => b.includes(label));
}
