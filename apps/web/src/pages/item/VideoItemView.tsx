import { useMemo, useState, type ReactNode } from "react";
import { Link } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import { Play, RotateCcw } from "lucide-react";
import { keys } from "@/api/hooks";
import type { ItemDetail, MediaFile } from "@/api/types";
import { PartOfCollections } from "@/components/lists/PartOfCollections";
import { MissingKeyNotice } from "@/components/MetadataNotice";
import { StarRating } from "@/components/StarRating";
import { Button, Select } from "@/components/ui";
import { audioPicked, automaticAudioLabel, codecLabel, formatBytes, formatDate, formatDuration, resolutionLabel, seasonTitle, streamLabel } from "@/lib/format";
import { saveStreamChoice } from "@/lib/progress";
import { usePlayItem } from "@/lib/usePlayItem";
import { playIntent } from "@/player/preload";
import { CastRow, CrewRows } from "./Credits";
import { ExtrasRow } from "./Extras";
import { MediaInfo } from "./MediaInfo";
import { MoreLikeThis } from "./MoreLikeThis";
import { EditDialogs, type EditDialog } from "./EditMenu";
import { MoreActions } from "./ItemActions";
import { ItemRequestLine } from "./ItemRequest";
import {
  ContentRating,
  Genres,
  ItemArt,
  ItemPageShell,
  MetaLine,
  MyListButton,
  QualityBadges,
  qualityBadgeLabels,
  RatingBadge,
  ResumeProgress,
  Summary,
  WatchedButton,
} from "./shared";
import { itemUrl } from "@/lib/urls";

export function versionLabel(f: MediaFile, i: number): string {
  const parts = [f.edition, resolutionLabel(f.width, f.height), codecLabel(f.video_codec), f.container?.toUpperCase(), formatBytes(f.size)].filter(Boolean);
  return parts.length ? parts.join(" · ") : `Version ${i + 1}`;
}

function LabeledSelect({ label, children, value, onChange }: { label: string; children: ReactNode; value: string; onChange: (v: string) => void }) {
  return (
    <label className="flex min-w-0 flex-col gap-1">
      <span className="text-xs font-medium text-subtle">{label}</span>
      <Select value={value} onChange={(e) => onChange(e.target.value)} className="max-w-[20rem] bg-surface-2/80 backdrop-blur">
        {children}
      </Select>
    </label>
  );
}

