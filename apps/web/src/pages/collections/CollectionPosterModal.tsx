import { createElement, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Ban, ImageUp, LayoutGrid, PaintBucket } from "lucide-react";
import { useLibrary } from "@/api/hooks";
import { errorMessage } from "@/api/client";
import {
  useResetCollectionPoster,
  useUpdateCollection,
  useUploadCollectionPoster,
  type CollectionArtwork,
  type CollectionDetail,
} from "@/api/lists";
import { Artwork } from "@/components/Artwork";
import { Modal } from "@/components/Modal";
import { useToast } from "@/components/Toast";
import { Button } from "@/components/ui";
import { cn } from "@/lib/cn";
import {
  COLLECTION_COLORS,
  COLLECTION_ICONS,
  DEFAULT_COLLECTION_COLOR,
  EMPTY_COLLECTION_COLOR,
  collectionColor,
  collectionIcon,
  defaultCollectionIcon,
  isCollectionColor,
  isCollectionIcon,
  type CollectionColorName,
  type CollectionIconName,
} from "@/lib/collectionIcons";
import { posterCandidates } from "@/lib/images";

type Background = "posters" | "color" | "custom";

function backgroundOf(artwork: CollectionArtwork): Background {
  return artwork === "custom" ? "custom" : artwork === "color" ? "color" : "posters";
}

/** Choose a collection's poster (administrators): its films' posters, with
 * an icon in a color or none; an icon on its color alone; or a picture
 * uploaded. */
