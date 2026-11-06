import { useSyncExternalStore } from "react";
import { Check, Globe } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useServer } from "@/api/hooks";
import { Menu, MenuItem, MenuSeparator } from "@/components/Menu";
import { browserLanguage, currentLanguage, deviceChoiceLanguage, LANGUAGES, onDeviceLanguage, setDeviceLanguage } from ".";

/**
 * The globe of the pages shown before signing in, a small pill with the
 * language shown by its two letters ("FR"): this device's language,
 * Automatic (the browser's) or one picked. Once signed in, an account's own
 * language comes before it; Automatic there means this choice.
 */
export function LanguagePicker() {
  const { t } = useTranslation();
  const server = useServer().data;
  const choice = useSyncExternalStore(onDeviceLanguage, deviceChoiceLanguage);
  const shown = currentLanguage();
  // What Automatic is: the browser's language, else the server's.
  const browser = browserLanguage(server?.language);
  const mark = (on: boolean) => (on ? <Check className="size-4 text-accent" /> : <span className="inline-block size-4" />);

  return (
    <Menu
      label={t("language.choose")}
      trigger={({ toggle, open }) => (
        <button
          type="button"
          onClick={toggle}
          aria-label={t("language.choose")}
          aria-expanded={open}
          title={t(`language.names.${shown}`)}
          className="flex items-center gap-1 rounded-full px-2 py-1 text-xs font-medium text-muted transition-colors hover:bg-white/5 hover:text-fg"
        >
          <Globe className="size-3.5" />
          <span lang={shown}>{shown.toUpperCase()}</span>
        </button>
      )}
    >
      {(close) => (
        <>
          <MenuItem
            icon={mark(!choice)}
            onClick={() => {
              setDeviceLanguage(null);
              close();
            }}
          >
            {t("language.automatic", { language: t(`language.names.${browser}`) })}
          </MenuItem>
          <MenuSeparator />
          {LANGUAGES.map((language) => (
            <MenuItem
              key={language}
              icon={mark(choice === language)}
              onClick={() => {
                setDeviceLanguage(language);
                close();
              }}
            >
              <span lang={language}>{t(`language.names.${language}`)}</span>
            </MenuItem>
          ))}
        </>
      )}
    </Menu>
  );
}

