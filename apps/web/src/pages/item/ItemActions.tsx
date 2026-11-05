import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useMutation } from "@tanstack/react-query";
import { Download, Ellipsis, FastForward, FolderPlus, ListPlus, RefreshCw } from "lucide-react";
import { api, errorMessage } from "@/api/client";
import { useRefreshItem } from "@/api/hooks";
import { collectable } from "@/api/lists";
import type { ItemDetail, MediaFile } from "@/api/types";
import { useAuth } from "@/auth";
import { useListMenu } from "@/components/lists/ListMenu";
import { Menu, MenuItem, MenuSeparator } from "@/components/Menu";
import { useToast } from "@/components/Toast";
import { IconButton } from "@/components/ui";
import { downloadName, fileUrl, saveFile } from "@/lib/download";
import { formatBytes } from "@/lib/format";
import { appShell } from "@/lib/shell";
import { EditDialogs, editActions, type EditDialog } from "./EditMenu";

/** Kinds whose intro and credits can be detected. */
const ANALYZABLE = new Set(["movie", "show", "season", "episode"]);

/**
 * The "More" menu of an item page: downloading the file, playlists and
 * collections, then (for administrators) editing and maintenance, each
 * with its name. `file` is the version the page plays (the first if not
 * given).
 */
export function MoreActions({ item, file = item.files[0] }: { item: ItemDetail; file?: MediaFile }) {
  const { t } = useTranslation(["item", "common"]);
  const user = useAuth();
  const lists = useListMenu();
  const toast = useToast();
  const refresh = useRefreshItem();
  const analyze = useMutation({ mutationFn: (id: number) => api.post<void>(`/api/items/${id}/analyze`) });
  const [dialog, setDialog] = useState<EditDialog | null>(null);

  const admin = user.is_admin;
  const edits = admin ? editActions(item) : [];
  const canCollect = admin && collectable(item);
  const canAnalyze = admin && ANALYZABLE.has(item.kind);
  // The Android app's web view keeps no files (the app downloads on its own).
  const download = file && appShell() !== "android" ? file : null;
  if (!lists && !admin && !download) return null;

  const findMarkers = () => {
    const scope = item.kind === "episode" ? "season" : item.kind === "show" ? "show" : "item";
    analyze.mutate(item.id, {
      onSuccess: () => toast.success(t(`actions.findingMarkers.${scope}`)),
      onError: (e) => toast.error(errorMessage(e)),
    });
  };

  return (
    <>
      <Menu
        // Opens leftwards: the button sits at the end of the row (near the edge on phones).
        trigger={({ open, toggle }) => (
          <IconButton
            label={t("actions.more")}
            size="lg"
            aria-haspopup="menu"
            aria-expanded={open}
            active={open}
            className="bg-surface-3/80 hover:bg-border max-sm:size-11"
            onClick={toggle}
          >
            <Ellipsis className="size-5" />
          </IconButton>
        )}
      >
        {(close) => (
          <>
            {download ? (
              <MenuItem icon={<Download />} onClick={() => (close(), saveFile(fileUrl(download, item), downloadName(item, download)))}>
                {t("common:actions.download")}
                <span className="ml-auto whitespace-nowrap pl-6 text-xs tabular-nums text-subtle">{formatBytes(download.size)}</span>
              </MenuItem>
            ) : null}
            {download && lists ? <MenuSeparator /> : null}
            {lists ? (
              <MenuItem icon={<ListPlus />} onClick={() => (close(), lists.addToPlaylist(item))}>
                {t("actions.addToPlaylist")}
              </MenuItem>
            ) : null}
            {lists && canCollect ? (
              <MenuItem icon={<FolderPlus />} onClick={() => (close(), lists.addToCollection(item))}>
                {t("actions.addToCollection")}
              </MenuItem>
            ) : null}
            {edits.length ? <MenuSeparator /> : null}
            {edits.map((edit) => (
              <MenuItem key={edit.dialog} icon={edit.icon} onClick={() => (close(), setDialog(edit.dialog))}>
                {edit.label}
              </MenuItem>
            ))}
            {admin ? (
              <>
                <MenuSeparator />
                <MenuItem
                  icon={<RefreshCw />}
                  disabled={refresh.isPending}
                  onClick={() => {
                    close();
                    refresh.mutate(item.id, {
                      onSuccess: () => toast.success(t("actions.refreshing")),
                      onError: (e) => toast.error(errorMessage(e)),
                    });
                  }}
                >
                  {t("actions.refreshMetadata")}
                </MenuItem>
              </>
            ) : null}
            {canAnalyze ? (
              <MenuItem icon={<FastForward />} disabled={analyze.isPending} onClick={() => (close(), findMarkers())}>
                {t("actions.findMarkers")}
              </MenuItem>
            ) : null}
          </>
        )}
      </Menu>
      <EditDialogs item={item} dialog={dialog} onClose={() => setDialog(null)} />
    </>
  );
}
