# ratelline

[English](README.md) · 한국어 · [日本語](README.ja.md) · [简体中文](README.zh-CN.md)

[![Verify desktop builds](https://github.com/likeratel/ratelline/actions/workflows/verify.yml/badge.svg)](https://github.com/likeratel/ratelline/actions/workflows/verify.yml)

<img src="public/ratelline.png" alt="배포 라인을 순찰하는 벌꿀오소리" width="160" />

ratelline은 배포 라인을 겁 없이 순찰하는 메뉴 막대·시스템 트레이 감시자입니다. 이름은 벌에 몇 번 쏘여도 물러서지 않는 벌꿀오소리(ratel)에서 따왔습니다. 실패한 배포를 숨기지 않고 성공과 똑같이 바로 보여 줍니다. 패널에서 최근 실행을 빠르게 확인하고, 대시보드에서 계정 연결·이력 검색·알림을 설정합니다. Tauri 2, React, TypeScript, Rust로 만들었으며 서비스 조회는 사용자 컴퓨터에서 실행됩니다. 언어는 시스템 설정을 따르고 한국어·영어·일본어·중국어 간체를 지원합니다.

> AI와 함께 만들었습니다. 방향을 정하고 제 계정으로 연동을 확인한 것은 저이고, 코드·테스트·문서는 [Claude Code](https://claude.com/claude-code)와의 세션에서 작성했습니다. [왜 만들었나](#왜-만들었나)를 참고하세요.

**다운로드:** 서명·공증된 macOS 빌드(Apple Silicon·Intel)는 [Releases 페이지](https://github.com/likeratel/ratelline/releases/latest)에 있습니다. [설치](#설치)와 [빠른 시작](#빠른-시작)을 참고하세요. Windows 설치 파일은 아직 공개하지 않았습니다.

> **검증 범위.** GitHub Actions, Vercel, Bitbucket Cloud는 실제 계정으로 동작을 확인했습니다. 나머지 서비스는 공식 CLI·API 문서를 기준으로 가상 데이터를 사용한 명령 계약 테스트만 거쳤고, 실계정 검증은 하지 않았습니다. [서비스 검증 기록](docs/provider-verification.md)과 [2026-10-06 제품 리뷰](docs/product-review-2026-10-06.md)를 참고하세요.

## 왜 만들었나

배포가 GitHub Actions, Vercel, AWS, Bitbucket에 흩어져 있어서 마지막 푸시가 올라갔는지 알려면 대시보드 네 개를 열어야 했습니다. 한눈에 답해 주고, 실패한 실행을 성공과 똑같이 보여 주며, 토큰을 내 컴퓨터 밖에 두지 않는 메뉴 막대 앱이 필요했습니다. 처음 이름은 crowline(전선 위의 까마귀)이었고, 벌꿀오소리가 마스코트가 되면서 ratelline이 됐습니다. 배포 감시자에게 바라는 태도가 그 동물에 있었기 때문입니다.

제가 한 일: 지원할 서비스와 조회 전용 범위를 정하고, GitHub Actions·Vercel·Bitbucket Cloud를 실제 계정으로 검증했으며, 나머지 서비스는 더 많이 된다고 주장하는 대신 계약 테스트로만 묶어 두었고, 모든 변경을 독립 리뷰를 거쳐 커밋했습니다. 구현·테스트·문서는 Claude Code로 작성했고, 라텔 그림은 프롬프트로 생성해 아이콘으로 직접 구성했습니다.

## 미리 보기

메뉴 막대 패널 → 배포 현황 → 서비스별 필터 → 계정 연결 → 프로젝트별 알림 설정 흐름을 확인해 보세요.

![서비스별 실행 조회와 계정 연결, 내 실행 알림 설정 데모](docs/images/walkthrough.gif)

가상 프로젝트와 계정으로 앱의 브라우저 미리보기를 촬영했습니다. 실제 배포나 알림 수신 장면은 아닙니다.

<details>
<summary>스크린샷 더 보기</summary>

**메뉴 막대 패널** — 실행 상태와 브랜치, 실행자를 빠르게 확인합니다.

<img src="docs/images/menu-bar.png" alt="플랫폼 아이콘과 실행자 이름이 표시된 메뉴 막대 요약 패널" width="420" />

**대시보드** — 진행 중인 실행과 최근 결과를 모아서 봅니다.

![GitHub Actions와 Vercel의 가상 배포 현황](docs/images/dashboard.png)

**서비스 연결** — 계정을 연결하고 모니터링할 프로젝트를 선택합니다.

![서비스 선택과 GitHub 계정 연결 화면](docs/images/connections.png)

**알림** — 프로젝트마다 내 실행만 받을지, 어떤 결과를 받을지 정합니다.

![프로젝트별 알림 대상과 결과 설정 화면](docs/images/notifications.png)

</details>

## 설치

Apple Silicon 또는 Intel Mac의 macOS 12 이상이 필요합니다. Windows·Linux 설치 파일은 공개하지 않습니다. Windows는 소스에서 빌드할 수 있고([개발](#개발) 참고), Linux 빌드는 검증하지 않았습니다.

1. [Releases 페이지](https://github.com/likeratel/ratelline/releases/latest)에서 최신 `ratelline_<version>_universal.dmg`를 내려받습니다. 각 릴리스에 파일의 SHA-256이 적혀 있으며 `shasum -a 256 <file>.dmg`로 확인할 수 있습니다.
2. DMG를 열고 **ratelline**을 **Applications**로 끌어다 놓습니다.
3. Applications에서 ratelline을 엽니다. 앱은 Developer ID로 서명되고 공증되어 있어 macOS 보안 설정을 우회하지 않고 열립니다. 메뉴 막대에서만 실행되며 Dock 아이콘은 없습니다.
4. 모니터링할 서비스마다 공식 CLI를 설치하고 로그인합니다. ratelline은 서비스 도구를 동봉하지 않으며 CLI 인증 정보를 저장하지 않습니다.

| 서비스 | CLI | 로그인 |
| --- | --- | --- |
| GitHub Actions, GitHub Deployments | `gh` | `gh auth login` |
| Vercel | `vercel` | `vercel login` 또는 앱의 **브라우저로 로그인** |
| AWS | `aws` (v2) | `aws configure` 또는 `aws sso login --profile <name>`. 앱에서 프로필의 SSO 로그인을 시작할 수 있음 |
| Railway / Netlify / Render / Heroku | `railway` / `netlify` / `render` / `heroku` | `<cli> login` |
| Fly.io | `fly` 또는 `flyctl` | `fly auth login` |
| GitLab.com | `glab` | `glab auth login` |
| DigitalOcean | `doctl` | `doctl auth init` |
| Google Cloud, Firebase | `gcloud` | `gcloud auth login` |
| Azure App Service | `az` | `az login` |
| EAS / Expo | `eas` | `eas login` |
| Bitbucket Cloud, Cloudflare | 없음 | 앱에 API 토큰 입력. [지원 범위](#지원-범위) 참고 |

Finder나 로그인 시 실행한 앱은 셸의 `PATH`를 상속하지 않습니다. 그래서 CLI는 `PATH`, 일반적인 설치 디렉터리(Homebrew, `~/.local/bin`, npm/pnpm/Volta 전역, asdf/mise shim), `$NVM_DIR`(기본 `~/.nvm`) 아래 nvm Node 버전에서 찾습니다. nvm은 `default` 별칭을 먼저, 그다음 설치된 다른 버전을 최신순으로 확인합니다. 찾은 CLI는 자기 디렉터리를 `PATH` 맨 앞에 두고 실행하므로 nvm으로 설치한 Node CLI는 같은 버전의 `node`를 사용합니다. 다른 버전 관리자(fnm, nodenv, n)는 CLI와, Node 기반 CLI라면 `node`를 `~/.local/bin` 같은 이 디렉터리 중 하나에 연결해야 합니다. 기본 별칭 매칭은 nvm의 모든 별칭 형식을 따라가지 않으며, 해석하지 못한 별칭은 설치된 최신 버전으로 대체합니다.

### 업데이트

자동 업데이트는 설정되어 있지 않습니다. 업데이트하려면 메뉴 막대 아이콘을 오른쪽 클릭해 **종료**를 선택하고, 새 DMG를 내려받아 Applications의 앱을 교체합니다. 설정, 연결, 알림 규칙은 유지됩니다. crowline에서 업그레이드한다면 ratelline을 설치해 한 번 실행해 설정을 복사한 뒤 `/Applications/crowline.app`을 삭제합니다.

### 삭제

1. 먼저 **설정 → 서비스 연결**에서 Bitbucket과 Cloudflare 연결을 제거합니다. 키체인에서 해당 토큰이 삭제됩니다. CLI 로그인은 각 CLI의 것이므로 건드리지 않습니다.
2. ratelline을 종료하고 `/Applications/ratelline.app`을 삭제합니다.
3. 설정도 지우려면 `~/Library/Application Support/io.ratelline.desktop`을 삭제합니다. crowline에서 업그레이드했다면 같은 폴더의 `io.crowline.desktop`도 삭제합니다.

## 빠른 시작

1. **패널 열기.** 메뉴 막대의 라텔을 클릭하면 최근 실행이 보입니다. 오른쪽 클릭하면 **패널 보기**, **대시보드 열기**, **새로고침**, **종료**가 나옵니다.
2. **서비스 연결.** 대시보드를 열고 **설정 → 서비스 연결**에서 서비스를 고른 뒤 계정을 연결합니다. CLI 서비스는 해당 CLI에 이미 로그인된 계정을 선택합니다.
3. **프로젝트 선택.** 모니터링할 프로젝트나 저장소를 검색해 선택합니다. 선택은 즉시 적용됩니다.
4. **표시할 실행 선택.** 서비스나 프로젝트마다 **내 실행** 또는 **전체 실행**을 정합니다. 실행자를 알 수 없는 실행은 **전체 실행**에만 나타납니다.
5. **알림 설정.** **설정 → 알림**에서 프로젝트마다 **끔 / 내 실행 / 전체 실행**을 고르고 알림을 받을 결과를 정합니다. 요청이 나타나면 macOS에서 ratelline의 알림을 허용합니다.
6. **메뉴 막대 조정.** **설정 → 메뉴 막대**에서 표시할 서비스와 프로젝트, 영역별 행 수, 진행 중인 실행이 있을 때 라텔이 걷는지 여부를 정합니다.

아래 섹션에서 각 단계를 자세히 설명합니다.

## 계정과 대상 연결

**설정 → 서비스 연결**에서 서비스를 고르고 계정을 연결한 뒤 모니터링할 프로젝트를 선택합니다. 설정은 바꾸는 즉시 적용됩니다. 계정 선택 옆 **별칭 편집**에서 이름을 입력하고 **별칭 저장**을 누르면 저장 완료가 표시됩니다. **취소** 또는 Esc로 미저장 변경을 버립니다. 검색 결과를 한 번에 전체 선택하거나 해제할 수 있으며, 검색에 가려진 항목과 다른 계정의 선택은 유지됩니다.

화면 표시 범위는 **앱 기본값 → 서비스 → 프로젝트** 순으로 적용하며 프로젝트 설정이 우선합니다. 서비스 연결에서 서비스·프로젝트별 **내 실행 / 전체 실행**을 선택하면 대시보드·이력·메뉴 막대에 함께 적용됩니다. 알림 규칙은 별도입니다. 새로 선택한 프로젝트는 **내 실행 · 모든 결과** 알림 규칙이 켜진 상태로 추가하며, 기존 규칙과 전체 알림 토글 값은 바꾸지 않습니다. 알림 토글을 끄면 프로젝트 규칙을 보존한 채 전송만 중지합니다. 전체 선택 버튼은 선택 상태에 따라 전체 해제로 바뀌며 현재 검색 결과에만 적용합니다. [서비스 검증 근거](docs/provider-verification.md)는 명령 계약 테스트와 실계정 조회를 구분합니다.

GitHub는 설치된 `gh` CLI를 사용합니다. `gh auth login`으로 로그인한 뒤 연결마다 CLI 사용자 계정을 선택하세요. Vercel은 설치된 `vercel` CLI를 사용하고 추가 계정의 CLI 설정을 분리합니다. Ratelline OAuth 등록이나 GitHub·Vercel 토큰 입력은 필요 없습니다. AWS는 공유 프로필을, 나머지 CLI 지원 서비스는 기존 로그인 세션을 재사용합니다.

기존 연결 ID·대상 선택·별칭·알림 규칙은 유지합니다. API 방식으로 연결했던 GitHub는 같은 CLI 사용자로 다시 연결하고, Vercel은 해당 CLI 설정으로 다시 로그인하세요. Bitbucket은 토큰 인증을 유지합니다. 자세한 내용은 [인증과 서비스 접근](docs/authentication.md)을 참고하세요.

## 지원 범위

ratelline은 설치된 공식 CLI를 우선 사용하며, AWS SDK 공유 프로필과 Bitbucket API 토큰 연결을 유지합니다. 서비스 실행 파일이나 Node 런타임을 동봉하지 않으며 배포 hook이나 프로젝트 코드 수정이 필요 없습니다.

| 플랫폼                            | 조회                         | 제한                                                                            |
| --------------------------------- | ---------------------------- | ------------------------------------------------------------------------------- |
| GitHub Actions                    | 워크플로 실행                | 설치된 gh, 연결별 CLI 사용자 선택, 최근 목록과 진행 상태 별도 조회 |
| Vercel                            | 프로젝트 배포                | 설치된 vercel, 추가 계정별 설정 분리, `BLOCKED` 제외 |
| AWS                               | CloudFormation, ECS, Amplify | 선택 profile/리전, 실행자 미확인                                                |
| GitHub Deployments                | 배포 기록·최신 상태          | Actions와 별개, 상태 상세 최대 20건                                             |
| Railway, Netlify, Render          | 배포 이력                    | 설치된 공식 CLI와 서비스별 최근 목록 제한                                       |
| Heroku, Fly.io                    | 앱 릴리스                    | Fly는 머신 상태가 아닌 릴리스 상태                                              |
| GitLab.com                        | 배포·CI 파이프라인           | self-managed, 실행자 판정 미지원                                                |
| Bitbucket Cloud                   | 저장소별 Pipelines           | 이메일·API 토큰 인증, 최근 이력·진행 상태 조회, 실행자 구분. Data Center 미지원 |
| DigitalOcean, Google Cloud, Azure | 배포·Cloud Build/Run·Kudu    | CLI별 범위·출력·페이지 제한                                                     |
| Cloudflare Pages / Workers        | Pages 배포, Worker 배포·Builds | 계정 ID와 OS 보안 저장소 API 토큰, 직접 읽기 전용 HTTP. Pages·Workers·Builds 권한을 구분 |
| Firebase                          | Hosting 게시 릴리스, App Hosting 롤아웃 | 설치된 `gcloud`와 선택된 활성 계정, 프로세스에서만 유지하는 단기 액세스 토큰 |
| EAS / Expo                        | 프로젝트별 Build, 게시된 Update | 설치된 `eas`, 프로젝트 UUID·소유자·slug 입력. 계정 전체 프로젝트 자동 탐색 없음 |

Cloudflare 토큰은 OS 자격 증명 저장소에 보관합니다. Pages에는 **Cloudflare Pages Read**, Worker 배포에는 **Workers Scripts Read**, Builds에는 **Workers CI Read** 권한이 필요합니다. Firebase 연결은 현재 `gcloud` 계정에 고정하고 액세스 토큰·프로젝트 목록에 `--account`를 사용하며 Firebase CLI 로그인은 사용하지 않습니다. EAS는 별도의 읽기 전용 문맥을 만들며 선택한 프로젝트 파일을 읽거나 변경하지 않습니다. 게시된 EAS Update에는 시작 실행자가 없고 실패한 게시 시도도 포함되지 않습니다. Supabase Edge Functions와 fastlane은 계속 CI 안내로 제공하며 신뢰할 수 있는 실행 이력은 GitHub Actions, GitLab CI 또는 fastlane을 실행한 CI에서 확인합니다. [서비스 결정표](docs/authentication.md#provider-decision-matrix)와 [모바일 플랫폼 조사](docs/mobile-provider-notes.md)를 참고하세요.

이 서비스 경로는 합성 어댑터 fixture와 브라우저 검증을 통과했으며 실계정 검증은 하지 않았습니다. 테스트에는 실제 인증 정보나 서비스 응답을 저장하지 않습니다. 결과와 한계는 [공동 제품 리뷰·검증 기록](docs/product-review-2026-10-06.md)에 정리했습니다.

### Bitbucket Cloud

**설정 → 서비스 연결 → Bitbucket**에서 Atlassian 이메일과 [API 토큰](https://support.atlassian.com/bitbucket-cloud/docs/create-an-api-token/)을 입력한 뒤 모니터링할 저장소를 선택합니다. 토큰에는 `read:user:bitbucket`, `read:workspace:bitbucket`, `read:repository:bitbucket`, `read:pipeline:bitbucket` 권한이 필요합니다. 별도 CLI 설치는 필요 없습니다. 기본 계정명은 Bitbucket에서 가져오며 연필 버튼으로 별칭을 정할 수 있습니다. **토큰 변경**으로 같은 계정의 인증 정보를 바꾸면 기존 저장소 선택을 유지합니다.

인증 정보는 OS 보안 저장소에만 보관하며 설정 파일·로그·설치 파일에는 넣지 않습니다. Bitbucket 연결을 제거하면 저장된 인증 정보도 삭제됩니다. 메뉴 막대 표시와 프로젝트별 알림은 기존 설정에서 관리합니다. 실행자를 확인할 수 없는 Pipeline은 ‘실행자 미확인’으로 남기며 ‘내 실행’에 포함하지 않습니다. 2026-10-02에 기존 연결의 저장소 3개에서 최근 실행·진행 상태 조회를 확인했습니다. 새 계정 연결 과정은 미검증이며 자동 테스트에는 가상 데이터를 사용합니다.

최근 Pipeline은 20개씩 불러옵니다. 진행 상태는 실행·대기·일시정지 상태별로 병렬 조회하며 상태당 최대 50개를 확인합니다. 저장소 탐색은 최대 1,000개 저장소, 저장소 50페이지, 워크스페이스 10페이지로 제한하며 한도 초과나 일부 실패는 화면에 표시합니다.

## 패널, 알림, 상태

실행 행의 X를 누르면 그 건만 숨깁니다. 새로고침·재실행 뒤에도 유지하며 실행 취소로 복원할 수 있습니다. 새 실행은 계속 표시하고, 숨긴 건은 트레이 집계와 이후 알림에서도 제외합니다.

- **메뉴 막대 패널:** 플랫폼 아이콘, 상태, 브랜치, 실행자와 대상별 조회 상태 요약을 표시합니다. GitHub 워크플로, Vercel 배포, Bitbucket 커스텀 파이프라인, GitLab 파이프라인·작업, Cloud Build 트리거 이름도 제공되는 경우 함께 표시합니다. 같은 이름은 한 번만 표시하며 바깥 클릭이나 `Esc`로 닫습니다.
- **대시보드:** 서비스·계정·**내 실행만 / 전체 실행**·화면 안의 실행 상태로 필터링하고 이력을 검색합니다. 서비스가 지원하면 이전 이력을 더 불러올 수 있습니다. 실행자 미확인 항목은 **전체 실행**에서 확인합니다. 상태 필터는 저장된 표시 범위나 알림 규칙을 바꾸지 않습니다. 실행 상세에서 수집된 맥락, 서비스가 전달한 실행 갱신 시각과 대상 조회 상태를 확인할 수 있습니다.
- **알림:** 프로젝트별로 **끔 / 내 실행 / 전체 실행**을 고르고 **모든 결과 / 성공만 / 실패만 / 직접 설정**을 선택합니다. 알림에는 서비스, 결과, 실행 이름·제목, 프로젝트, 브랜치, 실행자를 포함합니다. 긴 내용은 OS 배너에서 잘릴 수 있습니다. 앱을 켠 직후의 과거 결과는 알리지 않습니다.
- **메뉴 막대 설정:** 표시할 서비스·프로젝트와 영역별 1~10개 행을 선택합니다. 실행은 항상 최신순입니다. macOS에서 다른 앱 아이콘 사이의 위치는 Command 키를 누른 채 라텔 아이콘을 드래그해 바꿉니다. 진행 중인 실행이 있으면 macOS 메뉴 막대의 라텔이 걷습니다. **진행 중일 때 메뉴 막대 아이콘 움직이기**로 끌 수 있고, 시스템의 ‘동작 줄이기’가 켜져 있으면 멈춰 있습니다. Windows·Linux 트레이 아이콘은 움직이지 않습니다.
- **일반:** 언어, 로그인 시 실행, 업데이트 확인, 앱 버전을 관리합니다.

진행 항목은 24시간이 지나도 마지막 관측 상태와 함께 표시하며, 이것이 실패를 뜻하지는 않습니다. Vercel `BLOCKED`는 숨깁니다. 완료 이력은 최근 24시간 또는 7일로 조회합니다. ratelline은 조회 전용으로 배포·재실행·취소·인프라 변경을 하지 않습니다.

## 업데이트, 데이터, 한계

실행 중인 작업이 있으면 약 30초, 없으면 약 120초마다 조회하며 오류가 나면 간격을 늘립니다. 앱은 모니터링 대상별 조회 상태와 최신성을 표시하며, 서비스가 전달한 실행 갱신 시각은 로컬 조회 시각과 다릅니다. 화면 복귀나 네트워크 복구 시 오래되거나 불완전한 결과는 합쳐진 재조회 요청을 보낼 수 있습니다. 시간 초과, 일부 조회 실패, 서비스별 페이지 제한은 화면에 표시합니다. 늦게 조회된 결과는 알림에서 빠질 수 있으므로 전체 기록은 해당 서비스에서 확인하세요.

설정과 숨긴 실행 기록은 OS 앱 데이터 디렉터리의 `io.ratelline.desktop/settings.json`에 저장합니다. Bitbucket 토큰은 OS 보안 저장소에 보관합니다. GitHub 계정별 토큰은 gh를 통해 해당 조회 프로세스에서만 사용하며 Ratelline이 저장하지 않습니다. 인증 정보나 서비스 실행 파일은 설치 파일에 포함하지 않습니다.

**crowline에서 업그레이드:** ratelline의 이전 이름은 crowline입니다. 처음 실행할 때 새 데이터 폴더 `io.ratelline.desktop`이 비어 있으면 `io.crowline.desktop`의 설정과 계정 폴더를 복사합니다. 이전 폴더는 지우지 않으며, 복사 실패는 로그에 남기고 실행을 계속합니다. Bitbucket·Cloudflare 토큰은 처음 사용할 때 이전 보안 저장소 항목(`com.likeratel.crowline.*`)에서 읽어 새 이름(`com.likeratel.ratelline.*`)으로 저장합니다. macOS가 이전 키체인 항목 접근 허용을 한 번 물을 수 있습니다. 연결을 삭제하면 새 항목과 이전 항목을 함께 지웁니다. 자세한 내용은 [릴리스 문서](docs/releases.md#upgrading-from-crowline)를 참고하세요.

설정 파일을 읽거나 변환한 내용을 저장하지 못하면 원본 보존을 위해 설정·계정 변경을 잠그고 복구 안내를 표시합니다. 유효한 백업 파일이나 폴더 권한을 복구한 뒤 앱을 재시작하세요. 읽을 수 있는 설정과 모니터링 결과는 계속 표시합니다.

현재 빌드는 자동 업데이트가 설정되어 있지 않습니다. 아직 공개 업데이트 경로가 없습니다. 배포·업데이트 설정은 [릴리스 문서](docs/releases.md)를 참고하세요.

최신 CLI 우선 다중 계정 변경은 합성 데이터 테스트로 검증합니다. 모든 CLI 버전의 실제 로그인과 Windows 실행은 아직 미검증입니다.

로컬 검증 범위는 macOS Apple Silicon 빌드·실행, GitHub/Vercel 패널 데이터, Bitbucket 파이프라인 조회, 가상 데이터를 사용한 자동 테스트입니다. Windows 실행, 실제 AWS 연결, 추가 계정 브라우저 로그인, 최신 시스템 알림 배너 수신은 미검증입니다. 의존성 관련 확인 사항은 날짜가 명시된 [보안 검토](docs/security-review.md)에 기록되어 있습니다.

릴리스·플랫폼 조사·이미지 제작·과거 리뷰는 [문서 목록](docs/README.md)에서 확인할 수 있습니다.

[2026-10-04 제품 QA 기록](docs/qa-2026-10-04.md)에 전체 선택 성능, 화면 검증, 독립 리뷰와 출시 전 남은 조건을 정리했습니다. 전체 선택은 설정 저장 직후 조작할 수 있고 조회는 백그라운드에서 진행합니다. 선택 해제는 원격 조회를 실행하지 않습니다. 로컬 빌드 성공을 모든 서비스·운영체제의 실동작 검증으로 간주하지 않습니다.

[2026-10-06 제품 개선·UX 검증 기록](docs/qa-2026-10-06.md)에 조회 최신성, 실행 상세, 연결 동선과 상태바 동작 수정을 정리했습니다. 자동 검사와 네이티브 실동작 검증의 한계를 구분합니다.

## 개발

### 설정과 실행

Node.js 22.23.3 이상, pnpm 10, Rust stable이 필요합니다. macOS에는 Xcode Command Line Tools, Windows에는 Microsoft C++ Build Tools와 WebView2 등 [Tauri 필수 도구](https://v2.tauri.app/start/prerequisites/)를 준비합니다.

```sh
git clone https://github.com/likeratel/ratelline.git
cd ratelline
pnpm install --frozen-lockfile
pnpm tauri dev
```

### 빌드

```sh
pnpm desktop:build
```

패키지는 `src-tauri/target/release/bundle/`에 만듭니다. macOS 앱은 macOS에서, Windows 설치 파일은 Windows에서 빌드합니다. 저장소에는 macOS Apple Silicon, macOS Intel, Windows x64용 CI 설정이 있으며, 이는 카탈로그·설정일 뿐 원격 빌드나 공개 설치 파일이 있다는 근거가 아닙니다.

### 확인

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

현재 확인 명령이며, 고정 테스트 수나 현재 통과를 주장하지 않습니다.

## 라이선스

ratelline은 [MIT License](LICENSE)로 제공합니다. 현재 패키지와 로고 고지는 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)에 있습니다. 라텔 아이콘 출처는 [브랜드 노트](docs/brand.md)에 기록했습니다.

ratelline은 독립 프로젝트입니다. GitHub, Vercel, Amazon Web Services, Atlassian, Cloudflare, Google, Microsoft 등 모니터링 대상 서비스와 제휴·보증·후원 관계가 없습니다. 서비스 이름과 로고는 각 소유자의 상표이며 해당 서비스를 식별하는 용도로만 사용합니다.
