import type { ComponentChildren } from "preact";
import { useEffect, useRef, useState } from "preact/hooks";
import { errorMessage, isApiError } from "../../api/client";
import type { SubtitleResult } from "../../api/types";
import { channelsLabel, codecLabel, formatBitrate, formatClock, languageName } from "../../format/format";
import { t } from "../../i18n";
import { Cancel } from "../../net/transport";
import { mediaFacts, streamBlock, type FactBlock } from "../../format/media-info";
import type { PlaybackController, PlayerState } from "../../player/controller";
import type { EngineStats } from "../../player/engine";
import { SLEEP_CHOICES, sameChoice, sleepLabel } from "../../player/sleep";
import { speedLabel } from "../../player/speed";
import { formatOffset, SYNC_STEP_MS } from "../../subs/sync";
import { PanelRow } from "../kit/buttons";
import { F } from "../kit/focusable";
import { Icon } from "../kit/icons";
import { PanelTitle, SidePanel } from "../kit/panel";
import { useApp } from "../screens/common";
import { LANGUAGES } from "../screens/settings";

export type PlayerPanel = "subtitles" | "find" | "audio" | "quality" | "chapters" | "speed" | "sleep" | "info";

/** Subtitle search on this server, as last heard: not set up, or a server older than it. */
let searchUnavailable: "setup" | "older" | null = null;

/** Forgets what the server said of subtitle search (tests). */
export function forgetSubtitleSearch(): void {
  searchUnavailable = null;
}

/** Why the server can't search for subtitles: not set up (503 with `setup: false`), or older than search (404). */
function unavailableOf(e: unknown): "setup" | "older" | null {
  if (isApiError(e, 404)) return "older";
  if (isApiError(e, 503) && (e.body as { setup?: unknown } | undefined)?.setup === false) return "setup";
  return null;
}

/** A chapter's name: its title, or "Chapter N" (its title after it, unless it is one already). */
export function chapterName(title: string | null, n: number): string {
  if (!title) return t("player.chapters.numbered", { number: n });
  return /^chapter/i.test(title) ? title : t("player.chapters.numberedTitle", { number: n, title });
}

