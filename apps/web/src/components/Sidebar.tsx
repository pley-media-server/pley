import { useEffect, useRef, useState, type ReactNode } from "react";
import { Link, NavLink, useLocation } from "react-router";
import { ArrowUpDown, Compass, Film, House, Inbox, List, ListMusic, ListVideo, LoaderCircle, Music, Plus, Settings, Tv, Tv2, X } from "lucide-react";
import { useLibraries } from "@/api/hooks";
import { useLiveSummary } from "@/api/livetv";
import { usePlaylists } from "@/api/lists";
import { hasRequests, mayRequest, useRequestSummary } from "@/api/requests";
import type { LibraryKind } from "@/api/types";
import { useAuth } from "@/auth";
import { cn } from "@/lib/cn";
import { requestsBadge } from "@/lib/requests";
import { Logo } from "./Logo";
import { keepTabInside } from "./Modal";
import { SortLibrariesModal } from "./SortLibrariesModal";
import { IconButton, Skeleton } from "./ui";
import { discoverUrl, libraryUrl, playlistUrl, requestsUrl } from "@/lib/urls";

export function LibraryIcon({ kind, className }: { kind: LibraryKind; className?: string }) {
  const cls = cn("size-[18px] shrink-0", className);
  if (kind === "movies") return <Film className={cls} />;
  if (kind === "shows") return <Tv className={cls} />;
  return <Music className={cls} />;
}

function NavItem({ to, icon, children, end, trailing }: { to: string; icon: ReactNode; children: ReactNode; end?: boolean; trailing?: ReactNode }) {
  return (
    <NavLink
      to={to}
      end={end}
      className={({ isActive }) =>
        cn(
          "group relative flex h-8 items-center gap-2.5 rounded-md px-3 text-sm font-medium transition-colors duration-150",
          isActive ? "bg-white/[0.07] text-fg" : "text-muted hover:bg-white/[0.04] hover:text-fg",
        )
      }
    >
      {({ isActive }) => (
        <>
          {isActive ? <span className="absolute inset-y-1.5 left-0 w-[3px] rounded-r bg-accent" /> : null}
          <span className={cn("transition-colors", isActive ? "text-accent" : "text-subtle group-hover:text-muted")}>{icon}</span>
          <span className="min-w-0 flex-1 truncate">{children}</span>
          {trailing}
        </>
      )}
    </NavLink>
  );
}

function SidebarContent({ onClose }: { onClose?: () => void }) {
  const user = useAuth();
  const libs = useLibraries();
  const live = useLiveSummary();
  const [sorting, setSorting] = useState(false);
  // Shown once it has channels; administrators set it up in Settings.
  const showLive = live.data?.available ?? false;

  return (
    <nav className="flex h-full flex-col overflow-y-auto px-3 pb-4 pt-4" aria-label="Main">
      <div className="mb-5 flex items-center justify-between px-2">
        <Link to="/" aria-label="Pley home">
          <Logo />
        </Link>
        {onClose ? (
          <IconButton label="Close menu" size="sm" onClick={onClose}>
            <X className="size-4" />
          </IconButton>
        ) : null}
      </div>

      <div className="space-y-0.5">
        <NavItem to="/" end icon={<House className="size-4" />}>
          Home
        </NavItem>
        {showLive ? (
          <NavItem to="/live" icon={<Tv2 className="size-4" />}>
            Live TV
          </NavItem>
        ) : null}
        <RequestsNav />
      </div>

      <div className="mb-0.5 mt-4 flex h-7 items-center justify-between pl-3 pr-1 text-xs font-medium text-subtle">
        Libraries
        {(libs.data?.length ?? 0) > 1 ? (
          <IconButton label="Sort libraries" size="xs" className="text-subtle" onClick={() => setSorting(true)}>
            <ArrowUpDown className="size-3.5" />
          </IconButton>
        ) : null}
      </div>
      <SortLibrariesModal open={sorting} onClose={() => setSorting(false)} />
      <div className="space-y-0.5">
        {libs.isPending
          ? Array.from({ length: 3 }, (_, i) => <Skeleton key={i} className="mx-3 my-1.5 h-5" />)
          : libs.data?.map((lib) => (
              <NavItem
                key={lib.id}
                to={libraryUrl(lib.id)}
                icon={<LibraryIcon kind={lib.kind} className="size-4" />}
                trailing={
                  lib.scanning ? (
                    <LoaderCircle className="size-3.5 animate-spin text-accent" aria-label="Scanning" />
                  ) : null
                }
              >
                {lib.name}
              </NavItem>
            ))}
        {libs.data && libs.data.length === 0 ? (
          user.is_admin ? (
            <Link
              to="/settings/libraries?add=1"
              className="flex h-8 items-center gap-2.5 rounded-md px-3 text-sm text-muted transition-colors hover:bg-white/[0.04] hover:text-fg"
            >
              <Plus className="size-4 text-subtle" />
              Add library
            </Link>
          ) : (
            <p className="px-3 py-2 text-xs text-subtle">No libraries yet.</p>
          )
        ) : null}
      </div>

      <PlaylistsSection />

      <div className="flex-1" />

      {user.is_admin ? (
        <div className="mt-4 space-y-0.5">
          <NavItem to="/settings" icon={<Settings className="size-4" />}>
            Settings
          </NavItem>
        </div>
      ) : null}
    </nav>
  );
}

