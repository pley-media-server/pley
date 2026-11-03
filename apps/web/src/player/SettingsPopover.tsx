import { useRef, useState, type ReactNode } from "react";
import { Check, ChevronLeft, ChevronRight, Minus, Plus, Search, Settings2, Trash2, Upload } from "lucide-react";
import { SUBTITLE_FILE_MAX, SUBTITLE_FILE_TYPES } from "@/api/subtitles";
import type { MediaStream } from "@/api/types";
import { cn } from "@/lib/cn";
import { formatClock, streamLabel } from "@/lib/format";
import { useDismiss } from "@/lib/hooks";
import { AddSubtitleFile, FindSubtitles, subtitleSearchUnavailable, type SubtitleSources } from "./FindSubtitles";
import type { QualityOption } from "./quality";
import { SLEEP_MINUTES, type SleepChoice } from "./sleepTimer";
import { SPEEDS, speedLabel } from "./speed";
import {
  clampSubtitleOffset,
  formatSubtitleOffset,
  SUBTITLE_BACKGROUNDS,
  SUBTITLE_FONTS,
  SUBTITLE_OFFSET_MAX,
  SUBTITLE_OFFSET_STEP,
  SUBTITLE_POSITIONS,
  SUBTITLE_SIZES,
  type SubtitleStyle,
} from "./subtitles";

type Panel = "root" | "quality" | "audio" | "subtitles" | "find" | "upload" | "style" | "speed" | "sleep" | "details";

const PANEL_TITLE: Record<Exclude<Panel, "root">, string> = {
  quality: "Quality",
  audio: "Audio",
  subtitles: "Subtitles",
  find: "Find subtitles",
  upload: "Add subtitle file",
  style: "Subtitle style",
  speed: "Speed",
  sleep: "Sleep timer",
  details: "Playback details",
};

function GroupLabel({ children }: { children: ReactNode }) {
  return <div className="px-4 pb-1 pt-2 text-xs font-medium text-white/50">{children}</div>;
}

function Option({
  selected,
  onClick,
  hint,
  children,
}: {
  selected: boolean;
  onClick: () => void;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      role="menuitemradio"
      aria-checked={selected}
      onClick={onClick}
      className="flex w-full items-center gap-2.5 px-4 py-2 text-left text-sm transition-colors hover:bg-white/10"
    >
      <Check className={cn("size-4 shrink-0 text-accent", !selected && "invisible")} />
      <span className="min-w-0 flex-1">
        <span className="block truncate">{children}</span>
        {hint ? <span className="block truncate text-xs text-white/50">{hint}</span> : null}
      </span>
    </button>
  );
}

/** A row that does something rather than choose: find subtitles, add a file. */
function Action({ icon, hint, onClick, children }: { icon: ReactNode; hint?: string; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      role="menuitem"
      onClick={onClick}
      className="flex w-full items-center gap-2.5 px-4 py-2 text-left text-sm transition-colors hover:bg-white/10"
    >
      <span className="flex size-4 shrink-0 items-center justify-center text-white/70">{icon}</span>
      <span className="min-w-0 flex-1">
        <span className="block truncate">{children}</span>
        {hint ? <span className="block truncate text-xs text-white/50">{hint}</span> : null}
      </span>
    </button>
  );
}

/** `extra`: said after the value, never cut off ("+0.5 s"). */
function Nav({ label, value, extra, onClick }: { label: string; value?: string; extra?: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex w-full items-center gap-3 px-4 py-2.5 text-left text-sm transition-colors hover:bg-white/10"
    >
      <span className="flex-1">{label}</span>
      <span className="flex min-w-0 max-w-44 items-center gap-1.5 text-white/60">
        <span className="truncate">{value}</span>
        {extra ? <span className="shrink-0 tabular-nums text-accent">{extra}</span> : null}
      </span>
      <ChevronRight className="size-4 shrink-0 text-white/50" />
    </button>
  );
}

