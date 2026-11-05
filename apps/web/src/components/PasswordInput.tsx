import { forwardRef, useState, type InputHTMLAttributes } from "react";
import { Eye, EyeOff } from "lucide-react";
import { useTranslation } from "react-i18next";
import { TextInput } from "./ui";

/** A password field whose text can be shown, so it needs no second "confirm" field. */
export const PasswordInput = forwardRef<HTMLInputElement, Omit<InputHTMLAttributes<HTMLInputElement>, "type">>(function PasswordInput(
  { className, ...rest },
  ref,
) {
  const { t } = useTranslation("components");
  const [visible, setVisible] = useState(false);
  return (
    <div className="relative">
      <TextInput ref={ref} type={visible ? "text" : "password"} spellCheck={false} className={className ? `pr-10 ${className}` : "pr-10"} {...rest} />
      <button
        type="button"
        onClick={() => setVisible((v) => !v)}
        aria-label={visible ? t("passwordInput.hide") : t("passwordInput.show")}
        aria-pressed={visible}
        className="absolute inset-y-0 right-0 flex w-10 items-center justify-center rounded-r-md text-subtle transition-colors hover:text-fg"
      >
        {visible ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
      </button>
    </div>
  );
});
