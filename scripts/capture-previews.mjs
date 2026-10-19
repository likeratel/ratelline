// Captures the README preview assets from the browser preview's synthetic data.
// Usage: start `pnpm dev --host 127.0.0.1`, then run `node scripts/capture-previews.mjs`.
import { execFile } from "node:child_process";
import { mkdir, rm } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { chromium } from "@playwright/test";

const run = promisify(execFile);
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const baseUrl = process.env.PREVIEW_URL ?? "http://127.0.0.1:1420";
const ffmpeg = process.env.FFMPEG ?? "ffmpeg";
const imagesDirectory = join(root, "docs", "images");
const framesDirectory = join(root, "artifacts", "preview-frames");

const SCALE = 2;
// Taller than the 560px compact-height breakpoint so the preview notice is not clipped.
const WINDOW = Object.freeze({ width: 796, height: 576 });
const PANEL = Object.freeze({ width: 420, height: 540 });
const GIF = Object.freeze({ width: 760, fps: 10, background: "0x111214" });
const FRAME_SECONDS = Object.freeze([3, 3, 3, 3, 3]);

async function assertServer() {
  try {
    const response = await fetch(baseUrl);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
  } catch (error) {
    throw new Error(
      `Preview server is not reachable at ${baseUrl}. Run \`pnpm dev --host 127.0.0.1\` first. (${error.message})`,
    );
  }
}

async function openPage(browser, viewport, path) {
  const context = await browser.newContext({
    viewport,
    deviceScaleFactor: SCALE,
    locale: "ko-KR",
    colorScheme: "dark",
    reducedMotion: "reduce",
  });
  const page = await context.newPage();
  await page.goto(new URL(path, baseUrl).toString());
  await page.waitForLoadState("networkidle");
  await page.evaluate(() => document.fonts.ready);
  return page;
}

async function waitForImages(page) {
  await page.waitForFunction(() =>
    [...document.images].every(
      (image) => image.complete && image.naturalWidth > 0,
    ),
  );
}

async function shoot(page, path) {
  await page.mouse.move(0, 0);
  await waitForImages(page);
  await page.screenshot({ path, animations: "disabled", caret: "hide" });
}

async function openSettings(page, tab) {
  await page.getByRole("button", { name: "설정", exact: true }).click();
  await page.getByRole("tab", { name: tab, exact: true }).click();
  return page.getByRole("tabpanel", { name: tab, exact: true });
}

async function capturePanel(browser, path) {
  const page = await openPage(browser, PANEL, "/?view=panel");
  await page.locator(".panel-run").first().waitFor();
  await shoot(page, path);
  await page.context().close();
}

async function captureDashboard(browser, paths) {
  const page = await openPage(browser, WINDOW, "/?view=dashboard");
  await page.locator(".activity-row").first().waitFor();
  await shoot(page, paths.dashboard);

  await page.getByRole("button", { name: /^GitHub Actions/ }).click();
  await page.locator(".activity-row").first().waitFor();
  await shoot(page, paths.github);

  await page.getByRole("button", { name: /^전체 서비스/ }).click();
  const connections = await openSettings(page, "서비스 연결");
  await connections.locator(".connections-settings").waitFor();
  await shoot(page, paths.connections);

  const notifications = await openSettings(page, "알림");
  const row = (suffix) =>
    notifications.getByLabel(new RegExp(`^orbit/web · .* · ${suffix}$`));
  await row("알림 대상").selectOption("mine");
  await row("알림 결과").selectOption("failed");
  await notifications
    .locator(".notification-row")
    .last()
    .evaluate((element) => {
      element.parentElement.scrollIntoView({ block: "end" });
      window.scrollBy(0, 16);
    });
  await shoot(page, paths.notifications);
  await page.context().close();
}

async function encodeGif(frames, output) {
  const inputs = frames.flatMap((frame, index) => [
    "-loop",
    "1",
    "-t",
    String(FRAME_SECONDS[index]),
    "-i",
    frame,
  ]);
  const canvasHeight = Math.round((GIF.width * WINDOW.height) / WINDOW.width);
  const pad = `pad=${GIF.width}:${canvasHeight}:(ow-iw)/2:(oh-ih)/2:color=${GIF.background}`;
  const normalized = frames
    .map(
      (_, index) =>
        `[${index}:v]scale=-2:${canvasHeight}:flags=lanczos,scale='min(iw,${GIF.width})':-2:flags=lanczos,${pad},setsar=1,fps=${GIF.fps}[f${index}]`,
    )
    .join(";");
  const labels = frames.map((_, index) => `[f${index}]`).join("");
  const filter = `${normalized};${labels}concat=n=${frames.length}:v=1:a=0,split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle`;
  await run(ffmpeg, [
    "-y",
    "-loglevel",
    "error",
    ...inputs,
    "-filter_complex",
    filter,
    "-loop",
    "0",
    output,
  ]);
}

async function main() {
  await assertServer();
  await rm(framesDirectory, { recursive: true, force: true });
  await mkdir(framesDirectory, { recursive: true });
  const image = (name) => join(imagesDirectory, name);
  const frame = (name) => join(framesDirectory, name);
  const browser = await chromium.launch();
  try {
    await capturePanel(browser, image("menu-bar.png"));
    await captureDashboard(browser, {
      dashboard: image("dashboard.png"),
      github: frame("03-github.png"),
      connections: image("connections.png"),
      notifications: image("notifications.png"),
    });
  } finally {
    await browser.close();
  }
  await encodeGif(
    [
      image("menu-bar.png"),
      image("dashboard.png"),
      frame("03-github.png"),
      image("connections.png"),
      image("notifications.png"),
    ],
    image("walkthrough.gif"),
  );
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
