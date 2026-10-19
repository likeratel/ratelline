# ratelline

개인용 배포 모니터. Tauri 2 + React + TypeScript, Rust 서비스 연동.

- 라이선스는 MIT다. 로컬 Git 커밋은 허용되지만 push·릴리스·배포·저장소 공개 범위 변경은 사용자 요청이 해당 행동까지 허용하는지 확인한다.
- GitHub와 Vercel은 사용자가 설치한 공식 CLI를 사용하며 다중 계정을 지원한다. GitHub는 CLI 사용자별 인증을 선택하고 전역 계정을 전환하지 않는다. Vercel 추가 계정은 CLI 설정 디렉터리를 분리한다. AWS는 공유 profile을 재사용하고, CLI로 지원되지 않는 Bitbucket은 OS 보안 저장소의 API 토큰을 사용한다. 토큰·개인 계정·실제 API 응답을 코드, 테스트 fixture, 로그에 기록하지 않는다.
- 조회 전용 앱이다. 재배포·취소·인프라 변경 명령을 추가하지 않는다.
- 화면에 전달하는 외부 URL과 IPC 입력을 검증한다. 셸 문자열 조합으로 명령을 실행하지 않는다.
- 조회 실패, 부분 조회, 실행자 미확인을 빈 성공 결과로 표시하지 않는다.
- README에 실제 지원 범위와 검증 한계를 기록한다. 테스트 데이터는 가상의 조직과 사용자만 사용한다.
- TypeScript 확인: `pnpm check`, `pnpm test`, `pnpm build`, `pnpm test:e2e`.
- Rust 확인: `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets -- -D warnings` (src-tauri).
