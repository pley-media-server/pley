import { currentLanguage, t } from "@/i18n";

/**
 * Playback speed: the speeds offered, for one title at a time (the next
 * plays at normal speed). Browsers keep the voices' pitch as it speeds up.
 */
export const SPEEDS: readonly number[] = [0.5, 0.75, 1, 1.25, 1.5, 2];

/** "Normal", "1.5×". */
export function speedLabel(speed: number): string {
  return speed === 1 ? t("speed.normal", { ns: "player" }) : `${speed.toLocaleString(currentLanguage())}×`;
}

/** The speed after `speed`, one step faster (`1`) or slower (`-1`), at most to either end. */
export function stepSpeed(speed: number, by: 1 | -1): number {
  const at = SPEEDS.indexOf(speed);
  const from = at >= 0 ? at : SPEEDS.indexOf(1);
  return SPEEDS[Math.max(0, Math.min(SPEEDS.length - 1, from + by))]!;
}
