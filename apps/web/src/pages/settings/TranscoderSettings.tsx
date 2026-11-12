import { useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { CircleAlert, FolderOpen } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useSaveTranscodeDir, useSystemInfo } from "@/api/hooks";
import type { DolbyVisionFel, HwAccel, Settings, ToneMapping, TranscodeDir, TranscodePreset } from "@/api/types";
import { FolderPicker } from "@/components/FolderPicker";
import { useToast } from "@/components/Toast";
import { Badge, Button, Select, TextInput, Toggle } from "@/components/ui";
import { formatBytes } from "@/lib/format";
import { useDocumentTitle } from "@/lib/hooks";
import { Group, Link, Note, Row, SettingsForm, TestButton, useSettingsForm } from "./SettingsForm";
import {
  type AccelFit,
  accelChoices,
  accelReason,
  capitalize,
  choiceLabel,
  decodingReason,
  fitToAccel,
  knownTranscoder,
  memoryReason,
  presetNote,
  tonemapAccelReason,
  tonemapChoices,
  tonemapFallback,
  tonemapReason,
  transcoderOs,
} from "./transcodeChoices";

/** The settings this page edits. */
const KEYS = [
  "hwaccel",
  "hardware_decoding",
  "tonemapping",
  "dolby_vision_fel",
  "dolby_vision_fel_full",
  "transcode_preset",
  "transcode_video",
  "max_video_transcodes",
  "max_transcodes",
  "throttle_transcodes",
  "transcode_in_memory",
] as const satisfies readonly (keyof Settings)[];

/** x264's and x265's presets, by their own names; Automatic is Pley's. */
const PRESETS: TranscodePreset[] = ["auto", "ultrafast", "superfast", "veryfast", "faster", "fast", "medium"];

const GYAN_BUILDS = "https://www.gyan.dev/ffmpeg/builds/";
const JELLYFIN_FFMPEG = "https://github.com/jellyfin/jellyfin-ffmpeg/releases";

/** FFmpeg builds with libplacebo for the transcoder's system (every one's, when it runs elsewhere). */
function DolbyVisionBuilds({ os }: { os: string | null }) {
  const { t } = useTranslation("settings");
  const links = { gyan: <Link href={GYAN_BUILDS} />, jellyfin: <Link href={JELLYFIN_FFMPEG} /> };
  if (os === "windows") return <Trans t={t} i18nKey="transcoder.video.builds.windows" components={links} />;
  if (os === "linux") return <Link href={JELLYFIN_FFMPEG}>jellyfin-ffmpeg</Link>;
  if (os === "macos") return <>{t("transcoder.video.builds.macos")}</>;
  return <Trans t={t} i18nKey="transcoder.video.builds.any" components={links} />;
}

/**
 * Where the transcoder works. Saved in pley.toml for pley's next start: the
 * transcoder can't move what it is making while it runs.
 */
function TranscodeFolder({ dir }: { dir: TranscodeDir }) {
  const { t } = useTranslation(["settings", "common"]);
  const save = useSaveTranscodeDir();
  const toast = useToast();
  const [picking, setPicking] = useState(false);
  // The folder pley's next start tries: the one saved, even if it couldn't be used this time.
  const chosen = dir.next ?? dir.problem?.folder ?? dir.in_use;
  const choose = (path: string | null) =>
    save.mutate(path, {
      onSuccess: (saved) => toast.success(saved.next ? t("transcoder.folder.savedNextStart") : t("common:states.saved")),
      onError: (e) => toast.error(errorMessage(e)),
    });

  return (
    <Row label={t("transcoder.folder.label")} hint={t("transcoder.folder.hint")}>
      <div className="space-y-2">
        <div className="break-all font-mono text-xs text-fg/80">{dir.in_use}</div>
        {dir.problem ? (
          <p className="flex items-start gap-1.5 text-xs text-danger">
            <CircleAlert className="mt-px size-3.5 shrink-0" />
            <span>
              <Trans
                t={t}
                i18nKey="transcoder.folder.problem"
                values={{ folder: dir.problem.folder, reason: dir.problem.reason }}
                components={{ folder: <span className="break-all font-mono" /> }}
              />
            </span>
          </p>
        ) : null}
        {dir.next ? (
          <p className="text-xs text-accent">
            <Trans t={t} i18nKey="transcoder.folder.next" values={{ folder: dir.next }} components={{ folder: <span className="break-all font-mono" /> }} />
          </p>
        ) : null}
        {dir.fixed ? (
          <p className="text-xs text-subtle">{t("transcoder.folder.fixed")}</p>
        ) : (
          <div className="flex flex-wrap gap-2">
            <Button
              size="sm"
              variant="outline"
              icon={<FolderOpen className="size-4" />}
              loading={save.isPending}
              onClick={() => setPicking(true)}
            >
              {t("transcoder.folder.change")}
            </Button>
            {chosen !== dir.default ? (
              <Button size="sm" variant="ghost" disabled={save.isPending} onClick={() => choose(null)}>
                {t("transcoder.folder.useDefault")}
              </Button>
            ) : null}
          </div>
        )}
      </div>
      <FolderPicker
        open={picking}
        onClose={() => setPicking(false)}
        initialPath={chosen}
        onSelect={(path) => {
          setPicking(false);
          choose(path);
        }}
      />
    </Row>
  );
}

