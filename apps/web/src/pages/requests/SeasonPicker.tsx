import { useState } from "react";
import { useAskRequest, type DiscoverDetail, type RequestSummary } from "@/api/requests";
import { Artwork } from "@/components/Artwork";
import { Modal } from "@/components/Modal";
import { useToast } from "@/components/Toast";
import { Button } from "@/components/ui";
import { cn } from "@/lib/cn";
import { pluralize } from "@/lib/format";
import { useOnChange } from "@/lib/hooks";
import { allowanceText, askedText, defaultPicks, pickerButton, pickerSeasons, seasonAskable, seasonStateText } from "@/lib/requests";
import { CheckRow, failure } from "./RequestCard";

/**
 * Which seasons of a show to ask for: every season, specials last; those
 * missing or partly in a library ticked to begin with, up to what's left
 * to ask for; unaired ones and specials offered unticked; the rest showing
 * how they stand. Ticking more than is left says so and holds the button.
 */
export function SeasonPicker({
  title,
  summary,
  open,
  onClose,
}: {
  title: DiscoverDetail;
  summary: RequestSummary | undefined;
  open: boolean;
  onClose: () => void;
}) {
  const ask = useAskRequest();
  const toast = useToast();
  const seasons = pickerSeasons(title.seasons ?? []);
  const left = title.seasons_left;
  const [picked, setPicked] = useState<number[]>(() => defaultPicks(title, left));
  const [error, setError] = useState<string | null>(null);

  useOnChange(open, (o) => {
    if (!o) return;
    setPicked(defaultPicks(title, left));
    setError(null);
  });

  const over = left !== null && picked.length > left;
  const allowance = allowanceText({ ...title, movies_limit: summary?.movies_limit, seasons_limit: summary?.seasons_limit }, "show");
  const toggle = (n: number) => setPicked((p) => (p.includes(n) ? p.filter((x) => x !== n) : [...p, n]));

  const submit = () => {
    if (!picked.length) return;
    setError(null);
    ask.mutate(
      { kind: "show", tmdb_id: title.tmdb_id, seasons: [...picked].sort((a, b) => a - b) },
      {
        onSuccess: (asked) => {
          toast.success(askedText(asked));
          onClose();
        },
        onError: (e) => setError(failure(e, summary ?? { manage: false }, "show")),
      },
    );
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      locked={ask.isPending}
      size="md"
      title="Which seasons?"
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={ask.isPending}>
            Cancel
          </Button>
          <Button variant="primary" onClick={submit} loading={ask.isPending} disabled={over || !picked.length} data-autofocus>
            {picked.length ? pickerButton(picked.length) : "Request"}
          </Button>
        </>
      }
    >
      <div className="space-y-5">
        <div className="flex gap-4">
          <Artwork
            candidates={title.poster_url ? [title.poster_url] : []}
            width={64}
            alt=""
            title={title.title}
            className="aspect-[2/3] w-16 shrink-0 rounded-md ring-1 ring-white/10"
          />
          <div className="min-w-0 space-y-1">
            <div className="font-semibold leading-snug">{title.title}</div>
            {title.year ? <div className="text-sm text-muted">{title.year}</div> : null}
            <p className="pt-1 text-sm text-muted">
              {summary?.access === "auto"
                ? "Approved as you ask, and sent on to download."
                : "An administrator decides on it. You'll hear when it's ready to watch."}
            </p>
          </div>
        </div>

        <fieldset>
          <legend className="sr-only">Seasons</legend>
          <div className="max-h-80 space-y-0.5 overflow-y-auto rounded-md border border-border p-1.5">
            {seasons.map((s) => {
              const askable = seasonAskable(s, title);
              return (
                <CheckRow
                  key={s.number}
                  id={`pick-season-${s.number}`}
                  checked={askable && picked.includes(s.number)}
                  disabled={!askable}
                  onChange={() => toggle(s.number)}
                  aside={seasonStateText(s) ?? (s.episodes ? pluralize(s.episodes, "episode") : null)}
                >
                  {s.name || (s.number === 0 ? "Specials" : `Season ${s.number}`)}
                </CheckRow>
              );
            })}
          </div>
        </fieldset>

        <div aria-live="polite" className="space-y-1">
          {!picked.length ? <p className="text-sm text-muted">Pick a season.</p> : null}
          {allowance ? (
            <p className={cn("text-xs", over ? "text-danger" : "text-subtle")}>
              {allowance.text}
              {allowance.limits ? ` ${allowance.limits}` : ""}
            </p>
          ) : null}
        </div>
        {error ? (
          <p role="alert" className="text-sm text-danger">
            {error}
          </p>
        ) : null}
      </div>
    </Modal>
  );
}
