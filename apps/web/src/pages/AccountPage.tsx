import { useRef, useState, type FormEvent } from "react";
import { Link, useNavigate } from "react-router";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import {
  AppWindow,
  Camera,
  Check,
  History,
  KeyRound,
  Languages,
  Laptop,
  Lock,
  LogOut,
  ShieldCheck,
  Smartphone,
  Trash2,
  Tv,
  UserRound,
} from "lucide-react";
import { appKeys, appLabel, scopesLabel, useMyApps, useSignOutMyApp } from "@/api/apps";
import { errorMessage } from "@/api/client";
import { compatibilityTokenKeys } from "@/api/compatibilityTokens";
import {
  useChangePassword,
  useDeleteAvatar,
  useDevices,
  useLogout,
  useServer,
  useSetPin,
  useSignOutDevice,
  useUpdateProfile,
  useUploadAvatar,
} from "@/api/hooks";
import type { Device, ProfilePatch, SubtitleMode } from "@/api/types";
import { useAudioPlayer } from "@/audio/AudioPlayer";
import { useAuth } from "@/auth";
import { Avatar, AVATAR_COLORS, avatarColor } from "@/components/Avatar";
import { useToast } from "@/components/Toast";
import { HistoryList } from "@/components/HistoryList";
import { Badge, Button, buttonClass, Card, CardTitle, Field, Select, TextInput } from "@/components/ui";
import { versionLabel, WEB_COMMIT, WEB_VERSION } from "@/lib/build";
import { cn } from "@/lib/cn";
import { formatAgo, formatDay } from "@/lib/format";
import { LANGUAGES } from "@/lib/languages";
import { useDocumentTitle } from "@/lib/hooks";
import { passwordProblem } from "@/lib/passwords";
import { LanguageCard } from "./LanguageCard";
import { CompatibilityTokensCard } from "./CompatibilityTokensCard";
import { TwoStepCard } from "./TwoStepCard";

const MAX_AVATAR_BYTES = 10 * 1024 * 1024;

function ProfileCard() {
  const { t } = useTranslation(["pages", "common"]);
  const user = useAuth();
  const toast = useToast();
  const update = useUpdateProfile();
  const upload = useUploadAvatar();
  const removeAvatar = useDeleteAvatar();
  const fileRef = useRef<HTMLInputElement>(null);
  const [name, setName] = useState(user.display_name ?? "");
  const color = avatarColor(user);

  const saveName = (e: FormEvent) => {
    e.preventDefault();
    const value = name.trim();
    if (value === (user.display_name ?? "")) return;
    update.mutate(
      { display_name: value || null },
      { onSuccess: () => toast.success(t("account.profile.nameSaved")), onError: (err) => toast.error(errorMessage(err)) },
    );
  };

  const pick = (file: File | undefined) => {
    if (!file) return;
    if (!file.type.startsWith("image/")) return toast.error(t("account.profile.notAPicture"));
    if (file.size > MAX_AVATAR_BYTES) return toast.error(t("account.profile.tooBig"));
    upload.mutate(file, {
      onSuccess: () => toast.success(t("account.profile.pictureUpdated")),
      onError: (err) => toast.error(errorMessage(err)),
    });
  };

  return (
    <Card>
      <div className="flex flex-col gap-5 sm:flex-row sm:items-start">
        <div className="flex flex-col items-center gap-2">
          <button
            type="button"
            onClick={() => fileRef.current?.click()}
            className="group relative rounded-full outline-none ring-accent ring-offset-2 ring-offset-surface focus-visible:ring-2"
            aria-label={t("account.profile.changePicture")}
            disabled={upload.isPending}
          >
            <Avatar user={user} size="xl" />
            <span className="absolute inset-0 flex items-center justify-center rounded-full bg-black/55 opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100">
              <Camera className="size-7 text-white" />
            </span>
          </button>
          <input
            ref={fileRef}
            type="file"
            accept="image/*"
            className="hidden"
            onChange={(e) => {
              pick(e.target.files?.[0]);
              e.target.value = "";
            }}
          />
          <div className="flex gap-1">
            <Button size="sm" variant="ghost" loading={upload.isPending} onClick={() => fileRef.current?.click()}>
              {user.avatar_url ? t("account.profile.change") : t("account.profile.addPicture")}
            </Button>
            {user.avatar_url ? (
              <Button
                size="sm"
                variant="ghost"
                icon={<Trash2 className="size-3.5" />}
                loading={removeAvatar.isPending}
                onClick={() =>
                  removeAvatar.mutate(undefined, {
                    onSuccess: () => toast.success(t("account.profile.pictureRemoved")),
                    onError: (err) => toast.error(errorMessage(err)),
                  })
                }
              >
                {t("common:actions.remove")}
              </Button>
            ) : null}
          </div>
        </div>

        <div className="min-w-0 flex-1 space-y-4">
          <div>
            <div className="flex flex-wrap items-center gap-2">
              <span className="truncate text-lg font-semibold">{user.name}</span>
              {user.is_admin ? (
                <Badge tone="accent">
                  <ShieldCheck className="size-3" /> {t("account.profile.admin")}
                </Badge>
              ) : user.managed ? (
                <Badge tone="info">{t("account.profile.managed")}</Badge>
              ) : (
                <Badge>
                  <UserRound className="size-3" /> {t("account.profile.user")}
                </Badge>
              )}
            </div>
            <div className="text-sm text-muted">
              {t("account.profile.memberSince", { username: user.username, date: formatDay(user.created_at) })}
            </div>
          </div>

          <form onSubmit={saveName} className="flex items-end gap-2">
            <div className="min-w-0 flex-1">
              <Field label={t("account.profile.displayName")} htmlFor="profile-name">
                <TextInput
                  id="profile-name"
                  maxLength={64}
                  placeholder={user.username}
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                />
              </Field>
            </div>
            <Button type="submit" loading={update.isPending} disabled={name.trim() === (user.display_name ?? "")}>
              {t("common:actions.save")}
            </Button>
          </form>

          <div>
            <div className="mb-1.5 text-sm font-medium">{t("account.profile.color")}</div>
            <div className="flex flex-wrap gap-2">
              {AVATAR_COLORS.map((c) => {
                const selected = c.toLowerCase() === color.toLowerCase();
                return (
                  <button
                    key={c}
                    type="button"
                    aria-label={t("account.profile.colorNamed", { color: c })}
                    aria-pressed={selected}
                    onClick={() =>
                      update.mutate({ avatar_color: c }, { onError: (err) => toast.error(errorMessage(err)) })
                    }
                    className={cn(
                      "flex size-8 items-center justify-center rounded-full ring-offset-2 ring-offset-surface transition",
                      selected ? "ring-2 ring-fg" : "hover:ring-2 hover:ring-border-strong",
                    )}
                    style={{ backgroundColor: c }}
                  >
                    {selected ? <Check className="size-4 text-black/80" strokeWidth={3} /> : null}
                  </button>
                );
              })}
            </div>
            {user.avatar_url ? <p className="mt-1.5 text-xs text-subtle">{t("account.profile.colorFallback")}</p> : null}
          </div>
        </div>
      </div>
    </Card>
  );
}

