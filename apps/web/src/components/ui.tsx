import { forwardRef, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode, type SelectHTMLAttributes } from "react";
import { LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/cn";

type Variant = "primary" | "secondary" | "ghost" | "danger" | "outline";
type Size = "sm" | "md" | "lg";

const variants: Record<Variant, string> = {
  primary: "bg-accent text-accent-fg hover:bg-accent-hover shadow-sm shadow-black/30",
  secondary: "bg-surface-3 text-fg hover:bg-border",
  ghost: "bg-transparent text-muted hover:bg-white/5 hover:text-fg",
  danger: "bg-danger/90 text-white hover:bg-danger",
  outline: "border border-border-strong text-fg hover:bg-white/5",
};

const sizes: Record<Size, string> = {
  sm: "h-8 px-3 text-sm gap-1.5",
  md: "h-10 px-4 text-sm gap-2",
  lg: "h-12 px-6 text-base gap-2",
};

/** A button's look, for links. */
export function buttonClass(variant: Variant = "secondary", size: Size = "md", className?: string): string {
  return cn(
    "inline-flex select-none items-center justify-center whitespace-nowrap rounded-md font-medium transition-colors duration-150",
    variants[variant],
    sizes[size],
    className,
  );
}

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
  loading?: boolean;
  icon?: ReactNode;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "secondary", size = "md", loading, icon, className, children, disabled, type = "button", ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      disabled={disabled || loading}
      className={buttonClass(variant, size, cn("disabled:opacity-50", className))}
      {...rest}
    >
      {loading ? <LoaderCircle className="size-4 animate-spin" aria-hidden /> : icon}
      {children}
    </button>
  );
});

export interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  label: string;
  /** xs 28px, sm 32px, md 40px, lg 48px. A size class in `className` doesn't override it. */
  size?: "xs" | "sm" | "md" | "lg";
  active?: boolean;
}

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { label, size = "md", active, className, children, type = "button", ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      aria-label={label}
      title={label}
      className={cn(
        "inline-flex shrink-0 items-center justify-center rounded-full text-muted transition-colors duration-150 hover:bg-white/10 hover:text-fg disabled:opacity-40",
        size === "xs" && "size-7",
        size === "sm" && "size-8",
        size === "md" && "size-10",
        size === "lg" && "size-12",
        active && "text-accent hover:text-accent",
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
});

export const TextInput = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(function TextInput(
  { className, ...rest },
  ref,
) {
  return (
    <input
      ref={ref}
      className={cn(
        "h-10 w-full rounded-md border border-border bg-surface-2 px-3 text-sm text-fg placeholder:text-subtle transition-colors focus:border-accent focus:outline-none disabled:opacity-60",
        className,
      )}
      {...rest}
    />
  );
});

export const Select = forwardRef<HTMLSelectElement, SelectHTMLAttributes<HTMLSelectElement>>(function Select(
  { className, children, ...rest },
  ref,
) {
  return (
    <select
      ref={ref}
      className={cn(
        "h-9 rounded-md border border-border bg-surface-2 px-2.5 pr-8 text-sm text-fg transition-colors hover:border-border-strong focus:border-accent focus:outline-none",
        className,
      )}
      {...rest}
    >
      {children}
    </select>
  );
});

export function Field({
  label,
  hint,
  error,
  children,
  htmlFor,
}: {
  label: string;
  hint?: ReactNode;
  error?: string | null;
  children: ReactNode;
  htmlFor?: string;
}) {
  return (
    <div className="space-y-1.5">
      <label htmlFor={htmlFor} className="block text-sm font-medium text-fg">
        {label}
      </label>
      {children}
      {error ? <p className="text-xs text-danger">{error}</p> : hint ? <p className="text-xs text-subtle">{hint}</p> : null}
    </div>
  );
}

export function Toggle({
  id,
  checked,
  onChange,
  label,
  disabled,
}: {
  /** For a `<label htmlFor>` elsewhere. */
  id?: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      id={id}
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors duration-150 disabled:opacity-50",
        checked ? "bg-accent" : "bg-surface-3",
      )}
    >
      <span
        className={cn(
          "inline-block size-5 rounded-full bg-white shadow transition-transform duration-150",
          checked ? "translate-x-5.5" : "translate-x-0.5",
        )}
      />
    </button>
  );
}

export function Badge({
  children,
  className,
  tone = "neutral",
}: {
  children: ReactNode;
  className?: string;
  tone?: "neutral" | "accent" | "success" | "danger" | "info" | "warning";
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-xs font-semibold",
        tone === "neutral" && "bg-white/10 text-fg/90",
        tone === "accent" && "bg-accent/15 text-accent",
        tone === "success" && "bg-success/15 text-success",
        tone === "danger" && "bg-danger/15 text-danger",
        tone === "info" && "bg-info/15 text-info",
        tone === "warning" && "bg-warning/15 text-warning",
        className,
      )}
    >
      {children}
    </span>
  );
}

export function Spinner({ className, label }: { className?: string; label?: string }) {
  const { t } = useTranslation("components");
  return <LoaderCircle role="status" aria-label={label ?? t("ui.loading")} className={cn("size-5 animate-spin text-muted", className)} />;
}

export function PageSpinner() {
  return (
    <div className="flex min-h-[40vh] items-center justify-center">
      <Spinner className="size-8" />
    </div>
  );
}

export function Skeleton({ className }: { className?: string }) {
  return <div className={cn("skeleton rounded-md", className)} />;
}

export function EmptyState({
  icon,
  title,
  description,
  action,
  className,
}: {
  icon?: ReactNode;
  title: string;
  description?: ReactNode;
  action?: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-col items-center justify-center px-6 py-16 text-center", className)}>
      {icon ? <div className="mb-4 text-subtle [&>svg]:size-12">{icon}</div> : null}
      <h2 className="text-lg font-semibold text-fg">{title}</h2>
      {description ? <p className="mt-1.5 max-w-md text-sm text-muted">{description}</p> : null}
      {action ? <div className="mt-6">{action}</div> : null}
    </div>
  );
}

/** How far something is (0–1); with a `label`, a progress bar screen readers read, with its value. */
export function ProgressBar({ value, className, barClassName, label }: { value: number; className?: string; barClassName?: string; label?: string }) {
  const pct = Math.max(0, Math.min(1, value)) * 100;
  const named = label ? { role: "progressbar", "aria-label": label, "aria-valuemin": 0, "aria-valuemax": 100, "aria-valuenow": Math.round(pct) } : {};
  return (
    <div className={cn("h-1 w-full overflow-hidden rounded-full bg-white/15", className)} {...named}>
      <div className={cn("h-full rounded-full bg-accent transition-[width] duration-300", barClassName)} style={{ width: `${pct}%` }} />
    </div>
  );
}

export function Card({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn("rounded-lg border border-border bg-surface p-5", className)}>{children}</div>;
}

/** A card's heading, with an icon in the accent color. */
export function CardTitle({ icon, children }: { icon: ReactNode; children: ReactNode }) {
  return (
    <h2 className="mb-4 flex items-center gap-2 text-base font-semibold">
      <span className="text-accent [&>svg]:size-4">{icon}</span> {children}
    </h2>
  );
}

export function SectionTitle({ children, action, className }: { children: ReactNode; action?: ReactNode; className?: string }) {
  return (
    <div className={cn("mb-3 flex items-center justify-between gap-3", className)}>
      <h2 className="text-lg font-semibold tracking-tight text-fg">{children}</h2>
      {action}
    </div>
  );
}
