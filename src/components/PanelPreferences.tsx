import { accountDisplayName } from "../accountNames";
import {
  PROVIDERS,
  PROVIDER_NAMES,
  targetKey,
  type Connection,
  type Provider,
  type Settings,
} from "../domain";
import { useI18n } from "../i18n";
import { panelCopy } from "../panelCopy";
import { useSettingsChanges } from "../settingsChangesContext";
import { uxCopy } from "../uxCopy";
import { SettingFeedback } from "./SettingFeedback";
import "../styles/preferences.css";

type Props = {
  settings: Settings;
  busy: boolean;
  error?: string | null;
  connections?: Connection[];
  embedded?: boolean;
};

export function PanelPreferences({
  settings,
  connections = [],
  embedded = false,
}: Props) {
  const { locale } = useI18n();
  const { change } = useSettingsChanges();
  const p = (key: Parameters<typeof panelCopy>[1]) => panelCopy(locale, key);
  const providers = PROVIDERS.filter((provider) =>
    settings.accounts.some((account) => account.provider === provider),
  );
  const allProvidersEnabled =
    providers.length > 0 &&
    providers.every((provider) => settings.panel.providers.includes(provider));
  const selectableTargets = settings.targets.filter((target) =>
    settings.panel.providers.includes(target.provider),
  );
  const allTargetsEnabled =
    selectableTargets.length > 0 &&
    selectableTargets.every(
      (target) => !settings.panel.hiddenTargets.includes(targetKey(target)),
    );
  const providerEnabled = (provider: Provider) =>
    settings.panel.providers.includes(provider);

  return (
    <section className="panel-preferences" aria-label={p("title")}>
      {!embedded && (
        <header className="main-header">
          <div>
            <div className="eyebrow">MENU BAR</div>
            <h1>{p("title")}</h1>
          </div>
        </header>
      )}
      <details className="preferences-help">
        <summary>{p("tips")}</summary>
        <p>{p("iconPlacement")}</p>
        <p>{p("exclusions")}</p>
      </details>
      <fieldset>
        <legend>{p("services")}</legend>
        <div className="preference-bulk">
          <button
            className="button"
            disabled={!providers.length}
            onClick={() =>
              providers.forEach(
                (provider) =>
                  void change({
                    kind: "panelProvider",
                    provider,
                    enabled: !allProvidersEnabled,
                  }),
              )
            }
          >
            {uxCopy(locale, allProvidersEnabled ? "deselectAll" : "selectAll")}
          </button>
        </div>
        {providers.map((provider) => (
          <div key={provider} className="preference-control">
            <label className="preference-option">
              <input
                type="checkbox"
                checked={providerEnabled(provider)}
                onChange={(event) =>
                  void change({
                    kind: "panelProvider",
                    provider,
                    enabled: event.currentTarget.checked,
                  })
                }
              />
              {PROVIDER_NAMES[provider]}
            </label>
            <SettingFeedback changeKey={`panelProvider:${provider}`} />
          </div>
        ))}
      </fieldset>
      <fieldset>
        <legend>{p("projects")}</legend>
        <div className="preference-bulk">
          <button
            className="button"
            disabled={!selectableTargets.length}
            onClick={() =>
              selectableTargets.forEach(
                (target) =>
                  void change({
                    kind: "panelTarget",
                    targetKey: targetKey(target),
                    visible: !allTargetsEnabled,
                  }),
              )
            }
          >
            {uxCopy(locale, allTargetsEnabled ? "deselectAll" : "selectAll")}
          </button>
        </div>
        {settings.targets.length === 0 && <p>{p("noTargets")}</p>}
        {settings.targets.map((target) => {
          const key = targetKey(target);
          const account = settings.accounts.find(
            (item) => item.id === target.connectionId,
          );
          const accountName = account
            ? accountDisplayName(account, settings, connections)
            : target.connectionId;
          return (
            <div key={key} className="preference-control">
              <label className="preference-option">
                <input
                  type="checkbox"
                  disabled={!providerEnabled(target.provider)}
                  checked={!settings.panel.hiddenTargets.includes(key)}
                  onChange={(event) =>
                    void change({
                      kind: "panelTarget",
                      targetKey: key,
                      visible: event.currentTarget.checked,
                    })
                  }
                />
                <span>
                  {target.name}
                  <small>
                    {PROVIDER_NAMES[target.provider]} · {accountName}
                  </small>
                </span>
              </label>
              <SettingFeedback changeKey={`panelTarget:${key}`} />
            </div>
          );
        })}
      </fieldset>
      <fieldset>
        <legend>{p("sections")}</legend>
        <div className="preference-control">
          <label className="preference-option">
            <input
              type="checkbox"
              checked={settings.panel.showActive}
              onChange={(event) =>
                void change({
                  kind: "panel",
                  patch: { showActive: event.currentTarget.checked },
                })
              }
            />
            {p("active")}
          </label>
          <SettingFeedback changeKey="panel:showActive" />
        </div>
        <div className="preference-control">
          <label className="preference-option">
            <input
              type="checkbox"
              checked={settings.panel.showCompleted}
              onChange={(event) =>
                void change({
                  kind: "panel",
                  patch: { showCompleted: event.currentTarget.checked },
                })
              }
            />
            {p("completed")}
          </label>
          <SettingFeedback changeKey="panel:showCompleted" />
        </div>
        <div className="preference-control">
          <label className="preference-option">
            {p("limit")}
            <select
              className="period-select"
              value={settings.panel.rowLimit}
              onChange={(event) =>
                void change({
                  kind: "panel",
                  patch: { rowLimit: Number(event.currentTarget.value) },
                })
              }
            >
              {Array.from({ length: 10 }, (_, index) => index + 1).map(
                (count) => (
                  <option key={count} value={count}>
                    {count}
                  </option>
                ),
              )}
            </select>
          </label>
          <SettingFeedback changeKey="panel:rowLimit" />
        </div>
      </fieldset>
      <fieldset>
        <legend>{p("icon")}</legend>
        <div className="preference-control">
          <label className="preference-option">
            <input
              type="checkbox"
              checked={settings.preferences.animateMenuBarIcon}
              onChange={(event) =>
                void change({
                  kind: "menuBarAnimation",
                  value: event.currentTarget.checked,
                })
              }
            />
            {p("animateIcon")}
          </label>
          <SettingFeedback changeKey="menuBarAnimation" />
        </div>
        <p>{p("animateIconHelp")}</p>
      </fieldset>
    </section>
  );
}