function StepButton({ label, disabled, onClick, children }: { label: string; disabled?: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className="flex size-8 shrink-0 items-center justify-center rounded-full bg-white/10 transition-colors hover:bg-white/20 disabled:opacity-40"
    >
      {children}
    </button>
  );
}

/** Moves subtitles (text, and pictures the player draws) earlier or later, 0.1 s at a time. */
function SubtitleSync({ offset, onChange }: { offset: number; onChange: (offset: number) => void }) {
  const step = (by: number) => onChange(clampSubtitleOffset(offset + by));
  return (
    <>
      <div className="my-1 h-px bg-white/10" />
      <GroupLabel>Timing</GroupLabel>
      <div className="flex items-center gap-2 px-4 py-1.5">
        <StepButton label="Show subtitles earlier" disabled={offset <= -SUBTITLE_OFFSET_MAX} onClick={() => step(-SUBTITLE_OFFSET_STEP)}>
          <Minus className="size-4" />
        </StepButton>
        <span className="min-w-16 flex-1 text-center text-sm tabular-nums" aria-live="polite" data-testid="subtitle-offset">
          {formatSubtitleOffset(offset)}
        </span>
        <StepButton label="Show subtitles later" disabled={offset >= SUBTITLE_OFFSET_MAX} onClick={() => step(SUBTITLE_OFFSET_STEP)}>
          <Plus className="size-4" />
        </StepButton>
        <button
          type="button"
          disabled={offset === 0}
          onClick={() => onChange(0)}
          className="ml-1 h-8 rounded-md px-2.5 text-xs font-semibold transition-colors hover:bg-white/10 disabled:opacity-40"
        >
          Reset
        </button>
      </div>
    </>
  );
}

export interface SettingsPopoverProps {
  open: boolean;
  setOpen: (open: boolean) => void;
  /** How the video plays now ([`PlaybackDetails`]), in its own panel at the bottom of the menu. */
  details?: ReactNode;
  /** The qualities offered for this file, the original first. */
  qualities: QualityOption[];
  quality: string;
  /** What the original is ("1080p · 8.2 Mbps"). */
  originalHint: string | null;
  /** While Automatic plays: what it plays now ("Original", "8 Mbps 1080p"). */
  autoNow?: string | null;
  /** Why Automatic plays lower, when it does ("Lowered for your connection"). */
  autoWhy?: string | null;
  onQuality: (id: string) => void;
  audioStreams: MediaStream[];
  audioIndex: number | null;
  onAudio: (index: number) => void;
  subtitleStreams: MediaStream[];
  subtitleIndex: number | null;
  onSubtitle: (index: number | null) => void;
  /** How text subtitles look on this device: offered while the subtitles chosen are text. */
  subtitleStyle: SubtitleStyle;
  onSubtitleStyle: (style: SubtitleStyle) => void;
  /** Seconds subtitles are moved by (positive: later), for this file. */
  subtitleOffset: number;
  onSubtitleOffset: (offset: number) => void;
  /** Picture subtitles are drawn by the player (not burned into the video): they move too. */
  picturesMovable?: boolean;
  /** Finding subtitles and adding files, when the player offers it. */
  subtitleSources?: SubtitleSources | null;
  /** Whether this profile may delete an added track (an administrator, or whoever added it). */
  canDeleteSubtitle?: (stream: MediaStream) => boolean;
  onDeleteSubtitle?: (index: number) => void;
  /** Says why a subtitle file picked can't be added (too big). */
  onSubtitleFileRefused?: (message: string) => void;
  /** How fast it plays (1: normal), for this title. */
  speed: number;
  onSpeed: (speed: number) => void;
  /** The sleep timer, if set, and how long it has left (seconds; for minutes). */
  sleep: SleepChoice | null;
  sleepLeft: number | null;
  /** "End of this episode". */
  sleepEndLabel: string;
  onSleep: (choice: SleepChoice | null) => void;
}

