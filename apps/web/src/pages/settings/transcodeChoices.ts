import type { HwAccel, Settings, SystemInfo, ToneMapping, TranscoderInfo } from "@/api/types";
import { t } from "@/i18n";

/**
 * What Settings → Transcoder offers on this server: each choice
 * with why it can't work here, or with the hardware acceleration chosen, as
 * the server judges when saving (settings.rs, `hwaccel_problem`,
 * `tonemap_problem` and `fit_to_accel`). A choice saved before (another
 * GPU, another FFmpeg) stays chosen and says what happens instead. The
 * words are looked up when asked for, in the language shown then.
 */
export interface Choice<T> {
  value: T;
  label: string;
  /** Why it can't work here ("needs an NVIDIA graphics card"), or `null`. */
  reason: string | null;
}

/** The hardware accelerations' names; Automatic and software are said in the language shown. */
const ACCEL_BRAND: Record<Exclude<HwAccel, "auto" | "none">, string> = {
  nvenc: "NVIDIA NVENC",
  qsv: "Intel Quick Sync",
  amf: "AMD AMF",
  vaapi: "VAAPI",
  videotoolbox: "VideoToolbox",
  vulkan: "Vulkan",
};

function accelName(accel: HwAccel): string {
  if (accel === "auto") return t("choices.automatic", { ns: "settings" });
  if (accel === "none") return t("choices.softwareOnly", { ns: "settings" });
  return ACCEL_BRAND[accel];
}

/** The accelerations that need a GPU or an FFmpeg of their own. */
const ACCEL_NEEDS = ["nvenc", "qsv", "amf", "vaapi", "videotoolbox"] as const;
type NeedyAccel = (typeof ACCEL_NEEDS)[number];
const needsHardware = (accel: HwAccel): accel is NeedyAccel => (ACCEL_NEEDS as readonly HwAccel[]).includes(accel);

/** The menu's order; Vulkan only when saved (it is set through the API). */
const ACCELS: HwAccel[] = ["auto", "nvenc", "qsv", "amf", "vaapi", "videotoolbox", "none"];

const TONEMAP_SHORT: Partial<Record<ToneMapping, string>> = { libplacebo: "libplacebo", opencl: "OpenCL", zscale: "zscale" };

function tonemapName(mode: ToneMapping): string {
  if (mode === "auto") return t("choices.automatic", { ns: "settings" });
  if (mode === "none") return t("choices.disabled", { ns: "settings" });
  return TONEMAP_SHORT[mode] ?? mode;
}

const TONEMAPS: ToneMapping[] = ["auto", "libplacebo", "opencl", "zscale", "none"];

export const capitalize = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

/** The transcoder, when it runs and its FFmpeg says what it can do; `null` while nobody knows. */
export function knownTranscoder(system: SystemInfo | undefined): TranscoderInfo | null {
  const tc = system?.transcoder;
  // An FFmpeg that encodes nothing isn't there: what the GPU could do is unknown.
  return tc?.available && Array.isArray(tc.accels) && tc.encoders.length > 0 ? tc : null;
}

/** The transcoder's operating system: pley's when pley starts it, unknown when it runs elsewhere. */
export function transcoderOs(system: SystemInfo | undefined): string | null {
  return system?.os && system.transcoder.managed !== false ? system.os : null;
}

/** Why `accel` can never work on `os`, or `null`. */
function osReason(accel: HwAccel, os: string | null): string | null {
  if (!os) return null;
  if (accel === "vaapi" && os !== "linux") return t("choices.linuxOnly", { ns: "settings" });
  if (accel === "videotoolbox" && os !== "macos") return t("choices.macosOnly", { ns: "settings" });
  if ((accel === "nvenc" || accel === "qsv" || accel === "amf") && os === "macos") return t("choices.notOnMacos", { ns: "settings" });
  return null;
}

/** Why `accel` can't work on this server, or `null` (as far as anyone knows). */
export function accelReason(accel: HwAccel, system: SystemInfo | undefined): string | null {
  if (accel === "auto" || accel === "none" || accel === "vulkan") return null;
  const os = osReason(accel, transcoderOs(system));
  if (os) return os;
  const tc = knownTranscoder(system);
  return tc && !tc.accels.includes(accel) && needsHardware(accel) ? t(`choices.accelNeeds.${accel}`, { ns: "settings" }) : null;
}

