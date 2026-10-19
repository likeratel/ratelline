# Screenshots and demo

Updated 2026-10-19 with the ratelline honey badger branding. All four localized READMEs use these shared Korean UI assets, captured at 2x device scale.

| Asset | Contents |
| --- | --- |
| `menu-bar.png` | Panel view (420×540): recent runs with platform icons, execution names, branches, and actors |
| `dashboard.png` | Dashboard overview (796×576) with the synthetic-data notice |
| `connections.png` | Platform selection and account connection settings (796×576) |
| `notifications.png` | A project configured for notifications about failed runs by the current user (796×576) |
| `walkthrough.gif` | Panel → dashboard → GitHub filter → connections → notification rules; 3 seconds per view, 15-second loop, 760×550 |

## Capture source

The app renders fictional projects and accounts from [`src/demo.ts`](../../src/demo.ts) when opened in a browser. These assets contain no personal account data, credentials, or live API responses. The notification screen demonstrates settings, not macOS notification delivery. The GIF sequences actual preview screenshots; it is not a recording of a live deployment progressing.

## Refreshing

1. Start the preview with `pnpm dev --host 127.0.0.1`.
2. In another terminal, run `node scripts/capture-previews.mjs`. It uses Playwright Chromium with the Korean locale and fictional demo data only, overwrites the five assets in this directory, and writes intermediate GIF frames to the ignored `artifacts/preview-frames/`. Set `PREVIEW_URL` to use another preview address and `FFMPEG` to point at a specific `ffmpeg` binary (required for the GIF).
3. Inspect every image and a few GIF frames for clipping, layout breakage, and private data, then check the four README links. Adjust the viewport, scroll, or frame timing constants at the top of the script instead of editing assets by hand.

Screenshots show the current preview layout; native menu bar positioning, live authentication, and OS notification delivery require separate desktop verification.
