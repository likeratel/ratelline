import type { Locale } from "./i18n";

const en = {
  accountId: "Account ID",
  token: "API token",
  connect: "Connect",
  connecting: "Connecting…",
  renew: "Renew token",
  tokenHelp:
    "Saved in this app's data folder on this device. Require Cloudflare Pages Read for Pages, Workers Scripts Read for Workers, and Workers CI Read for Builds. Runs without an identified actor appear in All runs.",
  pagesSummary: "Read deployment history from the Cloudflare Pages API.",
  workersSummary:
    "Read Worker deployment records. Cloudflare Builds are a separate target type.",
  firebaseSummary:
    "When connected, Ratelline binds the current gcloud account—not the Firebase CLI login—to read Hosting releases and App Hosting rollouts.",
  easSummary:
    "Sign in with eas, then register a project to read builds and updates.",
  easHelp:
    "Enter the project UUID, owner, and slug. Ratelline reads without changing project files. Update failure history is unavailable.",
  projectId: "EAS project UUID",
  owner: "Owner",
  slug: "Slug",
  register: "Register project",
  registering: "Registering project…",
  registered: "Project added and monitoring selection saved.",
  reloadNeeded:
    "The connection was saved, but the view could not refresh. Try again.",
  retry: "Try again",
  ciGuidance: "View the CI record that ran fastlane.",
  supabaseGuidance:
    "Current function status and version differ from complete deployment-attempt history. Check run results through CI integration.",
  ciGithub: "View GitHub Actions",
  ciGitlab: "View GitLab CI",
  capabilityLabel: "Check through CI runs",
} as const;

const ko: Record<keyof typeof en, string> = {
  accountId: "계정 ID",
  token: "API 토큰",
  connect: "연결",
  connecting: "연결 중…",
  renew: "토큰 갱신",
  tokenHelp:
    "토큰은 이 기기의 앱 데이터 폴더에 저장합니다. Pages에는 Cloudflare Pages Read, Workers에는 Workers Scripts Read, Builds 조회에는 Workers CI Read 권한이 필요합니다. 실행자를 확인할 수 없는 항목은 전체 실행에서 볼 수 있습니다.",
  pagesSummary: "Cloudflare Pages API에서 배포 이력을 조회합니다.",
  workersSummary:
    "Worker 배포 기록을 조회합니다. Cloudflare Builds는 별도의 대상 유형입니다.",
  firebaseSummary:
    "연결할 때 현재 gcloud 계정을 고정해 Firebase CLI 로그인과 별개로 Hosting 릴리스와 App Hosting 롤아웃을 조회합니다.",
  easSummary:
    "eas에 로그인한 뒤 프로젝트를 등록하면 빌드와 업데이트를 조회할 수 있습니다.",
  easHelp:
    "프로젝트 UUID, 소유자, slug를 입력하세요. Ratelline은 프로젝트 파일을 변경하지 않고 조회합니다. 업데이트 실패 이력은 제공되지 않습니다.",
  projectId: "EAS 프로젝트 UUID",
  owner: "소유자",
  slug: "Slug",
  register: "프로젝트 등록",
  registering: "프로젝트 등록 중…",
  registered: "프로젝트를 추가했고 모니터링 선택을 저장했습니다.",
  reloadNeeded:
    "연결은 저장됐지만 화면을 갱신하지 못했습니다. 다시 시도하세요.",
  retry: "다시 시도",
  ciGuidance: "fastlane을 실행한 CI 기록을 확인하세요.",
  supabaseGuidance:
    "현재 함수 상태와 버전은 전체 배포 시도 이력과 다릅니다. 실행 결과는 CI 연계로 확인하세요.",
  ciGithub: "GitHub Actions 보기",
  ciGitlab: "GitLab CI 보기",
  capabilityLabel: "CI 실행으로 확인",
};