/** The most streams at once a server takes for a limit. */
const MOST = 64;

/** A limit as typed: a whole number from 1 to `most`. */
function limit(typed: string, most: number): number {
  return Math.min(Math.max(1, Math.floor(Number(typed)) || 1), most);
}

export function TranscoderSettings() {
  const { t } = useTranslation("settings");
  useDocumentTitle(t("transcoder.documentTitle"));
  const draft = useSettingsForm(KEYS);
  const system = useSystemInfo();
  const saved = draft.settings.data;
  const set = draft.set;
  // What changed with the last hardware acceleration chosen, until changed again.
  const [fitted, setFitted] = useState<AccelFit["notes"]>({});
  const chooseAccel = (form: Settings, accel: HwAccel) => {
    const fit = fitToAccel(form, accel, system.data);
    draft.setForm((f) => (f ? { ...f, hwaccel: accel, ...fit.changes } : f));
    setFitted(fit.notes);
  };

  return (
    <SettingsForm draft={draft} title={t("transcoder.title")} description={t("transcoder.description")}>
      {(form) => {
        const tc = system.data?.transcoder;
        // What the transcoder's FFmpeg can do, once it says (nothing is greyed out before).
        const known = knownTranscoder(system.data);
        const os = transcoderOs(system.data);
        const accels = accelChoices(saved?.hwaccel ?? form.hwaccel, system.data);
        const accelProblem = accelReason(form.hwaccel, system.data);
        const preset = presetNote(form.hwaccel, system.data);
        const tonemaps = tonemapChoices(system.data, form.hwaccel);
        const tonemapProblem = tonemapReason(form.tonemapping, system.data);
        // A combination saved before: the transcoder doesn't use Vulkan then.
        const tonemapClash = tonemapProblem ? null : tonemapAccelReason(form.tonemapping, form.hwaccel, system.data);
        const decodingProblem = decodingReason(form.hwaccel, system.data);
        // Vulkan decodes on the GPU: hardware decoding turns on with it, not off.
        const vulkanDecodes = form.hwaccel === "vulkan";
        // Shown while the changes wait to be saved.
        const fittedNote = (key: keyof AccelFit["notes"]) => (draft.dirty ? fitted[key] : undefined);
        const noThrottling = known?.throttling === false;
        const memoryProblem = memoryReason(system.data, form.throttle_transcodes);
        const fel = form.dolby_vision_fel ?? "base_layer";
        // A transcoder from before the choice tone maps by the metadata, whatever is saved.
        const felTooOld = tc?.available === true && tc.dolby_vision_fel === false;
        // Only FFmpeg builds that can rebuild the full picture offer it (Pley's own).
        const felFullAvailable = tc?.available === true && tc.dolby_vision_fel_full === true;
        const memoryBudget = tc?.memory?.available ? tc.memory.budget_bytes : 0;

        return (
          <>
            <Group
              title={t("transcoder.title")}
              description={
                tc
                  ? tc.available
                    ? t("transcoder.running", {
                        version: tc.version ?? "",
                        ffmpeg: tc.ffmpeg_version ?? "?",
                        encoder: tc.active_encoder ?? t("transcoder.noEncoder"),
                      })
                    : tc.error
                      ? t("transcoder.unavailableBecause", { error: tc.error })
                      : t("transcoder.unavailable")
                  : undefined
              }
            >
              <Row label={t("transcoder.connection")} hint={tc?.managed === false ? t("transcoder.external", { url: tc.url ?? "?" }) : t("transcoder.managed")}>
                <TestButton label={t("transcoder.test")} test={{ service: "transcoder" }} className="" />
              </Row>
              {/* An external transcoder keeps a folder of its own. */}
              {system.data?.transcode_dir && tc?.managed !== false ? <TranscodeFolder dir={system.data.transcode_dir} /> : null}
            </Group>

            <Group title={t("transcoder.video.title")} description={t("transcoder.video.description")}>
              <Row
                label={t("transcoder.video.accel")}
                htmlFor="hwaccel"
                hint={tc && tc.encoders.length ? t("transcoder.video.detected", { encoders: tc.encoders.join(", ") }) : t("transcoder.video.accelHint")}
              >
                <Select id="hwaccel" value={form.hwaccel} onChange={(e) => chooseAccel(form, e.target.value as HwAccel)} className="w-full">
                  {accels.map((o) => (
                    <option key={o.value} value={o.value} disabled={o.reason !== null}>
                      {choiceLabel(o)}
                    </option>
                  ))}
                </Select>
                {accelProblem ? <Note notice>{t("transcoder.video.softwareInstead", { reason: capitalize(accelProblem) })}</Note> : null}
              </Row>
              <Row label={t("transcoder.video.decoding")} hint={t("transcoder.video.decodingHint")}>
                <Toggle
                  checked={form.hardware_decoding}
                  disabled={decodingProblem !== null || (vulkanDecodes && form.hardware_decoding)}
                  onChange={(v) => {
                    set("hardware_decoding", v);
                    setFitted((f) => ({ ...f, hardware_decoding: undefined }));
                  }}
                  label={t("transcoder.video.decoding")}
                />
                {decodingProblem ? <Note>{t("transcoder.video.decodedOnCpu", { reason: capitalize(decodingProblem) })}</Note> : null}
                {fittedNote("hardware_decoding") ? (
                  <Note>{fittedNote("hardware_decoding")}</Note>
                ) : vulkanDecodes ? (
                  form.hardware_decoding ? (
                    <Note>{t("transcoder.video.vulkanAlwaysOn")}</Note>
                  ) : (
                    <Note notice>{t("transcoder.video.vulkanNeedsIt")}</Note>
                  )
                ) : null}
              </Row>
              <Row label={t("transcoder.video.preset")} htmlFor="preset" hint={t("transcoder.video.presetHint")}>
                <Select
                  id="preset"
                  value={form.transcode_preset}
                  onChange={(e) => set("transcode_preset", e.target.value as TranscodePreset)}
                  className="w-full"
                >
                  {PRESETS.map((p) => (
                    <option key={p} value={p}>
                      {p === "auto" ? t("transcoder.video.presetAuto") : p}
                    </option>
                  ))}
                </Select>
                {preset ? <Note>{preset}</Note> : null}
              </Row>
              <Row
                label={t("transcoder.video.tonemap")}
                htmlFor="tonemap"
                hint={
                  <>
                    {tc && tc.tonemapping.length
                      ? t(tc.dolby_vision ? "transcoder.video.tonemapAvailableDv" : "transcoder.video.tonemapAvailable", { list: tc.tonemapping.join(", ") })
                      : t("transcoder.video.tonemapHint")}
                    {tc?.available && !tc.dolby_vision ? (
                      <span className="mt-1 block">
                        <Trans t={t} i18nKey="transcoder.video.dolbyVisionNeeds" components={{ builds: <DolbyVisionBuilds os={os} /> }} />
                      </span>
                    ) : null}
                  </>
                }
              >
                <Select
                  id="tonemap"
                  value={form.tonemapping}
                  onChange={(e) => {
                    set("tonemapping", e.target.value as ToneMapping);
                    setFitted((f) => ({ ...f, tonemapping: undefined }));
                  }}
                  className="w-full"
                >
                  {tonemaps.map((o) => (
                    <option key={o.value} value={o.value} disabled={o.reason !== null}>
                      {choiceLabel(o)}
                    </option>
                  ))}
                </Select>
                {tonemapProblem ? (
                  <Note notice>{t("transcoder.video.tonemapInstead", { reason: capitalize(tonemapProblem), fallback: tonemapFallback(system.data) })}</Note>
                ) : tonemapClash ? (
                  <Note notice>
                    {t(form.hwaccel === "vulkan" ? "transcoder.video.softwareInstead" : "transcoder.video.tonemapWithAnother", { reason: capitalize(tonemapClash) })}
                  </Note>
                ) : fittedNote("tonemapping") ? (
                  <Note>{fittedNote("tonemapping")}</Note>
                ) : null}
              </Row>
              <Row label={t("transcoder.video.fel")} htmlFor="dolby-vision-fel" hint={t("transcoder.video.felHint")}>
                <Select
                  id="dolby-vision-fel"
                  value={fel}
                  disabled={felTooOld}
                  onChange={(e) => set("dolby_vision_fel", e.target.value as DolbyVisionFel)}
                  className="w-full"
                >
                  <option value="base_layer">{t("transcoder.video.felBaseLayer")}</option>
                  <option value="metadata">{t("transcoder.video.felMetadata")}</option>
                </Select>
                {felTooOld ? (
                  <Note notice>{t("transcoder.video.felOlderTranscoder")}</Note>
                ) : (
                  <Note>{t(fel === "metadata" ? "transcoder.video.felMetadataNote" : "transcoder.video.felBaseLayerNote")}</Note>
                )}
              </Row>
              <Row
                label={
                  <span className="inline-flex flex-wrap items-center gap-2">
                    {t("transcoder.video.felFull")}
                    <Badge tone="warning">{t("transcoder.video.experimental")}</Badge>
                  </span>
                }
                hint={t("transcoder.video.felFullHint")}
              >
                <Toggle
                  checked={(form.dolby_vision_fel_full ?? false) && felFullAvailable}
                  disabled={!felFullAvailable}
                  onChange={(v) => set("dolby_vision_fel_full", v)}
                  label={t("transcoder.video.felFull")}
                />
                {!felFullAvailable ? <Note>{t("transcoder.video.felFullUnavailable")}</Note> : null}
              </Row>
            </Group>

            <Group title={t("transcoder.load.title")} description={t("transcoder.load.description")}>
              <Row label={t("transcoder.load.allowVideo")} hint={t("transcoder.load.allowVideoHint")}>
                <Toggle checked={form.transcode_video} onChange={(v) => set("transcode_video", v)} label={t("transcoder.load.allowVideo")} />
              </Row>
              <Row label={t("transcoder.load.maxVideo")} htmlFor="max-video-transcodes" hint={t("transcoder.load.maxVideoHint")}>
                <TextInput
                  id="max-video-transcodes"
                  type="number"
                  min={1}
                  max={form.max_transcodes}
                  disabled={!form.transcode_video}
                  value={form.max_video_transcodes}
                  onChange={(e) => set("max_video_transcodes", limit(e.target.value, form.max_transcodes))}
                  className="w-28"
                />
              </Row>
              <Row
                // "total" stands out: it is what tells this limit from the one above it.
                label={<Trans t={t} i18nKey="transcoder.load.max" components={{ total: <strong className="font-bold" /> }} />}
                htmlFor="max-transcodes"
                hint={t("transcoder.load.maxHint")}
              >
                <TextInput
                  id="max-transcodes"
                  type="number"
                  min={1}
                  max={MOST}
                  value={form.max_transcodes}
                  onChange={(e) => set("max_transcodes", limit(e.target.value, MOST))}
                  // Once the total is typed: video transcodes are some of the streams, never more than all of them.
                  onBlur={() => draft.setForm((f) => (f ? { ...f, max_video_transcodes: Math.min(f.max_video_transcodes, f.max_transcodes) } : f))}
                  className="w-28"
                />
                {tc?.available && tc.max_jobs > 0 && form.max_transcodes > tc.max_jobs ? (
                  <Note>{t(tc.managed ? "transcoder.load.maxJobsManaged" : "transcoder.load.maxJobsExternal", { max: tc.max_jobs })}</Note>
                ) : null}
              </Row>
              <Row label={t("transcoder.load.throttle")} hint={t("transcoder.load.throttleHint")}>
                <Toggle
                  checked={form.throttle_transcodes}
                  disabled={noThrottling}
                  onChange={(v) => set("throttle_transcodes", v)}
                  label={t("transcoder.load.throttle")}
                />
                {noThrottling ? <Note>{t("transcoder.load.noThrottling")}</Note> : null}
              </Row>
              <Row
                label={t("transcoder.load.memory")}
                hint={memoryBudget ? t("transcoder.load.memoryHintUpTo", { size: formatBytes(memoryBudget) }) : t("transcoder.load.memoryHint")}
              >
                <Toggle
                  checked={form.transcode_in_memory}
                  disabled={memoryProblem !== null}
                  onChange={(v) => set("transcode_in_memory", v)}
                  label={t("transcoder.load.memory")}
                />
                {memoryProblem ? <Note>{t("transcoder.load.notInMemory", { reason: capitalize(memoryProblem) })}</Note> : null}
              </Row>
            </Group>
          </>
        );
      }}
    </SettingsForm>
  );
}