export function CollectionPosterModal({ collection: c, onClose }: { collection: CollectionDetail; onClose: () => void }) {
  const { t } = useTranslation(["item", "common"]);
  const toast = useToast();
  const library = useLibrary(c.library_id).data;
  const update = useUpdateCollection();
  const upload = useUploadCollectionPoster();
  const reset = useResetCollectionPoster();
  const fileRef = useRef<HTMLInputElement>(null);
  const [background, setBackground] = useState<Background>(backgroundOf(c.artwork));
  const [icon, setIcon] = useState<CollectionIconName | null>(isCollectionIcon(c.icon) ? c.icon : null);
  const [color, setColor] = useState<CollectionColorName>(isCollectionColor(c.icon_color) ? c.icon_color : DEFAULT_COLLECTION_COLOR);
  const busy = update.isPending || upload.isPending || reset.isPending;

  const chooseBackground = (next: Background) => {
    setBackground(next);
    // On its color alone a poster always has an icon: its library's to start with.
    if (next === "color" && !icon) setIcon(defaultCollectionIcon(library?.kind));
  };

  const done = () => {
    toast.success(t("collectionPoster.saved"));
    onClose();
  };
  const onError = (e: unknown) => toast.error(errorMessage(e));
  // What saving makes the poster.
  const target: CollectionArtwork =
    background === "color" ? "color" : background === "custom" ? "custom" : icon ? "icon" : "automatic";
  const onSave = () => {
    if (target === "automatic") reset.mutate(c.id, { onSuccess: done, onError });
    else if (icon)
      update.mutate(
        { id: c.id, icon, icon_color: color, background: target === "color" ? "color" : "posters" },
        { onSuccess: done, onError },
      );
  };
  // Nothing to save: the poster is already what's chosen.
  const unchanged = target === c.artwork && (target === "automatic" || (icon === c.icon && color === c.icon_color));

  const backgrounds: [Background, string, typeof ImageUp][] = [
    ["posters", t("collectionPoster.posters"), LayoutGrid],
    ["color", t("collectionPoster.color"), PaintBucket],
    ["custom", t("collectionPoster.custom"), ImageUp],
  ];
  const hint =
    background === "custom"
      ? t(c.artwork === "custom" ? "collectionPoster.customHas" : "collectionPoster.customHint")
      : background === "color"
        ? t("collectionPoster.colorHint")
        : icon
          ? t("collectionPoster.postersIconHint")
          : t(c.automatic ? "collectionPoster.postersTmdb" : "collectionPoster.postersHint");

  return (
    <Modal
      open
      onClose={onClose}
      title={t("collectionPoster.title")}
      size="lg"
      locked={busy}
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {t("common:actions.cancel")}
          </Button>
          {background === "custom" ? (
            <Button variant="primary" icon={<ImageUp className="size-4" />} onClick={() => fileRef.current?.click()} loading={upload.isPending} disabled={busy}>
              {upload.isPending ? t("edit.artwork.uploading") : t("edit.artwork.upload")}
            </Button>
          ) : (
            <Button variant="primary" onClick={onSave} loading={busy} disabled={busy || unchanged}>
              {t("common:actions.save")}
            </Button>
          )}
        </>
      }
    >
      <input
        ref={fileRef}
        type="file"
        accept="image/*"
        className="hidden"
        onChange={(e) => {
          const file = e.target.files?.[0];
          if (file) upload.mutate({ id: c.id, file }, { onSuccess: done, onError });
          e.target.value = "";
        }}
      />
      <div className="space-y-5">
        <div className="grid grid-cols-3 gap-2" role="radiogroup" aria-label={t("collectionPoster.background")}>
          {backgrounds.map(([value, label, Icon]) => (
            <button
              key={value}
              type="button"
              role="radio"
              aria-checked={background === value}
              onClick={() => chooseBackground(value)}
              className={cn(
                "flex h-11 items-center justify-center gap-2 rounded-md border text-sm font-medium transition-colors",
                background === value ? "border-accent bg-accent/10 text-fg" : "border-border text-muted hover:border-border-strong hover:text-fg",
              )}
            >
              <Icon className="hidden size-4 sm:block" /> {label}
            </button>
          ))}
        </div>

        <div className="flex gap-5">
          <div className="w-28 shrink-0 sm:w-36">
            <Preview collection={c} target={target} icon={icon} color={color} emptyIcon={defaultCollectionIcon(library?.kind)} />
          </div>
          <div className="min-w-0 flex-1 space-y-4">
            <p className="text-sm text-muted">{hint}</p>
            {target === "icon" || target === "color" ? (
              <div className="space-y-1.5">
                <span id="poster-colors" className="block text-sm font-medium text-fg">
                  {t("collectionPoster.color")}
                </span>
                <div className="flex flex-wrap gap-2" role="radiogroup" aria-labelledby="poster-colors">
                  {COLLECTION_COLORS.map(([name, rgb]) => (
                    <button
                      key={name}
                      type="button"
                      role="radio"
                      aria-checked={color === name}
                      aria-label={t(`collectionPoster.colors.${name}`)}
                      title={t(`collectionPoster.colors.${name}`)}
                      onClick={() => setColor(name)}
                      style={{ backgroundColor: rgb }}
                      className={cn(
                        "size-8 rounded-full ring-offset-2 ring-offset-surface transition-shadow",
                        color === name ? "ring-2 ring-fg" : "hover:ring-2 hover:ring-border-strong",
                      )}
                    />
                  ))}
                </div>
              </div>
            ) : null}
          </div>
        </div>

        {background !== "custom" ? (
          <div className="space-y-1.5">
            <span id="poster-icons" className="block text-sm font-medium text-fg">
              {t("collectionPoster.iconLabel")}
            </span>
            <div className="grid grid-cols-6 gap-2 sm:grid-cols-9" role="radiogroup" aria-labelledby="poster-icons">
              {/* Over the posters an icon is a choice; on a color alone it's
                  needed, so No icon stays in its place, turned off, and the
                  icons don't move when the background changes. */}
              <IconChoice
                label={t("collectionPoster.noIcon")}
                title={background === "color" ? t("collectionPoster.noIconOnColor") : undefined}
                checked={background === "posters" && icon === null}
                disabled={background === "color"}
                onClick={() => setIcon(null)}
                glyph={Ban}
              />
              {COLLECTION_ICONS.map(([name, glyph]) => (
                <IconChoice
                  key={name}
                  label={t(`collectionPoster.icons.${name}`)}
                  checked={icon === name}
                  onClick={() => setIcon(name)}
                  glyph={glyph}
                />
              ))}
            </div>
          </div>
        ) : null}
      </div>
    </Modal>
  );
}

function IconChoice({
  label,
  title = label,
  checked,
  disabled = false,
  onClick,
  glyph,
}: {
  label: string;
  title?: string;
  checked: boolean;
  disabled?: boolean;
  onClick: () => void;
  glyph: typeof Ban;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={checked}
      aria-label={label}
      title={title}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "flex aspect-square items-center justify-center rounded-md border transition-colors",
        checked
          ? "border-accent bg-accent/10 text-fg"
          : "border-border text-muted enabled:hover:border-border-strong enabled:hover:text-fg disabled:cursor-not-allowed disabled:opacity-40",
      )}
    >
      {createElement(glyph, { className: "size-5" })}
    </button>
  );
}

