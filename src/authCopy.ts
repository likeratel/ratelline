import type { Locale } from "./i18n";

const text = {
  installation: [
    "CLI 설치·업데이트 안내",
    "CLI installation and updates",
    "CLIのインストール・更新",
    "CLI 安装与更新",
  ],
  localHelp: [
    "공식 CLI를 설치하고 아래 명령으로 로그인한 뒤 앱을 다시 시작하세요. 버전이나 출력 형식이 호환되지 않으면 업데이트 안내를 표시합니다. 앱은 CLI를 설치하거나 업데이트하지 않습니다.",
    "Install the official CLI, sign in with the command below, then restart Ratelline. Incompatible commands or output show an update error. Ratelline does not install or update the CLI.",
    "公式CLIをインストールし、以下のコマンドでログイン後、アプリを再起動してください。互換性のないコマンドや出力には更新エラーが表示されます。アプリはCLIをインストール・更新しません。",
    "安装官方 CLI，使用以下命令登录后重启应用。命令或输出不兼容时会提示更新。应用不会安装或更新 CLI。",
  ],
  version: [
    "감지한 CLI 버전",
    "Detected CLI version",
    "検出したCLIバージョン",
    "检测到的 CLI 版本",
  ],
} satisfies Record<string, readonly [string, string, string, string]>;

export function authCopy(locale: Locale, key: keyof typeof text): string {
  return text[key][({ ko: 0, en: 1, ja: 2, "zh-CN": 3 } as const)[locale]];
}
