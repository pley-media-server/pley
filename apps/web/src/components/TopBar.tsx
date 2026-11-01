import { useEffect, useRef, useState } from "react";
import { Link, useLocation, useNavigate, useSearchParams } from "react-router";
import { LogOut, Menu as MenuIcon, RefreshCw, Search, Server, Settings, UserRound, Users, X } from "lucide-react";
import { useActivity, useLibraries, useLogout, useProfiles } from "@/api/hooks";
import { useRequestSummary } from "@/api/requests";
import type { ScanPhase } from "@/api/types";
import { useAudioPlayer } from "@/audio/AudioPlayer";
import { useAuth } from "@/auth";
import { cn } from "@/lib/cn";
import { pluralize } from "@/lib/format";
import { useDebouncedValue, useOnChange } from "@/lib/hooks";
import { requestsBadge } from "@/lib/requests";
import { appShell, SWITCH_SERVER_PATH } from "@/lib/shell";
import { AppDownloads, DOWNLOADS_PLACE } from "./AppDownloads";
import { Avatar } from "./Avatar";
import { Menu, MenuItem, MenuSeparator } from "./Menu";
import { IconButton } from "./ui";

function isTypingTarget(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el) return false;
  return el.isContentEditable || el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT";
}

function SearchBox({ className }: { className?: string }) {
  const navigate = useNavigate();
  const location = useLocation();
  const [params] = useSearchParams();
  const onSearchPage = location.pathname === "/search";
  const urlQuery = onSearchPage ? (params.get("q") ?? "") : "";
  const [value, setValue] = useState(urlQuery);
  const inputRef = useRef<HTMLInputElement>(null);
  const debounced = useDebouncedValue(value, 250);

  // Leaving the search page clears the box; arriving syncs it with the URL.
  useOnChange(onSearchPage, () => setValue(urlQuery));

  useEffect(() => {
    const q = debounced.trim();
    if (q === urlQuery.trim()) return;
    if (!q && !onSearchPage) return;
    void navigate(q ? `/search?q=${encodeURIComponent(q)}` : "/search", { replace: onSearchPage });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [debounced]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "/" && !isTypingTarget(e.target) && !e.metaKey && !e.ctrlKey) {
        e.preventDefault();
        inputRef.current?.focus();
        inputRef.current?.select();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className={cn("relative", className)}>
      <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-subtle" />
      <input
        ref={inputRef}
        type="search"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            const q = value.trim();
            if (q) void navigate(`/search?q=${encodeURIComponent(q)}`, { replace: onSearchPage });
          }
          if (e.key === "Escape") {
            setValue("");
            inputRef.current?.blur();
          }
        }}
        placeholder="Search"
        aria-label="Search your libraries"
        className="h-10 w-full rounded-full border border-transparent bg-surface-2 pl-9 pr-9 text-sm text-fg placeholder:text-subtle transition-colors focus:border-accent/70 focus:bg-surface-3 focus:outline-none [&::-webkit-search-cancel-button]:hidden"
      />
      {value ? (
        <button
          type="button"
          aria-label="Clear search"
          onClick={() => {
            setValue("");
            inputRef.current?.focus();
          }}
          className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded-full p-1 text-subtle hover:text-fg"
        >
          <X className="size-3.5" />
        </button>
      ) : (
        <kbd className="pointer-events-none absolute right-3 top-1/2 hidden -translate-y-1/2 rounded border border-border px-1.5 text-[10px] font-medium text-subtle md:block">
          /
        </kbd>
      )}
    </div>
  );
}

const PHASES: Record<ScanPhase, string> = {
  queued: "Queued",
  walking: "Finding files",
  probing: "Analyzing",
  metadata: "Fetching metadata",
  done: "Done",
};

function ScanIndicator() {
  const user = useAuth();
  const activity = useActivity();
  const libs = useLibraries();
  const scans = (activity.data?.scans ?? []).filter((s) => s.phase !== "done");
  const scanningLibs = (libs.data ?? []).filter((l) => l.scanning);
  const count = Math.max(scans.length, scanningLibs.length);
  if (count === 0) return null;

  const title = scans.length
    ? scans
        .map((s) => `${s.library_name}: ${PHASES[s.phase]}${s.total ? ` ${Math.round((s.processed / s.total) * 100)}%` : ""}`)
        .join("\n")
    : scanningLibs.map((l) => `${l.name}: scanning`).join("\n");
  const first = scans[0]?.library_name ?? scanningLibs[0]?.name ?? "";
  const label = count === 1 ? `Scanning ${first}` : `Scanning ${count} libraries`;
  const content = (
    <>
      <RefreshCw className="size-4 animate-spin text-accent" />
      <span className="hidden max-w-48 truncate text-xs font-medium md:inline">{label}</span>
    </>
  );
  const cls = "flex h-9 items-center gap-2 rounded-full px-3 text-muted transition-colors hover:bg-white/5 hover:text-fg";
  return user.is_admin ? (
    <Link to="/settings/dashboard" title={title} className={cls} aria-label={label}>
      {content}
    </Link>
  ) : (
    <div title={title} className={cls} aria-label={label}>
      {content}
    </div>
  );
}

