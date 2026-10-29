import { accountDisplayName } from "../accountNames";
import type { Account, Connection, Settings } from "../domain";
import { useI18n } from "../i18n";
import { causeCopy } from "../connectionFixCopy";

/** The selected connection's status line and verified account. */
export function ConnectionStateSummary({
  account,
  connection,
  settings,
  statusLabel,
  healthy,
}: {
  account: Account | undefined;
  connection: Connection | undefined;
  settings: Settings;
  statusLabel: string;
  healthy: boolean;
}) {
  const { locale, t } = useI18n();
  const name = account
    ? accountDisplayName(account, settings, connection ? [connection] : [])
    : null;
  return (
    <div className="connection-state" role="status">
      <span
        aria-hidden="true"
        className={`connection-dot ${healthy ? "connected" : ""}`}
      />
      <div>
        <strong>
          <span className="connection-status-label">{statusLabel} · </span>
          {name ?? t("connections.checkConnection")}
        </strong>
        {connection?.account && name && connection.account !== name && (
          <p>{connection.account}</p>
        )}
        {connection?.status !== "connected" &&
          (connection?.kind && connection.kind !== "other" ? (
            <p>{causeCopy(locale, connection.kind)}</p>
          ) : (
            connection?.message && <p>{connection.message}</p>
          ))}
      </div>
    </div>
  );
}
