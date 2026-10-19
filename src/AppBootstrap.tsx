import { Component, lazy, Suspense, type ReactNode } from "react";
import { useI18n } from "./i18n";

const Dashboard = lazy(() => import("./App"));
const Panel = lazy(() =>
  import("./components/TrayPanel").then(({ TrayPanel }) => ({
    default: TrayPanel,
  })),
);
const bootCopy = {
  ko: [
    "화면을 불러오는 중…",
    "화면을 불러오지 못했습니다. 다시 시도해 주세요.",
  ],
  en: ["Loading…", "Could not load this view. Please try again."],
  ja: ["画面を読み込み中…", "画面を読み込めませんでした。再試行してください。"],
  "zh-CN": ["正在加载…", "无法加载此视图，请重试。"],
} as const;

function StartupState({ failed = false }: { failed?: boolean }) {
  const { locale, t } = useI18n();
  return (
    <main className="boot" role={failed ? "alert" : "status"}>
      <h1>ratelline</h1>
      <p>{bootCopy[locale][failed ? 1 : 0]}</p>
      {failed && (
        <button className="button" onClick={() => window.location.reload()}>
          {t("common.retry")}
        </button>
      )}
    </main>
  );
}

class StartupBoundary extends Component<
  { children: ReactNode },
  { failed: boolean }
> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  render() {
    return this.state.failed ? <StartupState failed /> : this.props.children;
  }
}

export function AppBootstrap() {
  const dashboard =
    new URLSearchParams(window.location.search).get("view") === "dashboard";
  return (
    <StartupBoundary>
      <Suspense fallback={<StartupState />}>
        {dashboard ? <Dashboard /> : <Panel />}
      </Suspense>
    </StartupBoundary>
  );
}
