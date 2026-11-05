import { createContext, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { BookmarkMinus, BookmarkPlus, Eye, EyeOff, FolderPlus, ListPlus, ListX, Play, Shuffle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { errorMessage } from "@/api/client";
import { listable, useRemoveFromContinueWatching, useSetInWatchlist, useSetWatched, useWatchlist } from "@/api/hooks";
import { collectable } from "@/api/lists";
import type { ItemSummary } from "@/api/types";
import { useAuth } from "@/auth";
import { MenuItem, MenuSeparator } from "@/components/Menu";
import { useToast } from "@/components/Toast";
import { useDismiss } from "@/lib/hooks";
import { usePlayItem } from "@/lib/usePlayItem";
import { AddToCollectionModal } from "./AddToCollectionModal";
import { AddToPlaylistModal } from "./AddToPlaylistModal";

/** Where a card is, for what its menu offers. */
export interface CardMenuContext {
  /** In a Continue Watching row: it can be taken off. */
  continueWatching?: boolean;
}

interface ListMenuApi {
  /** The right-click menu of a card, at viewport coordinates. */
  openMenu: (item: ItemSummary, x: number, y: number, context?: CardMenuContext) => void;
  addToPlaylist: (item: ItemSummary) => void;
  /** Administrators, movies and shows. */
  addToCollection: (item: ItemSummary) => void;
}

const ListMenuContext = createContext<ListMenuApi | null>(null);

/** `null` outside <ListMenuProvider> (cards then keep the browser's menu). */
export function useListMenu(): ListMenuApi | null {
  return useContext(ListMenuContext);
}

/** Card context menus and the "Add to playlist / collection" dialogs. */
export function ListMenuProvider({ children }: { children: ReactNode }) {
  const [menu, setMenu] = useState<{ item: ItemSummary; x: number; y: number; context?: CardMenuContext } | null>(null);
  const [forPlaylist, setForPlaylist] = useState<ItemSummary | null>(null);
  const [forCollection, setForCollection] = useState<ItemSummary | null>(null);
  const api = useMemo<ListMenuApi>(
    () => ({
      openMenu: (item, x, y, context) => setMenu({ item, x, y, context }),
      addToPlaylist: setForPlaylist,
      addToCollection: setForCollection,
    }),
    [],
  );
  const actions = useCardActions();

  return (
    <ListMenuContext.Provider value={api}>
      {children}
      {menu ? (
        <CardMenu key={`${menu.item.id}:${menu.x}:${menu.y}`} {...menu} api={{ ...api, ...actions }} onClose={() => setMenu(null)} />
      ) : null}
      {/* Keyed so a dialog starts blank for every item. */}
      <AddToPlaylistModal key={`p${forPlaylist?.id ?? 0}`} item={forPlaylist} onClose={() => setForPlaylist(null)} />
      <AddToCollectionModal key={`c${forCollection?.id ?? 0}`} item={forCollection} onClose={() => setForCollection(null)} />
    </ListMenuContext.Provider>
  );
}

function isWatched(item: ItemSummary): boolean {
  if (item.kind === "show" || item.kind === "season") {
    return item.leaf_count != null && item.leaf_count > 0 && (item.viewed_leaf_count ?? 0) >= item.leaf_count;
  }
  return item.user_state.watched;
}

/**
 * What a card's menu does after it has closed: here, so that a failure can
 * still be told (a mutation's own callbacks end with the component).
 */
function useCardActions() {
  const toast = useToast();
  const setInWatchlist = useSetInWatchlist();
  const removeContinuing = useRemoveFromContinueWatching();
  const failed = (err: unknown) => toast.error(errorMessage(err));
  return {
    setInWatchlist: (item: ItemSummary, on: boolean) => setInWatchlist.mutate({ item, on }, { onError: failed }),
    removeContinuing: (item: ItemSummary) => removeContinuing.mutate(item, { onError: failed }),
  };
}

function CardMenu({
  item,
  x,
  y,
  context,
  api,
  onClose,
}: {
  item: ItemSummary;
  x: number;
  y: number;
  context?: CardMenuContext;
  api: ListMenuApi & ReturnType<typeof useCardActions>;
  onClose: () => void;
}) {
  const { t } = useTranslation(["components", "common"]);
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });
  const user = useAuth();
  const play = usePlayItem();
  const setWatched = useSetWatched();
  const onList = listable(item);
  const watchlist = useWatchlist(onList);
  const toast = useToast();
  useDismiss(ref, true, onClose);

  // Keep the menu inside the viewport.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    setPos({
      left: Math.max(8, Math.min(x, window.innerWidth - width - 8)),
      top: Math.max(8, Math.min(y, window.innerHeight - height - 8)),
    });
  }, [x, y]);

  useEffect(() => {
    const close = () => onClose();
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", close);
    window.addEventListener("blur", close);
    return () => {
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", close);
      window.removeEventListener("blur", close);
    };
  }, [onClose]);

  const run = (action: () => void) => () => {
    onClose();
    action();
  };
  const music = item.kind === "artist" || item.kind === "album";
  const watchable = item.kind === "movie" || item.kind === "episode" || item.kind === "show" || item.kind === "season";
  const watched = isWatched(item);
  const inList = !!watchlist.data?.some((i) => i.id === item.id);

  return createPortal(
    <div
      ref={ref}
      role="menu"
      aria-label={item.title}
      style={{ left: pos.left, top: pos.top }}
      onContextMenu={(e) => e.preventDefault()}
      className="fixed z-[55] min-w-52 max-w-72 animate-pop-in overflow-hidden rounded-lg border border-border bg-surface-2 py-1 shadow-xl shadow-black/50"
    >
      <div className="truncate px-3.5 pb-1.5 pt-1 text-xs font-semibold text-subtle">{item.title}</div>
      <MenuItem icon={<Play />} onClick={run(() => void play(item))}>
        {t("common:actions.play")}
      </MenuItem>
      {music ? (
        <MenuItem icon={<Shuffle />} onClick={run(() => void play(item, { shuffle: true }))}>
          {t("listMenu.shuffle")}
        </MenuItem>
      ) : null}
      <MenuItem icon={<ListPlus />} onClick={run(() => api.addToPlaylist(item))}>
        {t("listMenu.addToPlaylist")}
      </MenuItem>
      {user.is_admin && collectable(item) ? (
        <MenuItem icon={<FolderPlus />} onClick={run(() => api.addToCollection(item))}>
          {t("listMenu.addToCollection")}
        </MenuItem>
      ) : null}
      {onList ? (
        // Which one to offer is known once My List is in.
        <MenuItem
          icon={inList ? <BookmarkMinus /> : <BookmarkPlus />}
          disabled={!watchlist.data}
          onClick={run(() => api.setInWatchlist(item, !inList))}
        >
          {inList ? t("listMenu.removeFromMyList") : t("listMenu.addToMyList")}
        </MenuItem>
      ) : null}
      {watchable ? (
        <>
          <MenuSeparator />
          <MenuItem
            icon={watched ? <EyeOff /> : <Eye />}
            onClick={run(() =>
              setWatched.mutate({ id: item.id, watched: !watched }, { onError: (err) => toast.error(errorMessage(err)) }),
            )}
          >
            {watched ? t("listMenu.markUnwatched") : t("listMenu.markWatched")}
          </MenuItem>
        </>
      ) : null}
      {context?.continueWatching ? (
        <MenuItem icon={<ListX />} onClick={run(() => api.removeContinuing(item))}>
          {t("listMenu.removeFromContinueWatching")}
        </MenuItem>
      ) : null}
    </div>,
    document.body,
  );
}