/** The poster saving would give it, as near as the web app can draw it. */
function Preview({
  collection: c,
  target,
  icon,
  color,
  emptyIcon,
}: {
  collection: CollectionDetail;
  target: CollectionArtwork;
  icon: CollectionIconName | null;
  color: CollectionColorName;
  /** Its library's icon, drawn on slate when it has no posters at all. */
  emptyIcon: CollectionIconName;
}) {
  if (target === "icon" && icon) return <DrawnPreview collection={c} icon={icon} color={color} />;
  if (target === "color" && icon) return <DrawnPreview collection={c} icon={icon} color={color} alone />;
  // The automatic poster as it shows now, or as it would be drawn instead of
  // the one chosen now.
  if (target === "automatic" && c.artwork !== "automatic") {
    // TMDB's poster isn't one the web app has: a blank in its place, the
    // hint naming it, rather than the poster chosen now.
    if (c.automatic) return <Artwork candidates={[]} width={256} alt={c.title} title={c.title} className="aspect-[2/3] w-full rounded-md ring-1 ring-white/10" />;
    // With no posters at all, the server draws its library's icon on slate.
    if (!c.items.some((i) => i.poster_url)) return <DrawnPreview collection={c} icon={emptyIcon} color={EMPTY_COLLECTION_COLOR} alone />;
    return <DrawnPreview collection={c} />;
  }
  return (
    <Artwork
      candidates={[c.poster_url]}
      width={256}
      alt={c.title}
      title={c.title}
      className="aspect-[2/3] w-full rounded-md ring-1 ring-white/10"
    />
  );
}

/** What the server will draw, roughly: its first four films' posters laid
 * out together, and with an icon, fading into the color with the icon at the
 * bottom; or (`alone`, or no posters) the icon on its color. Without an
 * icon, fewer than four posters is the first film's alone. */
function DrawnPreview({
  collection: c,
  icon,
  color,
  alone = false,
}: {
  collection: CollectionDetail;
  icon?: string;
  color?: string;
  alone?: boolean;
}) {
  const glyph = icon ? collectionIcon(icon) : undefined;
  const rgb = collectionColor(color ?? DEFAULT_COLLECTION_COLOR);
  const posters = alone ? [] : c.items.filter((i) => i.poster_url).slice(0, 4);
  const band = `color-mix(in srgb, ${rgb} 65%, black)`;
  // Without an icon, one poster is only that film's own.
  if (!glyph && posters.length < 2) {
    return (
      <Artwork
        candidates={posters[0] ? posterCandidates(posters[0]) : []}
        width={256}
        alt={c.title}
        title={c.title}
        className="aspect-[2/3] w-full rounded-md ring-1 ring-white/10"
      />
    );
  }
  const cells = posters;
  return (
    <div
      className="relative aspect-[2/3] w-full overflow-hidden rounded-md ring-1 ring-white/10"
      style={{
        background: !glyph
          ? undefined
          : cells.length
            ? band
            : // On its color alone, the server's diagonal.
              `linear-gradient(150deg, color-mix(in srgb, ${rgb} 92%, white), color-mix(in srgb, ${rgb} 45%, black))`,
      }}
    >
      {cells.length ? (
        // Laid out as on the server: one fills it, two side by side, three
        // one tall and two stacked, four in a grid.
        <div className={cn("absolute inset-0 grid gap-px", cells.length > 1 && "grid-cols-2", cells.length > 2 && "grid-rows-2")}>
          {cells.map((item, n) => (
            <Artwork
              key={item.id}
              candidates={posterCandidates(item)}
              width={cells.length === 1 ? 256 : 128}
              alt=""
              title={item.title}
              className={cn("size-full", cells.length === 3 && n === 0 && "row-span-2")}
            />
          ))}
        </div>
      ) : null}
      {glyph ? (
        <>
          {cells.length ? (
            <div className="absolute inset-0" style={{ background: `linear-gradient(to bottom, transparent 58%, ${band} 90%)` }} />
          ) : null}
          {createElement(glyph, {
            "aria-hidden": true,
            strokeWidth: 1.75,
            className: cells.length
              ? "absolute bottom-[5%] left-1/2 size-1/5 -translate-x-1/2"
              : "absolute top-1/2 left-1/2 size-1/3 -translate-x-1/2 -translate-y-1/2",
            style: { color: "white" },
          })}
        </>
      ) : null}
    </div>
  );
}