/** The player's side panels (TvPlayerPanels): pick a row, it applies and closes; Back closes. */
export function PlayerPanels(p: {
  panel: PlayerPanel;
  state: PlayerState;
  controller: PlaybackController;
  positionMs: number;
  onClose: () => void;
  /** Opens another panel in this one's place (Find subtitles…). */
  onOpen?: (panel: PlayerPanel) => void;
  stats: () => EngineStats | null;
  engineName: string;
}) {
  const s = p.state;
  const c = p.controller;
  const me = useApp().me.value;
  const pick = (fn: () => void) => () => {
    fn();
    p.onClose();
  };
  switch (p.panel) {
    case "subtitles": {
      // Search the server can't do: not offered where it never will, said for an administrator who can set it up.
      const find = !!p.onOpen && searchUnavailable !== "older" && (searchUnavailable !== "setup" || !!me?.is_admin);
      return (
        <SidePanel onClose={p.onClose} label={t("player.controls.subtitles")}>
          <PanelTitle>{t("player.controls.subtitles")}</PanelTitle>
          {s.subtitleSync && <SyncRow offsetMs={s.subtitleOffsetMs} onChange={(ms) => c.setSubtitleOffset(ms)} />}
          {s.subtitles.map((track) => (
            <PanelRow
              key={String(track.index)}
              label={track.label}
              sub={track.added ? t("player.subtitles.added") : null}
              checked={track.selected}
              initial={track.selected}
              onSelect={pick(() => c.selectSubtitle(track.index))}
            />
          ))}
          {find && (
            <PanelRow
              fid="find-subtitles"
              icon="search"
              label={t("player.subtitles.find")}
              sub={searchUnavailable === "setup" ? t("player.subtitles.searchNotSetUp") : null}
              initial={!s.subtitles.length}
              onSelect={() => p.onOpen?.("find")}
            />
          )}
        </SidePanel>
      );
    }
    case "find":
      return <FindPanel controller={c} language={me?.subtitle_language ?? null} isAdmin={!!me?.is_admin} onClose={p.onClose} />;
    case "audio":
      return (
        <SidePanel onClose={p.onClose} label={t("player.controls.audio")}>
          <PanelTitle>{t("player.controls.audio")}</PanelTitle>
          {s.audio.map((track) => (
            <PanelRow
              key={String(track.index)}
              label={track.label}
              checked={track.selected}
              initial={track.selected}
              onSelect={pick(() => track.index !== null && c.selectAudio(track.index))}
            />
          ))}
        </SidePanel>
      );
    case "quality":
      return (
        <SidePanel onClose={p.onClose} label={t("player.controls.quality")}>
          <PanelTitle>{t("player.controls.quality")}</PanelTitle>
          {s.note && <div class="t-body c-muted panel-note">{s.note}</div>}
          {s.canPlayOriginal && (
            <PanelRow label={t("player.quality.playOriginal")} sub={t("player.quality.tryOriginal")} icon="rotate-ccw" onSelect={pick(() => c.playOriginal())} />
          )}
          {s.qualities.map((q) => (
            <PanelRow key={q.id} label={q.label} sub={q.hint} checked={q.selected} initial={q.selected} onSelect={pick(() => c.selectQuality(q.id))} />
          ))}
        </SidePanel>
      );
    case "chapters": {
      const at = s.chapters.reduce((found, ch, i) => (ch.startMs <= p.positionMs ? i : found), -1);
      return (
        <SidePanel onClose={p.onClose} label={t("player.controls.chapters")}>
          <PanelTitle>{t("player.controls.chapters")}</PanelTitle>
          {s.chapters.map((ch, i) => (
            <PanelRow
              key={ch.startMs}
              label={ch.title || t("player.chapters.numbered", { number: i + 1 })}
              sub={formatClock(ch.startMs)}
              checked={i === at}
              initial={i === Math.max(0, at)}
              onSelect={pick(() => c.seekTo(ch.startMs))}
            />
          ))}
        </SidePanel>
      );
    }
    case "speed":
      return (
        <SidePanel onClose={p.onClose} label={t("player.controls.speed")}>
          <PanelTitle>{t("player.controls.speed")}</PanelTitle>
          {s.speeds.map((rate) => (
            <PanelRow key={String(rate)} label={speedLabel(rate)} checked={rate === s.speed} initial={rate === s.speed} onSelect={pick(() => c.setSpeed(rate))} />
          ))}
        </SidePanel>
      );
    case "sleep":
      return (
        <SidePanel onClose={p.onClose} label={t("player.controls.sleep")}>
          <PanelTitle>{t("player.controls.sleep")}</PanelTitle>
          {SLEEP_CHOICES.map((choice) => {
            const on = sameChoice(choice, s.sleep.choice);
            return (
              <PanelRow
                key={choice.kind === "minutes" ? choice.minutes : choice.kind}
                label={sleepLabel(choice, s.kind)}
                sub={on && choice.kind === "minutes" && s.sleep.minutesLeft !== null ? t("player.sleep.minutesLeft", { count: s.sleep.minutesLeft }) : null}
                checked={on}
                initial={on}
                onSelect={pick(() => c.setSleep(choice))}
              />
            );
          })}
        </SidePanel>
      );
    case "info":
      return <InfoPanel state={s} controller={c} onClose={p.onClose} stats={p.stats} engineName={p.engineName} />;
  }
}

type Found = { kind: "results"; results: SubtitleResult[] } | { kind: "setup" } | { kind: "older" } | { kind: "error"; message: string };

/**
 * Find subtitles… (the web's): what OpenSubtitles has for the file, in the
 * profile's language or another picked here, those made for this very file
 * first; one picked is added on the server for everyone, and shows.
 */
