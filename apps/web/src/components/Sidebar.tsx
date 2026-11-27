import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { Link, NavLink, useLocation } from "react-router";
import {
  ArrowUpDown,
  BookHeadphones,
  Compass,
  Film,
  House,
  Inbox,
  List,
  LoaderCircle,
  Music,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  Settings,
  Tv,
  Tv2,
  X,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { useLibraries } from "@/api/hooks";
import { useLiveSummary } from "@/api/livetv";
import { hasRequests, mayRequest, useRequestSummary } from "@/api/requests";
import type { LibraryKind } from "@/api/types";
import { useAuth } from "@/auth";
import { cn } from "@/lib/cn";
import { requestsBadge } from "@/lib/requests";
import { Logo, LogoMark } from "./Logo";
import { keepTabInside } from "./Modal";
import { SortLibrariesModal } from "./SortLibrariesModal";
import { LONG_DELAY_MS, useTooltip } from "./Tooltip";
import { IconButton, Skeleton } from "./ui";
import { discoverUrl, libraryUrl, requestsUrl } from "@/lib/urls";

export function LibraryIcon({ kind, className }: { kind: LibraryKind; className?: string }) {
  // A size in `className` replaces the default: `cn` doesn't merge classes, and the later rule would win.
  const cls = cn(className ?? "size-[18px]", "shrink-0");
  if (kind === "movies") return <Film className={cls} />;
  if (kind === "shows") return <Tv className={cls} />;
  if (kind === "audiobooks") return <BookHeadphones className={cls} />;
  return <Music className={cls} />;
}

/** The sidebar narrowed to its icons: each item's name read out and shown on hover, not written. */
const Narrow = createContext(false);

function NavItem({
  to,
  icon,
  children,
  end,
  trailing,
  name,
}: {
  to: string;
  icon: ReactNode;
  children: ReactNode;
  end?: boolean;
  trailing?: ReactNode;
  /** Shown on hover when narrow, where `children` isn't plain text. */
  name?: string;
}) {
  const narrow = useContext(Narrow);
  const { trigger, tip } = useTooltip(narrow ? (name ?? (typeof children === "string" ? children : undefined)) : undefined);
  return (
    <>
      <NavLink
        {...trigger}
        to={to}
        end={end}
        className={({ isActive }) =>
          cn(
            "group relative flex h-8 items-center gap-2.5 rounded-md text-sm font-medium transition-colors duration-150",
            narrow ? "justify-center" : "px-3",
            isActive ? "bg-white/[0.07] text-fg" : "text-muted hover:bg-white/[0.04] hover:text-fg",
          )
        }
      >
        {({ isActive }) => (
          <>
            {isActive ? <span className="absolute inset-y-1.5 left-0 w-[3px] rounded-r bg-accent" /> : null}
            <span className={cn("transition-colors", isActive ? "text-accent" : "text-subtle group-hover:text-muted")}>{icon}</span>
            <span className={narrow ? "sr-only" : "min-w-0 flex-1 truncate"}>{children}</span>
            {/* Narrow, a badge or spinner sits on the icon's corner. */}
            {trailing && narrow ? (
              <>
                {" "}
                <span className="absolute left-1/2 top-0 ml-0.5 flex origin-top-left scale-75">{trailing}</span>
              </>
            ) : (
              trailing
            )}
          </>
        )}
      </NavLink>
      {tip}
    </>
  );
}

function SidebarContent({
  onClose,
  collapsed = false,
  onToggle,
}: {
  onClose?: () => void;
  collapsed?: boolean;
  onToggle?: () => void;
}) {
  const { t } = useTranslation("components");
  const user = useAuth();
  const libs = useLibraries();
  const live = useLiveSummary();
  const [sorting, setSorting] = useState(false);
  // Shown once it has channels; administrators set it up in Settings.
  const showLive = live.data?.available ?? false;
  const canSort = !collapsed && (libs.data?.length ?? 0) > 1;
  const canAdd = !!libs.data && libs.data.length === 0 && user.is_admin;
  // Each tip only while its control shows, so one can't stay up after its control goes.
  const sortTip = useTooltip(canSort ? t("sidebar.sortLibraries") : undefined, { delayMs: LONG_DELAY_MS });
  const addTip = useTooltip(collapsed && canAdd ? t("sidebar.addLibrary") : undefined);
  const toggleLabel = collapsed ? t("sidebar.expand") : t("sidebar.collapse");
  const toggleTip = useTooltip(onToggle ? toggleLabel : undefined, { delayMs: LONG_DELAY_MS });

  return (
    <Narrow.Provider value={collapsed}>
      <nav
        className={cn("flex h-full flex-col overflow-y-auto overflow-x-hidden pb-4 pt-4", collapsed ? "px-2" : "px-3")}
        aria-label={t("sidebar.main")}
      >
        <div className={cn("mb-5 flex items-center", collapsed ? "justify-center" : "justify-between px-2")}>
          <Link to="/" aria-label={t("sidebar.pleyHome")}>
            {collapsed ? <LogoMark className="size-7" /> : <Logo />}
          </Link>
          {onClose ? (
            <IconButton label={t("sidebar.closeMenu")} size="sm" onClick={onClose}>
              <X className="size-4" />
            </IconButton>
          ) : null}
        </div>

        <div className="space-y-0.5">
          <NavItem to="/" end icon={<House className="size-4" />}>
            {t("sidebar.home")}
          </NavItem>
          {showLive ? (
            <NavItem to="/live" icon={<Tv2 className="size-4" />}>
              {t("sidebar.liveTv")}
            </NavItem>
          ) : null}
          <RequestsNav />
        </div>

        {collapsed ? (
          // Narrow, a line parts the libraries from what's above; screen readers still hear their heading.
          <div className="mx-2 mb-2 mt-4 border-t border-border">
            <span className="sr-only">{t("sidebar.libraries")}</span>
          </div>
        ) : (
          <div className="mb-0.5 mt-4 flex h-7 items-center justify-between pl-3 pr-1 text-xs font-medium text-subtle">
            {t("sidebar.libraries")}
            {canSort ? (
              <>
                <IconButton
                  {...sortTip.trigger}
                  label={t("sidebar.sortLibraries")}
                  // The tip names it, not the browser's.
                  title={undefined}
                  size="xs"
                  className="text-subtle"
                  onClick={() => setSorting(true)}
                >
                  <ArrowUpDown className="size-3.5" />
                </IconButton>
                {sortTip.tip}
              </>
            ) : null}
          </div>
        )}
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
                      <LoaderCircle className="size-3.5 animate-spin text-accent" aria-label={t("sidebar.scanning")} />
                    ) : null
                  }
                >
                  {lib.name}
                </NavItem>
              ))}
          {/* Playlists sit with the libraries, as one more place to browse; the page explains how to make one. */}
          {libs.data?.length ? (
            <NavItem to="/playlists" icon={<List className="size-4" />}>
              {t("sidebar.playlists")}
            </NavItem>
          ) : null}
          {libs.data && libs.data.length === 0 ? (
            user.is_admin ? (
              <>
                <Link
                  {...addTip.trigger}
                  to="/settings/libraries?add=1"
                  className={cn(
                    "flex h-8 items-center gap-2.5 rounded-md text-sm text-muted transition-colors hover:bg-white/[0.04] hover:text-fg",
                    collapsed ? "justify-center" : "px-3",
                  )}
                >
                  <Plus className="size-4 text-subtle" />
                  <span className={collapsed ? "sr-only" : undefined}>{t("sidebar.addLibrary")}</span>
                </Link>
                {addTip.tip}
              </>
            ) : (
              <p className={collapsed ? "sr-only" : "px-3 py-2 text-xs text-subtle"}>{t("sidebar.noLibraries")}</p>
            )
          ) : null}
        </div>

        <div className="flex-1" />

        {user.is_admin || onToggle ? (
          // Settings shares its row with the button that narrows the sidebar; narrow, the button sits under it.
          <div className={cn("mt-4 flex", collapsed ? "flex-col gap-0.5" : "items-center justify-end gap-1")}>
            {user.is_admin ? (
              <div className={collapsed ? undefined : "min-w-0 flex-1"}>
                <NavItem to="/settings" icon={<Settings className="size-4" />}>
                  {t("sidebar.settings")}
                </NavItem>
              </div>
            ) : null}
            {onToggle ? (
              <>
                <IconButton
                  {...toggleTip.trigger}
                  label={toggleLabel}
                  // The tip names it, not the browser's.
                  title={undefined}
                  size="xs"
                  // Wide, it keeps out of the way until the pointer or the keyboard comes to it.
                  className={cn(
                    "text-subtle",
                    collapsed ? "self-center" : "mr-1 opacity-50 hover:opacity-100 focus-visible:opacity-100",
                  )}
                  aria-expanded={!collapsed}
                  onClick={onToggle}
                >
                  {collapsed ? <PanelLeftOpen className="size-4" /> : <PanelLeftClose className="size-3.5" />}
                </IconButton>
                {toggleTip.tip}
              </>
            ) : null}
          </div>
        ) : null}
      </nav>
    </Narrow.Provider>
  );
}

