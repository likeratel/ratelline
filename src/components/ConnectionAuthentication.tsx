import {
  PROVIDER_NAMES,
  type Account,
  type Connection,
  type Provider,
} from "../domain";
import { isGithubProvider } from "../cliAccounts";
import { connectionFixCopy } from "../connectionFixCopy";
import { GITHUB_SIGN_IN_COMMAND } from "../connectionFixes";
import { useI18n } from "../i18n";
import { getPlatformDetails } from "../platformDetails";
import { settingsCopy } from "../settingsCopy";
import { uxCopy } from "../uxCopy";
import { CommandCopy } from "./CommandCopy";
import { LocalCliHelp } from "./LocalCliHelp";

/** Sign-in and installation help for CLI-based connections. */
export function ConnectionAuthentication({
  provider,
  account,
  connection,
}: {
  provider: Provider;
  account: Account | undefined;
  connection: Connection | undefined;
}) {
  const { locale, t } = useI18n();
  const details = getPlatformDetails(provider, locale);
  return (
    <details
      id="connection-authentication"
      className="authentication-details"
      open={connection?.status !== "connected"}
    >
      <summary>{uxCopy(locale, "authentication")}</summary>
      {isGithubProvider(provider) ? (
        <div className="connection-help-body">
          <p className="field-help">
            {connectionFixCopy(locale, "githubSignInHelp")}
          </p>
          <CommandCopy command={GITHUB_SIGN_IN_COMMAND} />
        </div>
      ) : (
        <details className="connection-help">
          <summary>{settingsCopy(locale, "connectionHelp")}</summary>
          <p>
            {provider === "aws"
              ? account
                ? t("connections.awsSelected", {
                    profile: account.profile ?? account.label,
                    command: "aws sso login --profile <profile>",
                  })
                : t("connections.awsAdd")
              : account?.mode === "isolated"
                ? t("connections.appLogin", {
                    provider: PROVIDER_NAMES[provider],
                  })
                : details.cliCommand
                  ? t("connections.completeCli", {
                      command: details.cliCommand,
                    })
                  : details.connectionSummary}
          </p>
        </details>
      )}
      <LocalCliHelp
        key={provider}
        provider={provider}
        version={connection?.cliVersion}
      />
    </details>
  );
}
