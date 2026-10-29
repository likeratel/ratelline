import { expect, test, type Page } from "@playwright/test";

async function withBridgePatch(page: Page, patch: string) {
  await page.route(/\/src\/bridge\.ts(?:\?.*)?$/, async (route) => {
    const response = await route.fetch();
    const source = await response.text();
    await route.fulfill({ response, body: `${source}\n${patch}` });
  });
}

test("GitHub uses a chosen CLI account from the main screen without API setup", async ({
  page,
}) => {
  await page.goto("/?view=dashboard");
  await page.getByRole("button", { name: "설정", exact: true }).click();
  const connections = page.getByRole("tabpanel", {
    name: "서비스 연결",
    exact: true,
  });
  await connections
    .getByRole("button", { name: "계정 바꾸기 · orbit-developer" })
    .click();
  const list = connections.getByRole("region", { name: "GitHub 계정" });
  await expect(list).toBeFocused();
  await expect(list.getByText("사용 중", { exact: true })).toBeVisible();
  await expect(list.getByText("대상 1개 접근 불가")).toBeVisible();
  await list.getByRole("button", { name: "orbit-work 사용" }).click();
  await expect(
    connections.getByRole("button", { name: "계정 바꾸기 · orbit-work" }),
  ).toBeVisible();
  await expect(page.getByLabel("API 토큰", { exact: true })).toHaveCount(0);
  await page.getByLabel("서비스", { exact: true }).selectOption("vercel");
  await expect(
    page.getByRole("button", { name: "브라우저로 로그인" }),
  ).toBeVisible();
  await expect(page.getByLabel("API 토큰", { exact: true })).toHaveCount(0);
});

test("missing CLI accounts report an error and remain retryable", async ({
  page,
}) => {
  await withBridgePatch(
    page,
    `bridge.githubCliAccountStates = async () => { throw new Error("GitHub CLI was not found. Install gh and retry."); };`,
  );
  await page.goto("/?view=dashboard");
  await page.getByRole("button", { name: "설정", exact: true }).click();
  await page.getByRole("button", { name: /^계정 바꾸기/ }).click();
  const list = page.getByRole("region", { name: "GitHub 계정" });
  await expect(list.getByRole("alert")).toContainText(
    "GitHub CLI was not found",
  );
  await expect(
    list.getByRole("button", { name: "CLI 계정 새로고침", exact: true }),
  ).toBeEnabled();
  await expect(list.getByRole("button", { name: /사용$/ })).toHaveCount(0);
});

test("Vercel reauthentication never sends its stable identity as an AWS profile", async ({
  page,
}) => {
  await page.route(/\/src\/bridge\.ts(?:\?.*)?$/, async (route) => {
    const response = await route.fetch();
    const source = (await response.text()).replace(
      "let preview = demoSnapshot();",
      'let preview = demoSnapshot(); preview = { ...preview, settings: { ...preview.settings, accounts: preview.settings.accounts.map(account => account.provider === "vercel" ? { ...account, profile: "synthetic-vercel-id" } : account) } };',
    );
    await route.fulfill({
      response,
      body: `${source}\nwindow.__testBridge = bridge;`,
    });
  });
  await page.goto("/?view=dashboard");
  await expect(page.locator(".app-shell")).toBeVisible();
  await page.evaluate(() => {
    const backend = (
      window as unknown as {
        __testBridge: { login: (...args: unknown[]) => Promise<unknown> };
      }
    ).__testBridge;
    backend.login = async (provider, connectionId, profile) => {
      document.documentElement.dataset.loginProfile = String(profile);
      return {
        provider,
        connectionId,
        state: "error",
        message: "Synthetic CLI sign-in",
        code: null,
        url: null,
      };
    };
  });
  await page.getByRole("button", { name: "설정", exact: true }).click();
  await page.getByLabel("서비스", { exact: true }).selectOption("vercel");
  await page
    .getByRole("button", { name: "브라우저로 로그인", exact: true })
    .click();
  await expect(page.locator("html")).toHaveAttribute(
    "data-login-profile",
    "null",
  );
});
