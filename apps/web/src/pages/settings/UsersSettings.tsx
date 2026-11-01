import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { AppWindow, ChevronRight, Laptop, Lock, LogOut, Plus, ShieldAlert, ShieldCheck, Smartphone, Trash2, UserPlus } from "lucide-react";
import { appKeys, appLabel, scopesLabel, useSignOutUserApp, useUserApps } from "@/api/apps";
import { errorMessage } from "@/api/client";
import {
  useCreateUser,
  useDeleteUser,
  useLibraries,
  useResetUserTwoStep,
  useServer,
  useSignOutEverywhere,
  useUpdateUser,
  useUserDevices,
  useUsers,
  type UserPatch,
} from "@/api/hooks";
import type { Device, RequestAccess, User } from "@/api/types";
import { useAuth } from "@/auth";
import { AccessFields, type AccessValue } from "@/components/AccessFields";
import { Avatar } from "@/components/Avatar";
import { ConfirmModal, Modal } from "@/components/Modal";
import { useToast } from "@/components/Toast";
import { Badge, Button, EmptyState, Field, PageSpinner, Select, TextInput, Toggle } from "@/components/ui";
import { ageBadge, librariesLabel } from "@/lib/access";
import { cn } from "@/lib/cn";
import { passwordProblem } from "@/lib/passwords";
import { formatAgo } from "@/lib/format";
import { useDocumentTitle, useOnChange } from "@/lib/hooks";
import { mustSetUpTwoStep } from "@/lib/twoStep";

const PIN_RE = /^\d{4,8}$/;

function SwitchRow({
  label,
  description,
  checked,
  onChange,
  disabled,
}: {
  label: string;
  description: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <div className={cn("flex items-center justify-between gap-4 rounded-md border border-border bg-surface-2 px-3 py-2.5", disabled && "opacity-60")}>
      <div className="min-w-0">
        <div className="text-sm font-medium">{label}</div>
        <div className="text-xs text-subtle">{description}</div>
      </div>
      <Toggle checked={checked} onChange={onChange} label={label} disabled={disabled} />
    </div>
  );
}

/**
 * Two-step sign-in, which an administrator can turn off for someone who lost
 * their phone and recovery codes. When the server requires it, they are
 * signed out everywhere, and set it up again at their next sign-in.
 */
function TwoStepReset({ user }: { user: User }) {
  const reset = useResetUserTwoStep();
  const server = useServer();
  const toast = useToast();
  const [confirming, setConfirming] = useState(false);
  const required = !!server.data?.two_step_required;
  return (
    <section className="flex items-center justify-between gap-3">
      <div>
        <h3 className="text-sm font-medium">Two-step sign-in</h3>
        <p className="text-xs text-subtle">
          {required
            ? `Signs in with a code from an authenticator app after the password. Turning it off signs ${user.name} out everywhere, to set it up again at the next sign-in.`
            : "Signs in with a code from an authenticator app after the password."}
        </p>
      </div>
      {confirming ? (
        <div className="flex shrink-0 gap-1">
          <Button size="sm" variant="ghost" onClick={() => setConfirming(false)} disabled={reset.isPending}>
            Keep it
          </Button>
          <Button
            size="sm"
            variant="danger"
            loading={reset.isPending}
            onClick={() =>
              reset.mutate(user.id, {
                onSuccess: () =>
                  toast.success(
                    required
                      ? `${user.name} is signed out, and sets up two-step sign-in again at the next sign-in`
                      : `Two-step sign-in is off for ${user.name}`,
                  ),
                onError: (e) => toast.error(errorMessage(e)),
                onSettled: () => setConfirming(false),
              })
            }
          >
            Turn off
          </Button>
        </div>
      ) : (
        <Button size="sm" variant="ghost" onClick={() => setConfirming(true)} title="For someone who lost their phone and their recovery codes">
          Turn off
        </Button>
      )}
    </section>
  );
}

function isPhone(d: Device) {
  return /iPhone|iPad|Android/.test(d.name);
}

