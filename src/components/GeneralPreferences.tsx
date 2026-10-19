import { useEffect, useState } from "react";
import { bridge, errorMessage, isPreview } from "../bridge";
import { desktop, type DesktopStatus, type UpdateInfo } from "../desktopBridge";
import { languageSchema, type Connection, type Settings } from "../domain";
import { copy } from "../featureCopy";
import { useI18n } from "../i18n";
import { useSettingsChanges } from "../settingsChangesContext";
import { APP_VERSION } from "../version";
import { SettingFeedback } from "./SettingFeedback";
import "../styles/preferences.css";

type Props = {
  settings: Settings;
  busy: boolean;
  error?: string | null;
  connections?: Connection[];
};

export function GeneralPreferences({ settings, error }: Props) {
  const { locale } = useI18n();
  const { change } = useSettingsChanges();
  const c = (
    key: Parameters<typeof copy>[1],
    values?: Record<string, string | number>,
  ) => copy(locale, key, values);
  const [status, setStatus] = useState<DesktopStatus | null>(null);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [working, setWorking] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void desktop
      .status()
      .then((value) => {
        if (active) setStatus(value);
      })
      .catch((cause) => {
        if (active) setProblem(errorMessage(cause));
      });
    return () => {
      active = false;
    };
  }, []);

  const perform = async (action: () => Promise<void>) => {
    setWorking(true);
    setProblem(null);
    try {
      await action();
    } catch (cause) {
      setProblem(errorMessage(cause));
    } finally {
      setWorking(false);
    }
  };

  return (
    <section className="panel-preferences" aria-label={c("settings")}>
      <fieldset>
        <legend>{c("language")}</legend>
        <div className="preference-control">
          <div className="settings-field">
            <select
              aria-label={c("language")}
              value={settings.preferences.language}
              onChange={(event) =>
                void change({
                  kind: "language",
                  value: languageSchema.parse(event.currentTarget.value),
                })
              }
            >
              <option value="system">{c("system")}</option>
              <option value="ko">한국어</option>
              <option value="en">English</option>
              <option value="ja">日本語</option>
              <option value="zh-CN">简体中文</option>
            </select>
          </div>
          <SettingFeedback changeKey="language" />
        </div>
      </fieldset>
      <fieldset>
        <legend>{c("startup")}</legend>
        <label className="preference-option">
          <input
            type="checkbox"
            disabled={working || isPreview || !status}
            checked={status?.autostart ?? false}
            onChange={(event) => {
              const value = event.currentTarget.checked;
              void perform(async () => {
                const result = await desktop.autostart(value);
                setStatus((previous) =>
                  previous ? { ...previous, autostart: result } : previous,
                );
              });
            }}
          />
          {c("autostart")}
        </label>
        <div className="preference-control">
          <label className="preference-option">
            <input
              type="checkbox"
              disabled={working || !status?.updaterConfigured}
              checked={
                Boolean(status?.updaterConfigured) &&
                settings.preferences.checkUpdatesAutomatically
              }
              onChange={(event) =>
                void change({
                  kind: "automaticUpdates",
                  value: event.currentTarget.checked,
                })
              }
            />
            {c("autoUpdate")}
          </label>
          <SettingFeedback changeKey="automaticUpdates" />
        </div>
        {isPreview ? (
          <p>{c("desktopOnly")}</p>
        ) : desktop.installedVersion() ? (
          <p role="status">{c("restartNote")}</p>
        ) : status?.updaterConfigured ? (
          <>
            <button
              className="button"
              disabled={working}
              onClick={() =>
                void perform(async () => setUpdate(await desktop.check()))
              }
            >
              {c("checkUpdate")}
            </button>
            {update && (
              <p role="status">
                {update.available
                  ? c("updateAvailable", { version: update.version ?? "" })
                  : c("upToDate")}
              </p>
            )}
            {update?.available && (
              <>
                <p>{update.body}</p>
                <button
                  className="button"
                  disabled={working}
                  onClick={() =>
                    void perform(async () => {
                      await desktop.install(update.version!);
                      setUpdate(null);
                      setProblem(c("restartNote"));
                    })
                  }
                >
                  {c("installUpdate")}
                </button>
              </>
            )}
          </>
        ) : (
          <p>{c("updaterUnavailable")}</p>
        )}
        {status?.updaterConfigured && (
          <button
            className="text-button"
            disabled={working}
            onClick={() =>
              void perform(() =>
                bridge.open("https://github.com/likeratel/ratelline/releases"),
              )
            }
          >
            {c("releases")}
          </button>
        )}
      </fieldset>
      {(problem || error) && (
        <p className="notice warning" role="alert">
          {problem || error}
        </p>
      )}
      <div className="preferences-about">
        <span>{c("about")}</span>
        <strong>Ratelline {APP_VERSION}</strong>
      </div>
    </section>
  );
}
