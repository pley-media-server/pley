import { Suspense } from "react";
import { NavLink, Outlet } from "react-router";
import { Cpu, Inbox, LayoutDashboard, LibraryBig, MailPlus, Network, Server, Tv2, Users } from "lucide-react";
import { RequireAdmin } from "@/auth";
import { PageSpinner } from "@/components/ui";
import { cn } from "@/lib/cn";
import { useDocumentTitle } from "@/lib/hooks";

/** The first is where Settings opens. */
const NAV = [
  { to: "/settings/dashboard", label: "Dashboard", icon: LayoutDashboard },
  { to: "/settings/libraries", label: "Libraries", icon: LibraryBig },
  { to: "/settings/livetv", label: "Live TV", icon: Tv2 },
  { to: "/settings/users", label: "Users", icon: Users },
  { to: "/settings/invitations", label: "Invitations", icon: MailPlus },
  { to: "/settings/requests", label: "Requests", icon: Inbox },
  { to: "/settings/server", label: "Server", icon: Server },
  { to: "/settings/transcoder", label: "Transcoder", icon: Cpu },
  { to: "/settings/network", label: "Network", icon: Network },
];

export function SettingsLayout() {
  useDocumentTitle("Settings");
  return (
    <RequireAdmin>
      <div className="px-4 py-6 sm:px-8">
        <h1 className="mb-5 text-2xl font-bold tracking-tight">Settings</h1>
        <div className="flex flex-col gap-6 md:flex-row md:gap-8">
          <nav className="scrollbar-none -mx-4 flex gap-1 overflow-x-auto px-4 md:mx-0 md:w-48 md:shrink-0 md:flex-col md:px-0" aria-label="Settings">
            {NAV.map(({ to, label, icon: Icon }) => (
              <NavLink
                key={to}
                to={to}
                className={({ isActive }) =>
                  cn(
                    "flex h-9 shrink-0 items-center gap-2.5 rounded-md px-3 text-sm font-medium transition-colors",
                    isActive ? "bg-white/[0.07] text-fg" : "text-muted hover:bg-white/[0.04] hover:text-fg",
                  )
                }
              >
                {({ isActive }) => (
                  <>
                    <Icon className={cn("size-4", isActive ? "text-accent" : "text-subtle")} />
                    {label}
                  </>
                )}
              </NavLink>
            ))}
          </nav>
          <div className="min-w-0 flex-1">
            <Suspense fallback={<PageSpinner />}>
              <Outlet />
            </Suspense>
          </div>
        </div>
      </div>
    </RequireAdmin>
  );
}