function UserDevices({ user, self }: { user: User; self: boolean }) {
  const devices = useUserDevices(user.id);
  const apps = useUserApps(user.id);
  const signOut = useSignOutEverywhere();
  const qc = useQueryClient();
  const toast = useToast();
  const [confirming, setConfirming] = useState(false);
  const list = devices.data ?? [];
  const signedIn = list.length > 0 || (apps.data ?? []).length > 0;
  return (
    <section className="space-y-2">
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-sm font-medium">Signed-in devices</h3>
        {!self && signedIn && !confirming ? (
          <Button size="sm" variant="ghost" icon={<LogOut className="size-4" />} onClick={() => setConfirming(true)}>
            Sign out everywhere
          </Button>
        ) : null}
      </div>
      {confirming ? (
        <div className="flex items-center justify-between gap-3 rounded-md border border-border bg-surface-2 px-3 py-2.5">
          <p className="text-sm text-fg">Sign out {user.name} everywhere?</p>
          <div className="flex shrink-0 gap-1">
            <Button size="sm" variant="ghost" onClick={() => setConfirming(false)} disabled={signOut.isPending}>
              Cancel
            </Button>
            <Button
              size="sm"
              variant="danger"
              loading={signOut.isPending}
              onClick={() =>
                signOut.mutate(user.id, {
                  onSuccess: () => {
                    toast.success(`${user.name} was signed out everywhere`);
                    void qc.invalidateQueries({ queryKey: appKeys.user(user.id) });
                  },
                  onError: (e) => toast.error(errorMessage(e)),
                  onSettled: () => setConfirming(false),
                })
              }
            >
              Sign out everywhere
            </Button>
          </div>
        </div>
      ) : null}
      {devices.isPending ? (
        <p className="text-sm text-subtle">Loading…</p>
      ) : list.length === 0 ? (
        <p className="text-sm text-subtle">No signed-in devices.</p>
      ) : (
        <ul className="divide-y divide-white/[0.05] rounded-md border border-border">
          {list.map((d) => {
            const Icon = isPhone(d) ? Smartphone : Laptop;
            return (
              <li key={d.id} className="flex items-center gap-3 px-3 py-2 text-sm">
                <Icon className="size-4 shrink-0 text-subtle" />
                <span className="min-w-0 flex-1 truncate" title={d.user_agent ?? undefined}>
                  {d.name}
                </span>
                <span className="shrink-0 text-xs text-subtle">
                  {[d.ip, formatAgo(d.last_seen_at)].filter(Boolean).join(" · ")}
                </span>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}

/** The apps using someone's account, or allowed by them for another profile. */
function UserApps({ user }: { user: User }) {
  const apps = useUserApps(user.id);
  const signOut = useSignOutUserApp();
  const toast = useToast();
  const list = apps.data ?? [];
  return (
    <section className="space-y-2">
      <h3 className="text-sm font-medium">Apps</h3>
      {apps.isPending ? (
        <p className="text-sm text-subtle">Loading…</p>
      ) : apps.isError ? (
        <p className="text-sm text-danger">{errorMessage(apps.error)}</p>
      ) : list.length === 0 ? (
        <p className="text-sm text-subtle">No apps use this account.</p>
      ) : (
        <ul className="divide-y divide-white/[0.05] rounded-md border border-border">
          {list.map((a) => {
            const label = appLabel(a);
            return (
              <li key={a.id} className="flex items-center gap-3 px-3 py-2 text-sm">
                <AppWindow className="size-4 shrink-0 text-subtle" />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                    <span className="truncate" title={a.client_id}>
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
                      a.profile.id !== user.id ? `Using ${a.profile.name}` : null,
                      a.approved_by.id !== user.id ? `Allowed by ${a.approved_by.name}` : null,
                      scopesLabel(a.scopes),
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  </div>
                  <div className="truncate text-xs text-subtle">
                    {[a.device, a.ip, `last active ${formatAgo(a.last_used_at)}`].filter(Boolean).join(" · ")}
                  </div>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  aria-label={`Sign out ${label}`}
                  loading={signOut.isPending && signOut.variables?.app === a.id}
                  onClick={() =>
                    signOut.mutate(
                      { user: user.id, app: a.id },
                      {
                        onSuccess: () => toast.success(`Signed out ${label}`),
                        onError: (e) => toast.error(errorMessage(e)),
                      },
                    )
                  }
                >
                  Sign out
                </Button>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}

type Kind = "regular" | "managed";

function UserDialog({ user, open, onClose, onDelete }: { user: User | null; open: boolean; onClose: () => void; onDelete: (u: User) => void }) {
  const me = useAuth();
  const users = useUsers();
  const create = useCreateUser();
  const update = useUpdateUser();
  const toast = useToast();
  const isNew = user == null;
  const self = user?.id === me.id;

  const [kind, setKind] = useState<Kind>("regular");
  const [username, setUsername] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [password, setPassword] = useState("");
  const [isAdmin, setIsAdmin] = useState(false);
  const [home, setHome] = useState(true);
  const [disabled, setDisabled] = useState(false);
  const [access, setAccess] = useState<AccessValue>({ all_libraries: true, library_ids: [], max_age: null });
  const [pin, setPin] = useState("");
  const [clearPin, setClearPin] = useState(false);
  // What the Requests select was set to; `null` until then (a new account, or an administrator).
  const [requestChoice, setRequestChoice] = useState<RequestAccess | null>(null);
  // Whether it was set in this dialog, not kept from the account.
  const [requestPicked, setRequestPicked] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Reset the form whenever the dialog opens (for someone else); background
  // refetches don't touch what is being edited.
  useOnChange(open ? (user?.id ?? "new") : null, (key) => {
    if (key === null) return;
    setKind(user?.managed ? "managed" : "regular");
    // Administrators report "auto" whatever is kept for them: no choice to keep.
    setRequestChoice(user && !user.is_admin ? (user.request_access ?? null) : null);
    setRequestPicked(false);
    setUsername(user?.username ?? "");
    setDisplayName(user?.display_name ?? "");
    setPassword("");
    setIsAdmin(user?.is_admin ?? false);
    setHome(user?.home ?? true);
    setDisabled(user?.disabled ?? false);
    setAccess({
      all_libraries: user?.all_libraries ?? true,
      library_ids: user?.library_ids ?? [],
      max_age: user?.max_age ?? null,
    });
    setPin("");
    setClearPin(false);
    setError(null);
  });

  const managed = isNew ? kind === "managed" : (user?.managed ?? false) && !password;
  const otherAdmins = (users.data ?? []).filter((u) => u.is_admin && !u.disabled && u.id !== user?.id).length;
  const lastAdmin = !!user?.is_admin && otherAdmins === 0;
  const busy = create.isPending || update.isPending;
  // Servers before requests have no `request_access`.
  const requestsKnown = (users.data ?? []).some((u) => u.request_access !== undefined);
  // Administrators' requests are always approved as they make them.
  const asAdmin = isAdmin && !managed;
  // As the server has it: an account made without one doesn't ask if it's
  // a managed profile or has an age limit, and adding an age limit to one
  // that asks for approval stops it asking, unless picked here too.
  const ageAdded = !!user && user.max_age === null && access.max_age !== null;
  const requestAccess: RequestAccess =
    ageAdded && !requestPicked && requestChoice === "ask"
      ? "off"
      : (requestChoice ?? (managed || access.max_age !== null ? "off" : "ask"));

  const validate = (): string | null => {
    if (isNew && !username.trim()) return "Enter a username.";
    if (isNew && kind === "regular") {
      const problem = passwordProblem(password, username);
      if (problem) return problem;
    }
    if (!isNew && password) {
      const problem = passwordProblem(password, user?.username);
      if (problem) return problem;
    }
    if (pin && !PIN_RE.test(pin)) return "The PIN must be 4 to 8 digits.";
    if (!access.all_libraries && access.library_ids.length === 0) return "Pick at least one library, or allow all of them.";
    return null;
  };

  const save = () => {
    const problem = validate();
    if (problem) return setError(problem);
    setError(null);
    const name = displayName.trim();
    if (isNew) {
      create.mutate(
        {
          username: username.trim(),
          password: kind === "regular" ? password : undefined,
          display_name: name || undefined,
          is_admin: kind === "regular" && isAdmin,
          managed: kind === "managed",
          home,
          ...access,
          pin: pin || undefined,
          request_access: requestsKnown && !asAdmin ? requestAccess : undefined,
        },
        {
          onSuccess: (u) => {
            toast.success(`Added ${u.name}`);
            onClose();
          },
          onError: (e) => setError(errorMessage(e)),
        },
      );
      return;
    }
    if (!user) return;
    const patch: UserPatch = {};
    if (name !== (user.display_name ?? "")) patch.display_name = name || null;
    if (password) patch.password = password;
    if (isAdmin !== user.is_admin) patch.is_admin = isAdmin;
    if (home !== user.home) patch.home = home;
    if (disabled !== user.disabled) patch.disabled = disabled;
    if (access.all_libraries !== user.all_libraries) patch.all_libraries = access.all_libraries;
    const sameLibraries =
      access.library_ids.length === user.library_ids.length && access.library_ids.every((id) => user.library_ids.includes(id));
    if (!access.all_libraries && !sameLibraries) patch.library_ids = access.library_ids;
    if (access.max_age !== user.max_age) patch.max_age = access.max_age;
    if (pin) patch.pin = pin;
    else if (clearPin) patch.pin = null;
    // One no longer an administrator keeps what the dialog shows, not what was kept from before.
    // An age limit added says along with it whether the account still asks.
    if (requestsKnown && !asAdmin && (user.is_admin || ageAdded || requestAccess !== user.request_access)) patch.request_access = requestAccess;
    if (Object.keys(patch).length === 0) return onClose();
    update.mutate(
      { id: user.id, ...patch },
      {
        onSuccess: (u) => {
          toast.success(`Saved ${u.name}`);
          onClose();
        },
        onError: (e) => setError(errorMessage(e)),
      },
    );
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      locked={busy}
      size="lg"
      title={isNew ? "Add user" : `Edit ${user?.name ?? ""}`}
      footer={
        <>
          {!isNew && user && !self ? (
            <Button variant="ghost" className="mr-auto" icon={<Trash2 className="size-4 text-danger" />} onClick={() => onDelete(user)}>
              <span className="text-danger">Delete</span>
            </Button>
          ) : null}
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button variant="primary" onClick={save} loading={busy}>
            {isNew ? "Add user" : "Save"}
          </Button>
        </>
      }
    >
      <form
        className="space-y-5"
        onSubmit={(e) => {
          e.preventDefault();
          save();
        }}
      >
        {isNew ? (
          <div className="grid gap-2 sm:grid-cols-2">
            {(
              [
                { id: "regular", title: "Regular account", text: "Signs in with a username and password." },
                { id: "managed", title: "Managed profile", text: "No password: a child or guest you switch to from your profile." },
              ] as const
            ).map((o) => (
              <button
                key={o.id}
                type="button"
                aria-pressed={kind === o.id}
                onClick={() => {
                  setKind(o.id);
                  setRequestChoice(null);
                }}
                className={cn(
                  "rounded-lg border p-3 text-left transition-colors",
                  kind === o.id ? "border-accent bg-accent/10" : "border-border hover:border-border-strong",
                )}
              >
                <div className="text-sm font-semibold">{o.title}</div>
                <div className="mt-0.5 text-xs text-muted">{o.text}</div>
              </button>
            ))}
          </div>
        ) : user ? (
          <div className="flex items-center gap-3">
            <Avatar user={user} size="lg" />
            <div className="min-w-0">
              <div className="truncate font-semibold">{user.name}</div>
              <div className="text-sm text-muted">
                @{user.username} · {user.managed ? "Managed profile" : user.is_admin ? "Administrator" : "Regular account"}
              </div>
            </div>
          </div>
        ) : null}

        <div className="grid gap-4 sm:grid-cols-2">
          {isNew ? (
            <Field label="Username" htmlFor="user-username" hint={kind === "managed" ? "Only used internally for managed profiles." : undefined}>
              <TextInput id="user-username" autoComplete="off" value={username} onChange={(e) => setUsername(e.target.value)} />
            </Field>
          ) : null}
          <Field label="Display name" htmlFor="user-display" hint="Shown in the profile switcher and menus.">
            <TextInput
              id="user-display"
              autoComplete="off"
              placeholder={username || user?.username}
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
            />
          </Field>
          {isNew && kind === "regular" ? (
            <Field label="Password" htmlFor="user-password">
              <TextInput id="user-password" type="password" autoComplete="new-password" value={password} onChange={(e) => setPassword(e.target.value)} />
            </Field>
          ) : null}
          {!isNew && self ? (
            <p className="text-xs text-subtle">Your own password, PIN and two-step sign-in change in Account, which asks for your current password.</p>
          ) : null}
          {!isNew && !self ? (
            <Field
              label={user?.managed ? "Password" : "New password"}
              htmlFor="user-password"
              hint={
                user?.managed
                  ? "Setting one turns this profile into a regular account."
                  : user?.has_password === false
                    ? "This account has no password: set one so it can sign in."
                    : "Leave empty to keep the current one."
              }
            >
              <TextInput id="user-password" type="password" autoComplete="new-password" value={password} onChange={(e) => setPassword(e.target.value)} />
            </Field>
          ) : null}
        </div>

        <div className="space-y-2">
          <SwitchRow
            label="Administrator"
            description={managed ? "Managed profiles can't be administrators." : "Can manage libraries, users and server settings."}
            checked={isAdmin && !managed}
            onChange={setIsAdmin}
            disabled={managed || (self && isAdmin) || (lastAdmin && isAdmin)}
          />
          <SwitchRow
            label="Member of this home"
            description="Shown in the profile switcher, so household members can switch to each other."
            checked={home}
            onChange={setHome}
          />
          {!isNew ? (
            <SwitchRow
              label="Disabled"
              description="Can't sign in or play anything until enabled again."
              checked={disabled}
              onChange={setDisabled}
              disabled={self || (lastAdmin && !disabled)}
            />
          ) : null}
        </div>

        {requestsKnown ? (
          <Field
            label="Requests"
            htmlFor="user-requests"
            hint={
              asAdmin
                ? "Administrators' requests are approved as they make them."
                : access.max_age !== null
                  ? "Asking for movies and shows in Discover, which leaves out titles above the age limit."
                  : "Asking for movies and shows the libraries don't have, in Discover."
            }
          >
            <Select
              id="user-requests"
              className="h-10 w-full"
              value={asAdmin ? "auto" : requestAccess}
              disabled={asAdmin}
              onChange={(e) => {
                setRequestChoice(e.target.value as RequestAccess);
                setRequestPicked(true);
              }}
            >
              <option value="ask">May ask, for an administrator to approve</option>
              <option value="auto">May ask, approved as asked</option>
              <option value="off">May not ask</option>
            </Select>
          </Field>
        ) : null}

        <AccessFields value={access} onChange={setAccess} idPrefix="user" />

        {self ? null : (
          <Field
            label="Profile PIN"
            htmlFor="user-pin"
            hint={
              user?.has_pin && !clearPin
                ? "This profile has a PIN. Enter a new one to change it."
                : "Optional, 4 to 8 digits. Needed to switch to this profile."
            }
          >
            <div className="flex gap-2">
              <TextInput
                id="user-pin"
                inputMode="numeric"
                autoComplete="off"
                maxLength={8}
                placeholder={user?.has_pin && !clearPin ? "••••" : ""}
                value={pin}
                onChange={(e) => {
                  setPin(e.target.value.replace(/\D/g, ""));
                  setClearPin(false);
                }}
              />
              {user?.has_pin ? (
                <Button
                  variant="outline"
                  icon={<Lock className="size-4" />}
                  onClick={() => {
                    setPin("");
                    setClearPin(!clearPin);
                  }}
                >
                  {clearPin ? "Keep PIN" : "Remove PIN"}
                </Button>
              ) : null}
            </div>
          </Field>
        )}

        {!isNew && user?.two_step && !self ? <TwoStepReset user={user} /> : null}
        {!isNew && user ? <UserDevices key={user.id} user={user} self={self} /> : null}
        {!isNew && user ? <UserApps user={user} /> : null}

        {error ? <p className="text-sm text-danger">{error}</p> : null}
        <button type="submit" className="hidden" />
      </form>
    </Modal>
  );
}

function UserRow({ user, self, libraries, onOpen }: { user: User; self: boolean; libraries: ReturnType<typeof useLibraries>["data"]; onOpen: () => void }) {
  const age = ageBadge(user.max_age);
  const server = useServer();
  return (
    <li>
      <button
        type="button"
        onClick={onOpen}
        className="flex w-full items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-white/[0.03] focus-visible:bg-white/[0.04] focus-visible:outline-none"
      >
        <Avatar user={user} size="md" className={cn(user.disabled && "opacity-40 grayscale")} />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span className={cn("truncate font-medium", user.disabled && "text-muted")}>{user.name}</span>
            {user.name !== user.username ? <span className="truncate text-xs text-subtle">@{user.username}</span> : null}
            {self ? <span className="text-xs text-subtle">(you)</span> : null}
          </div>
          <div className="mt-1 flex flex-wrap gap-1.5">
            {user.is_admin ? <Badge tone="accent">Admin</Badge> : null}
            {user.managed ? <Badge tone="info">Managed</Badge> : null}
            {user.home ? <Badge>Home</Badge> : null}
            {user.disabled ? <Badge tone="danger">Disabled</Badge> : null}
            {!user.is_admin && !user.all_libraries ? <Badge>{librariesLabel(user, libraries)}</Badge> : null}
            {age && !user.is_admin ? <Badge>{age}</Badge> : null}
            {user.has_pin ? (
              <Badge>
                <Lock className="size-3" /> PIN
              </Badge>
            ) : null}
            {user.two_step ? (
              <Badge tone="success">
                <ShieldCheck className="size-3" /> 2FA
              </Badge>
            ) : mustSetUpTwoStep(user, server.data) && !user.disabled ? (
              <Badge tone="danger">
                <ShieldAlert className="size-3" /> 2FA not set up
              </Badge>
            ) : null}
          </div>
        </div>
        <ChevronRight className="size-4 shrink-0 text-subtle" />
      </button>
    </li>
  );
}

export function UsersSettings() {
  useDocumentTitle("Users");
  const me = useAuth();
  const users = useUsers();
  const libs = useLibraries();
  const del = useDeleteUser();
  const toast = useToast();
  const [dialog, setDialog] = useState<{ user: User | null } | null>(null);
  const [deleting, setDeleting] = useState<User | null>(null);

  if (users.isPending) return <PageSpinner />;
  if (users.isError) {
    return <EmptyState title="Couldn't load users" description={errorMessage(users.error)} action={<Button onClick={() => void users.refetch()}>Retry</Button>} />;
  }

  // Keep the dialog's user in sync with refetched data.
  const dialogUser = dialog?.user ? (users.data.find((u) => u.id === dialog.user?.id) ?? dialog.user) : null;

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">Users</h2>
          <p className="text-sm text-muted">Who can sign in, what each person can see, and the profiles of your home.</p>
        </div>
        <Button variant="primary" icon={<Plus className="size-4" />} onClick={() => setDialog({ user: null })}>
          Add user
        </Button>
      </div>

      {users.data.length === 0 ? (
        <EmptyState icon={<UserPlus />} title="No users" />
      ) : (
        <ul className="divide-y divide-white/[0.05] overflow-hidden rounded-lg border border-border bg-surface">
          {users.data.map((u) => (
            <UserRow key={u.id} user={u} self={u.id === me.id} libraries={libs.data} onOpen={() => setDialog({ user: u })} />
          ))}
        </ul>
      )}

      <UserDialog
        open={!!dialog}
        user={dialogUser}
        onClose={() => setDialog(null)}
        onDelete={(u) => {
          setDialog(null);
          setDeleting(u);
        }}
      />
      <ConfirmModal
        open={!!deleting}
        onClose={() => setDeleting(null)}
        title="Delete user?"
        danger
        confirmLabel="Delete"
        loading={del.isPending}
        message={
          <>
            <strong className="text-fg">{deleting?.name}</strong> will lose access, and their watch history and settings will be
            deleted.
          </>
        }
        onConfirm={() => {
          if (!deleting) return;
          del.mutate(deleting.id, {
            onSuccess: () => {
              toast.success(`Deleted ${deleting.name}`);
              setDeleting(null);
            },
            onError: (e) => toast.error(errorMessage(e)),
          });
        }}
      />
    </div>
  );
}
