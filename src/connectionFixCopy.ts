import type { Locale } from "./i18n";
import type { IssueKind } from "./domain";
import type { FixAction } from "./connectionFixes";

const text = {
  title: ["고칠 것", "Needs fixing", "要対応", "需要处理"],
  intro: [
    "문제가 있는 연결입니다. 버튼 하나로 해결을 시작하세요.",
    "These connections have a problem. Start each fix with one button.",
    "問題のある接続です。ボタン 1 つで対応を始められます。",
    "这些连接有问题。点击一个按钮即可开始处理。",
  ],
  affected: [
    "영향받는 대상 {count}개",
    "{count} affected targets",
    "影響を受ける対象 {count} 件",
    "{count} 个受影响的目标",
  ],
  details: ["자세히", "Details", "詳細", "详情"],
  needsAccount: [
    "이 연결에 사용할 CLI 계정을 아직 고르지 않았습니다.",
    "No CLI account is selected for this connection yet.",
    "この接続で使う CLI アカウントがまだ選ばれていません。",
    "尚未为此连接选择 CLI 账号。",
  ],
  signedOut: [
    "CLI 로그인이 필요합니다.",
    "The CLI needs you to sign in.",
    "CLI へのログインが必要です。",
    "需要登录 CLI。",
  ],
  missingCli: [
    "필요한 CLI를 찾지 못했습니다.",
    "The required CLI was not found.",
    "必要な CLI が見つかりません。",
    "未找到所需的 CLI。",
  ],
  noAccess: [
    "선택한 계정으로는 대상에 접근할 수 없습니다.",
    "The selected account cannot access these targets.",
    "選択したアカウントでは対象にアクセスできません。",
    "所选账号无法访问这些目标。",
  ],
  rateLimited: [
    "요청 한도에 도달했습니다. 잠시 후 다시 시도하세요.",
    "The request limit was reached. Try again shortly.",
    "リクエスト上限に達しました。しばらくしてから再試行してください。",
    "已达到请求上限，请稍后重试。",
  ],
  network: [
    "서비스에 연결할 수 없습니다. 네트워크를 확인하세요.",
    "The service could not be reached. Check the network.",
    "サービスに接続できません。ネットワークを確認してください。",
    "无法连接到服务，请检查网络。",
  ],
  timedOut: [
    "응답 시간이 초과되었습니다.",
    "The service did not respond in time.",
    "応答がタイムアウトしました。",
    "服务响应超时。",
  ],
  accountMismatch: [
    "로그인한 계정이 이 연결에 저장된 계정과 다릅니다.",
    "The signed-in account differs from the one saved for this connection.",
    "ログイン中のアカウントがこの接続に保存されたものと異なります。",
    "已登录的账号与此连接保存的账号不同。",
  ],
  unsupportedVersion: [
    "설치된 CLI 버전이 지원되지 않습니다. CLI를 업데이트하세요.",
    "The installed CLI version is not supported. Update the CLI.",
    "インストール済みの CLI バージョンは未対応です。CLI を更新してください。",
    "已安装的 CLI 版本不受支持，请更新 CLI。",
  ],
  other: [
    "조회하지 못했습니다.",
    "The lookup failed.",
    "照会できませんでした。",
    "查询失败。",
  ],
  chooseAccount: [
    "접근 가능한 계정으로 바꾸기",
    "Switch to an account with access",
    "アクセスできるアカウントに切り替える",
    "切换到有访问权限的账号",
  ],
  githubSignIn: [
    "로그인 방법 보기",
    "Show how to sign in",
    "ログイン方法を表示",
    "查看登录方法",
  ],
  providerSignIn: [
    "브라우저로 로그인",
    "Sign in with browser",
    "ブラウザでログイン",
    "通过浏览器登录",
  ],
  providerSwitchAccount: [
    "다른 계정으로 로그인",
    "Sign in with another account",
    "別のアカウントでログイン",
    "使用其他账号登录",
  ],
  installCli: [
    "설치 안내 보기",
    "Show installation help",
    "インストール手順を表示",
    "查看安装说明",
  ],
  retry: ["다시 시도", "Try again", "再試行", "重试"],
  openConnection: [
    "연결 설정 열기",
    "Open connection settings",
    "接続設定を開く",
    "打开连接设置",
  ],
  recheck: ["다시 확인", "Check again", "再確認", "重新检查"],
  copy: ["명령 복사", "Copy command", "コマンドをコピー", "复制命令"],
  copied: ["복사했습니다.", "Copied.", "コピーしました。", "已复制。"],
  copyFailed: [
    "복사하지 못했습니다. 명령을 직접 선택하세요.",
    "Copy failed. Select the command manually.",
    "コピーできませんでした。コマンドを手動で選択してください。",
    "复制失败，请手动选择命令。",
  ],
  githubSignInHelp: [
    "터미널에서 아래 명령으로 GitHub에 로그인한 뒤 다시 확인하세요. ratelline은 gh의 기본 계정을 바꾸지 않습니다.",
    "Sign in to GitHub in your terminal with the command below, then check again. ratelline never changes gh's active account.",
    "端末で次のコマンドを使って GitHub にログインし、再確認してください。ratelline は gh の既定アカウントを変更しません。",
    "请在终端用下面的命令登录 GitHub，然后重新检查。ratelline 不会更改 gh 的当前账号。",
  ],
  vercelSwitchHelp: [
    "로그인한 Vercel 계정으로는 이 프로젝트에 접근할 수 없습니다. 접근 권한이 있는 계정으로 다시 로그인하세요.",
    "The signed-in Vercel account cannot access this project. Sign in again with an account that has access.",
    "ログイン中の Vercel アカウントではこのプロジェクトにアクセスできません。アクセス権のあるアカウントで再ログインしてください。",
    "已登录的 Vercel 账号无法访问此项目。请使用有访问权限的账号重新登录。",
  ],
  checkingAccess: [
    "계정별 접근을 확인하는 중…",
    "Checking access for each account…",
    "アカウントごとのアクセスを確認しています…",
    "正在检查每个账号的访问权限…",
  ],
  chooseAmong: [
    "접근 가능한 계정이 여러 개입니다. 사용할 계정을 고르세요.",
    "Several accounts have access. Choose the one to use.",
    "アクセスできるアカウントが複数あります。使うアカウントを選んでください。",
    "有多个账号可以访问，请选择要使用的账号。",
  ],
  noAccessibleAccount: [
    "로그인한 gh 계정 중 이 연결의 대상에 모두 접근할 수 있는 계정이 없습니다. 접근 권한이 있는 계정으로 로그인하세요.",
    "None of the gh accounts you are signed in to can access every target of this connection. Sign in with an account that has access.",
    "ログイン中の gh アカウントに、この接続のすべての対象へアクセスできるものがありません。アクセス権のあるアカウントでログインしてください。",
    "已登录的 gh 账号都无法访问此连接的全部目标。请使用有访问权限的账号登录。",
  ],
  inconclusive: [
    "일부 계정의 접근을 확인하지 못했습니다. 네트워크를 확인한 뒤 다시 확인하세요.",
    "Access could not be checked for some accounts. Check the network, then check again.",
    "一部のアカウントのアクセスを確認できませんでした。ネットワークを確認してから再確認してください。",
    "部分账号的访问权限未能检查。请检查网络后重新检查。",
  ],
  identityChangeConfirm: [
    "이 연결은 다른 GitHub 사용자에 묶여 있습니다. 바꾸면 이 연결의 '내 실행'과 알림 기준이 @{login}(으)로 바뀌고 실행 기록을 다시 조회합니다.",
    "This connection is bound to another GitHub user. Switching makes @{login} the basis for this connection's My runs and notifications, and its runs are queried again.",
    "この接続は別の GitHub ユーザーに固定されています。切り替えると、この接続の「自分の実行」と通知の基準が @{login} に変わり、実行履歴を再照会します。",
    "此连接绑定到另一位 GitHub 用户。切换后，此连接的“我的运行”和通知将以 @{login} 为准，并重新查询运行记录。",
  ],
  confirmChange: ["바꾸기", "Switch", "切り替える", "切换"],
  cancel: ["취소", "Cancel", "キャンセル", "取消"],
  switched: [
    "{login} 계정으로 바꿨습니다. 다시 조회합니다.",
    "Switched to {login}. Checking again.",
    "{login} に切り替えました。再照会します。",
    "已切换到 {login}，正在重新查询。",
  ],
  useAccount: [
    "이 계정 사용",
    "Use this account",
    "このアカウントを使う",
    "使用此账号",
  ],
  useNamedAccount: [
    "{login} 사용",
    "Use {login}",
    "{login} を使う",
    "使用 {login}",
  ],
  inUse: ["사용 중", "In use", "使用中", "使用中"],
  usedElsewhere: [
    "다른 연결에서 사용 중",
    "Used by another connection",
    "別の接続で使用中",
    "已被其他连接使用",
  ],
  stateSignedOut: [
    "로그인 필요",
    "Sign-in needed",
    "ログインが必要",
    "需要登录",
  ],
  activeAccount: [
    "gh 활성 계정",
    "gh active account",
    "gh のアクティブアカウント",
    "gh 当前账号",
  ],
  accessAll: [
    "대상 {count}개 모두 접근 가능",
    "Can access all {count} targets",
    "対象 {count} 件すべてにアクセス可能",
    "可访问全部 {count} 个目标",
  ],
  accessVerified: [
    "계정을 확인했습니다",
    "Account verified",
    "アカウントを確認しました",
    "账号已验证",
  ],
  accessMissing: [
    "대상 {count}개 접근 불가",
    "Cannot access {count} targets",
    "対象 {count} 件にアクセス不可",
    "无法访问 {count} 个目标",
  ],
  accessUnknown: [
    "접근 확인을 완료하지 못했습니다",
    "Access check did not finish",
    "アクセス確認を完了できませんでした",
    "未能完成访问检查",
  ],
  identityMismatch: [
    "이 연결에 저장된 GitHub 사용자와 다릅니다",
    "Differs from the GitHub user saved for this connection",
    "この接続に保存された GitHub ユーザーと異なります",
    "与此连接保存的 GitHub 用户不同",
  ],
  accountListTitle: [
    "GitHub 계정",
    "GitHub account",
    "GitHub アカウント",
    "GitHub 账号",
  ],
  accountListHelp: [
    "이 연결은 여기서 고른 gh 계정으로만 조회하며 gh의 활성 계정이 바뀌어도 영향을 받지 않습니다.",
    "This connection reads only with the gh account chosen here and is not affected when gh's active account changes.",
    "この接続はここで選んだ gh アカウントだけで照会し、gh のアクティブアカウントが変わっても影響を受けません。",
    "此连接只使用这里选择的 gh 账号查询，gh 当前账号变化时不受影响。",
  ],
  addAccountHelp: [
    "계정을 추가하려면 터미널에서 아래 명령을 실행한 뒤 새로고침하세요.",
    "To add an account, run the command below in your terminal, then refresh.",
    "アカウントを追加するには、端末で次のコマンドを実行してから更新してください。",
    "要添加账号，请在终端运行下面的命令，然后刷新。",
  ],
  changeAccount: [
    "계정 바꾸기",
    "Change account",
    "アカウントを変更",
    "更换账号",
  ],
  noAccountSelected: [
    "아직 계정을 고르지 않았습니다. 아래에서 이 연결에 사용할 계정을 고르세요.",
    "No account selected yet. Choose the account this connection uses below.",
    "まだアカウントが選ばれていません。この接続で使うアカウントを下から選んでください。",
    "尚未选择账号。请在下方选择此连接使用的账号。",
  ],
  causeTargets: [
    "{cause} · 대상 {count}개",
    "{cause} · {count} targets",
    "{cause} · 対象 {count} 件",
    "{cause} · {count} 个目标",
  ],
} satisfies Record<string, readonly [string, string, string, string]>;

export type ConnectionFixCopyKey = keyof typeof text;

const localeIndex: Record<Locale, number> = {
  ko: 0,
  en: 1,
  ja: 2,
  "zh-CN": 3,
};

export function connectionFixCopy(
  locale: Locale,
  key: ConnectionFixCopyKey,
  values: Readonly<Record<string, string | number>> = {},
): string {
  return text[key][localeIndex[locale]].replace(
    /\{(\w+)\}/g,
    (match, name: string) =>
      Object.prototype.hasOwnProperty.call(values, name)
        ? String(values[name])
        : match,
  );
}

export const causeCopy = (locale: Locale, kind: IssueKind) =>
  connectionFixCopy(locale, kind);

export const actionCopy = (locale: Locale, action: FixAction) =>
  connectionFixCopy(locale, action);

export const connectionFixCopyKeys = Object.keys(
  text,
) as ConnectionFixCopyKey[];
export const connectionFixCopyEntries = (key: ConnectionFixCopyKey) =>
  text[key] as readonly string[];
