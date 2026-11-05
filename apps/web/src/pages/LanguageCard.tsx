import { Globe } from "lucide-react";
import { useTranslation } from "react-i18next";
import { errorMessage } from "@/api/client";
import { useServer, useUpdateProfile } from "@/api/hooks";
import { useAuth } from "@/auth";
import { useToast } from "@/components/Toast";
import { Card, CardTitle, Select } from "@/components/ui";
import { deviceLanguage, supportedLanguage } from "@/i18n";
import { LanguageOptions } from "@/i18n/LanguageOptions";

/** The account's language: the server's until it picks one. */
export function LanguageCard() {
  const { t } = useTranslation();
  const user = useAuth();
  const server = useServer().data;
  const update = useUpdateProfile();
  const toast = useToast();
  const serverLanguage = supportedLanguage(server?.language);
  const device = deviceLanguage() ?? "en";
  // What "Server default" stands for here: the server's language, or this device's.
  const fallback = serverLanguage
    ? t(`language.names.${serverLanguage}`)
    : `${t("language.deviceLanguage")}, ${t(`language.names.${device}`)}`;

  return (
    <Card>
      <CardTitle icon={<Globe />}>{t("language.label")}</CardTitle>
      <p className="mb-4 text-sm text-muted">{t("language.accountHint")}</p>
      <Select
        aria-label={t("language.label")}
        className="w-full sm:w-80"
        value={supportedLanguage(user.language) ?? ""}
        disabled={update.isPending}
        onChange={(e) => update.mutate({ language: e.target.value || null }, { onError: (err) => toast.error(errorMessage(err)) })}
      >
        <option value="">{t("language.serverDefault", { language: fallback })}</option>
        <LanguageOptions />
      </Select>
    </Card>
  );
}