function FindPanel(p: { controller: PlaybackController; language: string | null; isAdmin: boolean; onClose: () => void }) {
  const [language, setLanguage] = useState(p.language || "eng");
  const [choosing, setChoosing] = useState(false);
  const [answer, setAnswer] = useState<{ language: string; found: Found } | null>(null);
  const [adding, setAdding] = useState<string | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  useEffect(() => {
    const cancel = new Cancel();
    p.controller.findSubtitles(language, cancel).then(
      (results) => setAnswer({ language, found: { kind: "results", results } }),
      (e: unknown) => {
        if (cancel.cancelled) return;
        const why = unavailableOf(e);
        if (why) searchUnavailable = why;
        setAnswer({ language, found: why ? { kind: why } : { kind: "error", message: errorMessage(e) } });
      },
    );
    return () => cancel.cancel();
  }, [language]);

  if (choosing) {
    return (
      <SidePanel key="languages" onClose={() => setChoosing(false)} label={t("player.find.language")}>
        <PanelTitle>{t("player.find.language")}</PanelTitle>
        {LANGUAGES.map(([code, name]) => (
          <PanelRow
            key={code}
            label={name}
            checked={code === language}
            initial={code === language}
            onSelect={() => {
              setLanguage(code);
              setChoosing(false);
            }}
          />
        ))}
      </SidePanel>
    );
  }
  const add = (r: SubtitleResult) => {
    if (adding) return;
    setAdding(r.id);
    setFailed(null);
    p.controller.addFoundSubtitle(r).then(
      () => p.onClose(),
      (e: unknown) => {
        setAdding(null);
        setFailed(t("player.find.addFailed", { error: errorMessage(e) }));
      },
    );
  };
  const found = answer && answer.language === language ? answer.found : null;
  const name = languageName(language) ?? language;
  return (
    <SidePanel key="results" onClose={p.onClose} label={t("player.find.title")}>
      <PanelTitle>{t("player.find.title")}</PanelTitle>
      {found?.kind === "setup" || found?.kind === "older" ? (
        <div class="t-body c-muted panel-note">
          {found.kind === "older" ? t("player.find.needsUpdate") : p.isAdmin ? t("player.find.notSetUpAdmin") : t("player.find.notSetUp")}
        </div>
      ) : (
        <>
          <PanelRow
            fid="find-language"
            icon="captions"
            label={t("player.find.languageRow")}
            sub={name}
            initial={!found || found.kind !== "results" || !found.results.length}
            onSelect={() => setChoosing(true)}
          />
          {failed && <div class="t-body c-muted panel-note">{failed}</div>}
          {!found ? (
            <div class="t-body c-muted panel-note">{t("player.find.searching")}</div>
          ) : found.kind === "error" ? (
            <div class="t-body c-muted panel-note">{t("player.find.searchFailed", { error: found.message })}</div>
          ) : !found.results.length ? (
            <div class="t-body c-muted panel-note">{t("player.find.noResults", { language: name })}</div>
          ) : (
            found.results.map((r, i) => (
              <PanelRow
                key={r.id}
                label={r.name}
                sub={
                  adding === r.id
                    ? t("player.find.adding")
                    : [
                        r.matched === "hash" ? t("player.find.madeForFile") : null,
                        languageName(r.language),
                        r.format.toUpperCase(),
                        r.hearing_impaired ? t("player.find.sdh") : null,
                        r.forced ? t("player.find.forced") : null,
                        t("player.find.downloads", { count: r.downloads }),
                      ]
                        .filter(Boolean)
                        .join(" · ")
                }
                initial={i === 0}
                disabled={adding !== null && adding !== r.id}
                onSelect={() => add(r)}
              />
            ))
          )}
        </>
      )}
    </SidePanel>
  );
}

/** Subtitle sync: Left and Right move the subtitles 0.1 s, OK puts them back. */
function SyncRow(p: { offsetMs: number; onChange: (ms: number) => void }) {
  return (
    <F
      class="panel-row sync-row"
      label={t("player.subtitles.syncAt", { offset: formatOffset(p.offsetMs) })}
      onSelect={() => p.onChange(0)}
      onKey={(key) => {
        if (key === "left") {
          p.onChange(p.offsetMs - SYNC_STEP_MS);
          return true;
        }
        if (key === "right") {
          p.onChange(p.offsetMs + SYNC_STEP_MS);
          return true;
        }
        return false;
      }}
    >
      <div class="grow">
        <div class="t-label">{t("player.subtitles.sync")}</div>
        <div class="t-caption sub">{t("player.subtitles.okResets")}</div>
      </div>
      <Icon name="chevron-left" size={20} />
      <span class="t-label" style="padding:0 6px">
        {formatOffset(p.offsetMs)}
      </span>
      <Icon name="chevron-right" size={20} />
    </F>
  );
}