/**
 * The hardware accelerations to offer: those this system never has are left
 * out (VideoToolbox on Windows), unless saved; those it lacks the GPU or
 * FFmpeg for are offered with the reason.
 */
export function accelChoices(saved: HwAccel, system: SystemInfo | undefined): Choice<HwAccel>[] {
  const tc = knownTranscoder(system);
  const os = transcoderOs(system);
  const values = ACCELS.includes(saved) ? ACCELS : [...ACCELS.slice(0, -1), saved, "none" as const];
  return values
    .filter((value) => value === saved || !osReason(value, os))
    .map((value) => {
      const reason = accelReason(value, system);
      let label = accelName(value);
      if (value === "auto" && tc) {
        const first = tc.accels[0];
        label = t("choices.automaticWith", { ns: "settings", name: first ? accelName(first) : t("choices.softwareEncoding", { ns: "settings" }) });
      }
      return { value, label, reason };
    });
}

/** The hardware acceleration transcodes get with `accel` chosen: `null` for software or unknown. */
export function accelInUse(accel: HwAccel, system: SystemInfo | undefined): HwAccel | null {
  const tc = knownTranscoder(system);
  if (!tc || accel === "none") return null;
  if (accel === "auto") return tc.accels[0] ?? null;
  return accelReason(accel, system) ? null : accel;
}

/** A choice's text in its menu: with why it is greyed out, when it is. */
export const choiceLabel = (choice: Choice<unknown>) => (choice.reason ? `${choice.label} (${choice.reason})` : choice.label);

/** The tone mappers to offer with `accel` chosen, each with why it can't work here or with it. */
export function tonemapChoices(system: SystemInfo | undefined, accel: HwAccel): Choice<ToneMapping>[] {
  const tc = knownTranscoder(system);
  return TONEMAPS.map((value) => {
    const reason = tonemapReason(value, system) ?? tonemapAccelReason(value, accel, system);
    let label = tonemapName(value);
    if (value === "auto" && tc) {
      const first = tc.tonemapping[0] as ToneMapping | undefined;
      label = t("choices.automaticWith", { ns: "settings", name: first ? (TONEMAP_SHORT[first] ?? first) : t("choices.noneAvailable", { ns: "settings" }) });
    }
    return { value, label, reason };
  });
}

/** Why the tone mapper `mode` names can't work here, or `null`. */
export function tonemapReason(mode: ToneMapping, system: SystemInfo | undefined): string | null {
  const tc = knownTranscoder(system);
  if (!tc || mode === "auto" || mode === "none") return null;
  return tc.tonemapping.includes(mode) ? null : t(`choices.tonemapNeeds.${mode}`, { ns: "settings" });
}

/**
 * Whether the tone mapper `mode` goes with `accel`: Vulkan encodes on the
 * GPU only what libplacebo tone maps there (the transcoder's Vulkan
 * pipeline), and the rest in software. The others tone map before their
 * encoder, with any tone mapper.
 */
export const tonemapGoesWith = (accel: HwAccel, mode: ToneMapping) => accel !== "vulkan" || mode === "auto" || mode === "libplacebo";

/**
 * Why the tone mapper `mode` doesn't go with `accel`, or `null`: Vulkan's
 * rule, and the tone mappers whose frames didn't reach a VAAPI encoder in
 * the transcoder's test (it tone maps with another then).
 */
export function tonemapAccelReason(mode: ToneMapping, accel: HwAccel, system?: SystemInfo): string | null {
  if (!tonemapGoesWith(accel, mode)) return t("choices.notWith", { ns: "settings", name: accelName(accel) });
  const works = knownTranscoder(system)?.with_accel?.[accel]?.tonemappers;
  if (!works || mode === "auto" || mode === "none" || works.includes(mode)) return null;
  return t("choices.notWith", { ns: "settings", name: accelName(accel) });
}

/**
 * Why hardware decoding changes nothing with `accel` chosen, or `null`: as
 * the transcoder says each choice decodes here (a software encoder still
 * gets CUDA's decoder on an NVIDIA computer). Vulkan's are its own: it
 * decodes on the GPU, so hardware decoding is on with it.
 */
