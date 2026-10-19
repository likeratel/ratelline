import { expect, test } from "@playwright/test";

const settings = (page: import("@playwright/test").Page) =>
  page.getByRole("button", { name: "설정", exact: true });
const tab = (page: import("@playwright/test").Page, name: string) =>
  page.getByRole("tab", { name, exact: true });
const panel = (page: import("@playwright/test").Page, name: string) =>
  page.getByRole("tabpanel", { name, exact: true });
const exposePreviewBridge = async (page: import("@playwright/test").Page) => {
  await page.route(/\/src\/bridge\.ts(?:\?.*)?$/, async (route) => {
    const response = await route.fetch();
    const source = await response.text();
    const marker = "export const bridge = {";
    expect(source).toContain(marker);
    await route.fulfill({
      response,
      body: source.replace(
        marker,
        "export const bridge = window.__testBridge = {",
      ),
    });
  });
};

test("settings has one sidebar entry, keyboard tabs, and persists changes while returning to monitoring", async ({
  page,
}) => {
  await page.goto("/?view=dashboard");
  await expect(settings(page)).toHaveCount(1);
  await settings(page).click();
  const connections = tab(page, "서비스 연결");
  await connections.focus();
  await page.keyboard.press("ArrowRight");
  await expect(tab(page, "알림")).toBeFocused();
  const notifications = panel(page, "알림");
  await notifications
    .getByLabel("실행 결과 알림 받기", { exact: true })
    .check();
  await tab(page, "일반").click();
  await tab(page, "알림").click();
  await expect(
    notifications.getByLabel("실행 결과 알림 받기", { exact: true }),
  ).toBeChecked();
  await page.getByRole("button", { name: /^전체 서비스/ }).click();
  await expect(
    page.getByRole("heading", { name: "실행 현황", exact: true }),
  ).toBeVisible();
  await settings(page).click();
  await expect(
    panel(page, "알림").getByLabel("실행 결과 알림 받기", { exact: true }),
  ).toBeChecked();
});

test("long settings use page scroll without a save action in a short viewport", async ({
  page,
}) => {
  await page.setViewportSize({ width: 420, height: 420 });
  await page.goto("/?view=dashboard");
  await settings(page).click();
  await tab(page, "서비스 연결").click();
  const connections = panel(page, "서비스 연결");
  await expect(connections.locator(".connections-settings")).toBeVisible();
  await expect(
    connections.getByRole("button", { name: "설정 저장" }),
  ).toHaveCount(0);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
});

test("account aliases require explicit save and show successful persistence", async ({
  page,
}) => {
  await exposePreviewBridge(page);
  await page.goto("/?view=dashboard");
  await settings(page).click();
  const connections = panel(page, "서비스 연결");
  const accounts = connections.getByLabel("연결할 계정");
  await expect(accounts).toHaveValue("github:local");
  await expect(accounts).not.toContainText("GitHub CLI");
  await connections.getByRole("button", { name: "별칭 편집" }).click();
  const alias = connections.getByLabel("계정 별칭");
  await alias.fill("개인 배포");
  await expect(accounts).not.toContainText("개인 배포");
  await connections.getByRole("button", { name: "별칭 저장" }).click();
  await expect(
    connections.getByRole("status").filter({ hasText: "저장됨" }),
  ).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(async () => {
        const bridge = (
          window as unknown as {
            __testBridge: {
              snapshot: () => Promise<import("../src/domain").Snapshot>;
            };
          }
        ).__testBridge;
        return (await bridge.snapshot()).settings.preferences.accountAliases[
          "github:local"
        ];
      }),
    )
    .toBe("개인 배포");
  await expect(accounts).toContainText("개인 배포");
  await page.getByRole("button", { name: /^전체 서비스/ }).click();
  await settings(page).click();
  await expect(
    panel(page, "서비스 연결").getByLabel("연결할 계정"),
  ).toContainText("개인 배포");
});