const MODE_ORDER: SubtitleMode[] = ["off", "forced", "foreign", "always"];

/** Default tracks: what a film or show starts with until you pick tracks for it. */
export function PlaybackCard() {
  const { t } = useTranslation("pages");
  const user = useAuth();
  const toast = useToast();
  const update = useUpdateProfile();
  const readsSubtitles = user.subtitle_mode === "foreign" || user.subtitle_mode === "always";

  const save = (body: ProfilePatch, message: string) =>
    update.mutate(body, { onSuccess: () => toast.success(message), onError: (err) => toast.error(errorMessage(err)) });

  return (
    <Card>
      <CardTitle icon={<Languages />}>{t("account.playback.title")}</CardTitle>
      <p className="mb-4 text-sm text-muted">{t("account.playback.intro")}</p>
      <div className="grid gap-4 sm:grid-cols-2">
        <Field label={t("account.playback.audioLanguage")} htmlFor="audio-language">
          <Select
            id="audio-language"
            className="w-full"
            value={user.audio_language ?? ""}
            disabled={update.isPending}
            onChange={(e) => save({ audio_language: e.target.value || null }, t("account.playback.audioSaved"))}
          >
            <option value="">{t("account.playback.fileDefault")}</option>
            {LANGUAGES.map((l) => (
              <option key={l.code} value={l.code}>
                {l.name}
              </option>
            ))}
          </Select>
        </Field>
        <Field
          label={t("account.playback.subtitles")}
          htmlFor="subtitle-mode"
          hint={t(`account.playback.modes.${user.subtitle_mode}.hint`)}
        >
          <Select
            id="subtitle-mode"
            className="w-full"
            value={user.subtitle_mode}
            disabled={update.isPending}
            onChange={(e) => save({ subtitle_mode: e.target.value as SubtitleMode }, t("account.playback.subtitlesSaved"))}
          >
            {MODE_ORDER.map((m) => (
              <option key={m} value={m}>
                {t(`account.playback.modes.${m}.label`)}
              </option>
            ))}
          </Select>
        </Field>
        {readsSubtitles ? (
          <Field
            label={t("account.playback.subtitleLanguage")}
            htmlFor="subtitle-language"
            hint={user.subtitle_language || user.audio_language ? undefined : t("account.playback.subtitleLanguageHint")}
          >
            <Select
              id="subtitle-language"
              className="w-full"
              value={user.subtitle_language ?? ""}
              disabled={update.isPending}
              onChange={(e) => save({ subtitle_language: e.target.value || null }, t("account.playback.subtitleLanguageSaved"))}
            >
              <option value="">{t("account.playback.sameAsAudio")}</option>
              {LANGUAGES.map((l) => (
                <option key={l.code} value={l.code}>
                  {l.name}
                </option>
              ))}
            </Select>
          </Field>
        ) : null}
      </div>
    </Card>
  );
}

