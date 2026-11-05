import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { LoaderCircle } from "lucide-react";
import { errorMessage } from "@/api/client";
import { addFoundSubtitle, searchSubtitles, searchUnavailable, uploadSubtitle } from "@/api/subtitles";
import type { Id, SubtitleResult, SubtitleTrack } from "@/api/types";
import { useToast } from "@/components/Toast";
import { languageName, LANGUAGES, twoLetterCode } from "@/lib/languages";

/** What the player knows to find and add subtitles for what it plays. */
export interface SubtitleSources {
  itemId: Id;
  fileId: Id | null;
  /** The profile's subtitle language (ISO 639-2), searched first. */
  language: string | null;
  isAdmin: boolean;
  /** A track was added: the player lists it and shows it. */
  onAdded: (track: SubtitleTrack) => void;
}

/** Subtitle search on this server, as last heard: not set up, or a server older than it. */
let unavailable: "setup" | "older" | null = null;

/** Whether the server said it can't search for subtitles (since the page loaded). */
export function subtitleSearchUnavailable(): "setup" | "older" | null {
  return unavailable;
}

/** Forgets what the server said (tests). */
export function forgetSubtitleSearch(): void {
  unavailable = null;
}

/** A language picker over the dark menu, the profile's language first when it has one. */
function LanguageSelect({ value, onChange, label }: { value: string; onChange: (code: string) => void; label: string }) {
  const known = LANGUAGES.some((l) => l.code === value);
  return (
    <select
      aria-label={label}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="w-full rounded-md border border-white/15 bg-white/10 px-2 py-1.5 text-sm text-white outline-none focus-visible:ring-2 focus-visible:ring-white/60 [&>option]:bg-neutral-900"
    >
      {known ? null : <option value={value}>{languageName(value) ?? value}</option>}
      {LANGUAGES.map((l) => (
        <option key={l.code} value={l.code}>
          {l.name}
        </option>
      ))}
    </select>
  );
}

type Outcome =
  | { kind: "results"; results: SubtitleResult[] }
  | { kind: "setup" }
  | { kind: "older" }
  | { kind: "error"; message: string };

/**
 * The player's "Find subtitles…": what OpenSubtitles has for the file, in
 * the profile's language or another picked here, those made for this very
 * file first; one picked is added on the server for everyone, and shows.
 */
