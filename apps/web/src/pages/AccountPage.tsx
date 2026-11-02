import { useRef, useState, type FormEvent } from "react";
import { Link, useNavigate } from "react-router";
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
import { CompatibilityTokensCard } from "./CompatibilityTokensCard";
import { TwoStepCard } from "./TwoStepCard";

const MAX_AVATAR_BYTES = 10 * 1024 * 1024;

function ProfileCard() {
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
      { onSuccess: () => toast.success("Name saved"), onError: (err) => toast.error(errorMessage(err)) },
    );
  };

  const pick = (file: File | undefined) => {
    if (!file) return;
    if (!file.type.startsWith("image/")) return toast.error("Choose a picture (JPEG, PNG, WebP or GIF).");
    if (file.size > MAX_AVATAR_BYTES) return toast.error("Pictures must be smaller than 10 MB.");
    upload.mutate(file, {
      onSuccess: () => toast.success("Picture updated"),
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
            aria-label="Change picture"
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
              {user.avatar_url ? "Change" : "Add picture"}
            </Button>
            {user.avatar_url ? (
              <Button
                size="sm"
                variant="ghost"
                icon={<Trash2 className="size-3.5" />}
                loading={removeAvatar.isPending}
                onClick={() =>
                  removeAvatar.mutate(undefined, {
                    onSuccess: () => toast.success("Picture removed"),
                    onError: (err) => toast.error(errorMessage(err)),
                  })
                }
              >
                Remove
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
                  <ShieldCheck className="size-3" /> Admin
                </Badge>
              ) : user.managed ? (
                <Badge tone="info">Managed profile</Badge>
              ) : (
                <Badge>
                  <UserRound className="size-3" /> User
                </Badge>
              )}
            </div>
            <div className="text-sm text-muted">
              @{user.username} · member since {formatDay(user.created_at)}
            </div>
          </div>

          <form onSubmit={saveName} className="flex items-end gap-2">
            <div className="min-w-0 flex-1">
              <Field label="Display name" htmlFor="profile-name">
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
              Save
            </Button>
          </form>

          <div>
            <div className="mb-1.5 text-sm font-medium">Color</div>
            <div className="flex flex-wrap gap-2">
              {AVATAR_COLORS.map((c) => {
                const selected = c.toLowerCase() === color.toLowerCase();
                return (
                  <button
                    key={c}
                    type="button"
                    aria-label={`Color ${c}`}
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
            {user.avatar_url ? <p className="mt-1.5 text-xs text-subtle">Shown when your picture can't be.</p> : null}
          </div>
        </div>
      </div>
    </Card>
  );
}

const SUBTITLE_MODES: Record<SubtitleMode, { label: string; hint: string }> = {
  off: { label: "Off", hint: "Subtitles stay off until you turn them on." },
  forced: {
    label: "Only for foreign dialogue",
    hint: "Only forced subtitles, for signs and foreign dialogue.",
  },
  foreign: {
    label: "When the audio is in another language",
    hint: "Full subtitles come on when the audio isn't in your subtitle language.",
  },
  always: { label: "Always", hint: "Subtitles in your language come on whenever there are some." },
};
const MODE_ORDER: SubtitleMode[] = ["off", "forced", "foreign", "always"];

/** Default tracks: what a film or show starts with until you pick tracks for it. */
export function PlaybackCard() {
  const user = useAuth();
  const toast = useToast();
  const update = useUpdateProfile();
  const mode = SUBTITLE_MODES[user.subtitle_mode];
  const readsSubtitles = user.subtitle_mode === "foreign" || user.subtitle_mode === "always";

  const save = (body: ProfilePatch, message: string) =>
    update.mutate(body, { onSuccess: () => toast.success(message), onError: (err) => toast.error(errorMessage(err)) });

  return (
    <Card>
      <CardTitle icon={<Languages />}>Audio and subtitles</CardTitle>
      <p className="mb-4 text-sm text-muted">
        Used until you pick tracks for a title. Your picks are remembered for each title.
      </p>
      <div className="grid gap-4 sm:grid-cols-2">
        <Field label="Audio language" htmlFor="audio-language">
          <Select
            id="audio-language"
            className="w-full"
            value={user.audio_language ?? ""}
            disabled={update.isPending}
            onChange={(e) => save({ audio_language: e.target.value || null }, "Audio language saved")}
          >
            <option value="">The file's default</option>
            {LANGUAGES.map((l) => (
              <option key={l.code} value={l.code}>
                {l.name}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Subtitles" htmlFor="subtitle-mode" hint={mode.hint}>
          <Select
            id="subtitle-mode"
            className="w-full"
            value={user.subtitle_mode}
            disabled={update.isPending}
            onChange={(e) => save({ subtitle_mode: e.target.value as SubtitleMode }, "Subtitles saved")}
          >
            {MODE_ORDER.map((m) => (
              <option key={m} value={m}>
                {SUBTITLE_MODES[m].label}
              </option>
            ))}
          </Select>
        </Field>
        {readsSubtitles ? (
          <Field
            label="Subtitle language"
            htmlFor="subtitle-language"
            hint={
              user.subtitle_language || user.audio_language
                ? undefined
                : "Choose the language you read: without one, only forced subtitles come on."
            }
          >
            <Select
              id="subtitle-language"
              className="w-full"
              value={user.subtitle_language ?? ""}
              disabled={update.isPending}
              onChange={(e) => save({ subtitle_language: e.target.value || null }, "Subtitle language saved")}
            >
              <option value="">Same as the audio language</option>
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
  const user = useAuth();
  const toast = useToast();
  const setPin = useSetPin();
  const [pin, setPinValue] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);

  if (user.managed) {
    return (
      <Card>
        <CardTitle icon={<Lock />}>Profile PIN</CardTitle>
        <p className="text-sm text-muted">
          {user.has_pin ? "This profile is protected by a PIN." : "This profile has no PIN."} Ask an administrator to change
          it.
        </p>
      </Card>
    );
  }

  const submit = (next: string | null) => {
    setError(null);
    if (next !== null && !/^\d{4,8}$/.test(next)) return setError("The PIN must be 4 to 8 digits.");
    if (!password) return setError("Enter your password to confirm.");
    setPin.mutate(
      { pin: next, password },
      {
        onSuccess: () => {
          toast.success(next ? "PIN saved" : "PIN removed");
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
      <CardTitle icon={<Lock />}>Profile PIN</CardTitle>
      <p className="mb-4 text-sm text-muted">
        {user.has_pin
          ? "Your profile is protected: others in your home need this PIN to switch to it."
          : "Protect your profile so others in your home can't switch to it without a PIN."}
      </p>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          submit(pin);
        }}
        className="space-y-4"
      >
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label={user.has_pin ? "New PIN" : "PIN"} htmlFor="pin-new" hint="4 to 8 digits">
            <TextInput
              id="pin-new"
              inputMode="numeric"
              autoComplete="off"
              maxLength={8}
              value={pin}
              onChange={(e) => setPinValue(e.target.value.replace(/\D/g, ""))}
            />
          </Field>
          <Field label="Your password" htmlFor="pin-password">
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
              Remove PIN
            </Button>
          ) : null}
          <Button type="submit" variant="primary" loading={setPin.isPending && setPin.variables?.pin !== null} disabled={!pin}>
            {user.has_pin ? "Change PIN" : "Set PIN"}
          </Button>
        </div>
      </form>
    </Card>
  );
}

function PasswordCard() {
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
    if (next !== confirm) return setError("Passwords don't match.");
    change.mutate(
      { current_password: current, new_password: next },
      {
        onSuccess: () => {
          toast.success("Password changed");
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
      <CardTitle icon={<KeyRound />}>Change password</CardTitle>
      <form onSubmit={onSubmit} className="space-y-4">
        <Field label="Current password" htmlFor="pw-current">
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
          <Field label="New password" htmlFor="pw-new">
            <TextInput id="pw-new" type="password" autoComplete="new-password" value={next} onChange={(e) => setNext(e.target.value)} required />
          </Field>
          <Field label="Confirm new password" htmlFor="pw-confirm">
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
            Update password
          </Button>
        </div>
      </form>
    </Card>
  );
}

/** In place of the password form, for an account that has no password to change. */
function NoPasswordCard() {
  return (
    <Card>
      <CardTitle icon={<KeyRound />}>Password</CardTitle>
      <p className="text-sm text-muted">This account has no password: an administrator can give it one.</p>
    </Card>
  );
}

function deviceIcon(d: Device) {
  return /iPhone|iPad|Android/.test(d.name) ? Smartphone : Laptop;
}

function HistoryCard() {
  return (
    <Card>
      <CardTitle icon={<History />}>Watch history</CardTitle>
      <HistoryList scope={{}} pageSize={15} empty="Movies, episodes and songs you finish show up here." />
    </Card>
  );
}

function DevicesCard() {
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
    toast.success("Signed out on your other devices");
  };

  return (
    <Card>
      <div className="mb-4 flex items-center justify-between gap-3">
        <h2 className="flex items-center gap-2 text-base font-semibold">
          <Laptop className="size-4 text-accent" /> Devices
        </h2>
        {others.length > 1 ? (
          <Button size="sm" variant="ghost" icon={<LogOut className="size-4" />} loading={signOut.isPending} onClick={() => void signOutAll()}>
            Sign out all others
          </Button>
        ) : null}
      </div>
      {devices.isPending ? (
        <p className="text-sm text-subtle">Loading…</p>
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
                    {d.current ? <Badge tone="accent">This device</Badge> : null}
                  </div>
                  <div className="truncate text-xs text-subtle">
                    {[d.ip, d.current ? "active now" : `last active ${formatAgo(d.last_seen_at)}`, `signed in ${formatAgo(d.created_at)}`]
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
                        onSuccess: () => toast.success(`Signed out ${d.name}`),
                        onError: (err) => toast.error(errorMessage(err)),
                      })
                    }
                  >
                    Sign out
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
  const me = useAuth();
  const toast = useToast();
  const apps = useMyApps();
  const signOut = useSignOutMyApp();
  const list = apps.data ?? [];

  return (
    <Card>
      <div className="mb-4 flex items-center justify-between gap-3">
        <h2 className="flex items-center gap-2 text-base font-semibold">
          <AppWindow className="size-4 text-accent" /> Apps
        </h2>
        {/* Where the code a TV shows is typed in. */}
        <Link to="/link" className={buttonClass("ghost", "sm")}>
          <Tv className="size-4" aria-hidden /> Sign in on a TV
        </Link>
      </div>
      {apps.isPending ? (
        <p className="text-sm text-subtle">Loading…</p>
      ) : apps.isError ? (
        <p className="text-sm text-danger">{errorMessage(apps.error)}</p>
      ) : list.length === 0 ? (
        <p className="text-sm text-subtle">No apps use your account. Apps ask on a Pley page before they get access.</p>
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
                      <Badge tone="accent">Pley's own app</Badge>
                    ) : a.name ? (
                      <span className="truncate text-xs text-subtle">calls itself “{a.name}”</span>
                    ) : null}
                  </div>
                  <div className="truncate text-xs text-muted">
                    {[
                      a.profile.id !== me.id ? `Using ${a.profile.name}` : null,
                      a.approved_by.id !== me.id ? `Allowed by ${a.approved_by.name}` : null,
                      scopesLabel(a.scopes),
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  </div>
                  <div className="truncate text-xs text-subtle">
                    {[a.device, a.ip, `last active ${formatAgo(a.last_used_at)}`, `allowed ${formatAgo(a.created_at)}`]
                      .filter(Boolean)
                      .join(" · ")}
                  </div>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  aria-label={`Sign out ${label}`}
                  loading={signOut.isPending && signOut.variables === a.id}
                  onClick={() =>
                    signOut.mutate(a.id, {
                      onSuccess: () => toast.success(`Signed out ${label}`),
                      onError: (err) => toast.error(errorMessage(err)),
                    })
                  }
                >
                  Sign out
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
  useDocumentTitle("Account");
  const user = useAuth();
  const navigate = useNavigate();
  const logout = useLogout();
  const audio = useAudioPlayer();

  return (
    <div className="mx-auto max-w-2xl space-y-6 px-4 py-8 sm:px-8">
      <div className="flex items-center justify-between gap-3">
        <h1 className="text-2xl font-bold tracking-tight">Account</h1>
        <Button
          variant="outline"
          icon={<LogOut className="size-4" />}
          loading={logout.isPending}
          onClick={() => {
            audio.close();
            logout.mutate(undefined, { onSettled: () => void navigate("/login", { replace: true }) });
          }}
        >
          Sign out
        </Button>
      </div>
      <ProfileCard key={user.id} />
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
  const server = useServer().data;
  const web = versionLabel(WEB_VERSION, WEB_COMMIT);
  const serverLabel = server ? versionLabel(server.version, server.commit) : null;
  const source = server?.source;
  return (
    <p className="text-center text-xs text-muted" data-testid="build">
      {serverLabel && serverLabel !== web ? `Pley ${web} · server ${serverLabel}` : `Pley ${web}`}
      {source ? (
        <>
          {" · "}
          <a href={source} target="_blank" rel="noreferrer" className="hover:underline">
            Source code
          </a>
        </>
      ) : null}
    </p>
  );
}