/**
 * Discover, where this account may ask for titles, and its requests where
 * it has any (everyone's, for managers), with how many wait to be looked
 * at: in the danger colour while some failed.
 */
function RequestsNav() {
  const summary = useRequestSummary();
  const data = summary.data;
  if (!hasRequests(data) && !data?.manage) return null;
  const { count, danger } = requestsBadge(data);
  return (
    <>
      {mayRequest(data) ? (
        <NavItem to={discoverUrl()} icon={<Compass className="size-4" />}>
          Discover
        </NavItem>
      ) : null}
      <NavItem
        to={requestsUrl}
        icon={<Inbox className="size-4" />}
        trailing={
          // The spaces outside the spans, as names are made of their trimmed text: "Requests 3 waiting".
          count > 0 ? (
            <>
              {" "}
              <span
                className={cn(
                  "min-w-5 rounded-full px-1.5 text-center text-[11px] font-bold leading-5",
                  danger ? "bg-danger text-white" : "bg-accent text-accent-fg",
                )}
              >
                {count > 99 ? "99+" : count} <span className="sr-only">waiting</span>
              </span>
            </>
          ) : null
        }
      >
        {data?.manage ? "Requests" : "Your requests"}
      </NavItem>
    </>
  );
}

/** The user's playlists (the first few) and a link to all of them. */
function PlaylistsSection() {
  const playlists = usePlaylists();
  const shown = playlists.data?.slice(0, 8) ?? [];
  return (
    <>
      <div className="mb-0.5 mt-4 flex h-7 items-center px-3 text-xs font-medium text-subtle">Playlists</div>
      <div className="space-y-0.5">
        {shown.map((p) => (
          <NavItem
            key={p.id}
            to={playlistUrl(p.id)}
            icon={p.kind === "audio" ? <ListMusic className="size-4" /> : <ListVideo className="size-4" />}
          >
            {p.title}
          </NavItem>
        ))}
        <NavItem to="/playlists" end icon={<List className="size-4" />}>
          {shown.length ? "All playlists" : "Your playlists"}
        </NavItem>
      </div>
    </>
  );
}

export function Sidebar({ open, onClose }: { open: boolean; onClose: () => void }) {
  const location = useLocation();
  const drawerRef = useRef<HTMLElement>(null);

  // Close the drawer after navigating.
  useEffect(() => {
    onClose();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.pathname]);

  // Open, the drawer takes the keyboard (Tab stays in it, the page behind
  // doesn't scroll); closed, focus goes back to the button that opened it.
  useEffect(() => {
    if (!open) return;
    const opener = document.activeElement as HTMLElement | null;
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    drawerRef.current?.querySelector<HTMLElement>("a[href], button")?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      if (e.key === "Tab" && drawerRef.current) keepTabInside(e, drawerRef.current);
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.body.style.overflow = prevOverflow;
      if (opener?.isConnected) opener.focus();
    };
  }, [open, onClose]);

  return (
    <>
      <aside className="fixed inset-y-0 left-0 z-30 hidden w-60 border-r border-border bg-surface lg:block">
        <SidebarContent />
      </aside>
      {open ? (
        <div className="fixed inset-0 z-50 lg:hidden">
          <div className="absolute inset-0 animate-fade-in bg-black/60" onClick={onClose} />
          <aside
            ref={drawerRef}
            role="dialog"
            aria-modal="true"
            aria-label="Menu"
            className="absolute inset-y-0 left-0 w-72 max-w-[85vw] animate-pop-in border-r border-border bg-surface shadow-2xl"
          >
            <SidebarContent onClose={onClose} />
          </aside>
        </div>
      ) : null}
    </>
  );
}