/** The readout's sections: how it plays, the file, its streams, and what this TV says. */
export function readoutSections(state: PlayerState, stats: EngineStats | null, engineName: string): FactBlock[] {
  const d = state.details;
  const out: FactBlock[] = [];
  if (!d) return out;
  const playback = { label: t("player.info.playback"), value: d.pipeline ? `${d.method} · ${d.pipeline}` : d.method };
  const how: FactBlock = { title: t("player.info.howItPlays"), facts: d.why ? [playback, { label: t("player.info.why"), value: d.why }] : [playback] };
  for (const r of d.rows) how.facts.push({ label: r.label, value: r.value });
  how.facts.push({ label: t("player.info.quality"), value: state.quality });
  out.push(how);
  if (d.file) {
    const file = mediaFacts(d.file)[0];
    if (file) out.push(file);
  }
  if (d.video) out.push(streamBlock(d.video, t("player.info.video")));
  if (d.audio) out.push(streamBlock(d.audio, t("player.info.audio")));
  const subs: FactBlock = { title: t("player.info.subtitles"), facts: [] };
  if (!d.subtitle) subs.facts.push({ label: t("player.info.track"), value: t("player.info.off") });
  else {
    const codec = codecLabel(d.subtitle.codec);
    subs.facts.push({ label: t("player.info.format"), value: d.subtitle.text_based === false ? t("player.info.formatPicture", { codec }) : t("player.info.formatText", { codec }) });
    if (d.subtitle.language) subs.facts.push({ label: t("player.info.language"), value: d.subtitle.language });
    subs.facts.push({ label: t("player.info.shown"), value: d.subtitleDelivery === "burned_in" ? t("player.info.burnedIn") : t("player.info.drawnHere") });
  }
  out.push(subs);
  const tv: FactBlock = { title: t("player.info.thisTv"), facts: [{ label: t("player.info.player"), value: engineName }] };
  if (stats) {
    const decoded = [stats.width && stats.height ? `${stats.width}×${stats.height}` : "", stats.frameRate ? `${stats.frameRate.toFixed(3).replace(/\.?0+$/, "")} fps` : "", stats.hdr ?? "", stats.codec ? codecLabel(stats.codec) : ""]
      .filter(Boolean)
      .join(" · ");
    if (decoded) tv.facts.push({ label: t("player.info.decoded"), value: decoded });
    if (stats.decoder) {
      const decoder = stats.decoder;
      tv.facts.push({ label: t("player.info.decoder"), value: stats.hardware === false ? t("player.info.software", { decoder }) : t("player.info.hardware", { decoder }) });
    }
    if (stats.audioCodec) tv.facts.push({ label: t("player.info.audio"), value: [codecLabel(stats.audioCodec), channelsLabel(stats.audioChannels ?? null)].filter(Boolean).join(" ") });
    if (stats.audioOutput) tv.facts.push({ label: t("player.info.output"), value: stats.audioOutput });
    if (stats.bitrate) tv.facts.push({ label: t("player.info.currentBitrate"), value: formatBitrate(stats.bitrate) });
    if (stats.dropped !== undefined && stats.frames) tv.facts.push({ label: t("player.info.droppedFrames"), value: t("player.info.framesOf", { dropped: stats.dropped, frames: stats.frames }) });
    if (stats.bufferedAheadMs !== undefined) tv.facts.push({ label: t("player.info.bufferedAhead"), value: `${Math.round(stats.bufferedAheadMs / 1000)} s` });
    tv.facts.push({ label: t("player.info.stalls"), value: String(state.stalls) });
    // The TV's own lines ("Renderer: …"), as it writes them.
    for (const line of stats.extra ?? []) {
      const i = line.indexOf(":");
      if (i > 0) tv.facts.push({ label: line.slice(0, i), value: line.slice(i + 1).trim() });
    }
  }
  if (state.firstFrameMs !== null) tv.facts.push({ label: t("player.info.firstFrame"), value: `${state.firstFrameMs} ms` });
  out.push(tv);
  return out;
}

