import { useState } from "react";
import { Link } from "react-router";
import { FolderTree, ListVideo, Save, SlidersHorizontal, Tv2, Waypoints } from "lucide-react";
import { useTranslation } from "react-i18next";
import { errorMessage } from "@/api/client";
import { useUsers } from "@/api/hooks";
import { useLiveSettings, useLiveSources, useSaveLiveSettings, type LiveSettings } from "@/api/livetv";
import { useToast } from "@/components/Toast";
import { Button, buttonClass, EmptyState, PageSpinner, Select, Toggle } from "@/components/ui";
import { cn } from "@/lib/cn";
import { useDocumentTitle, useLocalStorage } from "@/lib/hooks";
import { Groups } from "./livetv/Groups";
import { Lineup } from "./livetv/Lineup";
import { Sources } from "./livetv/Sources";
import { Group, Note, Row } from "./SettingsForm";

type Tab = "sources" | "groups" | "channels" | "options";

const TABS: { key: Tab; icon: typeof Tv2 }[] = [
  { key: "sources", icon: Waypoints },
  { key: "groups", icon: FolderTree },
  { key: "channels", icon: ListVideo },
  { key: "options", icon: SlidersHorizontal },
];

/** Settings → Live TV: where channels come from, which show, and how they play. */
export function LiveTvSettings() {
  const { t } = useTranslation("livetv");
  useDocumentTitle(t("settingsTitle"));
  const [tab, setTab] = useLocalStorage<Tab>("pley.settings.live.tab", "sources");
  const sources = useLiveSources();
  const hasSources = (sources.data?.length ?? 0) > 0;
  return (
    <div className="space-y-5 pb-20">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{t("liveTv")}</h2>
          <p className="text-sm text-muted">{t("settings.intro")}</p>
        </div>
        {hasSources ? (
          <Link to="/live" className={buttonClass("outline", "sm")}>
            <Tv2 className="size-4" /> {t("settings.openLiveTv")}
          </Link>
        ) : null}
      </div>
      <div role="tablist" aria-label={t("settingsTitle")} className="scrollbar-none flex gap-1 overflow-x-auto border-b border-border">
        {TABS.map(({ key, icon: Icon }) => (
          <button
            key={key}
            role="tab"
            aria-selected={tab === key}
            onClick={() => setTab(key)}
            disabled={key !== "sources" && key !== "options" && !hasSources}
            className={cn(
              "-mb-px flex h-10 items-center gap-2 border-b-2 px-3 text-sm font-medium transition-colors disabled:opacity-40",
              tab === key ? "border-accent text-fg" : "border-transparent text-muted hover:text-fg",
            )}
          >
            <Icon className={cn("size-4", tab === key ? "text-accent" : "text-subtle")} />
            {t(`settings.tabs.${key}`)}
          </button>
        ))}
      </div>
      <div role="tabpanel">
        {tab === "sources" || (tab !== "options" && !hasSources) ? (
          <Sources />
        ) : tab === "groups" ? (
          <Groups />
        ) : tab === "channels" ? (
          <Lineup />
        ) : (
          <Options />
        )}
      </div>
    </div>
  );
}

/** The most lines a transcoded channel has; null: as many as it's sent with. */
const HEIGHTS: (number | null)[] = [null, 1080, 720, 480];

function Options() {
  const { t } = useTranslation(["livetv", "common"]);
  const settings = useLiveSettings();
  if (settings.isPending) return <PageSpinner />;
  if (settings.isError) {
    return (
      <EmptyState
        title={t("settings.options.couldntLoad")}
        description={errorMessage(settings.error)}
        action={<Button onClick={() => void settings.refetch()}>{t("common:actions.retry")}</Button>}
      />
    );
  }
  return <OptionsForm key={JSON.stringify(settings.data)} saved={settings.data} />;
}

