import type { Dispatch, SetStateAction } from "react";
import { useTranslation } from "react-i18next";
import { TriangleAlert } from "lucide-react";
import type { Settings } from "@/api/types";
import { Button, TextInput, Toggle } from "@/components/ui";
import { Group, Note, Row, SecretInput, TestButton } from "./SettingsForm";

/** The settings single sign-on uses, for the page's list of what it edits. */
export const SSO_KEYS = [
  "public_url",
  "sso_enabled",
  "sso_name",
  "sso_issuer",
  "sso_client_id",
  "sso_client_secret",
  "sso_new_accounts",
  "sso_group",
  "sso_admin_claim",
  "sso_admin_value",
] as const satisfies readonly (keyof Settings)[];

/** An address to copy into the provider's settings. */
function Copyable({ value }: { value: string }) {
  return (
    <code className="block select-all break-all rounded-md border border-border bg-surface-2 px-3 py-2 font-mono text-xs text-fg">{value}</code>
  );
}

/**
 * Signing in with an OpenID Connect provider (Authentik, Keycloak, Google…).
 * Pley asks it only who signs in, and keeps none of its tokens. The client
 * secret is never sent back: it is sent only when typed, and goes only to
 * the issuer and client it was saved for.
 */
export function SingleSignOnGroup({ form, setForm }: { form: Settings; setForm: Dispatch<SetStateAction<Settings | null>> }) {
  const { t } = useTranslation("settings");
  // A server before single sign-on sends none of it.
  if (form.sso_enabled === undefined) return null;
  const set = <K extends keyof Settings>(key: K, value: Settings[K]) => setForm((f) => (f ? { ...f, [key]: value } : f));
  // Another issuer or client doesn't get the saved secret (the server forgets it too).
  const setProvider = (key: "sso_issuer" | "sso_client_id", value: string) =>
    setForm((f) => (f ? { ...f, [key]: value, ...(f.sso_client_secret === undefined ? { sso_client_secret_set: false } : {}) } : f));
  const publicUrl = form.public_url ?? "";
  const base = publicUrl.trim().replace(/\/+$/, "") || window.location.origin;
  const clientId = form.sso_client_id ?? "";
  const registered = clientId.trim() !== "";
  const group = form.sso_group ?? "";
  const adminClaim = form.sso_admin_claim ?? "";
  const secretSaved = form.sso_client_secret_set === true;

  return (
    <Group title={t("server.sso.title")} description={t("server.sso.description")}>
      <Row label={t("server.sso.enabled")} hint={t("server.sso.enabledHint")}>
        <Toggle checked={form.sso_enabled} onChange={(v) => set("sso_enabled", v)} label={t("server.sso.enabled")} />
      </Row>
      <Row label={t("server.sso.publicUrl")} htmlFor="public-url" hint={t("server.sso.publicUrlHint")}>
        <TextInput
          id="public-url"
          inputMode="url"
          spellCheck={false}
          placeholder="https://pley.example.com"
          value={publicUrl}
          onChange={(e) => set("public_url", e.target.value)}
        />
      </Row>
      <Row label={t("server.sso.name")} htmlFor="sso-name" hint={t("server.sso.nameHint")}>
        <TextInput id="sso-name" placeholder="Authentik" value={form.sso_name ?? ""} onChange={(e) => set("sso_name", e.target.value)} />
      </Row>
      <Row label={t("server.sso.issuer")} htmlFor="sso-issuer" hint={t("server.sso.issuerHint")}>
        <TextInput
          id="sso-issuer"
          inputMode="url"
          spellCheck={false}
          placeholder="https://auth.example.com/application/o/pley/"
          value={form.sso_issuer ?? ""}
          onChange={(e) => setProvider("sso_issuer", e.target.value)}
        />
        <TestButton
          key={`${form.sso_issuer ?? ""}
${clientId}
${publicUrl}`}
          label={t("server.sso.test")}
          test={{ service: "sso", issuer: form.sso_issuer ?? "", client_id: clientId, public_url: publicUrl }}
        />
      </Row>
      <Row label={t("server.sso.clientId")} htmlFor="sso-client" hint={t("server.sso.clientIdHint")}>
        <TextInput id="sso-client" spellCheck={false} value={clientId} onChange={(e) => setProvider("sso_client_id", e.target.value)} />
      </Row>
      {registered ? (
        <Row label={t("server.sso.secret")} htmlFor="sso-secret" hint={t("server.sso.secretHint")}>
          <SecretInput
            id="sso-secret"
            what="key"
            value={form.sso_client_secret ?? ""}
            // Left empty, the saved secret stays.
            onChange={(v) => set("sso_client_secret", v || undefined)}
            placeholder={secretSaved ? t("server.sso.secretSaved") : t("server.sso.secretNone")}
          />
          {secretSaved && !form.sso_client_secret ? (
            <Button
              size="sm"
              variant="ghost"
              className="mt-2"
              onClick={() => setForm((f) => (f ? { ...f, sso_client_secret: "", sso_client_secret_set: false } : f))}
            >
              {t("server.sso.removeSecret")}
            </Button>
          ) : null}
        </Row>
      ) : null}
      <Row
        label={registered ? t("server.sso.redirectUri") : t("server.sso.metadataDocument")}
        hint={registered ? t("server.sso.redirectUriHint") : t("server.sso.metadataDocumentHint")}
      >
        <Copyable value={registered ? `${base}/api/auth/sso/callback` : `${base}/api/auth/sso/client`} />
      </Row>
      <Row label={t("server.sso.group")} htmlFor="sso-group" hint={t("server.sso.groupHint")}>
        <TextInput id="sso-group" spellCheck={false} placeholder="pley-users" value={group} onChange={(e) => set("sso_group", e.target.value)} />
      </Row>
      <Row label={t("server.sso.newAccounts")} hint={t("server.sso.newAccountsHint")}>
        <Toggle checked={form.sso_new_accounts ?? false} onChange={(v) => set("sso_new_accounts", v)} label={t("server.sso.newAccounts")} />
        {form.sso_new_accounts && !group.trim() ? (
          <p className="mt-2 flex items-start gap-2 rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-danger">
            <TriangleAlert className="mt-px size-3.5 shrink-0" />
            {t("server.sso.anyoneWarning")}
          </p>
        ) : null}
      </Row>
      <Row label={t("server.sso.adminClaim")} htmlFor="sso-admin-claim" hint={t("server.sso.adminClaimHint")}>
        <TextInput
          id="sso-admin-claim"
          spellCheck={false}
          placeholder="groups"
          value={adminClaim}
          onChange={(e) => set("sso_admin_claim", e.target.value)}
        />
      </Row>
      {adminClaim.trim() ? (
        <Row label={t("server.sso.adminValue")} htmlFor="sso-admin-value" hint={t("server.sso.adminValueHint")}>
          <TextInput
            id="sso-admin-value"
            spellCheck={false}
            placeholder="pley-admins"
            value={form.sso_admin_value ?? ""}
            onChange={(e) => set("sso_admin_value", e.target.value)}
          />
          <p className="mt-2 flex items-start gap-2 rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-danger">
            <TriangleAlert className="mt-px size-3.5 shrink-0" />
            {t("server.sso.adminWarning")}
          </p>
        </Row>
      ) : null}
      <Note>{t("server.sso.twoStep")}</Note>
      <Note>{t("server.sso.scopes")}</Note>
    </Group>
  );
}