function InfoPanel(p: { state: PlayerState; controller: PlaybackController; onClose: () => void; stats: () => EngineStats | null; engineName: string }) {
  const [, setTick] = useState(0);
  const [scroll, setScroll] = useState(0);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    p.controller.setDetailsOpen(true);
    const t = setInterval(() => setTick((n) => n + 1), 1000);
    return () => {
      clearInterval(t);
      p.controller.setDetailsOpen(false);
    };
  }, []);
  const sections = readoutSections(p.state, p.stats(), p.engineName);
  return (
    <SidePanel onClose={p.onClose} wide label={t("player.info.title")}>
      <F
        class="info-scroll"
        initial
        label={t("player.info.title")}
        onKey={(key) => {
          const max = Math.max(0, (box.current?.offsetHeight ?? 0) - 440);
          if (key === "down") {
            setScroll((s) => Math.min(max, s + 96));
            return true;
          }
          if (key === "up") {
            setScroll((s) => Math.max(0, s - 96));
            return true;
          }
          return key === "left" || key === "right";
        }}
      >
        <div ref={box} class="scroll-y" style={`transform:translateY(${-scroll}px)`}>
          <PanelTitle>{t("player.info.title")}</PanelTitle>
          {!sections.length ? <div class="t-body c-muted panel-note">{t("player.info.asking")}</div> : sections.map((s) => <Facts key={s.title} block={s} />)}
          <div style="height:24px" />
        </div>
      </F>
    </SidePanel>
  );
}

function Facts(p: { block: FactBlock }) {
  return (
    <div>
      <div class="t-title c-fg readout-heading">{p.block.title}</div>
      {p.block.facts.map((f) => (
        <div key={f.label} class="readout-fact">
          <div class="t-caption c-muted one-line readout-label">{f.label}</div>
          <div class="t-caption c-fg lines-3 readout-value">{f.value}</div>
        </div>
      ))}
    </div>
  );
}

/** The compact readout (Settings → Playback details) over the video while nothing else shows. */
export function CompactReadout(p: { state: PlayerState; controller: PlaybackController; stats: () => EngineStats | null }): ComponentChildren {
  const [, setTick] = useState(0);
  useEffect(() => {
    p.controller.setDetailsOpen(true);
    const t = setInterval(() => setTick((n) => n + 1), 1000);
    return () => {
      clearInterval(t);
      p.controller.setDetailsOpen(false);
    };
  }, []);
  const d = p.state.details;
  if (!d) return null;
  const stats = p.stats();
  const lines: [string, string][] = [[t("player.info.playback"), d.pipeline ? `${d.method} · ${d.pipeline}` : d.method]];
  for (const r of d.rows) lines.push([r.label, r.value]);
  const sub = d.subtitle;
  lines.push([t("player.info.subtitles"), sub ? [sub.language ?? "", codecLabel(sub.codec)].filter(Boolean).join(" · ") : t("player.info.off")]);
  // The file's bitrate, then what arrives now when that differs: "18 Mbps → 8 Mbps".
  const rates = [d.file?.bitrate ? formatBitrate(d.file.bitrate) : "", stats?.bitrate ? formatBitrate(stats.bitrate) : ""].filter((r, i, a) => r && a.indexOf(r) === i);
  if (rates.length) lines.push([t("player.details.bitrate"), rates.join(" → ")]);
  if (stats?.width && stats.height) lines.push([t("player.info.decoded"), `${stats.width}×${stats.height}${stats.hdr ? ` · ${stats.hdr}` : ""}`]);
  if (stats?.dropped !== undefined && stats.frames) lines.push([t("player.info.dropped"), t("player.info.framesOf", { dropped: stats.dropped, frames: stats.frames })]);
  if (stats?.bufferedAheadMs !== undefined) {
    const ahead = t("player.info.secondsAhead", { seconds: Math.round(stats.bufferedAheadMs / 1000) });
    lines.push([t("player.info.buffer"), stats.bandwidth ? `${ahead} · ${formatBitrate(stats.bandwidth)}` : ahead]);
  }
  return (
    <div class="compact-readout">
      {lines.map(([l, v]) => (
        <div key={l} class="readout-fact">
          <div class="t-caption c-muted one-line" style="width:96px;flex:none">
            {l}
          </div>
          <div class="t-caption c-fg one-line">{v}</div>
        </div>
      ))}
    </div>
  );
}

