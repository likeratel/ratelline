# Contributing

Thanks for your interest in ratelline. It is a small, read-only deployment monitor, so contributions are
expected to stay inside that scope.

## Ground rules

- ratelline only reads status. Do not add commands that redeploy, cancel, roll back, or change
  provider configuration.
- Never commit tokens, personal accounts, or real provider responses. Tests and fixtures use fictional
  organizations, users, projects, and identifiers only.
- Validate every external URL shown in the UI and every IPC input. Do not build shell command strings;
  pass fixed argument lists.
- A failed, partial, or unattributed query must be shown as such, never as an empty success.
- Keep the README honest about support and verification limits. A provider that passed only
  synthetic contract tests must not be described as live-verified.

## Development setup

See the [README](README.md#development-setup-and-run) for prerequisites. In short:

```sh
pnpm install --frozen-lockfile
pnpm tauri dev
```

## Checks before a pull request

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

## Commits and pull requests

- Use [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `docs:`, `chore:`, …).
- Describe the problem, the change, and the verification you actually ran. Do not claim checks you did
  not run.
- Adding a provider? Record the official CLI or API contract you implemented against and whether a
  live account was used, following [docs/provider-verification.md](docs/provider-verification.md).

## License of contributions

By submitting a pull request you agree that your contribution is licensed under the
[MIT License](LICENSE) that covers this repository.

## Reporting security issues

Please do not open a public issue for a vulnerability. Use GitHub's private vulnerability reporting
from the repository's **Security** tab instead.