test("notification rows support scope, results, filtering, and bulk changes", async ({
  page,
}) => {
  await page.goto("/?view=dashboard");
  await settings(page).click();
  await tab(page, "알림").click();
  const notifications = panel(page, "알림");
  const row = notifications
    .locator(".notification-row")
    .filter({ hasText: "orbit/web" });
  await row.getByRole("combobox", { name: /알림 대상/ }).selectOption("all");
  await row.getByRole("combobox", { name: /알림 결과/ }).selectOption("all");
  await notifications.getByPlaceholder("프로젝트 검색").fill("orbit/web");
  await expect(notifications.locator(".notification-row")).toHaveCount(1);
  await notifications.getByLabel("검색 결과 전체 선택").check();
  await notifications.getByLabel("선택 항목 실행 범위").selectOption("mine");
  await notifications.getByLabel("선택 항목 결과").selectOption("failed");
  await expect(row.getByRole("combobox", { name: /알림 대상/ })).toHaveValue(
    "mine",
  );
  await expect(row.getByRole("combobox", { name: /알림 결과/ })).toHaveValue(
    "failed",
  );
});

test("notification custom results retain the current selection and stay open while editing", async ({
  page,
}) => {
  await exposePreviewBridge(page);
  await page.goto("/?view=dashboard");
  await settings(page).click();
  await tab(page, "알림").click();
  const row = panel(page, "알림")
    .locator(".notification-row")
    .filter({ hasText: "orbit/web" });
  await row.getByRole("combobox", { name: /알림 대상/ }).selectOption("mine");
  const results = row.getByRole("combobox", { name: /알림 결과/ });
  await expect(results).toHaveValue("all");
  await results.selectOption("success");
  await expect(results).toHaveValue("success");
  await results.selectOption("custom");
  await row.getByLabel("실패", { exact: true }).check();
  await expect(row.getByLabel("성공", { exact: true })).toBeChecked();
  await expect(row.getByLabel("실패", { exact: true })).toBeChecked();
  await expect(results).toHaveValue("custom");
  await expect(row.locator(".notification-custom")).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(async () => {
        const bridge = (
          window as unknown as {
            __testBridge: {
              snapshot: () => Promise<import("../src/domain").Snapshot>;
            };
          }
        ).__testBridge;
        const rules = (await bridge.snapshot()).settings.preferences
          .notifications.rules;
        return rules.find((rule) => rule.targetKey.includes("orbit/web"));
      }),
    )
    .toMatchObject({ scope: "mine", statuses: ["success", "failed"] });
  await page.screenshot({
    path: "artifacts/ratelline-notification-settings.png",
  });
  await page.getByRole("button", { name: /^전체 서비스/ }).click();
  await settings(page).click();
  await tab(page, "알림").click();
  await row.getByLabel("성공", { exact: true }).uncheck();
  await expect(results).toHaveValue("custom");
  await row.getByLabel("취소", { exact: true }).uncheck();
  await expect(results).toHaveValue("custom");
});

test("changing one settings section does not overwrite another preference", async ({
  page,
}) => {
  await page.goto("/?view=dashboard");
  await settings(page).click();
  await tab(page, "알림").click();
  const notifications = panel(page, "알림");
  await notifications
    .getByLabel("실행 결과 알림 받기", { exact: true })
    .check();
  await tab(page, "메뉴 막대").click();
  const menuBar = panel(page, "메뉴 막대");
  await menuBar.getByLabel("최근 종료 표시", { exact: true }).uncheck();
  await tab(page, "알림").click();
  await expect(
    notifications.getByLabel("실행 결과 알림 받기", { exact: true }),
  ).toBeChecked();
  await tab(page, "메뉴 막대").click();
  await expect(
    menuBar.getByLabel("최근 종료 표시", { exact: true }),
  ).not.toBeChecked();
});

