import { useTranslation } from "react-i18next";
import { Lock } from "lucide-react";
import type { ImportPlan, PlanLimits, PlanUser } from "@/api/plexImport";
import { useRatingSystems } from "@/api/hooks";
import { Badge, Button, TextInput, Toggle } from "@/components/ui";
import { ageBadge } from "@/lib/access";
import { ErrorNote, StepFrame } from "./PlexImport";

/** By Plex user's id: whether they come, and their Plex PIN. */
export type PeopleChoices = Record<number, { bring: boolean; pin: string }>;

export function PeopleStep({
  plan,
  choices,
  onChange,
  onBack,
  onStart,
  starting,
  error,
}: {
  plan: ImportPlan;
  choices: PeopleChoices;
  onChange: (choices: PeopleChoices) => void;
  onBack: () => void;
  onStart: () => void;
  starting: boolean;
  error: string | null;
}) {
  const { t } = useTranslation(["settings", "common"]);
  // The Plex account first: it always comes, as the administrator.
  const users = [...plan.users].sort((a, b) => Number(b.admin) - Number(a.admin));

  return (
    <StepFrame
      stage="people"
      title={t("plexImport.people.title")}
      description={t("plexImport.people.description")}
      actions={
        <>
          <Button variant="ghost" onClick={onBack} disabled={starting}>
            {t("common:actions.back")}
          </Button>
          <Button variant="primary" onClick={onStart} loading={starting}>
            {t("plexImport.people.start")}
          </Button>
        </>
      }
    >
      <ul className="divide-y divide-border rounded-lg border border-border bg-surface-2">
        {users.map((user) => (
          <li key={user.id} className="p-3.5">
            <Person
              user={user}
              choice={choices[user.id] ?? { bring: true, pin: "" }}
              onChange={(c) => onChange({ ...choices, [user.id]: c })}
            />
          </li>
        ))}
      </ul>
      {error ? <ErrorNote>{error}</ErrorNote> : null}
    </StepFrame>
  );
}

function Person({ user, choice, onChange }: { user: PlanUser; choice: { bring: boolean; pin: string }; onChange: (c: { bring: boolean; pin: string }) => void }) {
  const { t } = useTranslation("settings");
  const id = `plex-user-${user.id}`;
  const bring = user.admin || choice.bring;
  return (
    <div className="space-y-2">
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-medium">{user.title}</span>
            {user.protected ? <Lock aria-label={t("plexImport.people.hasPin")} className="size-3.5 text-subtle" /> : null}
          </div>
          <p className="text-xs text-muted">
            {user.admin ? t("plexImport.people.you") : user.profile_id != null ? t("plexImport.people.found") : t("plexImport.people.newProfile")}
          </p>
        </div>
        {user.admin ? null : <Toggle checked={bring} onChange={(v) => onChange({ ...choice, bring: v })} label={t("plexImport.people.bring", { name: user.title })} />}
      </div>
      {bring && !user.admin ? <Limits limits={user.limits} restricted={user.restricted} /> : null}
      {bring && user.protected && !user.admin ? (
        <div className="flex flex-wrap items-center gap-2">
          <label htmlFor={`${id}-pin`} className="text-xs text-muted">
            {t("plexImport.people.pin")}
          </label>
          <div className="w-28">
            <TextInput
              id={`${id}-pin`}
              type="password"
              inputMode="numeric"
              autoComplete="off"
              maxLength={16}
              value={choice.pin}
              onChange={(e) => onChange({ ...choice, pin: e.target.value.replace(/\D/g, "") })}
              className="h-8 font-mono"
            />
          </div>
          <span className="text-xs text-subtle">{t("plexImport.people.pinHint")}</span>
        </div>
      ) : null}
    </div>
  );
}

/** What the profile may see, as Plex's restrictions become Pley's limits. */
function Limits({ limits, restricted }: { limits: PlanLimits; restricted: boolean }) {
  const { t } = useTranslation("settings");
  const systems = useRatingSystems();
  if (!restricted) return <p className="text-xs text-subtle">{t("plexImport.people.noLimits")}</p>;
  const age = ageBadge(systems.data, limits.max_age_system, limits.max_age);
  return (
    <div className="space-y-1.5 text-xs">
      <div className="flex flex-wrap items-center gap-1.5">
        <Badge tone="info">{age ?? t("plexImport.people.anyAge")}</Badge>
        {limits.show_labels.map((l) => (
          <Badge key={`show-${l}`} tone="success">
            {t("plexImport.people.showLabel", { label: l })}
          </Badge>
        ))}
        {limits.hide_labels.map((l) => (
          <Badge key={`hide-${l}`} tone="danger">
            {t("plexImport.people.hideLabel", { label: l })}
          </Badge>
        ))}
      </div>
      {limits.left_over.length > 0 ? (
        <p className="text-warning">{t("plexImport.people.leftOver", { count: limits.left_over.length, labels: limits.left_over.join(", ") })}</p>
      ) : null}
      {limits.not_kept.length > 0 ? (
        <div className="text-warning">
          {t("plexImport.people.notKept")}{" "}
          {limits.not_kept.map((rule) => (
            <code key={rule} className="mr-1 rounded bg-surface-3 px-1 py-0.5 font-mono text-[11px] text-fg/90">
              {rule}
            </code>
          ))}
        </div>
      ) : null}
    </div>
  );
}
