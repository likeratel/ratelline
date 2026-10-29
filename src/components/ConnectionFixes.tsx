import { useMemo } from "react";
import { accountDisplayName } from "../accountNames";
import { connectionFixCopy } from "../connectionFixCopy";
import { connectionFixes } from "../connectionFixes";
import type { Snapshot } from "../domain";
import { useI18n } from "../i18n";
import { ConnectionFixCard, type FixHandlers } from "./ConnectionFixCard";
import "../styles/connection-fixes.css";

type Props = FixHandlers & {
  snapshot: Snapshot;
  busy: boolean;
  focusConnectionId?: string;
  focusRequest?: number;
};

/** Collects every connection that needs attention above the connection form. */
export function ConnectionFixes({
  snapshot,
  busy,
  focusConnectionId,
  focusRequest,
  ...handlers
}: Props) {
  const { locale } = useI18n();
  const fixes = useMemo(() => connectionFixes(snapshot), [snapshot]);
  if (!fixes.length) return null;
  return (
    <section
      className="connection-fixes"
      aria-labelledby="connection-fixes-title"
    >
      <h2 id="connection-fixes-title">{connectionFixCopy(locale, "title")}</h2>
      <p className="field-help">{connectionFixCopy(locale, "intro")}</p>
      <ul>
        {fixes.map((fix) => {
          const account = snapshot.settings.accounts.find(
            (item) => item.id === fix.connectionId,
          );
          return (
            <ConnectionFixCard
              key={`${fix.connectionId}:${fix.kind}`}
              fix={fix}
              accountName={
                account
                  ? accountDisplayName(
                      account,
                      snapshot.settings,
                      snapshot.connections,
                    )
                  : fix.connectionId
              }
              busy={busy}
              focusRequest={
                fix.connectionId === focusConnectionId
                  ? focusRequest
                  : undefined
              }
              {...handlers}
            />
          );
        })}
      </ul>
    </section>
  );
}
