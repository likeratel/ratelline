import type { TranslationKey } from "./ko";

export const ja = {
  "bitbucket.reloadFailed":
    "アカウントは接続済みですが、画面を更新できませんでした。再読み込みしてください。",
  "platform.bitbucket": "AtlassianのメールアドレスとAPIトークンで接続します。",
  "bitbucket.email": "Atlassianのメールアドレス",
  "bitbucket.token": "APIトークン",
  "bitbucket.connect": "アカウントを接続",
  "bitbucket.renew": "トークンを変更",
  "bitbucket.createToken": "APIトークンを作成",
  "bitbucket.help":
    "Bitbucket用のAPIトークンを作成し、以下の読み取り権限を選択してください。",
  "bitbucket.connecting": "接続中…",
  "bitbucket.newAccount": "Bitbucketアカウントを接続",
  "bitbucket.invalid": "メールアドレスとAPIトークンを確認してください。",

  "common.retry": "再試行",
  "common.refresh": "更新",
  "common.cancel": "キャンセル",
  "common.updatedSuffix": " 更新",
  "common.period24h": "過去24時間",
  "common.period7d": "過去7日間",
  "common.targetCount": "{count}件の対象",
  "common.selectedCount": "{count}件選択中",
  "common.totalTargets": "対象は合計{count}件",
  "common.moreCount": "残り{count}件を表示",
  "common.unknownActor": "実行者不明",
  "boot.checking": "接続状態を確認しています…",
  "settingsRecovery.title": "設定を読み込めませんでした",
  "settingsRecovery.locked": "保存はロック中",
  "settingsRecovery.body":
    "元の設定は保持されています。設定ファイルとフォルダーの読み書き権限を復旧するか、バックアップを復元してからアプリを再起動してください。",
  "settingsRecovery.details": "エラー詳細",
  "nav.monitoring": "モニタリング",
  "nav.serviceFilter": "サービスフィルター",
  "nav.allServices": "すべてのサービス",
  "nav.panelPreferences": "メニューバー表示設定",
  "nav.connections": "接続設定",
  "nav.generalSettings": "一般設定",
  "nav.tagline": "実行モニター",
  "dashboard.preview":
    "プレビュー · サンプルデータです。実際の接続はデスクトップアプリで利用できます。",
  "dashboard.connectedProjects": "接続済みプロジェクト",
  "dashboard.title": "実行状況",
  "dashboard.subtitle": "選択したアカウントの最近の実行状況を確認できます。",
  "dashboard.active": "実行中",
  "dashboard.runs": "件",
  "dashboard.completed": "最近完了",
  "dashboard.elapsedPeriod": "{period}",
  "dashboard.connections": "サービス接続",
  "dashboard.monitoringTargets": "{count}件をモニタリング中",
  "dashboard.actorScope": "実行者の範囲",
  "dashboard.mineOnly": "自分の実行",
  "dashboard.allRuns": "すべての実行",
  "dashboard.searchPlaceholder": "実行名、プロジェクト、ブランチを検索",
  "dashboard.search": "実行を検索",
  "dashboard.clearSearch": "検索をクリア",
  "dashboard.period": "表示期間",
  "dashboard.mineDescription": "自分のpush · PR · 手動実行 · 再実行",
  "dashboard.allDescription": "自分のアカウントで参照できるすべての実行",
  "dashboard.unknownActors": "実行者不明の{count}件を「すべての実行」で確認",
  "dashboard.guideTitle": "表示基準について",
  "dashboard.guideHistory":
    "実行中の項目は最後に確認した状態を表示し、24時間を超えても完了が確認されるまで残ります。最近完了には選択期間内の結果が表示されます。それ以前の結果は対象ごとに「以前の実行をさらに表示」から読み込めます。",
  "dashboard.guideSettings":
    "「メニューバー表示設定」ではトレイに表示する項目を変更し、「接続設定」ではダッシュボードで監視するアカウントと対象を変更します。",
  "dashboard.pending":
    "新しい結果から表示しています。{count}件の対象について実行状況を確認中です。",
  "dashboard.dismissError": "エラー通知を閉じる",
  "dashboard.onboardingTitle": "最初の接続を追加しましょう",
  "dashboard.ready": "準備ができました",
  "dashboard.onboardingBody":
    "この端末のサービスAPIとローカルCLIを使い、実行中と最近の実行をまとめて表示します。",
  "dashboard.connectService": "サービスを接続",
  "dashboard.onboardingSteps":
    "アカウントを接続してプロジェクトを選ぶと、すぐに監視が始まります。",
  "dashboard.activeEmpty": "現在の条件に一致する実行中の項目はありません。",
  "dashboard.completedEmpty": "現在の条件に一致する最近の実行はありません。",
  "dashboard.needsAttention": "確認が必要",
  "dashboard.refreshing": "最新状況を確認しています…",
  "dashboard.neverUpdated": "まだ確認されていません",
  "dashboard.lastChecked": "{time}に確認",
  "dashboard.refreshCadence": "実行中は30秒ごと · 待機中は2分ごとに更新",
  "account.filter": "アカウントフィルター",
  "account.all": "すべてのアカウント",
  "activity.updated": "{time}に更新",
  "history.label": "以前の実行を表示",
  "history.lead": "新しい結果から表示します",
  "history.help":
    "以前の履歴が必要な場合は対象を選択して追加で読み込んでください。表示期間と実行者フィルターはそのまま適用されます。",
  "history.target": "履歴を表示する対象",
  "history.selectTarget": "対象を選択",
  "history.loading": "履歴を読み込み中…",
  "history.loadMore": "以前の実行をさらに表示",
  "history.end":
    "この対象には追加の履歴がないか、プラットフォームが提供する履歴の上限に達しています。",
  "accountPicker.defaultAccount": "{provider}アカウント",
  "accountPicker.cliSession": "{provider} CLIセッション",
  "accountPicker.unavailable": "現在接続できません",
  "accountPicker.immediate": "アカウントの追加・削除はすぐに反映されます。",
  "accountPicker.choose": "接続するアカウント",
  "accountPicker.add": "アカウントを追加",
  "accountPicker.removeAria": "このアカウント接続を削除",
  "accountPicker.removeTitle": "「{label}」の接続を削除しますか？",
  "accountPicker.removeBody":
    "この接続と保存済みの監視対象{count}件はすぐに削除されます。保存していない対象の選択内容は破棄されます。",
  "accountPicker.remove": "接続を削除",
  "accountPicker.localSession": "このデバイスのCLIセッション",
  "accountPicker.connectCli": "CLIセッションを接続",
  "accountPicker.awsProfile": "AWSプロファイルを接続",
  "accountPicker.separateAccount": "別のアカウントを接続",
  "accountPicker.name": "アカウント名",
  "accountPicker.namePlaceholder": "個人 / 会社",
  "accountPicker.profile": "AWSプロファイル",
  "accountPicker.loadProfiles": "ローカルプロファイルを読み込む",
  "accountPicker.addProfile": "プロファイル接続を追加",
  "accountPicker.addAccount": "アカウント接続を追加",
  "accountPicker.loginHelp":
    "以下の認証方法でアカウントを接続します。トークンはこの端末にのみ保存されます。",
  "tray.summary": "実行概要",
  "tray.close": "パネルを閉じる",
  "tray.preview": "プレビュー · サンプルデータ",
  "tray.scope": "{scope} · {visible}/{total}件",
  "tray.mine": "自分の実行",
  "tray.all": "すべての実行",
  "tray.checkScope": "ダッシュボードで確認",
  "tray.moreInDashboard": "残り{count}件をダッシュボードで表示",
  "tray.pending": "新しい結果から表示中 · {count}件の対象を確認中",
  "tray.issues": "確認事項{count}件 · 一部の情報が遅れる場合があります",
  "tray.checkConnection": "接続状態を確認してください。",
  "tray.connecting": "接続を確認しています…",
  "tray.onboardingTitle": "監視するプロジェクトを選択してください。",
  "tray.onboardingBody":
    "GitHub · Vercel · AWS\nこのデバイスの既存ログインを使用します。",
  "tray.connectDashboard": "ダッシュボードで接続",
  "tray.noVisibleTargets":
    "すべての対象が非表示になっています。ダッシュボードのメニューバー表示設定を確認してください。",
  "tray.noResults":
    "実行者・対象・期間の条件に一致する結果がありません。ダッシュボードで表示範囲と確認事項を確認してください。",
  "tray.activeEmpty": "現在の表示範囲には実行中の項目がありません。",
  "tray.completedEmpty":
    "選択した期間と表示範囲に一致する最近の完了項目はありません。",
  "tray.sectionsHidden": "表示するセクションはダッシュボードで設定できます。",
  "tray.unresolvedEmpty": "確認が必要な実行はありません。",
  "tray.checking": "確認中…",
  "tray.checked": "{time}に確認",
  "tray.waiting": "接続待ち",
  "tray.openDashboard": "ダッシュボードを開く",
  "connections.addedDiscoveryFailed":
    "接続は追加されましたが、対象を取得できませんでした。{error}",
  "connections.saveFailed":
    "設定を保存できませんでした。もう一度お試しください。",
  "connections.title": "サービス接続",
  "connections.subtitle": "このデバイスの既存認証を読み取り専用で使用します。",
  "connections.close": "設定を閉じる",
  "connections.content": "サービス接続設定",
  "connections.searchPlatform": "プラットフォームを検索",
  "connections.services": "接続サービス",
  "connections.noPlatforms": "一致するプラットフォームがありません。",
  "connections.providerSettings": "{provider}の接続設定",
  "connections.guideTitle": "接続手順",
  "connections.prepare": "接続の準備",
  "connections.reuseSelected":
    "選択中の「{label}」接続の既存認証を再利用します。再ログインは不要です。",
  "connections.awsSelected":
    "「{profile}」プロファイルとリージョンを確認してから、アプリの「SSOログイン」を使用してください。ターミナルからログインする場合は{command}を実行します。",
  "connections.awsAdd": "既存のAWSプロファイルを読み込むか追加してください。",
  "connections.appLogin":
    "「ブラウザでログイン」を使い、この接続だけを認証してください。既存の{provider} CLI既定セッションは変更されません。",
  "connections.reuseCli":
    "選択した既存のCLI接続を使用します。セッションがない場合や期限切れの場合のみ、ターミナルで{command}を実行してください。",
  "connections.completeCli":
    "ターミナルで{command}を完了してから「CLIセッションを接続」を選択してください。",
  "connections.verify": "接続を確認",
  "connections.verifyHelp": "選択した接続から参照できる対象を読み込みます。",
  "connections.selectSave": "対象を選択して保存",
  "connections.selectSaveHelp":
    "監視する対象だけを選択して設定を保存してください。",
  "connections.readOnly":
    "Ratellineは実行状態の参照のみを行い、リモートサービスを変更することはありません。",
  "connections.checkConnection": "接続を確認してください",
  "connections.tokenNotNeeded":
    "既存の認証を使用します。トークンの入力は不要です。",
  "connections.loginHelp":
    "下のボタンからログインし、参照できる対象を確認してください。",
  "connections.profile": "プロファイル",
  "connections.region": "リージョン",
  "connections.verifyTargets": "接続と対象を確認",
  "connections.ssoLogin": "SSOログイン",
  "connections.browserLogin": "ブラウザでログイン",
  "connections.openAuth": "認証ページを開く",
  "connections.loadTargets": "対象を読み込む",
  "connections.targets": "監視対象",
  "connections.searchTargets": "対象を検索",
  "connections.searchByName": "名前で検索",
  "connections.selectAllVisible": "検索結果をすべて選択",
  "connections.deselectAllVisible": "検索結果をすべて解除",
  "connections.noTargets":
    "参照できる対象がありません。アカウント、権限、リージョンを確認してください。",
  "connections.verifyFirst":
    "接続を確認すると、選択できる対象がここに表示されます。",
  "connections.actorHelp":
    "実行者情報を確認できない場合があります。「自分の実行」に表示されない項目は「すべての実行」で確認してください。",
  "connections.saveTargets": "監視対象を保存",
  "status.queued": "待機中",
  "status.running": "実行中",
  "status.waiting": "承認待ち",
  "status.success": "成功",
  "status.failed": "失敗",
  "status.canceled": "キャンセル済み",
  "status.skipped": "スキップ",
  "status.unknown": "状態不明",
  "platform.github":
    "インストール済み GitHub CLI のログインアカウントを選択します。",
  "platform.vercel":
    "インストール済み Vercel CLI を使い、追加アカウントは個別にログインします。",
  "platform.aws": "選択したAWSプロファイルとリージョンを使用します。",
  "platform.cliSession":
    "このデバイスでログイン済みの{provider} CLIセッションを使用します。",
  "platform.existingSession": "このデバイスの既存CLIセッションを使用します。",
  "platform.cloudflarePagesSummary":
    "Wrangler CLIでは、Pagesデプロイの日時と状態を安定して取得できません。",
  "platform.cloudflarePagesUnsupported":
    "Cloudflare Pagesのデプロイ結果はGitHub ActionsまたはGitLab CIで確認してください。",
  "platform.cloudflareWorkersSummary":
    "Wrangler CLIでは、アカウント内のすべてのWorkersデプロイ対象を自動検出できません。",
  "platform.cloudflareWorkersUnsupported":
    "Cloudflare Workersのデプロイ結果はGitHub ActionsまたはGitLab CIで確認してください。",
  "platform.easSummary":
    "EAS CLIでは、アカウント内のすべてのExpoプロジェクトを自動取得できません。",
  "platform.easUnsupported":
    "EAS Build・Updateを直接参照するには追加のプロジェクト接続情報が必要です。CIで実行したビルドはGitHub ActionsまたはGitLab CIで確認できます。",
  "platform.firebaseSummary":
    "Firebase CLIセッションだけでは、信頼できるデプロイ履歴を取得できません。",
  "platform.firebaseUnsupported":
    "Firebaseのデプロイ結果はGitHub ActionsまたはGitLab CIで確認してください。",
  "platform.supabaseSummary":
    "Supabase CLIでは、デプロイ履歴を安定して取得できません。",
  "platform.supabaseUnsupported":
    "Supabase Edge Functionsのデプロイ状態はGitHub ActionsまたはGitLab CIで確認してください。",
  "platform.fastlaneSummary":
    "fastlaneはクラウドデプロイサービスではありません。",
  "platform.fastlaneUnsupported":
    "fastlaneの実行結果はGitHub ActionsまたはGitLab CIで確認してください。",
} satisfies Record<TranslationKey, string>;
