# 최근 배포 중심 조회와 플랫폼 확장 계획

상태: 2026-09-30 당시의 조사·제안 기록. 이후 플랫폼 연동, 알림, Bitbucket 토큰 인증이 추가되었다. 아래 승인 대기·제외 범위는 당시 계획이며, 현재 지원 범위는 [README](../README.ko.md#지원-범위)를 따른다.

## 목표와 범위

ratelline은 로컬에서 현재 실행 중인 배포와 최근 결과를 빠르게 확인하는 조회 전용 앱이다. 상태바는 요약, 대시보드는 연결·대상·표시 설정과 필요한 추가 이력을 맡는다.

사용자 추가 지시: 특별한 설정 없이 가능한 플랫폼은 모두 지원하고 DeployBar 밖의 후보도 찾는다. ‘특별한 설정 없이’는 기존 CLI/profile 인증 재사용 또는 서비스의 표준 로그인, 자동 대상 탐색과 목록 선택을 기준으로 해석한다. 별도 OAuth 앱 등록·수동 토큰 발급·프로젝트 ID 입력을 기본 경로로 요구하지 않는다. 이번 제안은 조회 속도 개선과 아래 플랫폼 전체의 지원 가능성 검증 및 조건을 충족하는 연동 구현을 포함한다. 알림, 재배포, 취소, 로그 수집, 서버 동기화는 이번 범위에 포함하지 않는다.

## 조사 근거

DeployBar 기준 커밋: `e32bba03718bbf84ef24c266809a3f0a7b9075f5`.
아래는 소스 구조 비교다. 실제 계정으로 두 앱을 실행한 속도 측정 결과가 아니다.

| 항목 | 현재 ratelline | DeployBar에서 확인한 방식 | 적용 방향 |
| --- | --- | --- | --- |
| 최근 조회량 | GitHub: 저장소당 완료 5페이지 + 진행 상태 5종 각 2페이지, 페이지당 100건. 최소 6회, 최대 15회 배포 목록 호출 | Vercel 기본 20건, Railway·Netlify·Render·Cloudflare Pages 기본 10건 | 최신 20건을 첫 페이지로 제공하고 추가 이력은 요청 시 조회 |
| Vercel 조회 | 프로젝트마다 최대 5×100건, 받은 뒤 기간 필터 | 대상별 소량 목록 | 서버 조건과 작은 첫 페이지 활용. 진행 확인은 별도로 보완 |
| 병렬성과 표시 시점 | 계정 최대 6개 병렬이지만 계정 안의 대상은 순차, 전체 `join_all` 완료 뒤 게시 | 공통 provider 프로토콜과 registry. 계정 조회 루프 자체는 순차 | 제한된 대상 병렬 조회와 완료된 대상부터 점진 게시. DeployBar의 순차 실행을 그대로 도입하지 않음 |
| AWS | 대상마다 SDK 설정 로딩, CloudFormation 이벤트 최대 5페이지 순차 | AWS 지원은 비교 대상 아님 | profile/region별 클라이언트 재사용, 현재 상태 우선 조회, 최근 이벤트 조기 종료 |
| 화면 요약 | 진행·종료 실행 목록 | 프로젝트·서비스·환경·브랜치별 대표 배포 선택 | 진행 항목은 보존하고 종료 이력은 대상별 최신 결과 위주로 요약 |
| 확장·인증 | 3개 provider 분기와 CLI/profile 인증 | provider descriptor, 공통 상태, 계정별 Keychain 토큰 | 공통 adapter는 참고하되 ratelline은 기존 CLI 로그인 재사용 우선 |

추가 비용: GitHub/Vercel 연결 확인에서 매번 CLI 버전을 실행하며 Vercel은 수집 전에 사용자 조회를 반복한다. 현재 24시간 제한은 화면과 집계에 적용되므로 네트워크 요청량은 줄이지 않는다.

근거 코드:
- ratelline: `src-tauri/src/state.rs`, `providers/github.rs`, `providers/vercel.rs`, `providers/aws/mod.rs`, `cli.rs`.
- [DeployBar VercelProvider](https://github.com/snapre/DeployBar/blob/e32bba03718bbf84ef24c266809a3f0a7b9075f5/Sources/DeployBarCore/VercelProvider.swift)
- [DeployBar RailwayProvider](https://github.com/snapre/DeployBar/blob/e32bba03718bbf84ef24c266809a3f0a7b9075f5/Sources/DeployBarCore/RailwayProvider.swift)
- [DeployBar RefreshCoordinator](https://github.com/snapre/DeployBar/blob/e32bba03718bbf84ef24c266809a3f0a7b9075f5/Sources/DeployBarCore/RefreshCoordinator.swift)
- [DeployBar DeploymentSnapshotFocus](https://github.com/snapre/DeployBar/blob/e32bba03718bbf84ef24c266809a3f0a7b9075f5/Sources/DeployBarCore/DeploymentSnapshotFocus.swift)
- [DeployBar supported providers and trust model](https://github.com/snapre/DeployBar#readme)

## 1. 최근 조회 우선으로 변경

- 첫 조회: 대상별 최신 20건. 캐시가 있으면 즉시 표시하고 갱신 시각을 구분한다. 정상 대상의 결과는 느린 계정·대상을 기다리지 않고 게시한다.
- 진행 확인: 최신 20건 밖의 실행도 누락하지 않도록 provider가 지원하는 진행 상태 조회와 이미 추적 중인 실행 ID 확인을 분리한다. 초기 진행 확인 중에는 완료된 전체 목록처럼 표시하지 않는다. 확인 한도 도달·실패는 부분 조회로 안내한다.
- GitHub의 생성 시각과 재실행 시작 시각은 다르므로 `created` 조건만으로 오래된 실행의 최근 재실행을 제외하지 않는다.
- 추가 이력: 대시보드에서 대상과 기간을 선택하고 ‘더 보기’를 누를 때만 다음 페이지를 가져온다. 기존 24시간/7일 설정은 보존하지만 백그라운드 갱신에서 모든 페이지를 다시 읽지 않는다.
- 페이지 상태는 계정·대상·조건별로 분리한다. 계정 삭제·필터 변경·새 조회 시 커서를 무효화한다. 추가 이력 결과와 실시간 결과를 실행 ID로 합치며 오래된 응답이 최신 상태를 덮어쓰지 않게 한다.
- 대상 조회는 제한된 동시성으로 실행한다. 전체 계정 제한과 CLI 프로세스 제한을 함께 고려해 대기열 때문에 호출이 실패하지 않게 한다.
- CLI 버전은 도구 초기화 결과를 재사용한다. 인증 확인 결과 재사용 시 계정 전환·로그인·401/403에서 무효화하며 다른 사용자의 캐시가 섞이지 않게 한다.
- AWS는 profile/region별 설정·클라이언트를 재사용하고 현재 스택·ECS 상태를 우선한다. 이벤트 목록은 최근 조건 충족 시 중단한다.
- `BLOCKED` 및 시작 후 24시간 초과 진행 항목의 제외 정책을 유지한다. 429는 Retry-After 등 provider 지침을 따르고 오류 시 기존 결과에 갱신 실패를 명시한다.

## 2. 플랫폼별 연동 구조

공통 descriptor는 서비스 이름·아이콘·인증 방식·검색 가능한 대상·실행자 식별 지원 여부를 제공한다. adapter는 연결 확인, 대상 탐색, 최근 배포, 진행 확인, 추가 이력을 담당한다. 내부 반환값은 실행·조회 오류·다음 커서·조회 범위/완전성 정보다. 외부 API별 차이는 adapter 내부에 둔다.

기존 GitHub/Vercel CLI 세션과 AWS profile을 유지하고 신규 플랫폼도 설치된 공식 CLI의 인증을 재사용한다. 표준 브라우저/디바이스 로그인이 가능한 경우 로그인 후 대상을 자동 탐색한다. 별도 키를 붙여넣는 토큰 설정 화면이나 새 자격 증명 저장소는 이번 기본 경로에 도입하지 않는다. CLI가 없거나 로그인하지 않은 경우 ‘설치 필요/로그인 필요’를 명확히 표시하며 지원 완료·연결 성공으로 가장하지 않는다. CLI가 없는 환경까지 추가 설정 없이 지원하려면 번들/표준 인증 경로가 실제 가능한지 확인하고, 불가능한 항목은 한계로 명시한다. 임의 OAuth client ID를 사용하거나 사용자 자격 증명을 화면·로그·fixture에 기록하지 않는다.

실행자 정보가 없는 플랫폼은 ‘내 실행’으로 추정하지 않는다. UI에서 해당 제한과 전체 실행 보기 경로를 설명한다. 네트워크 API 주소와 외부 상세 링크는 플랫폼별 검증 규칙을 적용한다.

기존 설정에는 기본값을 적용하고 계정·대상·표시 설정을 보존한다. 새 구조로의 변환은 실제 저장 성공 후 반영한다.

## 3. 플랫폼 지원 범위와 연결 화면

4개만 우선 지원하는 제한은 사용자 답변에 따라 제거한다. 아래 목록 전체를 검증 대상으로 두고, 기존 인증 재사용 + 대상 자동 탐색 + 신뢰할 수 있는 배포 상태 조회가 성립하는 연동을 구현한다. 조회 경로는 공식 문서에서 확인했지만, 각 서비스의 실제 인증과 원본 응답은 아직 검증하지 않았다.

| 플랫폼 | 재사용할 경로 | 범위·확인할 한계 |
| --- | --- | --- |
| GitHub Deployments | 기존 `gh api` | 기존 Actions와 구분. 저장소 탐색과 배포별 상태 조회 호출 수 제한 |
| Railway | `railway list --json`, `railway api`, deployment list | 프로젝트·서비스·환경 자동 탐색. 로컬 프로젝트 link에 의존하지 않는 명시적 대상 전달 |
| Netlify | `netlify api` | 사이트와 최근 deploy. 기존 CLI 로그인 재사용 |
| Render | `render services`, `render deploys list --output json` | workspace와 서비스 선택. 현재 workspace 전역 설정을 임의 변경하지 않음 |
| Cloudflare Pages | `wrangler pages project list`, deployment list | 계정·프로젝트 자동 탐색과 기존 OAuth 재사용 |
| Cloudflare Workers | Wrangler deployment 조회 | Pages와 별개. Workers Builds의 진행 상태까지 기존 권한으로 조회 가능한지 검증; 활성 deployment와 build 상태를 혼동하지 않음 |
| Heroku | `heroku apps --json`, releases | 릴리스에는 코드 배포뿐 아니라 설정 변경도 포함. 빌드 이전 단계는 별도 지원 여부 표시 |
| Fly.io | `fly apps list`, `fly apps releases --json` | 릴리스 상태. 서비스 running 상태를 deployment running으로 표시하지 않음 |
| GitLab | `glab api`, 기존 login | 프로젝트·pipeline·deployment. 먼저 gitlab.com, 자체 호스팅은 기존 인증 host 및 URL 검증이 확인된 경우만 |
| DigitalOcean App Platform | `doctl apps`, list-deployments | 기존 doctl 인증이 있으면 추가 토큰 입력 없이 조회. 신규 doctl 로그인 자체는 PAT를 요구하므로 무설정 최초 연결로 표시하지 않음 |
| Google Cloud Run / Cloud Build | `gcloud run revisions list`, `gcloud builds list` | project/region 자동 탐색. 이미 접근 가능한 API만 읽고 API 활성화나 IAM 변경은 하지 않음 |
| Azure App Service | `az webapp log deployment list` | subscription/resource group/app 자동 탐색, 기존 Entra 로그인 재사용. 해당 배포 방식이 제공하는 상태만 표시 |
| AWS Amplify | 기존 AWS profile, list-apps/list-branches/list-jobs | 기존 CloudFormation/ECS 외의 호스팅 배포도 포함. 기존 권한만 사용 |
| Firebase Hosting / App Hosting | Firebase CLI의 사이트·채널 조회 및 rollout API 검토 | 기존 로그인으로 최신 릴리스/rollout을 안전하게 읽을 수 있는 경로 확인 후 구현. 정적 Hosting의 게시 결과와 App Hosting 빌드는 구분 |
| Supabase Edge Functions | CLI functions list / 공식 API 검토 | 함수 ACTIVE만으로 배포 진행·성공을 알 수 없음. 신뢰할 수 있는 배포 이력 경로 확인 전에는 배포 모니터 지원으로 표시하지 않음 |

현재 PATH 조사에서는 gh/vercel/aws/supabase만 발견했다. 이는 도구 존재 확인이며 로그인·실서비스 검증 결과가 아니다. 다른 CLI를 자동 설치하거나 로그인하지 않았다.

대시보드의 ‘연결 추가’는 플랫폼 검색 → 감지된 로그인 또는 표준 로그인 → 자동 탐색 목록에서 대상 선택 → 상태바 표시로 구성한다. 플랫폼별로 현재 지원 범위와 설치/로그인 상태를 설명한다. 알 수 없는 플랫폼을 이름만 추가해 완료로 처리하지 않는다.

진행 목록과 종료 요약은 구분하고, 종료 요약은 계정·안정적인 대상 ID·환경·브랜치 기준으로 묶는다. GitHub Actions는 workflow도 구분해 다른 실행을 덮지 않는다. 같은 이름의 다른 계정 프로젝트가 합쳐지지 않게 한다.

공식 API 확인 자료:
- [GitHub workflow runs](https://docs.github.com/en/rest/actions/workflow-runs): 상태·날짜 조건과 페이지 지원. 생성 날짜만으로 최근 재실행 보장을 할 수 없음.
- [Vercel list deployments](https://vercel.com/docs/rest-api/deployments/list-deployments): limit/state/since/until 지원. 현재 공식 문서가 v7을 안내하므로 기존 v6과 응답 호환성을 구현 전 확인.
- [Railway Public API](https://docs.railway.com/integrations/api)
- [Netlify API](https://docs.netlify.com/api-and-cli-guides/api-guides/get-started-with-api/)
- [Render list deploys](https://api-docs.render.com/reference/list-deploys)
- [Cloudflare Pages deployments](https://developers.cloudflare.com/api/resources/pages/subresources/projects/subresources/deployments/methods/list/)

## 완료 조건과 검증

- 요청 횟수·최초 결과 표시 시간·전체 갱신 시간을 분리 측정한다. 우선 합성 HTTP/CLI fake에서 동일한 데이터·지연으로 전후를 비교하고 실제 계정 측정은 별도로 표시한다. 측정 전 배속이나 초 단위 개선을 단정하지 않는다.
- 첫 최근 페이지는 대상당 목록 호출 1회로 제한한다(인증·별도 진행 조회는 별도 집계). 정상 대상의 첫 결과는 느린 대상의 응답/타임아웃 전 게시된다. 첫 조회에서 추가 이력 페이지를 읽지 않는다.
- 최신 페이지 밖의 진행 항목, 오래된 실행의 최근 재실행, 상태 변경, BLOCKED, 정확한 24시간 경계, 빈 다음 페이지·중복 페이지·커서 무효화, 429·부분 실패·캐시를 검증한다.
- 갱신 도중 계정 삭제·설정 변경이 이전 결과를 복원하지 않는지 검증한다. 조회와 설정 저장 중 UI 전체가 불필요하게 잠기지 않게 한다.
- 구현한 새 플랫폼 각각의 인증, 대상 탐색, 파싱, 상태 매핑, 페이지/조회 한도, 오류 표시, 시크릿 비노출 테스트를 갖춘다. 합성 fixture만 사용한다.
- TypeScript: `pnpm check`, `pnpm test`, `pnpm build`, `pnpm test:e2e`.
- Rust: `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets -- -D warnings`.
- 가능한 역할 위임 도구로 독립 리뷰를 수행한다. 현재 세션에는 역할 라우팅 가능한 subagent 도구가 없으므로 이 상태가 유지되면 직접 검토와 독립 리뷰 미수행을 구분해 보고한다.
- README에 실제 지원 플랫폼·인증 방식·최신 목록과 추가 이력의 의미·실서비스 및 OS별 검증 한계를 반영한다. 구현·검증 뒤 사용자 검수를 받는다.


추가 공식 CLI 자료:
- [Railway API CLI](https://docs.railway.com/cli/api), [deployment list](https://docs.railway.com/cli/deployment)
- [Netlify API CLI](https://cli.netlify.com/commands/api/)
- [Render CLI](https://render.com/docs/cli-reference)
- [Wrangler Pages](https://developers.cloudflare.com/workers/wrangler/commands/pages/)
- [Heroku CLI](https://devcenter.heroku.com/articles/heroku-cli-commands)
- [Fly releases](https://fly.io/docs/flyctl/apps-releases/)
- [GitLab API CLI](https://docs.gitlab.com/cli/api/), [login](https://docs.gitlab.com/cli/auth/login/)
- [DigitalOcean deployments](https://docs.digitalocean.com/reference/doctl/reference/apps/list-deployments/), [PAT-based initial login](https://docs.digitalocean.com/reference/doctl/how-to/install/)
- [Cloud Build list](https://docs.cloud.google.com/sdk/gcloud/reference/builds/list), [Cloud Run revisions](https://docs.cloud.google.com/sdk/gcloud/reference/run/revisions/list)
- [Azure deployments](https://learn.microsoft.com/en-us/cli/azure/webapp/log/deployment?view=azure-cli-latest)
- [AWS Amplify jobs](https://docs.aws.amazon.com/cli/latest/reference/amplify/list-jobs.html)
- [Firebase CLI](https://firebase.google.com/docs/cli), [Hosting releases](https://firebase.google.com/docs/reference/hosting/rest/v1beta1/sites.releases/list)
- [Supabase functions list](https://supabase.com/docs/reference/cli/supabase-functions-list)

## 구현 결과 및 검증 (2026-09-30)

사용자가 구현을 승인한 범위에서 최근 페이지·추가 이력·플랫폼 카탈로그를 통합했다. DeployBar의 작은 최근 목록과 공급자별 상태 매핑을 참고했고, 계정/대상별 결과를 즉시 게시하는 ratelline 조회 구조를 유지했다.

### 요청과 표시 시점

| 경로 | 이전 코드 | 구현된 경로 |
| --- | --- | --- |
| GitHub 첫 목록 | 대상당 목록 6–15회 완료 후 게시 | 최신 20건 목록 1회 후 게시 |
| GitHub 진행 확인 | 이력과 함께 처리 | 첫 목록 뒤 5개 상태를 병렬 조회, 상태당 최대 2페이지 |
| Vercel 첫 목록 | 최대 500건, 목록 1–5회 | 최신 20건 목록 1회 후 게시 |
| Vercel 진행 확인 | 전체 이력에 포함 | BUILDING/INITIALIZING/QUEUED 별도 조회, 상태당 최대 2페이지 |
| 추가 이력 | 매 갱신 시 선조회 | 사용자가 선택한 대상만 커서 1페이지 조회; 계정 재확인 포함 |

따라서 **모든 경우 총 요청 수가 감소하는 것은 아니다**. 특히 Vercel은 진행 항목 누락을 줄이는 별도 상태 조회 때문에 총 요청 수가 증가할 수 있다. 최적화 기준은 첫 결과 표시와 불필요한 과거 이력 조회 감소다. CLI 버전은 프로세스 내 공유 캐시, AWS 클라이언트는 profile/region별 재사용을 적용했다. 계정 3개·계정당 대상 2개까지 동시 조회한다.

합성 CLI 테스트 `state::refresh::tests::synthetic_slow_target_does_not_block_first_page_publication`은 대상 2개 중 하나의 최근 목록에 300ms 지연을 넣었다. 실행 1회 관측은 첫 페이지 게시 **160ms**, 전체 대상 완료 **487ms**, 최근 목록 **2회**, 진행 조회 **10회**였다. 전체 완료까지 기다리는 게시 장벽이면 같은 실행에서 표시가 487ms 뒤로 밀린다. 이는 실서비스 전후 벤치마크나 보장된 개선 배수가 아니다. 테스트는 시간 임계값 대신 느린 대상 완료 전에 첫 대상 결과가 게시되었는지와 추가 이력 페이지가 호출되지 않았는지를 검증한다.

### 동작 및 지원 범위

- Vercel BLOCKED는 내부 캐시에는 종료 관측으로 보존해 오래된 Running을 대체하고, 화면과 개수에서는 제외한다. 이후 진행 조회 실패가 BLOCKED를 진행 중으로 되살리지 않는 회귀 테스트를 추가했다.
- 설정 수정·계정 삭제 뒤 이전 세대의 응답을 버린다. 추가 이력은 서버에 보관한 대상/커서/세대와 일치해야 하며, 현재 CLI 계정도 다시 확인한다.
- 첫 목록은 점진적으로 게시되며 확인 중인 대상 수를 표시한다. 백그라운드 조회가 연결 설정을 잠그지 않는다.
- 실제 지원 목록과 제한은 README의 `Additional platforms` 표가 정본이다. Azure/DigitalOcean처럼 페이지 제한을 제공하지 않는 CLI는 실행 시간·출력 한도 안에서 한 번 조회한 뒤 최근 20건을 보인다. 모든 플랫폼이 같은 페이지 기능을 제공한다고 주장하지 않는다.
- EAS 자동 탐색, Cloudflare, Firebase, Supabase 직접 조회는 CLI 계약 제약 때문에 미지원으로 표시한다. EAS의 선택적 프로젝트 입력 방식은 별도 사용자 답변 전까지 구현하지 않았다. fastlane은 GitHub Actions/GitLab CI 실행을 통해 감시한다.

### 검증과 리뷰 근거

- Rust `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` 통과. `cargo test`: 109개 통과. 공급자별 합성 파싱/상태/입력, 캐시 격리, BLOCKED, 24시간 경계, 페이지 커서, CLI 버전 캐시, 점진적 게시 포함.
- TypeScript `pnpm check`, `pnpm test`(16개), `pnpm build` 통과.
- Playwright `pnpm test:e2e`: 12개 통과. 대시보드 분리, 플랫폼 검색/지원 제한, 이력 추가 조회, 조회 중 설정 저장 포함.
- 독립 auditor가 Netlify의 표시 이름 기반 계정 식별과 저장 metadata 기반 실행자 판정 두 문제를 지적했다. 인증된 `user.id`를 사용하고 backend identity만 전달하도록 수정했으며 회귀 테스트가 통과했다.
- 이후 서버 재시작으로 독립 reviewer와 auditor 재검토 세션은 `not_found`가 되었다. 재시작 뒤 Azure 통합, GitLab CI, BLOCKED 보존 변경은 메인의 직접 검토·테스트만 수행했다. 전체 독립 리뷰 완료로 처리하지 않는다.
- 실계정 API 접근, 모든 신규 CLI의 실제 설치 버전, Windows 실행, 새 패키지의 native IPC/트레이 통합은 이번 검증에 포함되지 않는다.

## 실행 앱 교체 확인 (2026-09-30 09:24 KST)

사용자가 오래된 항목과 느린 조회를 보고한 시점에 실행 중인 프로세스는 2026-09-29 17:27 빌드였다. 소스의 BLOCKED/24시간 필터와 조회 변경이 실제 실행 앱에 반영되지 않았음을 프로세스 시작 시간·실행 파일 수정 시간·구 UI에서 확인했다.

플랫폼 약어를 로컬 SVG 로고와 상태 배지로 바꾼 뒤 macOS release 앱을 다시 만들었다. 최종 Vite JS 해시가 실행 파일에 포함되었는지 검사했고, 구 프로세스를 종료해 09:23:49에 새 프로세스를 실행했다. Native UI에서 오래된 진행 항목 22개가 더 이상 표시되지 않고 진행 중 0개, 최근 목록 12개, 조회 완료 및 GitHub/Vercel 로고 표시를 확인했다. 이는 현재 선택된 계정/대상의 실행 확인이며 모든 공급자의 실계정 검증을 의미하지 않는다. 실제 조회 속도의 정밀 전후 측정은 하지 않았다.

Go가 설치되어 있지 않아 `tools:prepare`의 helper 재컴파일은 실패했다. 기존 로컬 앱의 동봉 도구가 현재 `tools-lock.json`의 source pin/버전/Vercel lock SHA-256과 일치하는 것을 확인하고 복사 후 실행 파일 해시를 대조해 재사용했다. `pnpm tauri build --bundles app`는 통과했다. TypeScript check, unit 16개, 기존 패널 브라우저 6개 및 추가 로고 표시 테스트 1개, formatter를 통과했다. 19개 provider 매핑이 17개 동봉 SVG에 연결됨을 확인했고 외부 참조·스크립트가 없음을 검사했다. 출처와 라이선스는 `THIRD_PARTY_NOTICES.md`에 기록했다.

## 대상별 진행 조회의 대기 제거 (2026-09-30)

최근 목록 전체가 끝난 뒤 진행 조회 전체를 시작하던 장벽을 제거했다. 최근 페이지 작업을 우선 큐에 넣고, 결과를 게시하면 해당 대상의 진행 조회를 큐 뒤에 추가한다. 모든 최근 요청이 시작된 뒤에는 빈 슬롯에서 진행 조회를 처리하므로 느린 최근 응답 하나가 전체 진행 조회를 막지 않는다. 진행 조회가 아직 시작하지 않은 최근 목록보다 먼저 슬롯을 점유하지도 않는다. 계정 3개, 계정당 대상 2개, CLI 계정별 프로세스 4개의 기존 상한은 유지한다. 설정 세대가 바뀌면 아직 시작하지 않은 대상과 후속 진행 조회를 건너뛰며, 이미 도착한 구세대 결과는 기존 게시 가드가 폐기한다.

같은 합성 테스트에 빠른 대상의 **진행 조회 완료**도 느린 대상의 첫 페이지보다 앞서는지 검증을 추가했다. 최종 우선순위 스케줄러의 1회 실행은 첫 페이지 185ms, 빠른 대상 진행 조회 완료 194ms, 전체 완료 501ms였다. 대상 3개와 느린 진행 조회를 사용한 추가 테스트로, 세 번째 최근 요청이 진행 조회 완료를 기다리지 않고 시작되는 것도 검증했다. 프로세스 시작 비용이 포함된 합성 결과이며 실서비스 속도 개선 배수로 해석하지 않는다. 정확한 밀리초 기준을 테스트에 고정하지 않고 게시 순서와 진행 대상 목록으로 대기 제거를 확인한다.

## 대시보드 UX·패널 닫힘 검증 (2026-09-30)

- `DESIGN.md`에 4px 간격 체계, 36px 일반/28px 트레이 컨트롤, 단일 대화상자 스크롤 영역을 정의하고 `src/styles/base.css`에 공통 토큰을 모았다. 가짜 워크스페이스 표시를 제거하고 대시보드 표시 기준, 계정 상태별 연결 안내를 추가했다.
- 패널 표시 성공 시 바깥 클릭 닫힘을 활성화한다. 이전에 큐에 들어간 blur는 현재 네이티브 포커스를 재확인해 무시한다. 닫기/토글/대시보드 진입은 같은 숨김 경로를 사용한다.
- 독립 reviewer가 발견한 진행 조회의 최근 목록 차단, 좁은 화면의 140px 이력 선택창, 미지원 탭의 숨겨진 저장 오류, AWS 안내 명령의 가로 넘침을 수정했다. 각각 게시 순서, 높이, 오류 표시, 실제 넘침 여부에 대한 회귀 검증을 추가했다.
- `pnpm check`, `pnpm test` 16개, `pnpm test:e2e` 19개, `pnpm format:check`, Rust `cargo fmt --check`, `cargo test` 113개, `cargo clippy --all-targets -- -D warnings` 통과. 마지막 안내 명령 줄바꿈 수정 후 해당 E2E도 다시 통과했다.
- 기존 패널 설정 테스트는 Vite가 붙이는 `?t=` 쿼리를 요청 가로채기 패턴이 놓쳐 테스트용 화면 전환 함수가 주입되지 않았다. 쿼리를 허용하고 주입 성공을 명시적으로 확인하도록 고쳤다. 제품 설정 저장 오류로 분류하지 않는다.
- Safari/WebKit의 가상 데이터 화면에서 선택창 정렬과 연결 본문 스크롤 시 헤더·저장 버튼 고정을 확인했다. 브라우저 모드는 실제 인증/IPC를 검증하지 않는다.

최종 `pnpm tauri build --bundles app` 통과. 패키지 실행 파일에 최종 JS `index-DegQKbJO.js`와 CSS `index-O33-sJ5m.css`가 모두 들어 있음을 확인했다. 실행 파일 수정 시각은 09:54:48 KST이며 기존 프로세스를 종료하고 09:55:33에 새 앱을 시작했다. 네이티브 대시보드의 워크스페이스 제거, 정렬된 선택창, 표시 기준 안내와 연결 가이드, 현재 선택된 GitHub/Vercel 결과를 확인했다.

외부 창 클릭은 수행했지만 자동화가 숨김 상태의 창을 조회하거나 다시 활성화할 수 있어 실제 숨김 여부의 독립적인 판정에는 한계가 있었다. 따라서 간헐적 외부 클릭의 네이티브 완전 검증으로 기록하지 않는다. 이벤트 순서·포커스 미수신·stale blur·재표시의 단위 검증과 독립 소스 리뷰를 근거로 수정했다.