export function VideoItemView({ item }: { item: ItemDetail }) {
  const play = usePlayItem();
  const isEpisode = item.kind === "episode";
  const [edit, setEdit] = useState<EditDialog | null>(null);

  const [fileId, setFileId] = useState<number | null>(item.files[0]?.id ?? null);
  const file = item.files.find((f) => f.id === fileId) ?? item.files[0];
  const audioStreams = useMemo(() => file?.streams.filter((s) => s.kind === "audio") ?? [], [file]);
  const subStreams = useMemo(() => file?.streams.filter((s) => s.kind === "subtitle") ?? [], [file]);

  // "auto" → the server's pick for this user (remembered choice for the show or movie);
  // per-file selections reset when the version changes. An audio of `null` is Automatic:
  // no track picked, so each player plays its best track of the language.
  const qc = useQueryClient();
  const [audio, setAudio] = useState<{ file: number | null; index: number | null | "auto" }>({ file: null, index: "auto" });
  const [sub, setSub] = useState<{ file: number | null; index: number | null | "auto" }>({ file: null, index: "auto" });
  const audioChoice = audio.file === (file?.id ?? null) ? audio.index : "auto";
  const audioIndex = audioChoice === "auto" ? (file && audioPicked(file) ? file.selected_audio : null) : audioChoice;
  const subChoice = sub.file === (file?.id ?? null) ? sub.index : "auto";
  const subIndex = subChoice === "auto" ? (file?.selected_subtitle ?? null) : subChoice;

  /**
   * Remember a choice made here for the whole show (or this movie). Audio left out
   * (`undefined`: only subtitles were picked) keeps the audio as it was chosen.
   */
  const remember = (nextAudio: number | null | undefined, nextSub: number | null) => {
    if (!file) return;
    // This item's copy says so at once: the player starts from it, maybe
    // before the choice is saved (and leaves out an Off or a default track).
    qc.setQueryData<ItemDetail>(keys.item(item.id), (old) =>
      old
        ? {
            ...old,
            files: old.files.map((f) =>
              f.id === file.id
                ? {
                    ...f,
                    ...(nextAudio === undefined ? {} : { selected_audio: nextAudio, audio_picked: nextAudio !== null }),
                    selected_subtitle: nextSub,
                  }
                : f,
            ),
          }
        : old,
    );
    // Automatic is asked for again here too, for the language the server now starts with.
    void saveStreamChoice(item.id, { file_id: file.id, audio_index: nextAudio, subtitle_index: nextSub }).then(() =>
      qc.invalidateQueries({ queryKey: ["item"], predicate: (q) => q.queryKey[1] !== item.id || nextAudio === null }),
    );
  };

  const resumable = !item.user_state.watched && item.user_state.position_ms > 0;

  const onPlay = (fromStart: boolean) =>
    void play(item, {
      fromStart,
      fileId: item.files.length > 1 ? file?.id : undefined,
      // A track picked here plays as picked; otherwise the server picks the one this browser plays best.
      audio: audioChoice !== "auto" && audioChoice != null ? audioChoice : undefined,
      sub: subIndex ?? undefined,
    });

  const art = isEpisode ? (
    <div className="w-full max-w-md md:w-[22rem] lg:w-[26rem]">
      <ItemArt item={item} aspect="thumb" width={416} />
    </div>
  ) : (
    <div className="hidden w-52 shrink-0 sm:block md:w-60 lg:w-64">
      <ItemArt item={item} aspect="poster" width={256} />
    </div>
  );

  return (
    <ItemPageShell item={item}>
      <div className={isEpisode ? "flex flex-col gap-6 lg:flex-row lg:gap-10" : "flex flex-col gap-6 sm:flex-row sm:gap-10"}>
        <div className="shrink-0">{art}</div>
        <div className="min-w-0 flex-1 space-y-4">
          {isEpisode ? (
            <div className="space-y-1">
              {item.grandparent_id ? (
                <Link
                  to={itemUrl(item.grandparent_id)}
                  className="text-lg font-semibold text-fg/90 transition-colors hover:text-accent"
                >
                  {item.grandparent_title}
                </Link>
              ) : null}
              <div className="text-sm text-muted">
                {item.parent_id ? (
                  <Link to={itemUrl(item.parent_id)} className="hover:text-fg">
                    {item.parent_index != null ? seasonTitle({ title: "", index: item.parent_index }) : (item.parent_title ?? "Season")}
                  </Link>
                ) : null}
                {item.index != null ? <span> · Episode {item.index}</span> : null}
              </div>
            </div>
          ) : null}

          <h1 className="text-3xl font-bold leading-tight tracking-tight text-balance sm:text-4xl">{item.title}</h1>

          <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
            <MetaLine
              parts={[
                isEpisode ? formatDate(item.release_date) : item.year,
                // The edition of the version that plays.
                file?.edition,
                formatDuration(item.duration_ms),
                item.content_rating ? <ContentRating key="cr" value={item.content_rating} /> : null,
                item.rating != null && item.rating > 0 ? <RatingBadge key="r" rating={item.rating} /> : null,
                // The quality of the version that plays.
                qualityBadgeLabels(file).length ? <QualityBadges key="q" file={file} /> : null,
              ]}
            />
            <StarRating item={item} />
          </div>

          <Genres genres={item.genres} />
          <PartOfCollections item={item} />

          {/* One line on a phone: the others are square there, and Play gives way first. */}
          <div className="flex flex-wrap items-center gap-2 pt-1 max-sm:flex-nowrap sm:gap-2.5">
            {item.files.length ? (
              <Button
                variant="primary"
                size="lg"
                icon={<Play className="size-5 shrink-0 fill-current" />}
                onClick={() => onPlay(false)}
                className="max-sm:min-w-0 max-sm:px-4"
                {...playIntent(item.kind)}
              >
                <span className="truncate">{resumable ? "Resume" : "Play"}</span>
              </Button>
            ) : (
              <Button variant="secondary" size="lg" disabled>
                Unavailable
              </Button>
            )}
            {resumable ? (
              <Button
                variant="secondary"
                size="lg"
                icon={<RotateCcw className="size-4" />}
                onClick={() => onPlay(true)}
                className="max-sm:w-11 max-sm:px-0"
                {...playIntent(item.kind)}
              >
                <span className="hidden sm:inline">From start</span>
              </Button>
            ) : null}
            {item.kind === "movie" ? <MyListButton item={item} /> : null}
            <WatchedButton item={item} />
            <MoreActions item={item} file={file} />
          </div>
          {item.kind === "movie" ? <ItemRequestLine item={item} /> : null}

          {item.kind === "movie" && item.tmdb_id == null && !item.summary ? <MissingKeyNotice className="max-w-3xl" /> : null}
          <EditDialogs item={item} dialog={edit} onClose={() => setEdit(null)} />

          <ResumeProgress item={item} />

          {item.tagline ? <p className="text-base italic text-fg/75">{item.tagline}</p> : null}
          <Summary text={item.summary} />

          {file && (item.files.length > 1 || audioStreams.length > 1 || subStreams.length > 0) ? (
            <div className="flex flex-wrap gap-3 pt-2">
              {item.files.length > 1 ? (
                <LabeledSelect label="Version" value={String(file.id)} onChange={(v) => setFileId(Number(v))}>
                  {item.files.map((f, i) => (
                    <option key={f.id} value={f.id}>
                      {versionLabel(f, i)}
                    </option>
                  ))}
                </LabeledSelect>
              ) : null}
              {audioStreams.length > 1 ? (
                <LabeledSelect
                  label="Audio"
                  value={audioIndex == null ? "auto" : String(audioIndex)}
                  onChange={(v) => {
                    const index = v === "auto" ? null : Number(v);
                    setAudio({ file: file.id, index });
                    remember(index, subIndex);
                  }}
                >
                  <option value="auto">{automaticAudioLabel(file)}</option>
                  {audioStreams.map((s) => (
                    <option key={s.index} value={s.index}>
                      {streamLabel(s)}
                    </option>
                  ))}
                </LabeledSelect>
              ) : null}
              {subStreams.length > 0 ? (
                <LabeledSelect
                  label="Subtitles"
                  value={subIndex == null ? "off" : String(subIndex)}
                  onChange={(v) => {
                    const index = v === "off" ? null : Number(v);
                    setSub({ file: file.id, index });
                    remember(audioChoice === "auto" ? undefined : audioIndex, index);
                  }}
                >
                  <option value="off">Off</option>
                  {subStreams.map((s) => (
                    <option key={s.index} value={s.index}>
                      {streamLabel(s)}
                    </option>
                  ))}
                </LabeledSelect>
              ) : null}
            </div>
          ) : null}

          <dl className="grid max-w-3xl grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 pt-2 text-sm">
            <CrewRows crew={item.crew} />
            {item.original_title && item.original_title !== item.title ? (
              <>
                <dt className="text-subtle">Original title</dt>
                <dd className="text-fg/85">{item.original_title}</dd>
              </>
            ) : null}
            {item.release_date && !isEpisode ? (
              <>
                <dt className="text-subtle">Released</dt>
                <dd className="text-fg/85">{formatDate(item.release_date)}</dd>
              </>
            ) : null}
            {item.studio ? (
              <>
                <dt className="text-subtle">{isEpisode ? "Network" : "Studio"}</dt>
                <dd className="text-fg/85">{item.studio}</dd>
              </>
            ) : null}
            {item.user_state.play_count > 0 ? (
              <>
                <dt className="text-subtle">Plays</dt>
                <dd className="text-fg/85">{item.user_state.play_count}</dd>
              </>
            ) : null}
          </dl>

          <MediaInfo files={item.files} />
        </div>
      </div>
      <ExtrasRow extras={item.extras} />
      <CastRow title={isEpisode ? "Guest Stars" : "Cast"} credits={item.cast} />
      {isEpisode ? null : <MoreLikeThis item={item} />}
    </ItemPageShell>
  );
}