test("menu bar icon animation defaults on and persists when turned off", async ({
  page,
}) => {
  await page.goto("/?view=dashboard");
  await settings(page).click();
  await tab(page, "메뉴 막대").click();
  const menuBar = panel(page, "메뉴 막대");
  const animation = menuBar.getByLabel(
    "진행 중일 때 메뉴 막대 아이콘 움직이기",
    { exact: true },
  );
  await expect(animation).toBeChecked();
  await expect(menuBar).toContainText("동작 줄이기");
  await animation.uncheck();
  await tab(page, "일반").click();
  await tab(page, "메뉴 막대").click();
  await expect(animation).not.toBeChecked();
});

test("English settings tabs and recovery links open Connections from General", async ({
  page,
}) => {
  await page.route(/\/src\/bridge\.ts(?:\?.*)?$/, async (route) => {
    const response = await route.fetch();
    const source = await response.text();
    const marker = "let preview = demoSnapshot();";
    expect(source).toContain(marker);
    await route.fulfill({
      response,
      body: source.replace(
        marker,
        `${marker}
        preview = { ...preview, connections: preview.connections.map((connection) => connection.provider === "github" ? { ...connection, status: "signed-out", message: "Synthetic expired session" } : connection) };`,
      ),
    });
  });
  await page.goto("/?view=dashboard");
  await settings(page).click();
  await tab(page, "일반").click();
  const general = panel(page, "일반");
  await general.getByLabel("언어", { exact: true }).selectOption("en");
  await expect(tab(page, "Connections")).toBeVisible();
  await expect(tab(page, "Notifications")).toBeVisible();
  await expect(tab(page, "Menu bar")).toBeVisible();
  await expect(tab(page, "General")).toBeVisible();
  await page.getByRole("button", { name: /^All services/ }).click();
  const health = page.locator(".service-health");
  await expect(health).toHaveAttribute("open", "");
  await health.getByRole("button", { name: "Open connections" }).click();
  await expect(
    panel(page, "Connections").locator(".connections-settings"),
  ).toBeVisible();
});

test("service diagnostics distinguish sign-in problems and include a recovery action", async ({
  page,
}) => {
  await page.route(/\/src\/bridge\.ts(?:\?.*)?$/, async (route) => {
    const response = await route.fetch();
    const source = await response.text();
    const marker = "let preview = demoSnapshot();";
    expect(source).toContain(marker);
    await route.fulfill({
      response,
      body: source.replace(
        marker,
        `${marker}
    preview = {...preview, connections:preview.connections.map(c=>c.provider==='github'?{...c,status:'signed-out',message:'Synthetic expired session'}:c)};`,
      ),
    });
  });
  await page.goto("/?view=dashboard");
  await settings(page).click();
  await tab(page, "일반").click();
  await page.getByRole("button", { name: /^전체 서비스/ }).click();
  await expect(page.locator(".service-health")).toHaveAttribute("open", "");
  await expect(page.locator(".service-health")).toContainText("로그인 필요");
  await page
    .locator(".service-health")
    .getByRole("button", { name: "연결 설정 열기" })
    .click();
  await expect(
    page.getByRole("tabpanel", { name: "서비스 연결" }),
  ).toBeVisible();
});

for (const language of [
  { locale: "en-US", heading: "Run overview" },
  { locale: "ja-JP", heading: "実行状況" },
  { locale: "zh-CN", heading: "运行概览" },
]) {
  test(`system language selects ${language.locale}`, async ({ browser }) => {
    const context = await browser.newContext({ locale: language.locale });
    const page = await context.newPage();
    await page.goto("http://127.0.0.1:1420/?view=dashboard");
    await expect(page.locator("html")).toHaveAttribute(
      "lang",
      language.locale === "en-US"
        ? "en"
        : language.locale === "ja-JP"
          ? "ja"
          : "zh-CN",
    );
    await expect(
      page.getByRole("heading", { name: language.heading, exact: true }),
    ).toBeVisible();
    await context.close();
  });
}

