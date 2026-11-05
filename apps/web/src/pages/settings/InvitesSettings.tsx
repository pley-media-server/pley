import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, Copy, Link2, MailPlus, Plus, Trash2 } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useCreateInvite, useDeleteInvite, useInvites, useLibraries } from "@/api/hooks";
import type { Invite } from "@/api/types";
import { AccessFields, type AccessValue } from "@/components/AccessFields";
import { ConfirmModal, Modal } from "@/components/Modal";
import { useToast } from "@/components/Toast";
import { Badge, Button, EmptyState, Field, IconButton, PageSpinner, Select, TextInput } from "@/components/ui";
import { ageBadge, librariesLabel } from "@/lib/access";
import { copyText } from "@/lib/clipboard";
import { formatRelative } from "@/lib/format";
import { useDocumentTitle } from "@/lib/hooks";

export function inviteUrl(code: string): string {
  return `${window.location.origin}/invite/${code}`;
}

function CopyButton({ text, label }: { text: string; label?: string }) {
  const { t } = useTranslation(["settings", "common"]);
  const [copied, setCopied] = useState(false);
  return (
    <IconButton
      label={copied ? t("common:actions.copied") : (label ?? t("invites.copyLink"))}
      size="sm"
      onClick={() =>
        void copyText(text).then((ok) => {
          if (!ok) return;
          setCopied(true);
          window.setTimeout(() => setCopied(false), 1500);
        })
      }
    >
      {copied ? <Check className="size-4 text-success" /> : <Copy className="size-4" />}
    </IconButton>
  );
}

function status(invite: Invite): { state: "used" | "expired" | "active"; tone: "success" | "neutral" | "danger" } {
  if (invite.uses_left <= 0) return { state: "used", tone: "neutral" };
  if (invite.expires_at != null && invite.expires_at * 1000 <= Date.now()) return { state: "expired", tone: "danger" };
  return { state: "active", tone: "success" };
}

/** How long a link lasts, in days; 0 is never. */
const EXPIRY_OPTIONS = [1, 7, 30, 0];

function CreateInviteModal({ open, onClose, onCreated }: { open: boolean; onClose: () => void; onCreated: (i: Invite) => void }) {
  const { t } = useTranslation(["settings", "common"]);
  const create = useCreateInvite();
  const [days, setDays] = useState(7);
  const [uses, setUses] = useState(1);
  const [note, setNote] = useState("");
  const [access, setAccess] = useState<AccessValue>({ all_libraries: true, library_ids: [], max_age: null });
  const [error, setError] = useState<string | null>(null);

  const close = () => {
    setDays(7);
    setUses(1);
    setNote("");
    setAccess({ all_libraries: true, library_ids: [], max_age: null });
    setError(null);
    onClose();
  };

  const submit = () => {
    if (!access.all_libraries && access.library_ids.length === 0) return setError(t("invites.create.pickLibrary"));
    if (!Number.isFinite(uses) || uses < 1) return setError(t("invites.create.atLeastOne"));
    setError(null);
    create.mutate(
      { expires_in_days: days, uses, note: note.trim() || undefined, ...access },
      {
        onSuccess: (invite) => {
          onCreated(invite);
          close();
        },
        onError: (e) => setError(errorMessage(e)),
      },
    );
  };

  return (
    <Modal
      open={open}
      onClose={close}
      locked={create.isPending}
      title={t("invites.new")}
      footer={
        <>
          <Button variant="ghost" onClick={close} disabled={create.isPending}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" onClick={submit} loading={create.isPending}>
            {t("invites.create.createLink")}
          </Button>
        </>
      }
    >
      <form
        className="space-y-4"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <p className="text-sm text-muted">{t("invites.create.intro")}</p>
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label={t("invites.create.expires")} htmlFor="invite-days">
            <Select id="invite-days" className="h-10 w-full" value={String(days)} onChange={(e) => setDays(Number(e.target.value))}>
              {EXPIRY_OPTIONS.map((d) => (
                <option key={d} value={d}>
                  {d === 0 ? t("invites.never") : t("invites.days", { count: d })}
                </option>
              ))}
            </Select>
          </Field>
          <Field label={t("invites.create.uses")} htmlFor="invite-uses">
            <TextInput
              id="invite-uses"
              type="number"
              min={1}
              max={1000}
              value={uses}
              onChange={(e) => setUses(Math.max(1, Math.floor(Number(e.target.value) || 1)))}
            />
          </Field>
        </div>
        <Field label={t("invites.create.note")} htmlFor="invite-note" hint={t("invites.create.noteHint")}>
          <TextInput id="invite-note" maxLength={200} value={note} onChange={(e) => setNote(e.target.value)} />
        </Field>
        <AccessFields value={access} onChange={setAccess} idPrefix="invite" />
        {error ? <p className="text-sm text-danger">{error}</p> : null}
        <button type="submit" className="hidden" />
      </form>
    </Modal>
  );
}

