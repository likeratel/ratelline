import { useEffect, useRef } from "react";
import { connectionFixCopy } from "../connectionFixCopy";
import { useI18n } from "../i18n";

/** Asks before a connection is rebound to a different GitHub user. */
export function IdentityChangeConfirm({
  login,
  busy,
  onConfirm,
  onCancel,
}: {
  login: string;
  busy: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { locale } = useI18n();
  const confirm = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    confirm.current?.focus();
  }, []);
  return (
    <div
      className="identity-change-confirm"
      role="group"
      aria-label={connectionFixCopy(locale, "useNamedAccount", { login })}
    >
      <p>{connectionFixCopy(locale, "identityChangeConfirm", { login })}</p>
      <button
        ref={confirm}
        type="button"
        className="button primary"
        disabled={busy}
        onClick={onConfirm}
      >
        {connectionFixCopy(locale, "confirmChange")}
      </button>
      <button
        type="button"
        className="button"
        disabled={busy}
        onClick={onCancel}
      >
        {connectionFixCopy(locale, "cancel")}
      </button>
    </div>
  );
}
