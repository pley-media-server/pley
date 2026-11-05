import { formatNumber, t } from "../i18n";

/**
 * Playback speed (the web's player/speed.ts): the speeds offered, for one
 * title at a time (the next plays at normal speed). Only an engine that
 * plays them with their sound offers them, which Samsung's AVPlay doesn't:
 * its speeds are a silent fast-forward (docs/TV_PLATFORMS.md).
 */
export const SPEEDS: readonly number[] = [0.5, 0.75, 1, 1.25, 1.5, 2];

/** "Normal", "1.5×" ("1,5×" in French). */
export function speedLabel(speed: number): string {
  return speed === 1 ? t("player.speed.normal") : `${formatNumber(speed)}×`;
}
