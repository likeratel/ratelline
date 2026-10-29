import { ArrowUpRight } from "lucide-react";
import { bridge, errorMessage } from "../bridge";
import type { LoginStatus } from "../domain";
import { useI18n } from "../i18n";

/** Progress of an in-app CLI sign-in, with its device code and link. */
export function LoginNotice({
  login,
  onError,
  onLoadTargets,
}: {
  login: LoginStatus;
  onError: (message: string) => void;
  onLoadTargets: () => void;
}) {
  const { t } = useI18n();
  return (
    <div
      className={`notice ${login.state === "error" ? "warning" : ""}`}
      role="status"
    >
      <span>{login.message}</span>
      {login.code && <code className="device-code">{login.code}</code>}
      {login.url && (
        <button
          className="text-button"
          onClick={() =>
            void bridge
              .open(login.url!)
              .catch((cause: unknown) => onError(errorMessage(cause)))
          }
        >
          {t("connections.openAuth")} <ArrowUpRight size={13} />
        </button>
      )}
      {login.state === "complete" && (
        <button className="text-button" onClick={onLoadTargets}>
          {t("connections.loadTargets")}
        </button>
      )}
    </div>
  );
}