const ja: Record<keyof typeof en, string> = {
  accountId: "アカウント ID",
  token: "API トークン",
  connect: "接続",
  connecting: "接続中…",
  renew: "トークンを更新",
  tokenHelp:
    "トークンはこの端末のアプリデータフォルダーに保存されます。Pages には Cloudflare Pages Read、Workers には Workers Scripts Read、Builds の取得には Workers CI Read 権限が必要です。実行者を確認できない項目はすべての実行で確認できます。",
  pagesSummary: "Cloudflare Pages API からデプロイ履歴を取得します。",
  workersSummary:
    "Worker のデプロイ記録を取得します。Cloudflare Builds は別の対象種別です。",
  firebaseSummary:
    "接続時に現在の gcloud アカウントを固定し、Firebase CLI のログインとは別に Hosting リリースと App Hosting ロールアウトを取得します。",
  easSummary:
    "eas にログインしてからプロジェクトを登録すると、ビルドとアップデートを取得できます。",
  easHelp:
    "プロジェクト UUID、owner、slug を入力してください。Ratelline はプロジェクトファイルを変更せずに取得します。アップデートの失敗履歴は利用できません。",
  projectId: "EAS プロジェクト UUID",
  owner: "所有者",
  slug: "Slug",
  register: "プロジェクトを登録",
  registering: "プロジェクトを登録中…",
  registered: "プロジェクトを追加し、監視対象の選択を保存しました。",
  reloadNeeded:
    "接続は保存されましたが、画面を更新できませんでした。再試行してください。",
  retry: "再試行",
  ciGuidance: "fastlane を実行した CI の記録を確認してください。",
  supabaseGuidance:
    "現在の関数状態とバージョンは、完全なデプロイ試行履歴とは異なります。実行結果は CI 連携で確認してください。",
  ciGithub: "GitHub Actions を表示",
  ciGitlab: "GitLab CI を表示",
  capabilityLabel: "CI 実行で確認",
};

const zhCN: Record<keyof typeof en, string> = {
  accountId: "账户 ID",
  token: "API 令牌",
  connect: "连接",
  connecting: "正在连接…",
  renew: "更新令牌",
  tokenHelp:
    "令牌保存在此设备的应用数据文件夹中。Pages 需要 Cloudflare Pages Read，Workers 需要 Workers Scripts Read，查询 Builds 需要 Workers CI Read 权限。无法识别执行者的项目可在全部运行中查看。",
  pagesSummary: "从 Cloudflare Pages API 读取部署历史。",
  workersSummary: "读取 Worker 部署记录。Cloudflare Builds 是单独的目标类型。",
  firebaseSummary:
    "连接时固定当前 gcloud 账户，并与 Firebase CLI 登录分开读取 Hosting 发布记录和 App Hosting 发布批次。",
  easSummary: "登录 eas 后注册项目，即可读取构建和更新。",
  easHelp:
    "请输入项目 UUID、owner 和 slug。Ratelline 在不更改项目文件的情况下查询。无法提供更新失败历史。",
  projectId: "EAS 项目 UUID",
  owner: "所有者",
  slug: "Slug",
  register: "注册项目",
  registering: "正在注册项目…",
  registered: "已添加项目并保存监控选择。",
  reloadNeeded: "连接已保存，但无法刷新视图。请重试。",
  retry: "重试",
  ciGuidance: "查看运行 fastlane 的 CI 记录。",
  supabaseGuidance:
    "当前函数状态和版本与完整部署尝试历史不同。请通过 CI 集成确认运行结果。",
  ciGithub: "查看 GitHub Actions",
  ciGitlab: "查看 GitLab CI",
  capabilityLabel: "通过 CI 运行确认",
};

const catalogs: Record<Locale, Record<keyof typeof en, string>> = {
  en,
  ko,
  ja,
  "zh-CN": zhCN,
};

export function expansionCopy(locale: Locale, key: keyof typeof en): string {
  return catalogs[locale][key];
}
