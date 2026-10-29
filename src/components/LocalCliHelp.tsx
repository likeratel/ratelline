import { useState } from "react";
import { bridge, errorMessage } from "../bridge";
import { authCopy } from "../authCopy";
import { useI18n } from "../i18n";
import type { Provider } from "../domain";
import { getPlatformDetails } from "../platformDetails";

const guides: Partial<Record<Provider, string>> = {
  github: "https://cli.github.com/",
  "github-deployments": "https://cli.github.com/",
  vercel: "https://vercel.com/docs/cli",
  aws: "https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html",
  railway: "https://docs.railway.com/guides/cli",
  netlify: "https://docs.netlify.com/cli/get-started/",
  render: "https://render.com/docs/cli",
  heroku: "https://devcenter.heroku.com/articles/heroku-cli",
  fly: "https://fly.io/docs/flyctl/install/",
  gitlab: "https://docs.gitlab.com/cli/",
  digitalocean: "https://docs.digitalocean.com/reference/doctl/how-to/install/",
  gcloud: "https://cloud.google.com/sdk/docs/install",
  firebase: "https://cloud.google.com/sdk/docs/install",
  eas: "https://docs.expo.dev/eas/cli/",
  azure: "https://learn.microsoft.com/cli/azure/install-azure-cli",
};

export function LocalCliHelp({
  provider,
  version,
  open = false,
}: {
  provider: Provider;
  version?: string | null;
  open?: boolean;
}) {
  const { locale } = useI18n();
  const [error, setError] = useState<string | null>(null);
  const guide = guides[provider];
  if (!guide) return null;
  return (
    <details className="connection-help" open={open}>
      <summary>{authCopy(locale, "installation")}</summary>
      <p>{authCopy(locale, "localHelp")}</p>
      <code>{getPlatformDetails(provider, locale).cliCommand}</code>
      {version && (
        <p>
          {authCopy(locale, "version")}: {version}
        </p>
      )}
      <button
        type="button"
        className="text-button"
        onClick={() => {
          void bridge
            .open(guide)
            .catch((cause: unknown) => setError(errorMessage(cause)));
        }}
      >
        {authCopy(locale, "installation")}
      </button>
      {error && <p role="alert">{error}</p>}
    </details>
  );
}
