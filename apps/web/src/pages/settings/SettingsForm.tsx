import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useLocation } from "react-router";
import { CircleAlert, CircleCheck, Eye, EyeOff, Info, Save } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useSaveSettings, useSettings, useTestService } from "@/api/hooks";
import type { ServiceTest, Settings } from "@/api/types";
import { useToast } from "@/components/Toast";
import { Button, Card, EmptyState, IconButton, PageSpinner, TextInput } from "@/components/ui";
import { cn } from "@/lib/cn";
import { useOnChange } from "@/lib/hooks";

/*
 * What the settings pages that edit the server's settings (Server,
 * Transcoder, Requests) share: the form, its save bar, and the rows it is
 * made of.
 */

export function Group({ title, description, children }: { title: string; description?: string; children: ReactNode }) {
  return (
    <Card className="space-y-5">
      <div>
        <h3 className="text-base font-semibold">{title}</h3>
        {description ? <p className="mt-0.5 text-sm text-muted">{description}</p> : null}
      </div>
      <div className="space-y-5">{children}</div>
    </Card>
  );
}

export function Row({ label, hint, children, htmlFor }: { label: string; hint?: ReactNode; children: ReactNode; htmlFor?: string }) {
  return (
    <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)] sm:items-center sm:gap-6">
      <div>
        <label htmlFor={htmlFor} className="text-sm font-medium">
          {label}
        </label>
        {hint ? <div className="mt-0.5 text-xs text-subtle">{hint}</div> : null}
      </div>
      <div className="min-w-0">{children}</div>
    </div>
  );
}

/**
 * Under a choice: why it is greyed out here, or (`notice`) what happens
 * instead of a choice saved before that can't work here.
 */
export function Note({ notice, children }: { notice?: boolean; children: ReactNode }) {
  return (
    <p className={cn("mt-1.5 flex items-start gap-1.5 text-xs", notice ? "text-fg/80" : "text-subtle")}>
      {notice ? <CircleAlert className="mt-px size-3.5 shrink-0 text-danger" /> : <Info className="mt-px size-3.5 shrink-0" />}
      <span>{children}</span>
    </p>
  );
}

/** A key or PIN: hidden unless asked for. */
export function SecretInput({
  id,
  value,
  onChange,
  what,
  placeholder,
  required,
  className,
}: {
  id: string;
  value: string;
  onChange: (value: string) => void;
  what: "key" | "pin";
  placeholder?: string;
  required?: boolean;
  className?: string;
}) {
  const { t } = useTranslation("settings");
  const [visible, setVisible] = useState(false);
  const label = t(`form.secret.${visible ? "hide" : "show"}${what === "pin" ? "Pin" : "Key"}`);
  return (
    <div className="flex gap-2">
      <TextInput
        id={id}
        type={visible ? "text" : "password"}
        autoComplete="off"
        spellCheck={false}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder ?? t("form.notSet")}
        required={required}
        className={cn("font-mono text-xs", className)}
      />
      <IconButton label={label} onClick={() => setVisible((s) => !s)} className="rounded-md bg-surface-2">
        {visible ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
      </IconButton>
    </div>
  );
}

/** A link out of Pley; in translated text, `<Trans>` gives it its words. */
export function Link({ href, children }: { href: string; children?: ReactNode }) {
  return (
    <a href={href} target="_blank" rel="noreferrer" className="text-accent hover:underline">
      {children}
    </a>
  );
}

/**
 * Tests a service with what the form holds (a key not saved yet) and says
 * how it went. Give it a `key` that changes with the key it tests, so an
 * answer about another key doesn't linger.
 */
export function TestButton({ test, label, className = "mt-2" }: { test: ServiceTest; label: string; className?: string }) {
  const { t } = useTranslation("settings");
  const check = useTestService();
  const result = check.data;
  return (
    <div className={cn("flex min-w-0 items-start gap-3", className)}>
      <Button size="sm" variant="outline" loading={check.isPending} onClick={() => check.mutate(test)} aria-label={label} className="shrink-0">
        {t("form.test")}
      </Button>
      <div role="status" className="min-w-0 pt-1 text-xs">
        {check.isError ? (
          <span className="text-danger">{errorMessage(check.error)}</span>
        ) : result ? (
          <span className={cn("flex items-start gap-1.5", result.ok ? "text-success" : "text-danger")}>
            {result.ok ? <CircleCheck className="mt-px size-3.5 shrink-0" /> : <CircleAlert className="mt-px size-3.5 shrink-0" />}
            <span>
              {result.message}
              {result.ok ? <span className="text-subtle"> {result.ms} ms</span> : null}
            </span>
          </span>
        ) : null}
      </div>
    </div>
  );
}