test("pending notification changes merge with an unrelated external snapshot field", async ({
  page,
}) => {
  await page.route(/\/src\/bridge\.ts(?:\?.*)?$/, async (route) => {
    const response = await route.fetch();
    const source = await response.text();
    const marker = "if (isPreview) return () => {};";
    const changeMarker =
      "const validated = settingsChangeSchema.parse(change);";
    const bridgeMarker = "export const bridge = {";
    expect(source).toContain(marker);
    expect(source).toContain(changeMarker);
    expect(source).toContain(bridgeMarker);
    await route.fulfill({
      response,
      body: source
        .replace(bridgeMarker, "export const bridge = window.__testBridge = {")
        .replace(
          changeMarker,
          `${changeMarker}
          if (validated.kind === "notificationRules") await new Promise((resolve) => {
            window.__testChangeWaiting = true;
            window.addEventListener("test-release-change", resolve, { once: true });
          });`,
        )
        .replace(
          marker,
          `
      if (isPreview) {
        const update = (event) => {
          preview = event.detail;
          handler(preview);
        };
        window.addEventListener('test-new-snapshot', update);
        return () => window.removeEventListener('test-new-snapshot', update);
      }
    `,
        ),
    });
  });
  await page.goto("/?view=dashboard");
  await page.getByRole("button", { name: "설정", exact: true }).click();
  await page.getByRole("tab", { name: "알림", exact: true }).click();
  await expect(
    page.getByLabel("실행 결과 알림 받기", { exact: true }),
  ).not.toBeChecked();
  const project = page
    .locator(".notification-row")
    .filter({ hasText: "orbit/web" });
  await project
    .getByRole("combobox", { name: /알림 대상/ })
    .selectOption("all");
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as unknown as { __testChangeWaiting?: boolean })
            .__testChangeWaiting,
      ),
    )
    .toBe(true);
  await page.evaluate(async () => {
    const bridge = (
      window as unknown as {
        __testBridge: {
          snapshot: () => Promise<import("../src/domain").Snapshot>;
        };
      }
    ).__testBridge;
    const snapshot = await bridge.snapshot();
    window.dispatchEvent(
      new CustomEvent("test-new-snapshot", {
        detail: {
          ...snapshot,
          settings: {
            ...snapshot.settings,
            preferences: {
              ...snapshot.settings.preferences,
              notifications: {
                ...snapshot.settings.preferences.notifications,
                enabled: true,
              },
            },
          },
        },
      }),
    );
    window.dispatchEvent(new Event("test-release-change"));
  });
  await expect(
    project.getByRole("combobox", { name: /알림 결과/ }),
  ).toHaveValue("all");
  await expect(
    page.getByLabel("실행 결과 알림 받기", { exact: true }),
  ).toBeChecked();
  await expect
    .poll(() =>
      page.evaluate(async () => {
        const bridge = (
          window as unknown as {
            __testBridge: {
              snapshot: () => Promise<import("../src/domain").Snapshot>;
            };
          }
        ).__testBridge;
        const notifications = (await bridge.snapshot()).settings.preferences
          .notifications;
        return {
          enabled: notifications.enabled,
          scope: notifications.rules.find((rule) =>
            rule.targetKey.includes("orbit/web"),
          )?.scope,
        };
      }),
    )
    .toEqual({ enabled: true, scope: "all" });
  await page.getByRole("button", { name: /전체 서비스/ }).click();
  await page.getByRole("button", { name: "설정", exact: true }).click();
  await page.getByRole("tab", { name: "알림", exact: true }).click();
  await expect(
    project.getByRole("combobox", { name: /알림 대상/ }),
  ).toHaveValue("all");
  await expect(
    project.getByRole("combobox", { name: /알림 결과/ }),
  ).toHaveValue("all");
  await expect
    .poll(() =>
      page.evaluate(async () => {
        const bridge = (
          window as unknown as {
            __testBridge: {
              snapshot: () => Promise<import("../src/domain").Snapshot>;
            };
          }
        ).__testBridge;
        const notifications = (await bridge.snapshot()).settings.preferences
          .notifications;
        return {
          enabled: notifications.enabled,
          scope: notifications.rules.find((rule) =>
            rule.targetKey.includes("orbit/web"),
          )?.scope,
        };
      }),
    )
    .toEqual({ enabled: true, scope: "all" });
});
