import type { Locale } from "./i18n";

const text = {
  title: [
    "연결 진행 상태",
    "Connection progress",
    "接続の進行状況",
    "连接进度",
  ],
  tool: ["도구 준비", "Tool readiness", "ツールの準備", "工具就绪"],
  credential: ["자격 증명", "Credentials", "認証情報", "凭据"],
  account: ["계정 확인", "Account verification", "アカウント確認", "账户验证"],
  project: [
    "프로젝트 선택",
    "Project selection",
    "プロジェクトの選択",
    "项目选择",
  ],
  query: [
    "첫 원격 조회",
    "First remote query",
    "最初のリモート照会",
    "首次远程查询",
  ],
  cliReady: [
    "CLI를 확인했습니다.",
    "CLI detected.",
    "CLIを確認しました。",
    "已检测到 CLI。",
  ],
  cliMissing: [
    "필요한 CLI를 찾지 못했습니다.",
    "Required CLI not found.",
    "必要なCLIが見つかりません。",
    "未找到所需的 CLI。",
  ],
  cliUnknown: [
    "계정 확인 시 CLI 상태를 확인합니다.",
    "CLI status will be checked with the account.",
    "アカウント確認時にCLIの状態を確認します。",
    "验证账户时会检查 CLI 状态。",
  ],
  credentialSaved: [
    "자격 증명을 저장했습니다. 원격 확인은 아직 필요합니다.",
    "Credentials are saved; remote verification is still needed.",
    "認証情報を保存しました。リモート確認はまだ必要です。",
    "凭据已保存，仍需远程验证。",
  ],
  credentialNeeded: [
    "자격 증명을 추가해야 합니다.",
    "Add credentials to continue.",
    "続行するには認証情報を追加してください。",
    "请添加凭据以继续。",
  ],
  credentialRegistered: [
    "계정은 등록되었지만 자격 증명 확인이 필요합니다.",
    "The account is registered, but its credentials need verification.",
    "アカウントは登録されていますが、認証情報の確認が必要です。",
    "账户已注册，但需要验证凭据。",
  ],
  credentialVerified: [
    "자격 증명을 원격으로 확인했습니다.",
    "Credentials are verified remotely.",
    "認証情報をリモートで確認しました。",
    "已远程验证凭据。",
  ],
  profileReady: [
    "공유 AWS 프로필을 선택했습니다.",
    "A shared AWS profile is selected.",
    "共有AWSプロファイルを選択しました。",
    "已选择共享 AWS 配置文件。",
  ],
  profileNeeded: [
    "확인할 AWS 프로필을 선택해 주세요.",
    "Choose an AWS profile to verify.",
    "確認するAWSプロファイルを選択してください。",
    "请选择要验证的 AWS 配置文件。",
  ],
  accountVerified: [
    "현재 계정을 원격으로 확인했습니다.",
    "The current account is verified remotely.",
    "現在のアカウントをリモートで確認しました。",
    "已远程验证当前账户。",
  ],
  accountNeeded: [
    "계정을 선택하거나 추가해 주세요.",
    "Choose or add an account.",
    "アカウントを選択または追加してください。",
    "请选择或添加账户。",
  ],
  accountCheck: [
    "원격 계정 확인이 필요합니다.",
    "Remote account verification is needed.",
    "リモートでのアカウント確認が必要です。",
    "需要远程验证账户。",
  ],
  accountFailed: [
    "계정을 확인하지 못했습니다.",
    "Could not verify the account.",
    "アカウントを確認できませんでした。",
    "无法验证账户。",
  ],
  projectsSaved: [
    "{count}개 프로젝트가 저장되었습니다.",
    "{count} project(s) saved.",
    "{count}件のプロジェクトを保存しました。",
    "已保存 {count} 个项目。",
  ],
  projectsNeeded: [
    "먼저 모니터링할 프로젝트를 저장해 주세요.",
    "Save a project to monitor first.",
    "最初に監視するプロジェクトを保存してください。",
    "请先保存要监控的项目。",
  ],
  projectsSaving: [
    "선택한 프로젝트를 저장하고 있습니다.",
    "Saving the selected project.",
    "選択したプロジェクトを保存しています。",
    "正在保存所选项目。",
  ],
  queryAwaitingProjectSave: [
    "프로젝트 선택이 저장된 뒤 원격 조회를 시작합니다.",
    "The remote query starts after the project selection is saved.",
    "プロジェクトの選択を保存した後にリモート照会を開始します。",
    "保存项目选择后将开始远程查询。",
  ],
  queryComplete: [
    "원격 조회를 완료했습니다.",
    "Remote query completed.",
    "リモート照会が完了しました。",
    "远程查询已完成。",
  ],
  queryChecking: [
    "원격 조회 중입니다.",
    "Remote query is in progress.",
    "リモート照会中です。",
    "正在进行远程查询。",
  ],
  queryUnknown: [
    "프로젝트는 저장되었지만 원격 조회는 아직 완료되지 않았습니다.",
    "Project settings are saved, but the remote query has not completed yet.",
    "プロジェクトは保存されましたが、リモート照会はまだ完了していません。",
    "项目设置已保存，但远程查询尚未完成。",
  ],
  queryPartial: [
    "일부 프로젝트의 원격 조회가 완료되지 않았습니다.",
    "Some project queries did not complete.",
    "一部のプロジェクトのリモート照会が完了していません。",
    "部分项目查询尚未完成。",
  ],
  queryFailed: [
    "원격 조회에 실패했습니다.",
    "Remote query failed.",
    "リモート照会に失敗しました。",
    "远程查询失败。",
  ],
  queryStale: [
    "마지막 원격 조회 결과가 오래되었습니다.",
    "The last remote query result is stale.",
    "最後のリモート照会結果が古くなっています。",
    "上次远程查询结果已过期。",
  ],
  lastSuccess: [
    "마지막 성공 {time}",
    "Last success {time}",
    "最終成功 {time}",
    "上次成功 {time}",
  ],
  manageAccount: [
    "계정 관리",
    "Manage account",
    "アカウントを管理",
    "管理账户",
  ],
  checkAccount: ["계정 확인", "Check account", "アカウントを確認", "验证账户"],
  chooseGithubAccount: [
    "계정 선택",
    "Choose account",
    "アカウントを選択",
    "选择账号",
  ],
  githubAccountNeeded: [
    "이 연결이 사용할 GitHub 계정을 골라야 합니다.",
    "Choose the GitHub account this connection uses.",
    "この接続で使う GitHub アカウントを選んでください。",
    "请选择此连接使用的 GitHub 账号。",
  ],
  selectProjects: [
    "프로젝트 선택",
    "Select projects",
    "プロジェクトを選択",
    "选择项目",
  ],
  retryQuery: ["다시 조회", "Query again", "もう一度照会", "再次查询"],
  complete: ["완료", "Complete", "完了", "已完成"],
  waiting: ["대기", "Pending", "保留", "待处理"],
  warning: ["확인 필요", "Needs action", "要確認", "需要处理"],
  checking: ["확인 중", "Checking", "確認中", "正在检查"],
  detail: [
    "{label} 세부 정보",
    "{label} details",
    "{label}の詳細",
    "{label}详细信息",
  ],
} satisfies Record<string, readonly [string, string, string, string]>;

export type ConnectionProgressCopyKey = keyof typeof text;

const localeIndex: Record<Locale, number> = {
  ko: 0,
  en: 1,
  ja: 2,
  "zh-CN": 3,
};

export function connectionProgressCopy(
  locale: Locale,
  key: ConnectionProgressCopyKey,
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