/**
 * The settings a page edits (`keys`, a constant): the form starts from the
 * saved settings, which are already there when the page opens again (cached)
 * or was opened by its route, and returns to them after saving. Saving sends
 * the page's own changes only.
 */
export function useSettingsForm(keys: readonly (keyof Settings)[]) {
  const settings = useSettings();
  const [form, setForm] = useState<Settings | null>(() => settings.data ?? null);

  // A link to one field (`#tmdb-key`) focuses it once the form is there.
  const { hash } = useLocation();
  const loaded = form !== null;
  useEffect(() => {
    if (loaded && hash) document.getElementById(decodeURIComponent(hash.slice(1)))?.focus();
  }, [loaded, hash]);

  useOnChange(settings.data, (data) => {
    if (data) setForm(data);
  });

  const changes = useMemo(() => {
    const changed: Partial<Settings> = {};
    if (!form || !settings.data) return changed;
    for (const k of keys) {
      if (form[k] !== settings.data[k]) (changed as Record<string, unknown>)[k] = form[k];
    }
    return changed;
  }, [form, settings.data, keys]);

  const set = <K extends keyof Settings>(key: K, value: Settings[K]) => setForm((f) => (f ? { ...f, [key]: value } : f));
  return { settings, form, setForm, set, changes, dirty: Object.keys(changes).length > 0 };
}

/** A page of settings: loading, the rows `children` makes of the form, and the save bar. */
export function SettingsForm({
  draft,
  title,
  description,
  onSaved,
  children,
}: {
  draft: ReturnType<typeof useSettingsForm>;
  title: string;
  description?: string;
  onSaved?: (changes: Partial<Settings>) => void;
  children: (form: Settings) => ReactNode;
}) {
  const { t } = useTranslation(["settings", "common"]);
  const { settings, form, setForm, changes, dirty } = draft;
  const save = useSaveSettings();
  const toast = useToast();

  if (settings.isPending || (!form && !settings.isError)) return <PageSpinner />;
  if (settings.isError || !form) {
    return (
      <EmptyState
        title={t("form.couldntLoad")}
        description={errorMessage(settings.error)}
        action={<Button onClick={() => void settings.refetch()}>{t("common:actions.retry")}</Button>}
      />
    );
  }

  const onSave = () => {
    if (!dirty) return;
    save.mutate(changes, {
      onSuccess: () => {
        toast.success(t("form.saved"));
        onSaved?.(changes);
      },
      onError: (e) => toast.error(errorMessage(e)),
    });
  };

  return (
    <form
      className="space-y-5 pb-20"
      onSubmit={(e) => {
        e.preventDefault();
        onSave();
      }}
    >
      <div>
        <h2 className="text-lg font-semibold">{title}</h2>
        <p className="text-sm text-muted">{description ?? t("form.applyNow")}</p>
      </div>

      {children(form)}

      {/* Floats over the page only with something to save: at rest it would cover the rows on a phone. */}
      <div className={cn("flex justify-end", (dirty || save.isPending) && "sticky bottom-4 z-10")}>
        <div className="flex items-center gap-3 rounded-lg border border-border bg-surface-2/95 px-3 py-2 shadow-xl backdrop-blur">
          {dirty ? <span className="text-xs text-muted">{t("form.unsaved")}</span> : null}
          <Button variant="ghost" size="sm" disabled={!dirty || save.isPending} onClick={() => settings.data && setForm(settings.data)}>
            {t("common:actions.reset")}
          </Button>
          <Button type="submit" variant="primary" size="sm" icon={<Save className="size-4" />} loading={save.isPending} disabled={!dirty}>
            {t("common:actions.save")}
          </Button>
        </div>
      </div>
    </form>
  );
}
