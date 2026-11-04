import { useNavigate } from "react-router";
import { Play, Radio, Star } from "lucide-react";
import { episodeLabel, programProgress, useSetFavorite, type Channel, type LiveCategory, type Program } from "@/api/livetv";
import { Modal } from "@/components/Modal";
import { PlayOn } from "@/components/PlayOn";
import { Badge, Button, ProgressBar } from "@/components/ui";
import { sizedImage } from "@/lib/images";
import { ChannelLogo, formatGuideDay, formatSpan, liveUrl, timeLeft, useNow } from "./common";

/** A programme's details, with what can be done with its channel. */
export function ProgramModal({
  channel,
  program,
  category,
  onClose,
}: {
  channel: Channel | null;
  program: Program | null;
  /** Where the channel was chosen: channel up and down stay there. */
  category?: LiveCategory;
  onClose: () => void;
}) {
  const navigate = useNavigate();
  const now = useNow(10_000);
  const favorite = useSetFavorite();
  if (!channel || !program) return null;
  const airing = program.start <= now && program.stop > now;
  const ended = program.stop <= now;
  const episode = episodeLabel(program);
  const image = sizedImage(program.image, 640);
  return (
    <Modal
      open
      onClose={onClose}
      size="lg"
      title={
        <span className="flex items-center gap-3">
          <ChannelLogo channel={channel} width={48} showName={false} className="h-8 w-12 shrink-0 rounded bg-surface-3" />
          <span className="min-w-0 truncate">
            {channel.number ? <span className="mr-1.5 tabular-nums text-subtle">{channel.number}</span> : null}
            {channel.name}
          </span>
        </span>
      }
      footer={
        <>
          <Button
            variant="ghost"
            icon={<Star className={channel.favorite ? "size-4 fill-current text-accent" : "size-4"} />}
            onClick={() => favorite.mutate({ id: channel.id, favorite: !channel.favorite })}
          >
            {channel.favorite ? "Favourite" : "Add to favourites"}
          </Button>
          {/* Its menu opens above the footer; once a device took the channel, the dialog makes way for the remote. */}
          <PlayOn
            kind="live"
            label="Watch on…"
            size="md"
            placement="above"
            align="right"
            cast={() => ({ channel_id: channel.id })}
            onCast={onClose}
          />
          <Button variant="primary" icon={<Play className="size-4 fill-current" />} onClick={() => void navigate(liveUrl(channel, category))}>
            {airing ? "Watch now" : "Watch channel"}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-5 sm:flex-row">
        {image ? (
          <img src={image} alt="" className="aspect-video w-full shrink-0 rounded-md bg-surface-2 object-cover sm:w-56" />
        ) : null}
        <div className="min-w-0 flex-1">
          <h3 className="text-xl font-bold leading-tight tracking-tight [text-wrap:balance]">{program.title}</h3>
          {program.subtitle || episode ? (
            <p className="mt-1 text-sm text-muted">
              {[episode, program.subtitle].filter(Boolean).join(" · ")}
            </p>
          ) : null}
          <div className="mt-3 flex flex-wrap items-center gap-1.5 text-sm text-muted">
            <span className="tabular-nums">
              {formatGuideDay(program.start, now)} · {formatSpan(program)}
            </span>
            {airing ? (
              <Badge tone="danger" className="ml-1">
                <Radio className="size-3" /> On now
              </Badge>
            ) : null}
            {program.is_live ? <Badge tone="danger">Live</Badge> : null}
            {program.is_new ? <Badge tone="accent">New</Badge> : null}
            {program.rating ? <Badge>{program.rating}</Badge> : null}
          </div>
          {airing ? (
            <div className="mt-3 flex items-center gap-3">
              <ProgressBar value={programProgress(program, now)} className="h-1.5" />
              <span className="shrink-0 text-xs text-subtle">{timeLeft(program, now)}</span>
            </div>
          ) : null}
          {program.description ? (
            <p className="mt-4 whitespace-pre-line text-sm leading-relaxed text-fg/90">{program.description}</p>
          ) : (
            <p className="mt-4 text-sm text-subtle">The guide says nothing more of it.</p>
          )}
          {program.categories.length ? (
            <div className="mt-4 flex flex-wrap gap-1.5">
              {program.categories.map((c) => (
                <span key={c} className="rounded-full border border-border px-2.5 py-0.5 text-xs text-muted">
                  {c}
                </span>
              ))}
            </div>
          ) : null}
          {ended ? <p className="mt-4 text-xs text-subtle">It has ended.</p> : null}
        </div>
      </div>
    </Modal>
  );
}