function OptionsForm({ saved }: { saved: LiveSettings }) {
  const { t } = useTranslation(["livetv", "common"]);
  const save = useSaveLiveSettings();
  const toast = useToast();
  const [form, setForm] = useState(saved);
  const set = <K extends keyof LiveSettings>(key: K, value: LiveSettings[K]) => setForm((f) => ({ ...f, [key]: value }));
  const dirty = JSON.stringify(form) !== JSON.stringify(saved);
  return (
    <form
      className="space-y-5"
      onSubmit={(e) => {
        e.preventDefault();
        save.mutate(form, { onSuccess: () => toast.success(t("settings.options.saved")), onError: (err) => toast.error(errorMessage(err)) });
      }}
    >
      <Group title={t("settings.options.whoWatches")} description={t("settings.options.whoWatchesHint")}>
        <Row label={t("settings.options.openTo")} htmlFor="live-access">
          <Select id="live-access" value={form.access} onChange={(e) => set("access", e.target.value as LiveSettings["access"])} className="w-full">
            <option value="everyone">{t("settings.options.everyone")}</option>
            <option value="home">{t("settings.options.homeUsers")}</option>
            <option value="admins">{t("settings.options.adminsOnly")}</option>
          </Select>
          {form.access === "home" ? <Note>{t("settings.options.homeNote")}</Note> : null}
        </Row>
        <Row label={t("settings.options.ageLimited")} hint={t("settings.options.ageLimitedHint")}>
          <Toggle
            checked={form.age_limited}
            onChange={(v) => set("age_limited", v)}
            label={t("settings.options.ageLimitedToggle")}
            disabled={form.access === "admins"}
          />
        </Row>
        <People form={form} set={set} />
      </Group>

      <Group title={t("settings.options.playing")} description={t("settings.options.playingHint")}>
        <Row label={t("settings.options.deinterlace")} hint={t("settings.options.deinterlaceHint")}>
          <Toggle checked={form.deinterlace} onChange={(v) => set("deinterlace", v)} label={t("settings.options.deinterlaceToggle")} />
        </Row>
        <Row label={t("settings.options.transcoded")} hint={t("settings.options.transcodedHint")} htmlFor="live-height">
          <Select
            id="live-height"
            value={String(form.max_height ?? "")}
            onChange={(e) => set("max_height", e.target.value ? Number(e.target.value) : null)}
            className="w-full"
          >
            {HEIGHTS.map((h) => (
              <option key={h ?? "source"} value={h ?? ""}>
                {h ? `${h}p` : t("settings.options.asSent")}
              </option>
            ))}
          </Select>
          <Note>{t("settings.options.h264Note")}</Note>
        </Row>
      </Group>

      <Group title={t("settings.options.channelsAndGuide")}>
        <Row label={t("settings.options.readEvery")} htmlFor="live-refresh">
          <Select id="live-refresh" value={form.refresh_hours} onChange={(e) => set("refresh_hours", Number(e.target.value))} className="w-full">
            {[3, 6, 12, 24, 48, 168].map((h) => (
              <option key={h} value={h}>
                {h === 168
                  ? t("settings.options.everyWeek")
                  : h === 24
                    ? t("settings.options.everyDay")
                    : h === 48
                      ? t("settings.options.everyTwoDays")
                      : t("settings.options.everyHours", { count: h })}
              </option>
            ))}
          </Select>
        </Row>
        <Row label={t("settings.options.hideOffline")} hint={t("settings.options.hideOfflineHint")}>
          <Toggle checked={form.hide_offline} onChange={(v) => set("hide_offline", v)} label={t("settings.options.hideOffline")} />
        </Row>
        <Row label={t("settings.options.guideDays")} htmlFor="live-days">
          <Select id="live-days" value={form.guide_days} onChange={(e) => set("guide_days", Number(e.target.value))} className="w-full">
            {[1, 2, 3, 5, 7, 10, 14].map((d) => (
              <option key={d} value={d}>
                {t("settings.options.days", { count: d })}
              </option>
            ))}
          </Select>
        </Row>
      </Group>

      <div className={cn("flex justify-end", (dirty || save.isPending) && "sticky bottom-4 z-10")}>
        <div className="flex items-center gap-3 rounded-lg border border-border bg-surface-2/95 px-3 py-2 shadow-xl backdrop-blur">
          {dirty ? <span className="text-xs text-muted">{t("settings.options.unsaved")}</span> : null}
          <Button variant="ghost" size="sm" disabled={!dirty || save.isPending} onClick={() => setForm(saved)}>
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

/** Each person's own access, whatever "Open to" says: as the rule says, always, or never. */
function People({
  form,
  set,
}: {
  form: LiveSettings;
  set: <K extends keyof LiveSettings>(key: K, value: LiveSettings[K]) => void;
}) {
  const { t } = useTranslation("livetv");
  const users = useUsers();
  const people = (users.data ?? []).filter((u) => !u.is_admin);
  if (!people.length) return null;
  const choice = (id: number) => (form.allowed_users.includes(id) ? "always" : form.denied_users.includes(id) ? "never" : "rule");
  const choose = (id: number, value: string) => {
    set("allowed_users", value === "always" ? [...form.allowed_users.filter((x) => x !== id), id] : form.allowed_users.filter((x) => x !== id));
    set("denied_users", value === "never" ? [...form.denied_users.filter((x) => x !== id), id] : form.denied_users.filter((x) => x !== id));
  };
  return (
    <Row label={t("settings.options.people")} hint={t("settings.options.peopleHint")}>
      <ul className="divide-y divide-border/60 rounded-lg border border-border">
        {people.map((u) => (
          <li key={u.id} className="flex items-center gap-3 px-3 py-2">
            <span className="min-w-0 flex-1 truncate text-sm">
              {u.name}
              {!u.home ? <span className="ml-2 text-xs text-subtle">{t("settings.options.notInHome")}</span> : null}
            </span>
            <Select value={choice(u.id)} onChange={(e) => choose(u.id, e.target.value)} aria-label={t("settings.options.personAccess", { name: u.name })} className="h-8 w-36 text-sm">
              <option value="rule">{t("settings.options.asOpenTo")}</option>
              <option value="always">{t("settings.options.always")}</option>
              <option value="never">{t("settings.options.never")}</option>
            </Select>
          </li>
        ))}
      </ul>
    </Row>
  );
}
