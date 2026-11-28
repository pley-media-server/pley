import type { ReactNode } from "react";
import { Check, ChevronDown, type LucideIcon } from "lucide-react";
import { NavLink, useLocation, useNavigate } from "react-router";
import { Menu } from "@/components/Menu";
import { cn } from "@/lib/cn";

type Item = { to: string; label: string; icon: LucideIcon };

/**
 * A page's sections (Settings, Account): down its left side, the one shown
 * marked; in a narrow window, one picker showing the section, whose menu
 * lists them all.
 */
export function SideNav({ label, items }: { label: string; items: readonly Item[] }) {
  const { pathname } = useLocation();
  const navigate = useNavigate();
  const current = items.find((item) => pathname === item.to || pathname.startsWith(`${item.to}/`)) ?? items[0]!;
  const Icon = current.icon;

  return (
    <>
      <div className="md:hidden">
        <Menu
          label={label}
          align="left"
          className="right-0 px-1.5"
          trigger={({ open, toggle }) => (
            <button
              type="button"
              onClick={toggle}
              aria-haspopup="menu"
              aria-expanded={open}
              aria-label={`${label}: ${current.label}`}
              className={cn(
                "flex h-14 w-full items-center gap-3 rounded-xl border bg-surface bg-(image:--sidebar) pl-2.5 pr-3.5 text-left shadow-sm shadow-shade/20 transition-colors",
                open ? "border-accent" : "border-border hover:border-border-strong",
              )}
            >
              <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-accent text-accent-fg shadow-sm">
                <Icon className="size-[18px]" />
              </span>
              <span className="min-w-0 flex-1 truncate text-[15px] font-semibold text-fg">{current.label}</span>
              <ChevronDown className={cn("size-5 shrink-0 text-muted transition-transform duration-200", open && "rotate-180")} />
            </button>
          )}
        >
          {(close) =>
            items.map((item) => {
              const here = item === current;
              const ItemIcon = item.icon;
              return (
                <button
                  key={item.to}
                  type="button"
                  role="menuitem"
                  aria-current={here ? "page" : undefined}
                  onClick={() => {
                    close();
                    if (!here) void navigate(item.to);
                  }}
                  className={cn(
                    "my-0.5 flex w-full items-center gap-3 rounded-lg px-2 py-1.5 text-left text-sm font-medium outline-none transition-colors",
                    here ? "bg-accent/10 text-fg" : "text-muted hover:bg-fg/5 hover:text-fg focus-visible:bg-fg/10 focus-visible:text-fg",
                  )}
                >
                  <span
                    className={cn(
                      "flex size-8 shrink-0 items-center justify-center rounded-lg",
                      here ? "bg-accent text-accent-fg" : "bg-fg/[0.06] text-subtle",
                    )}
                  >
                    <ItemIcon className="size-4" />
                  </span>
                  <span className="min-w-0 flex-1 truncate">{item.label}</span>
                  {here ? <Check className="size-4 shrink-0 text-accent" strokeWidth={2.5} /> : null}
                </button>
              );
            })
          }
        </Menu>
      </div>
      <nav className="hidden gap-1 md:flex md:w-52 md:shrink-0 md:flex-col" aria-label={label}>
        {items.map(({ to, label: name, icon: ItemIcon }) => (
          <NavLink
            key={to}
            to={to}
            className={({ isActive }) =>
              cn(
                "flex h-8 shrink-0 items-center gap-2.5 rounded-md px-3 text-sm font-medium whitespace-nowrap transition-colors",
                isActive ? "bg-fg/[0.07] text-fg" : "text-muted hover:bg-fg/[0.04] hover:text-fg",
              )
            }
          >
            {({ isActive }) => (
              <>
                <ItemIcon className={cn("size-4 shrink-0", isActive ? "text-accent" : "text-subtle")} />
                {name}
              </>
            )}
          </NavLink>
        ))}
      </nav>
    </>
  );
}

/**
 * A page of sections (Settings, Account), the same for each: its title, its
 * sections (SideNav) and the section shown beside them, its cards 1.25rem
 * apart.
 */
export function SectionedPage({ title, items, children }: { title: string; items: readonly Item[]; children: ReactNode }) {
  return (
    <div className="px-4 py-6 sm:px-8">
      <h1 className="mb-5 text-2xl font-bold tracking-tight">{title}</h1>
      <div className="flex flex-col gap-6 md:flex-row md:gap-8">
        <SideNav label={title} items={items} />
        <div className="min-w-0 flex-1 space-y-5">{children}</div>
      </div>
    </div>
  );
}