function PinCard() {
  const { t } = useTranslation("pages");
  const user = useAuth();
  const toast = useToast();
  const setPin = useSetPin();
  const [pin, setPinValue] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);

  if (user.managed) {
    return (
      <Card>
        <CardTitle icon={<Lock />}>{t("account.pin.title")}</CardTitle>
        <p className="text-sm text-muted">{user.has_pin ? t("account.pin.managedHasPin") : t("account.pin.managedNoPin")}</p>
      </Card>
    );
  }

  const submit = (next: string | null) => {
    setError(null);
    if (next !== null && !/^\d{4,8}$/.test(next)) return setError(t("account.pin.badPin"));
    if (!password) return setError(t("account.pin.needPassword"));
    setPin.mutate(
      { pin: next, password },
      {
        onSuccess: () => {
          toast.success(next ? t("account.pin.saved") : t("account.pin.removed"));
          setPinValue("");
          setPassword("");
        },
        // The server says which: the password, or the PIN.
        onError: (err) => setError(errorMessage(err)),
      },
    );
  };

  return (
    <Card>
      <CardTitle icon={<Lock />}>{t("account.pin.title")}</CardTitle>
      <p className="mb-4 text-sm text-muted">{user.has_pin ? t("account.pin.protected") : t("account.pin.unprotected")}</p>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          submit(pin);
        }}
        className="space-y-4"
      >
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label={user.has_pin ? t("account.pin.newPin") : t("account.pin.pin")} htmlFor="pin-new" hint={t("account.pin.pinHint")}>
            <TextInput
              id="pin-new"
              inputMode="numeric"
              autoComplete="off"
              maxLength={8}
              value={pin}
              onChange={(e) => setPinValue(e.target.value.replace(/\D/g, ""))}
            />
          </Field>
          <Field label={t("fields.yourPassword")} htmlFor="pin-password">
            <TextInput
              id="pin-password"
              type="password"
              autoComplete="current-password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
          </Field>
        </div>
        {error ? <p className="text-sm text-danger">{error}</p> : null}
        <div className="flex justify-end gap-2">
          {user.has_pin ? (
            <Button variant="ghost" loading={setPin.isPending && setPin.variables?.pin === null} onClick={() => submit(null)}>
              {t("account.pin.remove")}
            </Button>
          ) : null}
          <Button type="submit" variant="primary" loading={setPin.isPending && setPin.variables?.pin !== null} disabled={!pin}>
            {user.has_pin ? t("account.pin.change") : t("account.pin.set")}
          </Button>
        </div>
      </form>
    </Card>
  );
}

function PasswordCard() {
  const { t } = useTranslation("pages");
  const toast = useToast();
  const qc = useQueryClient();
  const change = useChangePassword();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string | null>(null);

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    const problem = passwordProblem(next);
    if (problem) return setError(problem);
    if (next !== confirm) return setError(t("account.password.noMatch"));
    change.mutate(
      { current_password: current, new_password: next },
      {
        onSuccess: () => {
          toast.success(t("account.password.changed"));
          // A new password signs the account's apps out, and ends its tokens for other apps.
          void qc.invalidateQueries({ queryKey: appKeys.mine });
          void qc.invalidateQueries({ queryKey: compatibilityTokenKeys.mine });
          setCurrent("");
          setNext("");
          setConfirm("");
        },
        // The server says which: the current password, or the new one.
        onError: (err) => setError(errorMessage(err)),
      },
    );
  };

  return (
    <Card>
      <CardTitle icon={<KeyRound />}>{t("account.password.title")}</CardTitle>
      <form onSubmit={onSubmit} className="space-y-4">
        <Field label={t("account.password.current")} htmlFor="pw-current">
          <TextInput
            id="pw-current"
            type="password"
            autoComplete="current-password"
            value={current}
            onChange={(e) => setCurrent(e.target.value)}
            required
          />
        </Field>
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label={t("account.password.new")} htmlFor="pw-new">
            <TextInput id="pw-new" type="password" autoComplete="new-password" value={next} onChange={(e) => setNext(e.target.value)} required />
          </Field>
          <Field label={t("account.password.confirm")} htmlFor="pw-confirm">
            <TextInput
              id="pw-confirm"
              type="password"
              autoComplete="new-password"
              value={confirm}
              onChange={(e) => setConfirm(e.target.value)}
              required
            />
          </Field>
        </div>
        {error ? <p className="text-sm text-danger">{error}</p> : null}
        <div className="flex justify-end">
          <Button type="submit" variant="primary" loading={change.isPending}>
            {t("account.password.update")}
          </Button>
        </div>
      </form>
    </Card>
  );
}