/**
 * Discover, where this account may ask for titles, and its requests where
 * it has any (everyone's, for managers), with how many wait to be looked
 * at: in the danger color while some failed.
 */
function RequestsNav() {
  const { t } = useTranslation("components");
  const summary = useRequestSummary();
  const data = summary.data;
  if (!hasRequests(data) && !data?.manage) return null;
  const { count, danger } = requestsBadge(data);
  return (
    <>
      {mayRequest(data) ? (
        <NavItem to={discoverUrl()} icon={<Compass className="size-4" />}>
          {t("sidebar.discover")}
        </NavItem>
      ) : null}
      <NavItem
        to={requestsUrl}
        name={data?.manage ? t("sidebar.requests") : t("sidebar.yourRequests")}
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
                {count > 99 ? "99+" : count} <span className="sr-only">{t("sidebar.waiting")}</span>
              </span>
            </>
          ) : null
        }
      >
        {data?.manage ? t("sidebar.requests") : t("sidebar.yourRequests")}
      </NavItem>
    </>
  );
}

export function Sidebar({
  open,
  onClose,
  collapsed = false,
  onToggle,
}: {
  open: boolean;
  onClose: () => void;
  /** Narrowed to its icons, on a wide screen (the drawer is always whole). */
  collapsed?: boolean;
  onToggle?: () => void;
}) {
  const { t } = useTranslation("components");
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
      <aside
        className={cn(
          "fixed inset-y-0 left-0 z-30 hidden border-r border-border bg-surface transition-[width] duration-200 motion-reduce:transition-none lg:block",
          collapsed ? "w-16" : "w-60",
        )}
      >
        <SidebarContent collapsed={collapsed} onToggle={onToggle} />
      </aside>
      {open ? (
        <div className="fixed inset-0 z-50 lg:hidden">
          <div className="absolute inset-0 animate-fade-in bg-black/60" onClick={onClose} />
          <aside
            ref={drawerRef}
            role="dialog"
            aria-modal="true"
            aria-label={t("sidebar.menu")}
            className="absolute inset-y-0 left-0 w-72 max-w-[85vw] animate-pop-in border-r border-border bg-surface shadow-2xl"
          >
            <SidebarContent onClose={onClose} />
          </aside>
        </div>
      ) : null}
    </>
  );
}