function UserMenu() {
  const user = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const logout = useLogout();
  const audio = useAudioPlayer();
  const profiles = useProfiles(user.home);
  const canSwitch = (profiles.data?.length ?? 0) > 1;

  return (
    <Menu
      trigger={({ toggle, open }) => (
        <button
          type="button"
          onClick={toggle}
          aria-label="Account menu"
          aria-expanded={open}
          className="flex size-9 items-center justify-center rounded-full ring-accent ring-offset-2 ring-offset-bg transition hover:ring-2"
        >
          <Avatar user={user} size="md" />
        </button>
      )}
    >
      {(close) => (
        <>
          <div className="flex items-center gap-3 px-3.5 pb-2 pt-1.5">
            <Avatar user={user} size="sm" />
            <div className="min-w-0">
              <div className="truncate text-sm font-semibold">{user.name}</div>
              <div className="text-xs text-subtle">
                {user.is_admin ? "Administrator" : user.managed ? "Managed profile" : "User"}
              </div>
            </div>
          </div>
          <MenuSeparator />
          {canSwitch ? (
            <MenuItem
              icon={<Users />}
              onClick={() => {
                close();
                void navigate(`/profiles?next=${encodeURIComponent(location.pathname + location.search)}`);
              }}
            >
              Switch profile
            </MenuItem>
          ) : null}
          <MenuItem
            icon={<UserRound />}
            onClick={() => {
              close();
              void navigate("/account");
            }}
          >
            Account
          </MenuItem>
          {user.is_admin ? (
            <MenuItem
              icon={<Settings />}
              onClick={() => {
                close();
                void navigate("/settings");
              }}
            >
              Settings
            </MenuItem>
          ) : null}
          <MenuSeparator />
          {appShell() ? (
            <MenuItem
              icon={<Server />}
              onClick={() => {
                close();
                audio.close();
                // A page load, not a route: the app catches it and shows its servers.
                window.location.assign(SWITCH_SERVER_PATH);
              }}
            >
              {/* The desktop app's picker lists its downloads too. */}
              {appShell() === "desktop" ? DOWNLOADS_PLACE : "Switch server"}
            </MenuItem>
          ) : null}
          <MenuItem
            icon={<LogOut />}
            onClick={() => {
              close();
              audio.close();
              logout.mutate(undefined, { onSettled: () => void navigate("/login", { replace: true }) });
            }}
          >
            Sign out
          </MenuItem>
        </>
      )}
    </Menu>
  );
}

/** The menu's button, small screens' way to the sidebar: it carries the number on Requests, as the sidebar's entry does. */
function MenuButton({ onMenu }: { onMenu: () => void }) {
  const summary = useRequestSummary();
  const { count, danger } = requestsBadge(summary.data);
  return (
    <IconButton
      className="relative lg:hidden"
      label={count > 0 ? `Open menu, ${pluralize(count, "request")} waiting` : "Open menu"}
      onClick={onMenu}
    >
      <MenuIcon className="size-5" />
      {count > 0 ? (
        <span
          aria-hidden
          className={cn(
            "absolute right-1 top-1 min-w-4 rounded-full px-1 text-center text-[10px] font-bold leading-4",
            danger ? "bg-danger text-white" : "bg-accent text-accent-fg",
          )}
        >
          {count > 9 ? "9+" : count}
        </span>
      ) : null}
    </IconButton>
  );
}

export function TopBar({ onMenu }: { onMenu: () => void }) {
  return (
    <header className="sticky top-0 z-20 flex h-16 items-center gap-2 border-b border-white/[0.04] bg-bg/85 px-3 backdrop-blur-md sm:gap-3 sm:px-6 lg:px-8">
      <MenuButton onMenu={onMenu} />
      <SearchBox className="min-w-0 max-w-xl flex-1" />
      <div className="ml-auto flex items-center gap-1.5">
        {appShell() === "desktop" ? <AppDownloads /> : null}
        <ScanIndicator />
        <UserMenu />
      </div>
    </header>
  );
}