function CreatedModal({ invite, onClose }: { invite: Invite | null; onClose: () => void }) {
  const { t } = useTranslation(["settings", "common"]);
  const url = invite ? inviteUrl(invite.code) : "";
  return (
    <Modal
      open={!!invite}
      onClose={onClose}
      size="md"
      title={t("invites.ready.title")}
      footer={
        <Button variant="primary" onClick={onClose}>
          {t("common:actions.done")}
        </Button>
      }
    >
      <div className="space-y-3">
        <p className="text-sm text-muted">{t("invites.ready.send")}</p>
        <div className="flex items-center gap-2 rounded-md border border-border bg-surface-2 py-1 pl-3 pr-1">
          <Link2 className="size-4 shrink-0 text-accent" />
          <input
            readOnly
            value={url}
            onFocus={(e) => e.target.select()}
            className="min-w-0 flex-1 bg-transparent text-sm text-fg outline-none"
            aria-label={t("invites.ready.link")}
          />
          <CopyButton text={url} />
        </div>
        {invite?.expires_at ? <p className="text-xs text-subtle">{t("invites.ready.expires", { when: formatRelative(invite.expires_at) })}</p> : null}
      </div>
    </Modal>
  );
}

export function InvitesSettings() {
  const { t } = useTranslation(["settings", "common"]);
  useDocumentTitle(t("invites.title"));
  const invites = useInvites();
  const libs = useLibraries();
  const del = useDeleteInvite();
  const toast = useToast();
  const [creating, setCreating] = useState(false);
  const [created, setCreated] = useState<Invite | null>(null);
  // "expires" / "expired" is decided once per visit, not on every render.
  const [now] = useState(() => Date.now());
  const [deleting, setDeleting] = useState<Invite | null>(null);

  if (invites.isPending) return <PageSpinner />;
  if (invites.isError) {
    return (
      <EmptyState
        title={t("invites.couldntLoad")}
        description={errorMessage(invites.error)}
        action={<Button onClick={() => void invites.refetch()}>{t("common:actions.retry")}</Button>}
      />
    );
  }

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{t("invites.title")}</h2>
          <p className="text-sm text-muted">{t("invites.description")}</p>
        </div>
        <Button variant="primary" icon={<Plus className="size-4" />} onClick={() => setCreating(true)}>
          {t("invites.new")}
        </Button>
      </div>

      {invites.data.length === 0 ? (
        <EmptyState
          icon={<MailPlus />}
          title={t("invites.emptyTitle")}
          description={t("invites.emptyDescription")}
        />
      ) : (
        <ul className="divide-y divide-white/[0.05] overflow-hidden rounded-lg border border-border bg-surface">
          {invites.data.map((invite) => {
            const st = status(invite);
            const age = ageBadge(invite.max_age);
            return (
              <li key={invite.code} className="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3">
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-mono text-sm">{invite.code}</span>
                    <Badge tone={st.tone}>{t(`invites.status.${st.state}`)}</Badge>
                    <Badge>{librariesLabel(invite, libs.data)}</Badge>
                    {age ? <Badge>{age}</Badge> : null}
                  </div>
                  <div className="mt-1 truncate text-xs text-subtle">
                    {[
                      invite.note,
                      st.state === "active" ? t("invites.signUpsLeft", { count: invite.uses_left }) : null,
                      invite.expires_at
                        ? t(invite.expires_at * 1000 > now ? "invites.expires" : "invites.expired", { when: formatRelative(invite.expires_at) })
                        : t("invites.neverExpires"),
                      t("invites.created", { when: formatRelative(invite.created_at) }),
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  </div>
                </div>
                <div className="flex items-center gap-1">
                  {st.state === "active" ? <CopyButton text={inviteUrl(invite.code)} /> : null}
                  <IconButton label={t("invites.delete")} size="sm" onClick={() => setDeleting(invite)}>
                    <Trash2 className="size-4" />
                  </IconButton>
                </div>
              </li>
            );
          })}
        </ul>
      )}

      <CreateInviteModal open={creating} onClose={() => setCreating(false)} onCreated={setCreated} />
      <CreatedModal invite={created} onClose={() => setCreated(null)} />
      <ConfirmModal
        open={!!deleting}
        onClose={() => setDeleting(null)}
        title={t("invites.deleteTitle")}
        danger
        confirmLabel={t("common:actions.delete")}
        loading={del.isPending}
        message={t("invites.deleteMessage")}
        onConfirm={() => {
          if (!deleting) return;
          del.mutate(deleting.code, {
            onSuccess: () => {
              toast.success(t("invites.deleted"));
              setDeleting(null);
            },
            onError: (e) => toast.error(errorMessage(e)),
          });
        }}
      />
    </div>
  );
}
