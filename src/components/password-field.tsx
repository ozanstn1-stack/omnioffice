import { Eye, EyeOff } from "lucide-react";
import { useState } from "react";
import { useT } from "../lib/i18n";

/**
 * A password input with the show/hide toggle the Protect and Unlock panels
 * share. The two screens had their own copies, which drifted on `autoComplete`
 * (`new-password` where a new password is typed, `off` where an existing one
 * is checked) - that difference is a prop here, and everything else is one
 * implementation.
 */
export function PasswordField({
  value,
  onChange,
  placeholder,
  focusOnMount = false,
  autoComplete = "off",
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  /** Focus the input when it mounts (the unlock panel's only field). */
  focusOnMount?: boolean;
  autoComplete?: "new-password" | "current-password" | "off";
}) {
  const t = useT();
  const [visible, setVisible] = useState(false);
  return (
    <div className="relative">
      <input
        className="input pr-10"
        type={visible ? "text" : "password"}
        value={value}
        // eslint-disable-next-line jsx-a11y/no-autofocus -- the password field is the dialog's only target; focusing it is the expected flow
        autoFocus={focusOnMount}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
        autoComplete={autoComplete}
        spellCheck={false}
      />
      <button
        type="button"
        className="absolute right-1 top-1/2 -translate-y-1/2 icon-btn"
        onClick={() => setVisible((previous) => !previous)}
        aria-label={visible ? t("common.hide") : t("common.show")}
        tabIndex={-1}
      >
        {visible ? <EyeOff size={15} /> : <Eye size={15} />}
      </button>
    </div>
  );
}
