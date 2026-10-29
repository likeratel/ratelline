import type { Locale } from "./i18n";
const messages = {
  refresh: [
    "CLI 계정 새로고침",
    "Refresh CLI accounts",
    "CLI アカウントを更新",
    "刷新 CLI 账号",
  ],
  preview: [
    "브라우저 미리보기에서는 가상 CLI 계정을 사용합니다.",
    "Browser preview uses fictional CLI accounts.",
    "ブラウザプレビューでは架空の CLI アカウントを使用します。",
    "浏览器预览使用虚构的 CLI 账号。",
  ],
  noAccounts: [
    "로그인한 CLI 계정이 없습니다. gh auth login으로 먼저 로그인하세요.",
    "No signed-in CLI accounts. Run gh auth login first.",
    "ログイン済みの CLI アカウントがありません。gh auth login を実行してください。",
    "没有已登录的 CLI 账号，请先运行 gh auth login。",
  ],
} as const;
export function cliCopy(locale: Locale, key: keyof typeof messages) {
  return messages[key][{ ko: 0, en: 1, ja: 2, "zh-CN": 3 }[locale]];
}
