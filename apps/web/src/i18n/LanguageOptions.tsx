import { useTranslation } from "react-i18next";
import { LANGUAGES } from ".";

/** An `<option>` for each language Pley is in, each named in itself ("Français"). */
export function LanguageOptions() {
  const { t } = useTranslation();
  return LANGUAGES.map((language) => (
    <option key={language} value={language} lang={language}>
      {t(`language.names.${language}`)}
    </option>
  ));
}
