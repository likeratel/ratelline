# ratelline

[English](README.md) · [한국어](README.ko.md) · 日本語 · [简体中文](README.zh-CN.md)

[![Verify desktop builds](https://github.com/likeratel/ratelline/actions/workflows/verify.yml/badge.svg)](https://github.com/likeratel/ratelline/actions/workflows/verify.yml)

<img src="public/ratelline.png" alt="デプロイラインを見回るラーテル" width="160" />

ratelline はデプロイラインを恐れずに見回るメニューバー・システムトレイの監視役です。名前は、ハチに刺されても引き下がらないラーテル（ミツアナグマ）に由来します。失敗したデプロイも隠さず、成功と同じようにすぐ見せます。パネルで最近の実行を確認し、ダッシュボードでアカウント接続、履歴検索、通知を設定できます。Tauri 2、React、TypeScript、Rust 製で、サービスへの問い合わせは利用者のコンピューター上で行います。表示言語はシステム設定に従い、日本語・英語・韓国語・簡体字中国語に対応します。

> AI と一緒に作りました。方向性を決め、自分のアカウントで連携を確認したのは私で、コード・テスト・ドキュメントは [Claude Code](https://claude.com/claude-code) とのセッションで書きました。[なぜ作ったか](#なぜ作ったか)を参照してください。

**ダウンロード:** 署名・公証済みの macOS ビルド（Apple Silicon・Intel）は [Releases ページ](https://github.com/likeratel/ratelline/releases/latest)にあります。[インストール](#インストール)と[クイックスタート](#クイックスタート)を参照してください。Windows インストーラーはまだ公開していません。

> **検証範囲。** GitHub Actions、Vercel、Bitbucket Cloud は実際のアカウントで動作を確認しています。その他のサービスは公式 CLI・API ドキュメントに基づき、架空データによるコマンド契約テストのみを行っており、実アカウントでの検証はしていません。[プロバイダー検証記録](docs/provider-verification.md)と [2026-10-06 の製品レビュー](docs/product-review-2026-10-06.md)を参照してください。

## なぜ作ったか

デプロイが GitHub Actions、Vercel、AWS、Bitbucket に分散していて、最後のプッシュが反映されたか知るには四つのダッシュボードを開く必要がありました。ひと目で答えてくれて、失敗した実行も成功と同じように見せ、トークンを自分のコンピューターの外に置かないメニューバーアプリが欲しかったのです。最初の名前は crowline（電線の上のカラス）で、ラーテルがマスコットになったときに ratelline になりました。デプロイ監視に求める態度がその動物にあったからです。

私がしたこと: 対応するサービスと読み取り専用の範囲を決め、GitHub Actions・Vercel・Bitbucket Cloud を実際のアカウントで検証し、ほかのサービスは対応を誇張せずに契約テストだけに留め、すべての変更を独立したレビューを経てコミットしました。実装・テスト・ドキュメントは Claude Code で書き、ラーテルの絵はプロンプトから生成してアイコンに手作業で組み込みました。

## プレビュー

メニューバーパネルから、デプロイ状況の確認、サービスでの絞り込み、アカウント接続、プロジェクトごとの通知設定まで。

![サービスで実行を絞り込み、接続と通知を設定するデモ](docs/images/walkthrough.gif)

架空のプロジェクトとアカウントを使い、アプリのブラウザプレビューを韓国語 UI で撮影しています。実際のデプロイや通知の受信を示すものではありません。

<details>
<summary>スクリーンショットを見る</summary>

**メニューバーパネル** — 実行状況、ブランチ、実行者をすばやく確認できます。

<img src="docs/images/menu-bar.png" alt="サービスのアイコンと実行者を表示するメニューバーパネル" width="420" />

**ダッシュボード** — 実行中の処理と最近の結果をまとめて確認できます。

![架空の GitHub Actions と Vercel のデプロイ状況](docs/images/dashboard.png)

**接続** — サービスとアカウントを選び、監視するプロジェクトを設定します。

![サービスの選択と GitHub アカウントの接続設定](docs/images/connections.png)

**通知** — プロジェクトごとに、誰の実行を対象にするか、どの結果を通知するかを選べます。

![プロジェクトごとの通知対象と結果の設定](docs/images/notifications.png)

</details>

## インストール

Apple Silicon または Intel の macOS 12 以降が必要です。Windows・Linux 用インストーラーは公開していません。Windows はソースからビルドできます（[開発](#開発)を参照）。Linux でのビルドは検証していません。

1. [Releases ページ](https://github.com/likeratel/ratelline/releases/latest)から最新の `ratelline_<version>_universal.dmg` をダウンロードします。各リリースにはファイルの SHA-256 が記載されているので、`shasum -a 256 <file>.dmg` で確認してください。
2. DMG を開き、**ratelline** を **Applications** にドラッグします。
3. Applications から ratelline を開きます。アプリは Developer ID 署名と公証済みなので、macOS のセキュリティ設定を回避せずに開けます。メニューバーだけで動作し、Dock アイコンはありません。
4. 監視したいサービスごとに公式 CLI をインストールしてサインインします。ratelline はサービスのツールを同梱せず、CLI の認証情報も保存しません。

| サービス | CLI | サインイン |
| --- | --- | --- |
| GitHub Actions、GitHub Deployments | `gh` | `gh auth login` |
| Vercel | `vercel` | `vercel login`、またはアプリの**ブラウザでログイン** |
| AWS | `aws`（v2） | `aws configure` または `aws sso login --profile <name>`。アプリからプロファイルの SSO サインインを開始することもできます |
| Railway / Netlify / Render / Heroku | `railway` / `netlify` / `render` / `heroku` | `<cli> login` |
| Fly.io | `fly` または `flyctl` | `fly auth login` |
| GitLab.com | `glab` | `glab auth login` |
| DigitalOcean | `doctl` | `doctl auth init` |
| Google Cloud、Firebase | `gcloud` | `gcloud auth login` |
| Azure App Service | `az` | `az login` |
| EAS / Expo | `eas` | `eas login` |
| Bitbucket Cloud、Cloudflare | なし | アプリで API トークンを入力。[対応監視](#対応監視)を参照 |

Finder やログイン時に開いたアプリはシェルの `PATH` を引き継がないため、CLI は `PATH`、一般的なインストール先（Homebrew、`~/.local/bin`、npm/pnpm/Volta のグローバル、asdf/mise の shim）、`$NVM_DIR`（既定 `~/.nvm`）配下の nvm Node バージョンの順で探します。nvm では `default` エイリアスを先に、その後にインストール済みのほかのバージョンを新しい順に探します。見つかった CLI は自身のディレクトリを `PATH` の先頭にして実行するため、nvm でインストールした Node 製 CLI は対応する `node` を使います。ほかのバージョン管理ツール（fnm、nodenv、n）では、CLI と、Node 製 CLI の場合は `node` を `~/.local/bin` などこれらのディレクトリのいずれかにリンクしてください。`default` エイリアスの照合はすべての nvm エイリアス形式には対応しておらず、解決できないエイリアスはインストール済みの最新バージョンにフォールバックします。

### アップデート

自動更新は設定していません。更新するには、メニューバーのアイコンを右クリックして**終了**を選び、新しい DMG をダウンロードして Applications のアプリを置き換えます。設定、接続、通知ルールは保持されます。crowline からアップグレードする場合は、ratelline をインストールして一度起動し、設定をコピーしてから `/Applications/crowline.app` を削除します。

### アンインストール

1. ratelline を終了し、`/Applications/ratelline.app` を削除します。
2. 設定と保存済みの Bitbucket・Cloudflare トークンも削除する場合は、`~/Library/Application Support/io.ratelline.desktop` を削除します。crowline からアップグレードした場合は、同じフォルダーの `io.crowline.desktop` も削除します。CLI のサインインは各 CLI のものであり、変更されません。

## クイックスタート

1. **パネルを開く。** メニューバーのラーテルをクリックすると最近の実行が表示されます。右クリックすると**パネルを表示**、**ダッシュボードを開く**、**更新**、**終了**を選べます。
2. **サービスを接続する。** ダッシュボードを開き、**設定 → サービス接続**でサービスを選んでアカウントを接続します。CLI サービスでは、その CLI にすでにサインインしているアカウントを選びます。
3. **プロジェクトを選ぶ。** 監視するプロジェクトやリポジトリを検索して選択します。選択はすぐに反映されます。
4. **誰の実行を表示するか選ぶ。** サービスまたはプロジェクトごとに**自分の実行**か**すべての実行**を設定します。実行者が不明な実行は**すべての実行**にだけ表示されます。
5. **通知を設定する。** **設定 → 通知**でプロジェクトごとに**オフ / 自分の実行 / すべての実行**を選び、続けて通知する結果を選びます。求められたら macOS で ratelline の通知を許可してください。
6. **メニューバーを調整する。** **設定 → メニューバー**で、表示するサービスとプロジェクト、各欄の行数、実行中にラーテルを歩かせるかどうかを選びます。

以下の各セクションで、それぞれの手順を詳しく説明します。

## 接続と対象選択

**設定 → サービス接続**でサービスとアカウントを選び、監視するプロジェクトを指定します。プロジェクト選択はすぐに反映されます。アカウントの別名は編集後に保存ボタンで確定し、完了表示を確認できます。 検索結果をまとめて選択・解除できます。検索で非表示になった項目や、別のアカウントの選択は変わりません。

表示範囲はアプリ既定値 → サービス → プロジェクトの順で継承し、プロジェクト設定を優先します。「自分の実行 / すべての実行」はダッシュボード・履歴・メニューバー共通です。通知ルールは独立しています。新規監視プロジェクトは「自分の実行・すべての結果」の通知ルールが有効になり、既存ルールと全体の通知スイッチは保持します。別名は保存・キャンセル操作と保存完了表示を備えます。一括選択は検索結果に対する「すべて選択 / すべて解除」の切り替えです。[サービス検証の根拠](docs/provider-verification.md)は契約テストと実アカウント検証を区別しています。

GitHub はインストール済みの `gh` CLI を使います。`gh auth login` 後、接続ごとに CLI ユーザーを選択してください。Vercel はインストール済み CLI を使い、追加アカウントの設定を分離します。Ratelline OAuth 登録や GitHub/Vercel トークン入力は不要です。AWS は共有プロファイルを使います。

接続 ID、対象、別名、通知ルールは維持します。API 接続から移行した GitHub は同じ CLI ユーザーを選び、Vercel は該当設定で再ログインしてください。Bitbucket はトークン認証を維持します。[認証の詳細](docs/authentication.md)。

## 対応監視

インストール済み公式 CLI を優先し、AWS SDK 共有プロファイルと Bitbucket API トークンも使用します。CLI や Node ランタイムは同梱せず、プロジェクト変更も不要です。

| プラットフォーム                  | 監視                            | 制限                                                                                        |
| --------------------------------- | ------------------------------- | ------------------------------------------------------------------------------------------- |
| GitHub Actions                    | ワークフロー実行                | インストール済み gh、接続ごとの CLI ユーザー選択。 |
| Vercel                            | プロジェクトデプロイ            | インストール済み vercel、追加アカウントの設定分離。`BLOCKED` は除外。 |
| AWS                               | CloudFormation、ECS、Amplify    | 選択したプロファイルとリージョン、実行者不明                                                |
| GitHub Deployments                | 記録と最新状態                  | Actions とは別、状態詳細は最大 20 件                                                        |
| Railway、Netlify、Render          | デプロイ履歴                    | インストール済み公式 CLI とサービス別の最近一覧制限                                         |
| Heroku、Fly.io                    | アプリリリース                  | Fly はマシン状態ではなくリリース状態                                                        |
| GitLab.com                        | デプロイ・CI パイプライン       | self-managed と実行者判定は未対応                                                           |
| Bitbucket Cloud                   | リポジトリの Pipelines          | メールアドレスと API トークン、最近の履歴・実行中の状態・実行者の判定。Data Center は未対応 |
| DigitalOcean、Google Cloud、Azure | デプロイ・Cloud Build/Run・Kudu | CLI ごとの対象/出力/ページ制限                                                              |
| Cloudflare Pages / Workers | Pages デプロイ、Worker デプロイ・Builds | アカウント ID とアプリデータフォルダーに保存した API トークン、直接の読み取り専用 HTTP。Pages・Workers・Builds の権限を分離 |
| Firebase | Hosting の公開済みリリース、App Hosting ロールアウト | インストール済み `gcloud` と選択中の有効アカウント、プロセス内だけの短期アクセストークン |
| EAS / Expo | プロジェクト単位の Build、公開済み Update | インストール済み `eas`、プロジェクト UUID・owner・slug を入力。アカウント全体の自動検出はなし |

Cloudflare トークンはアプリデータフォルダーに保存します（[アップデート、データ、制限](#アップデートデータ制限)を参照）。Pages には **Cloudflare Pages Read**、Worker デプロイには **Workers Scripts Read**、Builds には **Workers CI Read** が必要です。Firebase 接続は現在の `gcloud` アカウントに固定し、アクセストークンとプロジェクト一覧に `--account` を使います。Firebase CLI のログインは使いません。EAS は独自の読み取り専用コンテキストを作成し、選択したプロジェクトのファイルを読んだり変更したりしません。公開済み EAS Update には開始実行者がなく、失敗した公開試行も含まれません。Supabase Edge Functions と fastlane は引き続き CI 案内であり、信頼できる実行履歴は GitHub Actions、GitLab CI、または fastlane を実行した CI で確認します。[プロバイダー決定表](docs/authentication.md#provider-decision-matrix)と[モバイルプロバイダー調査](docs/mobile-provider-notes.md)を参照してください。

これらのサービス経路は合成アダプター fixture とブラウザー検証に合格しましたが、実アカウントでは検証していません。テストには実際の認証情報やサービス応答を保存しません。結果と限界は[共同製品レビュー・検証記録](docs/product-review-2026-10-06.md)に記載しています。

### Bitbucket Cloud

**設定 → サービス接続 → Bitbucket** で Atlassian のメールアドレスと [API トークン](https://support.atlassian.com/bitbucket-cloud/docs/create-an-api-token/)を入力し、監視するリポジトリを選択します。必要な権限は `read:user:bitbucket`、`read:workspace:bitbucket`、`read:repository:bitbucket`、`read:pipeline:bitbucket` です。CLI のインストールは不要です。アカウント名は Bitbucket から取得し、鉛筆ボタンで表示名を変更できます。**トークンを変更**すると、監視対象を保持したまま同じアカウントの認証情報を更新できます。

認証情報はアプリデータフォルダーの `credentials.json` に保存し、設定ファイル・ログ・インストーラーには含めません。Bitbucket 接続を削除すると、保存した認証情報も削除されます。メニューバー表示とプロジェクト別通知は既存の設定を使います。実行者を確認できない Pipeline は不明として扱い、自分の実行には含めません。2026-10-02 に接続済みの3リポジトリで最近の実行と進行状況の取得を確認しました。新規アカウントの接続手順は未検証で、自動テストには架空のデータを使用しています。

最近の Pipeline は20件ずつ読み込みます。実行中・待機中・一時停止中の状態を並列で照会し、各状態で最大50件を確認します。リポジトリ検出は1,000件、リポジトリ50ページ、ワークスペース10ページが上限です。上限到達や一部の取得失敗は画面に表示します。

## パネル、通知、状態

実行行の X でその実行だけを非表示にします。更新・再起動後も維持し、元に戻す操作で復元できます。新しい実行は表示され、非表示の実行はトレイ集計と以後の通知からも除外されます。

- **メニューバーパネル:** サービスのアイコン、状態、ブランチ、実行者と対象ごとの照会状態の要約を表示します。取得できる場合は GitHub ワークフロー、Vercel デプロイ、Bitbucket カスタムパイプライン、GitLab パイプライン・ジョブ、Cloud Build トリガーの名前も表示します。同じ名前は重複させません。外側のクリックか `Esc` で閉じます。
- **ダッシュボード:** サービス、アカウント、自分の実行・すべての実行、画面内の実行状態で絞り込み、履歴を検索できます。サービスが対応していれば過去の履歴を追加で読み込めます。実行者不明の項目はすべての実行に含まれます。状態フィルターは保存済みの表示範囲や通知ルールを変更しません。実行の詳細では取得済みの文脈、サービスが返す実行更新時刻、対象の照会状態を確認できます。
- **通知:** プロジェクトごとにオフ・自分の実行・すべての実行と、すべての結果・成功のみ・失敗のみ・カスタムを選びます。通知にはサービス、結果、実行名・タイトル、プロジェクト、ブランチ、実行者を含めます。長い内容は OS のバナーで省略される場合があります。起動直後に取得した過去の結果は通知しません。
- **メニューバー設定:** 表示するサービス・プロジェクトと各欄の行数（1～10）を選びます。実行は常に新しい順です。macOS でアイコンの位置を変えるには、Command キーを押しながらラーテルのアイコンをドラッグします。実行中の項目があると macOS メニューバーのラーテルが歩きます。**実行中はメニューバーのアイコンを動かす**でオフにでき、システムの「視差効果を減らす」がオンのときは止まったままです。Windows・Linux のトレイアイコンは動きません。
- **一般:** 言語、ログイン時の起動、更新確認、アプリのバージョンを管理します。

進行中の項目は 24 時間を超えても最後に観測した状態とともに表示し、失敗とは見なしません。Vercel の `BLOCKED` は非表示にします。完了履歴は 24 時間または 7 日間で表示できます。ratelline は参照専用で、デプロイ・再実行・キャンセル・インフラ変更は行いません。

## アップデート、データ、制限

実行中の処理があるときは約 30 秒、ないときは約 120 秒間隔で取得し、エラー後は間隔を延ばします。アプリは監視対象ごとの照会状態と鮮度を表示し、サービスが返す実行の更新時刻はローカル照会時刻とは別です。画面への復帰またはネットワーク復旧時、古い・不完全な結果はまとめた再照会を要求できます。タイムアウト、部分的な取得失敗、ページ上限は画面に表示します。遅れて取得した結果は通知されない場合があるため、完全な記録は各サービスで確認してください。

設定と非表示実行の記録は OS アプリデータ内の `io.ratelline.desktop/settings.json` に保存します。Bitbucket と Cloudflare のトークンは同じフォルダーの `io.ratelline.desktop/credentials.json`（macOS では `~/Library/Application Support/io.ratelline.desktop/credentials.json`）に保存します。このファイルは暗号化されていない JSON で、macOS と Linux では所有者だけが読み書きできる権限（0600）で作成し、Windows ではユーザープロファイルフォルダーの権限に従います。同じユーザー権限で動くプロセスは読み取ることができ、Time Machine などのフォルダーバックアップにも含まれます。ファイルが破損している場合や新しいバージョンが書いた形式の場合はエラーとして表示し、上書きしません。修復または削除してからトークンを再入力してください。以前のバージョンはこれらのトークンを OS 資格情報ストア（macOS のキーチェーン）に保存していました。起動時、保存済みトークンのない Bitbucket・Cloudflare 接続ごとにキーチェーンからトークンを一度だけ探してファイルへ移し、ファイルへの保存が成功した後にのみキーチェーン項目を削除します。移行・項目なし・拒否のいずれの結果でも検索記録をファイル（`keychainChecked`）に残すため、各接続がキーチェーンを読むのは最大一度で、移行後はアクセス許可を求められなくなります。キーチェーン項目を削除できなかった場合や、ファイル保存と項目削除の間にアプリが終了した場合は、項目がキーチェーンに残ります。失敗はアプリの標準エラー出力にのみ記録され、通常どおりアプリを開いた場合は表示されません。必要であればキーチェーンアクセスで `com.likeratel.ratelline.*` または `com.likeratel.crowline.*` の項目を削除してください。トークンを読み取れない、または保存できない場合、その接続はトークンを利用できないと表示します。Bitbucket は **トークンを変更**、Cloudflare は **トークンを更新** で再入力できます。gh から取得したユーザー別トークンは該当プロセスでのみ使用し、Ratelline は保存しません。

**crowline からのアップグレード:** ratelline の旧名は crowline です。初回起動時に新しいデータフォルダー `io.ratelline.desktop` が空であれば、`io.crowline.desktop` から設定とアカウントフォルダーをコピーします。旧フォルダーは削除せず、コピーの失敗はログに記録して起動を続けます。Bitbucket と Cloudflare のトークンは起動時にキーチェーンの `com.likeratel.ratelline.*` エントリ、なければ旧 `com.likeratel.crowline.*` エントリから読み取って `credentials.json` に移し、ファイルへの保存後に両方のエントリを削除します。macOS が旧キーチェーン項目へのアクセス許可を一度求めることがあります。接続を削除するとファイルのエントリだけを削除し、キーチェーンにはアクセスしません。詳しくは[リリース文書](docs/releases.md#upgrading-from-crowline)を参照してください。

設定の読み込みや移行後の保存に失敗した場合は、元のファイルを保護して設定・アカウント変更をロックし、復旧案内を表示します。有効なバックアップまたはフォルダーの権限を復元してから、アプリを再起動してください。読み込めた設定と監視結果は引き続き表示します。

現在のビルドでは自動更新を設定していません。公開の更新元はまだありません。配布・更新設定は[リリース手順](docs/releases.md)を参照してください。

最新の CLI 優先・複数アカウント変更は合成データで検証します。すべての CLI バージョンでの実ログインと Windows 実行は未検証です。

ローカルで確認済みなのは macOS Apple Silicon のビルド・起動、GitHub/Vercel のパネル表示、Bitbucket パイプラインの取得、架空データによる自動テストです。Windows、実際の AWS 接続、追加アカウントのブラウザログイン、最新の OS 通知バナーは未検証です。依存関係の確認事項は日付付きの[セキュリティレビュー](docs/security-review.md)に記録しています。

その他の資料は[ドキュメント一覧](docs/README.md)をご覧ください。

[2026-10-04の製品QA記録](docs/qa-2026-10-04.md)に選択操作の速度、画面確認、独立レビューとリリース前の課題をまとめています。一括選択は設定保存後すぐに操作でき、取得はバックグラウンドで続きます。選択解除はリモート取得を行いません。ローカルビルドの成功は、全サービス・OSの実動作検証を意味しません。

[2026-10-06の改善・UX検証記録](docs/qa-2026-10-06.md)に、取得結果の鮮度、実行詳細、接続フロー、メニューバー操作の修正と、自動テスト・ネイティブ検証それぞれの限界を記載しています。

## 開発

### セットアップと実行

Node.js 22.23.3 以降、pnpm 10、Rust stable が必要です。macOS は Xcode Command Line Tools、Windows は Microsoft C++ Build Tools と WebView2 などの [Tauri 前提条件](https://v2.tauri.app/start/prerequisites/)を準備してください。

```sh
git clone https://github.com/likeratel/ratelline.git
cd ratelline
pnpm install --frozen-lockfile
pnpm tauri dev
```

### ビルド

```sh
pnpm desktop:build
```

パッケージは `src-tauri/target/release/bundle/` に作成されます。macOS アプリは macOS、Windows インストーラーは Windows でビルドします。リポジトリのマルチプラットフォーム CI 設定は、リモートビルドや公開インストーラーの証拠ではありません。

### 確認

```sh
pnpm check
pnpm test
pnpm exec playwright install chromium
pnpm test:e2e
pnpm build
cd src-tauri
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

これは現在の確認コマンドであり、固定件数や現時点の成功結果は主張しません。

## ライセンス

ratelline は [MIT License](LICENSE) で提供します。現在のパッケージとロゴの告知は [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)にあります。ラーテルアイコンの来歴は[ブランドノート](docs/brand.md)にあります。

ratelline は独立したプロジェクトです。GitHub、Vercel、Amazon Web Services、Atlassian、Cloudflare、Google、Microsoft など監視対象のサービスとは提携・承認・後援の関係にありません。サービス名とロゴは各所有者の商標であり、該当サービスを識別する目的でのみ使用しています。
