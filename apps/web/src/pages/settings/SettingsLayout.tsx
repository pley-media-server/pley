import { Suspense } from "react";
import { useTranslation } from "react-i18next";
import { Outlet } from "react-router";
import { Cpu, Inbox, LayoutDashboard, LibraryBig, MailPlus, Network, Server, Tv2, Users } from "lucide-react";
import { RequireAdmin } from "@/auth";
import { SectionedPage } from "@/components/SideNav";
import { PageSpinner } from "@/components/ui";
import { useDocumentTitle } from "@/lib/hooks";

/** The first is where Settings opens. */
const NAV = [
  { to: "/settings/dashboard", label: "dashboard", icon: LayoutDashboard },
  { to: "/settings/libraries", label: "libraries", icon: LibraryBig },
  { to: "/settings/livetv", label: "livetv", icon: Tv2 },
  { to: "/settings/users", label: "users", icon: Users },
  { to: "/settings/invitations", label: "invitations", icon: MailPlus },
  { to: "/settings/requests", label: "requests", icon: Inbox },
  { to: "/settings/server", label: "server", icon: Server },
  { to: "/settings/transcoder", label: "transcoder", icon: Cpu },
  { to: "/settings/network", label: "network", icon: Network },
] as const;

export function SettingsLayout() {
  const { t } = useTranslation("settings");
  useDocumentTitle(t("layout.title"));
  return (
    <RequireAdmin>
      <SectionedPage title={t("layout.title")} items={NAV.map(({ to, label, icon }) => ({ to, label: t(`layout.nav.${label}`), icon }))}>
        <Suspense fallback={<PageSpinner />}>
          <Outlet />
        </Suspense>
      </SectionedPage>
    </RequireAdmin>
  );
}