/** In place of the password form, for an account that has no password to change. */
function NoPasswordCard() {
  const { t } = useTranslation("pages");
  return (
    <Card>
      <CardTitle icon={<KeyRound />}>{t("account.password.noPasswordTitle")}</CardTitle>
      <p className="text-sm text-muted">{t("account.password.noPassword")}</p>
    </Card>
  );
}

function deviceIcon(d: Device) {
  return /iPhone|iPad|Android/.test(d.name) ? Smartphone : Laptop;
}

function HistoryCard() {
  const { t } = useTranslation("pages");
  return (
    <Card>
      <CardTitle icon={<History />}>{t("account.history.title")}</CardTitle>
      <HistoryList scope={{}} pageSize={15} empty={t("account.history.empty")} />
    </Card>
  );
}

function DevicesCard() {
  const { t } = useTranslation(["pages", "common"]);
  const toast = useToast();
  const devices = useDevices();
  const signOut = useSignOutDevice();
  const list = devices.data ?? [];
  const others = list.filter((d) => !d.current);

  const signOutAll = async () => {
    for (const d of others) {
      try {
        await signOut.mutateAsync(d.id);
      } catch (err) {
        toast.error(errorMessage(err));
        return;
      }
    }
    toast.success(t("account.devices.signedOutOthers"));
  };

  return (
    <Card>
      <div className="mb-4 flex items-center justify-between gap-3">
        <h2 className="flex items-center gap-2 text-base font-semibold">
          <Laptop className="size-4 text-accent" /> {t("account.devices.title")}
        </h2>
        {others.length > 1 ? (
          <Button size="sm" variant="ghost" icon={<LogOut className="size-4" />} loading={signOut.isPending} onClick={() => void signOutAll()}>
            {t("account.devices.signOutOthers")}
          </Button>
        ) : null}
      </div>
      {devices.isPending ? (
        <p className="text-sm text-subtle">{t("common:states.loading")}</p>
      ) : devices.isError ? (
        <p className="text-sm text-danger">{errorMessage(devices.error)}</p>
      ) : (
        <ul className="divide-y divide-white/[0.05] rounded-md border border-border">
          {list.map((d) => {
            const Icon = deviceIcon(d);
            return (
              <li key={d.id} className="flex items-center gap-3 px-3 py-2.5">
                <Icon className="size-5 shrink-0 text-subtle" />
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-sm font-medium" title={d.user_agent ?? undefined}>
                      {d.name}
                    </span>
                    {d.current ? <Badge tone="accent">{t("account.devices.thisDevice")}</Badge> : null}
                  </div>
                  <div className="truncate text-xs text-subtle">
                    {[
                      d.ip,
                      d.current ? t("account.devices.activeNow") : t("account.devices.lastActive", { when: formatAgo(d.last_seen_at) }),
                      t("account.devices.signedIn", { when: formatAgo(d.created_at) }),
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  </div>
                </div>
                {!d.current ? (
                  <Button
                    size="sm"
                    variant="ghost"
                    loading={signOut.isPending && signOut.variables === d.id}
                    onClick={() =>
                      signOut.mutate(d.id, {
                        onSuccess: () => toast.success(t("account.devices.signedOut", { name: d.name })),
                        onError: (err) => toast.error(errorMessage(err)),
                      })
                    }
                  >
                    {t("common:actions.signOut")}
                  </Button>
                ) : null}
              </li>
            );
          })}
        </ul>
      )}
    </Card>
  );
}

/** Apps allowed to use this account (or allowed by it, for another profile), on pley's own page. */
function AppsCard() {
  const { t } = useTranslation(["pages", "common"]);
  const me = useAuth();
  const toast = useToast();
  const apps = useMyApps();
  const signOut = useSignOutMyApp();
  const list = apps.data ?? [];

  return (
    <Card>
      <div className="mb-4 flex items-center justify-between gap-3">
        <h2 className="flex items-center gap-2 text-base font-semibold">
          <AppWindow className="size-4 text-accent" /> {t("account.apps.title")}
        </h2>
        {/* Where the code a TV shows is typed in. */}
        <Link to="/link" className={buttonClass("ghost", "sm")}>
          <Tv className="size-4" aria-hidden /> {t("account.apps.signInOnTv")}
        </Link>
      </div>
      {apps.isPending ? (
        <p className="text-sm text-subtle">{t("common:states.loading")}</p>
      ) : apps.isError ? (
        <p className="text-sm text-danger">{errorMessage(apps.error)}</p>
      ) : list.length === 0 ? (
        <p className="text-sm text-subtle">{t("account.apps.none")}</p>
      ) : (
        <ul className="divide-y divide-white/[0.05] rounded-md border border-border">
          {list.map((a) => {
            const label = appLabel(a);
            return (
              <li key={a.id} className="flex items-center gap-3 px-3 py-2.5">
                <AppWindow className="size-5 shrink-0 text-subtle" />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                    <span className="truncate text-sm font-medium" title={a.client_id}>
                      {label}
                    </span>
                    {a.first_party ? (
                      <Badge tone="accent">{t("account.apps.firstParty")}</Badge>
                    ) : a.name ? (
                      <span className="truncate text-xs text-subtle">{t("account.apps.callsItself", { name: a.name })}</span>
                    ) : null}
                  </div>
                  <div className="truncate text-xs text-muted">
                    {[
                      a.profile.id !== me.id ? t("account.apps.using", { name: a.profile.name }) : null,
                      a.approved_by.id !== me.id ? t("account.apps.allowedBy", { name: a.approved_by.name }) : null,
                      scopesLabel(a.scopes),
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  </div>
                  <div className="truncate text-xs text-subtle">
                    {[
                      a.device,
                      a.ip,
                      t("account.apps.lastActive", { when: formatAgo(a.last_used_at) }),
                      t("account.apps.allowed", { when: formatAgo(a.created_at) }),
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  </div>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  aria-label={t("account.apps.signOut", { name: label })}
                  loading={signOut.isPending && signOut.variables === a.id}
                  onClick={() =>
                    signOut.mutate(a.id, {
                      onSuccess: () => toast.success(t("account.apps.signedOut", { name: label })),
                      onError: (err) => toast.error(errorMessage(err)),
                    })
                  }
                >
                  {t("common:actions.signOut")}
                </Button>
              </li>
            );
          })}
        </ul>
      )}
    </Card>
  );
}

export function AccountPage() {
  const { t } = useTranslation(["pages", "common"]);
  useDocumentTitle(t("account.title"));
  const user = useAuth();
  const navigate = useNavigate();
  const logout = useLogout();
  const audio = useAudioPlayer();

  return (
    <div className="mx-auto max-w-2xl space-y-6 px-4 py-8 sm:px-8">
      <div className="flex items-center justify-between gap-3">
        <h1 className="text-2xl font-bold tracking-tight">{t("account.title")}</h1>
        <Button
          variant="outline"
          icon={<LogOut className="size-4" />}
          loading={logout.isPending}
          onClick={() => {
            audio.close();
            logout.mutate(undefined, { onSettled: () => void navigate("/login", { replace: true }) });
          }}
        >
          {t("common:actions.signOut")}
        </Button>
      </div>
      <ProfileCard key={user.id} />
      <LanguageCard />
      <PlaybackCard />
      <PinCard />
      {user.managed ? null : user.has_password === false ? <NoPasswordCard /> : <PasswordCard />}
      <TwoStepCard />
      <HistoryCard />
      <DevicesCard />
      <AppsCard />
      <CompatibilityTokensCard />
      <BuildLine />
    </div>
  );
}

/**
 * Which Pley this is: the web app's build and, when it differs, the
 * server's; and where the server's source is offered.
 */
function BuildLine() {
  const { t } = useTranslation("pages");
  const server = useServer().data;
  const web = versionLabel(WEB_VERSION, WEB_COMMIT);
  const serverLabel = server ? versionLabel(server.version, server.commit) : null;
  const source = server?.source;
  return (
    <p className="text-center text-xs text-muted" data-testid="build">
      {serverLabel && serverLabel !== web ? t("account.build.withServer", { web, server: serverLabel }) : `Pley ${web}`}
      {source ? (
        <>
          {" · "}
          <a href={source} target="_blank" rel="noreferrer" className="hover:underline">
            {t("account.build.sourceCode")}
          </a>
        </>
      ) : null}
    </p>
  );
}