export function decodingReason(accel: HwAccel, system: SystemInfo | undefined): string | null {
  const tc = knownTranscoder(system);
  if (!tc || accel === "vulkan") return null;
  if (!tc.hardware_decoding) return t("choices.noGpuDecoder", { ns: "settings" });
  if (tc.with_accel?.[accel]?.hardware_decoding !== false) return null;
  const encodes = accelInUse(accel, system);
  return t("choices.notWith", { ns: "settings", name: encodes ? accelName(encodes) : t("choices.softwareEncoding", { ns: "settings" }) });
}

/** What follows from choosing `accel`, for what `form` holds that doesn't go with it. */
export interface AccelFit {
  changes: Partial<Pick<Settings, "tonemapping" | "hardware_decoding">>;
  /** What changed and why, by setting. */
  notes: { tonemapping?: string; hardware_decoding?: string };
}

/**
 * What `form` changes to with `accel` chosen, as the server does when it
 * saves (`fit_to_accel`): a tone mapper that doesn't go with it gives way to
 * Automatic, and hardware decoding is turned on for Vulkan.
 */
export function fitToAccel(form: Pick<Settings, "tonemapping" | "hardware_decoding">, accel: HwAccel, system?: SystemInfo): AccelFit {
  const fit: AccelFit = { changes: {}, notes: {} };
  if (!tonemapGoesWith(accel, form.tonemapping)) {
    fit.changes.tonemapping = "auto";
    fit.notes.tonemapping = t("choices.fitLibplaceboOnly", { ns: "settings", accel: accelName(accel) });
  } else if (tonemapAccelReason(form.tonemapping, accel, system)) {
    fit.changes.tonemapping = "auto";
    fit.notes.tonemapping = t("choices.fitUnreached", { ns: "settings", tonemap: tonemapName(form.tonemapping), accel: accelName(accel) });
  }
  if (accel === "vulkan" && !form.hardware_decoding) {
    fit.changes.hardware_decoding = true;
    fit.notes.hardware_decoding = t("choices.fitDecoding", { ns: "settings", accel: accelName(accel) });
  }
  return fit;
}

/**
 * What the software encoder preset applies to with `accel` chosen, when a
 * GPU encodes: what the transcoder leaves to software with it (HEVC, or
 * HDR kept in 10-bit HEVC), and the transcodes software takes over.
 * `null` with software encoding, where it applies to everything.
 */
export function presetNote(accel: HwAccel, system: SystemInfo | undefined): string | null {
  const encodes = accelInUse(accel, system);
  if (!encodes) return null;
  const name = accelName(encodes);
  const software = knownTranscoder(system)?.with_accel?.[accel]?.software ?? [];
  if (software.includes("hevc")) return t("choices.presetHevc", { ns: "settings", name });
  if (software.includes("hdr")) return t("choices.presetHdr", { ns: "settings", name });
  return t("choices.presetSoftware", { ns: "settings", name });
}

/**
 * Why transcodes' segments can't be kept in memory here, or `null`: as the
 * transcoder says (nobody knows while it isn't running), and only
 * throttled transcodes are, since their lead on the player is bounded.
 */
export function memoryReason(system: SystemInfo | undefined, throttle: boolean): string | null {
  const tc = system?.transcoder;
  if (tc?.available) {
    if (!tc.memory) return t("choices.newerTranscoder", { ns: "settings" });
    if (!tc.memory.available) return tc.memory.reason ?? t("choices.notAvailableHere", { ns: "settings" });
  }
  return !throttle || knownTranscoder(system)?.throttling === false ? t("choices.needsThrottling", { ns: "settings" }) : null;
}

/** What a saved tone mapper that can't work here gives instead: Automatic's choice. */
export function tonemapFallback(system: SystemInfo | undefined): string {
  const first = knownTranscoder(system)?.tonemapping[0] as ToneMapping | undefined;
  return first
    ? t("choices.tonemappedWith", { ns: "settings", name: TONEMAP_SHORT[first] ?? first })
    : t("choices.noTonemapper", { ns: "settings" });
}
