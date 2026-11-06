/** Longest label the server keeps, in characters. */
export const MAX_LABEL = 120;

/**
 * `labels` with `text` added, trimmed: unchanged when it's empty or there
 * already, whatever its case (the server matches labels so).
 */
export function addLabel(labels: string[], text: string): string[] {
  const label = text.trim().slice(0, MAX_LABEL);
  if (!label || labels.some((l) => l.toLowerCase() === label.toLowerCase())) return labels;
  return [...labels, label];
}

/** Labels in both lists, whatever their case: always and never shown at once, which the server refuses. */
export function inBoth(show: string[], hide: string[]): string[] {
  return show.filter((s) => hide.some((h) => h.toLowerCase() === s.toLowerCase()));
}

/** Whether two lists hold the same labels, whatever their order. */
export function sameLabels(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((label) => b.includes(label));
}
