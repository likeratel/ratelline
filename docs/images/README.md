# Screenshots and demo

Updated 2026-10-02. All four localized READMEs use these shared Korean UI assets.

| Asset | Contents |
| --- | --- |
| `menu-bar.png` | Recent runs with platform icons, execution names, branches, and actors |
| `dashboard.png` | Deployment overview |
| `connections.png` | Platform selection and account connection settings |
| `notifications.png` | A project configured for notifications about failed runs by the current user |
| `walkthrough.gif` | Panel → dashboard → GitHub filter → connections → notification rules; 15-second loop |

## Capture source

The app renders fictional projects and accounts from [`src/demo.ts`](../../src/demo.ts) when opened in a browser. These assets contain no personal account data, credentials, or live API responses. The notification screen demonstrates settings, not macOS notification delivery. The GIF sequences actual preview screenshots; it is not a recording of a live deployment progressing.

## Refreshing

1. Run `pnpm dev` and open the local preview (`/?view=dashboard` or `/?view=panel`). Keep the synthetic-data notice visible on the dashboard.
2. Capture the panel and relevant settings with the Korean UI. Use fictional demo data only. The panel image can also be taken from `artifacts/ratelline-panel.png` after `pnpm test:e2e`.
3. Crop browser chrome out of screenshots. Keep a consistent canvas for GIF frames and enough time to read each view. Encode the GIF with an optimized palette.
4. Replace the existing asset names, inspect every frame for clipping or private data, and check the four README links. Keep temporary captures and encoder inputs under ignored `artifacts/`.

Screenshots show the current preview layout; native menu bar positioning, live authentication, and OS notification delivery require separate desktop verification.