export function SettingsPopover(props: SettingsPopoverProps) {
  const { open, setOpen, details, qualities, quality, originalHint, autoNow, autoWhy, onQuality } = props;
  const { audioStreams, audioIndex, onAudio, subtitleStreams, subtitleIndex, onSubtitle } = props;
  const { subtitleStyle, onSubtitleStyle, subtitleOffset, onSubtitleOffset, picturesMovable, speed, onSpeed } = props;
  const { sleep, sleepLeft, sleepEndLabel, onSleep } = props;
  const { subtitleSources: sources, canDeleteSubtitle, onDeleteSubtitle, onSubtitleFileRefused } = props;
  const [panel, setPanel] = useState<Panel>("root");
  const [subtitleFile, setSubtitleFile] = useState<File | null>(null);
  /** The added track whose deletion is being confirmed. */
  const [deleting, setDeleting] = useState<number | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const ref = useRef<HTMLDivElement>(null);
  useDismiss(ref, open, () => {
    setOpen(false);
    setPanel("root");
  });

  const currentAudio = audioStreams.find((s) => s.index === audioIndex) ?? audioStreams.find((s) => s.default) ?? audioStreams[0];
  const currentSub = subtitleStreams.find((s) => s.index === subtitleIndex);
  // Picture subtitles burned into the video can't be moved; drawn by the player, they can.
  const movableSub = currentSub && (currentSub.text_based !== false || picturesMovable) ? currentSub : undefined;
  // The style is for text: pictures keep their own look, and subtitles off have none.
  const styled = !!currentSub && currentSub.text_based !== false;
  // Subtitle style goes back to the menu while it doesn't apply (C turned the subtitles off).
  const view = panel === "style" && !styled ? "root" : panel;
  const sleepValue =
    sleep === null ? "Off" : sleep === "end" ? sleepEndLabel : sleepLeft !== null ? `${formatClock(sleepLeft)} left` : `${sleep} min`;
  const qualityLabel =
    quality === "auto" ? `Automatic · ${autoNow ?? "Original"}` : (qualities.find((q) => q.id === quality)?.label ?? "Original");
  const qualityHint = (id: string): string | undefined => {
    if (id === "original") return originalHint ?? undefined;
    if (id === "auto") {
      return quality === "auto" && autoNow
        ? [`Now ${autoNow}`, autoWhy].filter(Boolean).join(" · ")
        : "Dynamically adjust to conditions";
    }
    return undefined;
  };

  const close = () => {
    setOpen(false);
    setPanel("root");
    setDeleting(null);
  };
  // Search the server can't do: not offered where it never will, said for an administrator who can set it up.
  const searchUnavailable = subtitleSearchUnavailable();
  const showFind = !!sources && searchUnavailable !== "older" && (searchUnavailable !== "setup" || sources.isAdmin);
  const pickFile = (file: File | undefined) => {
    if (!file) return;
    if (file.size > SUBTITLE_FILE_MAX) {
      onSubtitleFileRefused?.("Subtitle files can be up to 2 MB.");
      return;
    }
    setSubtitleFile(file);
    setPanel("upload");
  };

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        aria-label="Playback settings"
        aria-expanded={open}
        onClick={() => {
          setOpen(!open);
          setPanel("root");
        }}
        className={cn(
          "flex size-10 items-center justify-center rounded-full text-white/90 transition hover:bg-white/15 hover:text-white",
          open && "bg-white/15",
        )}
      >
        <Settings2 className={cn("size-5 transition-transform duration-300", open && "rotate-45")} />
      </button>
      {open ? (
        <div
          role="menu"
          className="absolute bottom-full right-0 mb-3 max-h-[60vh] w-80 animate-pop-in overflow-y-auto rounded-xl border border-white/10 bg-black/90 py-2 text-white shadow-2xl"
        >
          {view === "root" ? (
            <>
              <Nav label="Quality" value={qualityLabel} onClick={() => setPanel("quality")} />
              {audioStreams.length > 1 ? (
                <Nav label="Audio" value={currentAudio ? streamLabel(currentAudio) : "Default"} onClick={() => setPanel("audio")} />
              ) : null}
              <Nav
                label="Subtitles"
                value={currentSub ? streamLabel(currentSub) : subtitleStreams.length ? "Off" : "None"}
                extra={movableSub && subtitleOffset !== 0 ? formatSubtitleOffset(subtitleOffset) : undefined}
                onClick={() => setPanel("subtitles")}
              />
              {styled ? (
                <Nav
                  label="Subtitle style"
                  value={SUBTITLE_SIZES.find((s) => s.id === subtitleStyle.size)?.label ?? "Normal"}
                  onClick={() => setPanel("style")}
                />
              ) : null}
              <Nav label="Speed" value={speedLabel(speed)} onClick={() => setPanel("speed")} />
              <Nav label="Sleep timer" value={sleepValue} onClick={() => setPanel("sleep")} />
              {details ? (
                <>
                  <div className="my-1 h-px bg-white/10" />
                  <Nav label="Playback details" onClick={() => setPanel("details")} />
                </>
              ) : null}
            </>
          ) : (
            <>
              <button
                type="button"
                onClick={() => setPanel(panel === "find" || panel === "upload" ? "subtitles" : "root")}
                className="flex w-full items-center gap-2 px-3 py-2 text-sm font-semibold transition-colors hover:bg-white/10"
              >
                <ChevronLeft className="size-4" />
                {PANEL_TITLE[view]}
              </button>
              <div className="my-1 h-px bg-white/10" />
              {panel === "quality"
                ? qualities.map((q) => (
                    <Option
                      key={q.id}
                      selected={q.id === quality}
                      hint={qualityHint(q.id)}
                      onClick={() => {
                        onQuality(q.id);
                        close();
                      }}
                    >
                      {q.label}
                    </Option>
                  ))
                : null}
              {panel === "audio"
                ? audioStreams.map((s) => (
                    <Option
                      key={s.index}
                      selected={s.index === currentAudio?.index}
                      onClick={() => {
                        onAudio(s.index);
                        close();
                      }}
                    >
                      {streamLabel(s)}
                    </Option>
                  ))
                : null}
              {view === "style" ? (
                <>
                  <GroupLabel>Size</GroupLabel>
                  {SUBTITLE_SIZES.map((s) => (
                    <Option
                      key={s.id}
                      selected={s.id === subtitleStyle.size}
                      onClick={() => onSubtitleStyle({ ...subtitleStyle, size: s.id })}
                    >
                      {s.label}
                    </Option>
                  ))}
                  <GroupLabel>Background</GroupLabel>
                  {SUBTITLE_BACKGROUNDS.map((b) => (
                    <Option
                      key={b.id}
                      selected={b.id === subtitleStyle.background}
                      onClick={() => onSubtitleStyle({ ...subtitleStyle, background: b.id })}
                    >
                      {b.label}
                    </Option>
                  ))}
                  <GroupLabel>Font</GroupLabel>
                  {SUBTITLE_FONTS.map((f) => (
                    <Option
                      key={f.id}
                      selected={f.id === subtitleStyle.font}
                      onClick={() => onSubtitleStyle({ ...subtitleStyle, font: f.id })}
                    >
                      <span style={{ fontFamily: f.family }}>{f.label}</span>
                    </Option>
                  ))}
                  <GroupLabel>Position</GroupLabel>
                  {SUBTITLE_POSITIONS.map((p) => (
                    <Option
                      key={p.id}
                      selected={p.id === subtitleStyle.position}
                      onClick={() => onSubtitleStyle({ ...subtitleStyle, position: p.id })}
                    >
                      {p.label}
                    </Option>
                  ))}
                </>
              ) : null}
              {panel === "details" ? details : null}
              {panel === "find" && sources ? <FindSubtitles sources={sources} onDone={close} /> : null}
              {panel === "upload" && sources && subtitleFile ? <AddSubtitleFile sources={sources} file={subtitleFile} onDone={close} /> : null}
              {panel === "speed"
                ? SPEEDS.map((s) => (
                    <Option
                      key={s}
                      selected={s === speed}
                      onClick={() => {
                        onSpeed(s);
                        close();
                      }}
                    >
                      {speedLabel(s)}
                    </Option>
                  ))
                : null}
              {panel === "subtitles" ? (
                <>
                  <Option
                    selected={subtitleIndex == null}
                    onClick={() => {
                      onSubtitle(null);
                      close();
                    }}
                  >
                    Off
                  </Option>
                  {subtitleStreams.map((s) => {
                    const option = (
                      <Option
                        key={s.index}
                        selected={s.index === subtitleIndex}
                        hint={s.added ? "Added" : undefined}
                        onClick={() => {
                          onSubtitle(s.index);
                          close();
                        }}
                      >
                        {streamLabel(s)}
                      </Option>
                    );
                    if (!s.added || !onDeleteSubtitle || !canDeleteSubtitle?.(s)) return option;
                    if (deleting === s.index) {
                      return (
                        <div key={s.index} className="flex items-center gap-2 px-4 py-2 text-sm" role="group" aria-label={`Delete ${streamLabel(s)}`}>
                          <span className="min-w-0 flex-1 truncate">Delete for everyone?</span>
                          <button
                            type="button"
                            onClick={() => {
                              setDeleting(null);
                              onDeleteSubtitle(s.index);
                            }}
                            className="h-7 rounded-md bg-danger px-2.5 text-xs font-semibold text-white transition-opacity hover:opacity-90"
                          >
                            Delete
                          </button>
                          <button type="button" onClick={() => setDeleting(null)} className="h-7 rounded-md px-2.5 text-xs font-semibold hover:bg-white/10">
                            Keep
                          </button>
                        </div>
                      );
                    }
                    return (
                      <div key={s.index} className="flex items-center pr-2">
                        <div className="min-w-0 flex-1">{option}</div>
                        <button
                          type="button"
                          aria-label={`Delete ${streamLabel(s)}`}
                          title="Delete these subtitles"
                          onClick={() => setDeleting(s.index)}
                          className="flex size-8 shrink-0 items-center justify-center rounded-full text-white/60 transition-colors hover:bg-white/10 hover:text-white"
                        >
                          <Trash2 className="size-4" />
                        </button>
                      </div>
                    );
                  })}
                  {sources ? (
                    <>
                      <div className="my-1 h-px bg-white/10" />
                      {showFind ? (
                        <Action
                          icon={<Search className="size-4" />}
                          hint={searchUnavailable === "setup" ? "Subtitle search isn't set up" : undefined}
                          onClick={() => setPanel("find")}
                        >
                          Find subtitles…
                        </Action>
                      ) : null}
                      <Action icon={<Upload className="size-4" />} onClick={() => fileInput.current?.click()}>
                        Add subtitle file…
                      </Action>
                      <input
                        ref={fileInput}
                        type="file"
                        accept={SUBTITLE_FILE_TYPES.join(",")}
                        className="hidden"
                        data-testid="subtitle-file"
                        onChange={(e) => {
                          pickFile(e.target.files?.[0]);
                          e.target.value = "";
                        }}
                      />
                    </>
                  ) : null}
                  {movableSub ? <SubtitleSync offset={subtitleOffset} onChange={onSubtitleOffset} /> : null}
                </>
              ) : null}
              {panel === "sleep"
                ? ([null, ...SLEEP_MINUTES, "end"] as const).map((choice) => (
                    <Option
                      key={String(choice)}
                      selected={choice === sleep}
                      hint={choice !== null && choice !== "end" && choice === sleep ? sleepValue : undefined}
                      onClick={() => {
                        onSleep(choice);
                        close();
                      }}
                    >
                      {choice === null ? "Off" : choice === "end" ? sleepEndLabel : `${choice} minutes`}
                    </Option>
                  ))
                : null}
            </>
          )}
        </div>
      ) : null}
    </div>
  );
}