export function FindSubtitles({ sources, onDone }: { sources: SubtitleSources; onDone: () => void }) {
  const { t } = useTranslation("player");
  const toast = useToast();
  const [language, setLanguage] = useState(sources.language || "eng");
  const [answer, setAnswer] = useState<{ language: string; outcome: Outcome } | null>(null);
  const [adding, setAdding] = useState<string | null>(null);
  const { itemId, fileId } = sources;

  useEffect(() => {
    const abort = new AbortController();
    searchSubtitles(itemId, fileId, [twoLetterCode(language)], abort.signal).then(
      (results) => setAnswer({ language, outcome: { kind: "results", results } }),
      (err: unknown) => {
        if (abort.signal.aborted) return;
        const why = searchUnavailable(err);
        if (why) unavailable = why;
        setAnswer({ language, outcome: why ? { kind: why } : { kind: "error", message: errorMessage(err) } });
      },
    );
    return () => abort.abort();
  }, [itemId, fileId, language]);

  const pick = (result: SubtitleResult) => {
    if (adding) return;
    setAdding(result.id);
    addFoundSubtitle(itemId, fileId, result.id).then(
      (track) => {
        setAdding(null);
        sources.onAdded(track);
        onDone();
      },
      (err: unknown) => {
        setAdding(null);
        toast.error(t("find.addFailed", { error: errorMessage(err) }));
      },
    );
  };

  const outcome = answer && answer.language === language ? answer.outcome : null;
  if (outcome?.kind === "setup" || outcome?.kind === "older") {
    return (
      <div className="px-4 py-2 text-sm text-white/80" role="status">
        {outcome.kind === "setup" ? t("find.notSetUp") : t("find.searchNeedsUpdate")}
        {outcome.kind === "setup" && sources.isAdmin ? (
          <Link to="/settings/server" className="mt-2 block font-semibold text-accent hover:underline" onClick={onDone}>
            {t("find.setUp")}
          </Link>
        ) : null}
      </div>
    );
  }
  return (
    <>
      <div className="px-4 pb-2 pt-1">
        <LanguageSelect value={language} onChange={setLanguage} label={t("find.language")} />
      </div>
      {!outcome ? (
        <div className="flex items-center gap-2 px-4 py-2 text-sm text-white/60" role="status">
          <LoaderCircle className="size-4 animate-spin" />
          {t("find.searching")}
        </div>
      ) : outcome.kind === "error" ? (
        <div className="px-4 py-2 text-sm text-white/80" role="alert">
          {t("find.searchFailed", { error: outcome.message })}
        </div>
      ) : !outcome.results.length ? (
        <div className="px-4 py-2 text-sm text-white/60" role="status">
          {t("find.noResults", { language: languageName(language) ?? language })}
        </div>
      ) : (
        outcome.results.map((r) => (
          <button
            key={r.id}
            type="button"
            role="menuitem"
            disabled={adding !== null}
            onClick={() => pick(r)}
            className="flex w-full items-start gap-2.5 px-4 py-2 text-left text-sm transition-colors hover:bg-white/10 disabled:cursor-default disabled:opacity-60 disabled:hover:bg-transparent"
          >
            <span className="flex size-4 shrink-0 items-center justify-center pt-0.5">
              {adding === r.id ? <LoaderCircle className="size-4 animate-spin text-accent" /> : null}
            </span>
            <span className="min-w-0 flex-1">
              <span className="block truncate" title={r.name}>
                {r.name}
              </span>
              <span className="block truncate text-xs text-white/50">
                {[
                  languageName(r.language),
                  r.format.toUpperCase(),
                  r.hearing_impaired ? t("find.sdh") : null,
                  r.forced ? t("find.forced") : null,
                  t("find.downloads", { count: r.downloads }),
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </span>
              {r.matched === "hash" ? <span className="block text-xs text-accent">{t("find.madeForFile")}</span> : null}
            </span>
          </button>
        ))
      )}
    </>
  );
}

/**
 * "Add subtitle file…" once a file is picked: its language (the profile's
 * subtitle language at first), then it goes to the server, for everyone,
 * and shows.
 */
export function AddSubtitleFile({ sources, file, onDone }: { sources: SubtitleSources; file: File; onDone: () => void }) {
  const { t } = useTranslation(["player", "common"]);
  const toast = useToast();
  const [language, setLanguage] = useState(sources.language || "eng");
  const [sending, setSending] = useState(false);
  const send = () => {
    setSending(true);
    uploadSubtitle(sources.itemId, sources.fileId, file, language).then(
      (track) => {
        setSending(false);
        sources.onAdded(track);
        onDone();
      },
      (err: unknown) => {
        setSending(false);
        toast.error(
          searchUnavailable(err) === "older"
            ? t("find.uploadNeedsUpdate")
            : t("find.addFailed", { error: errorMessage(err) }),
        );
      },
    );
  };
  return (
    <div className="space-y-2 px-4 pb-2 pt-1">
      <div className="truncate text-sm" title={file.name}>
        {file.name}
      </div>
      <LanguageSelect value={language} onChange={setLanguage} label={t("find.fileLanguage")} />
      <button
        type="button"
        disabled={sending}
        onClick={send}
        className="flex h-8 w-full items-center justify-center gap-2 rounded-md bg-accent text-sm font-semibold text-accent-fg transition-colors hover:bg-accent-hover disabled:opacity-60"
      >
        {sending ? <LoaderCircle className="size-4 animate-spin" /> : null}
        {sending ? t("find.adding") : t("common:actions.add")}
      </button>
    </div>
  );
}
