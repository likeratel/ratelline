import { expect, test, type Page } from "@playwright/test";

async function openConnections(page: Page, query = "?view=dashboard") {
  await page.goto(`/${query}`);
  await page.getByRole("button", { name: "설정", exact: true }).click();
  return page.getByRole("tabpanel", { name: "서비스 연결", exact: true });
}

async function patchBridge(page: Page, patch: string) {
  await page.route(/\/src\/bridge\.ts(?:\?.*)?$/, async (route) => {
    const response = await route.fetch();
    const source = await response.text();
    await route.fulfill({ response, body: `${source}\n${patch}` });
  });
}

test("problem connections are collected at the top with one action each", async ({
  page,
}) => {
  const connections = await openConnections(
    page,
    "?view=dashboard&demo=issues",
  );
  const fixes = connections.getByRole("region", { name: "고칠 것" });
  const cards = fixes.getByRole("listitem");
  await expect(cards).toHaveCount(2);
  await expect(cards.nth(0)).toContainText(
    "이 연결에 사용할 CLI 계정을 아직 고르지 않았습니다.",
  );
  await expect(cards.nth(0)).toContainText("영향받는 대상 1개");
  await expect(cards.nth(0).getByRole("button")).toHaveText(
    "접근 가능한 계정으로 바꾸기",
  );
  await expect(cards.nth(1)).toContainText("CLI 로그인이 필요합니다.");
  await expect(
    cards.nth(1).getByRole("button", { name: "브라우저로 로그인" }),
  ).toBeVisible();
  const box = (await fixes.boundingBox())!;
  expect(box.y).toBeLessThan(200);
});

test("the only account with access is selected in one step", async ({
  page,
}) => {
  const connections = await openConnections(
    page,
    "?view=dashboard&demo=issues",
  );
  const card = connections
    .getByRole("region", { name: "고칠 것" })
    .getByRole("listitem")
    .filter({ hasText: "GitHub" });
  await card
    .getByRole("button", { name: "접근 가능한 계정으로 바꾸기" })
    .click();
  await expect(
    connections
      .getByRole("region", { name: "고칠 것" })
      .getByRole("listitem")
      .filter({ hasText: "GitHub" }),
  ).toHaveCount(0);
  await expect(
    connections.getByRole("button", { name: "계정 바꾸기 · orbit-developer" }),
  ).toBeFocused();
  await expect(
    connections.getByRole("status").filter({
      hasText: "orbit-developer 계정으로 바꿨습니다.",
    }),
  ).toHaveCount(1);
});

test("several accounts with access are offered as a short choice", async ({
  page,
}) => {
  await patchBridge(
    page,
    `const diagnose = bridge.diagnoseGithubConnection;
bridge.diagnoseGithubConnection = async (id) => (await diagnose(id)).map((item) => ({ ...item, canAccessAll: true, inaccessibleTargets: [] }));`,
  );
  const connections = await openConnections(
    page,
    "?view=dashboard&demo=issues",
  );
  const card = connections
    .getByRole("region", { name: "고칠 것" })
    .getByRole("listitem")
    .filter({ hasText: "GitHub" });
  await card
    .getByRole("button", { name: "접근 가능한 계정으로 바꾸기" })
    .click();
  await expect(card).toContainText("접근 가능한 계정이 여러 개입니다.");
  const first = card.getByRole("button", { name: "orbit-developer 사용" });
  await expect(first).toBeFocused();
  await card.getByRole("button", { name: "orbit-work 사용" }).click();
  await expect(
    connections.getByRole("button", { name: "계정 바꾸기 · orbit-work" }),
  ).toBeVisible();
});

test("no account with access leads to the gh sign-in command", async ({
  page,
}) => {
  await patchBridge(
    page,
    `const diagnose = bridge.diagnoseGithubConnection;
bridge.diagnoseGithubConnection = async (id) => (await diagnose(id)).map((item) => ({ ...item, canAccessAll: false, inaccessibleTargets: ["github:orbit/web"] }));`,
  );
  const connections = await openConnections(
    page,
    "?view=dashboard&demo=issues",
  );
  const card = connections
    .getByRole("region", { name: "고칠 것" })
    .getByRole("listitem")
    .filter({ hasText: "GitHub" });
  await card
    .getByRole("button", { name: "접근 가능한 계정으로 바꾸기" })
    .click();
  await expect(card).toContainText("모두 접근할 수 있는 계정이 없습니다");
  await expect(card.locator("code")).toHaveText(
    "gh auth login --hostname github.com --web",
  );
  await expect(card.getByRole("button", { name: "명령 복사" })).toBeFocused();
  await expect(card.getByRole("button", { name: "다시 확인" })).toBeVisible();
});

const otherUserPatch = `const diagnose = bridge.diagnoseGithubConnection;
bridge.diagnoseGithubConnection = async (id) => (await diagnose(id)).map((item) => item.login === "orbit-work" ? { ...item, canAccessAll: false, identityMismatch: true, inaccessibleTargets: [] } : { ...item, canAccessAll: false, inaccessibleTargets: ["github:orbit/web"] });
const connect = bridge.connectGithubCliAccount;
bridge.connectGithubCliAccount = async (...args) => { document.documentElement.dataset.bind = JSON.stringify(args.slice(2)); return connect(...args); };`;

for (const choice of ["바꾸기", "취소"] as const) {
  test(`switching to another GitHub user needs confirmation (${choice})`, async ({
    page,
  }) => {
    await patchBridge(page, otherUserPatch);
    const connections = await openConnections(
      page,
      "?view=dashboard&demo=issues",
    );
    const card = connections
      .getByRole("region", { name: "고칠 것" })
      .getByRole("listitem")
      .filter({ hasText: "GitHub" });
    const primary = card.getByRole("button", {
      name: "접근 가능한 계정으로 바꾸기",
    });
    await primary.click();
    await expect(card).toContainText(
      "'내 실행'과 알림 기준이 @orbit-work(으)로 바뀌고",
    );
    await expect(
      card.getByRole("button", { name: "바꾸기", exact: true }),
    ).toBeFocused();
    await expect(page.locator("html")).not.toHaveAttribute("data-bind");
    await card.getByRole("button", { name: choice, exact: true }).click();
    if (choice === "바꾸기") {
      await expect(page.locator("html")).toHaveAttribute(
        "data-bind",
        '["orbit-work",true]',
      );
      await expect(
        connections.getByRole("button", { name: "계정 바꾸기 · orbit-work" }),
      ).toBeVisible();
    } else {
      await expect(primary).toBeFocused();
      await expect(page.locator("html")).not.toHaveAttribute("data-bind");
    }
  });
}

test("a plain account choice never asks to change the identity", async ({
  page,
}) => {
  await patchBridge(
    page,
    `const connect = bridge.connectGithubCliAccount;
bridge.connectGithubCliAccount = async (...args) => { document.documentElement.dataset.bind = JSON.stringify(args.slice(2)); return connect(...args); };`,
  );
  const connections = await openConnections(
    page,
    "?view=dashboard&demo=issues",
  );
  await connections
    .getByRole("region", { name: "고칠 것" })
    .getByRole("button", { name: "접근 가능한 계정으로 바꾸기" })
    .click();
  await expect(page.locator("html")).toHaveAttribute(
    "data-bind",
    '["orbit-developer",false]',
  );
});

for (const [kind, cause, action] of [
  ["signedOut", "CLI 로그인이 필요합니다.", "로그인 방법 보기"],
  ["missingCli", "필요한 CLI를 찾지 못했습니다.", "설치 안내 보기"],
  ["network", "서비스에 연결할 수 없습니다.", "다시 시도"],
  ["rateLimited", "요청 한도에 도달했습니다.", "다시 시도"],
] as const) {
  test(`a GitHub ${kind} problem offers "${action}"`, async ({ page }) => {
    await patchBridge(
      page,
      `preview = { ...preview, connections: preview.connections.map((item) => item.connectionId === "github:local" ? { ...item, status: "${kind === "signedOut" ? "signed-out" : kind === "missingCli" ? "missing-cli" : "error"}", kind: "${kind}", message: "Synthetic ${kind}" } : item) };`,
    );
    const connections = await openConnections(page);
    const card = connections
      .getByRole("region", { name: "고칠 것" })
      .getByRole("listitem");
    await expect(card).toContainText(cause);
    await card.getByRole("button", { name: action, exact: true }).click();
    if (kind === "signedOut")
      await expect(card.locator("code")).toHaveText(
        "gh auth login --hostname github.com --web",
      );
    if (kind === "missingCli")
      await expect(
        card.getByRole("button", { name: "다시 확인" }),
      ).toBeFocused();
  });
}

test("one cause across many statuses is shown once in service health", async ({
  page,
}) => {
  await patchBridge(
    page,
    `preview = { ...preview, queryHealth: preview.queryHealth.map((item) => item.connectionId === "github:local" ? { ...item, status: "error" } : item), issues: ["queued", "in_progress", "waiting"].map((status) => ({ provider: "github", connectionId: "github:local", targetId: "github:orbit/web", kind: "noAccess", message: "GitHub could not read active " + status + " runs." })) };`,
  );
  await page.goto("/?view=dashboard");
  const health = page.locator(".service-health");
  await expect(health).toHaveAttribute("open", "");
  const row = health
    .locator(".service-health-row")
    .filter({ hasText: "GitHub" });
  await expect(row).toContainText(
    "선택한 계정으로는 대상에 접근할 수 없습니다.",
  );
  await row.getByText("오류 상세").click();
  await expect(
    row.getByText(/선택한 계정으로는 대상에 접근할 수 없습니다\. · 대상 1개/),
  ).toHaveCount(1);
  await row
    .getByRole("button", { name: "접근 가능한 계정으로 바꾸기" })
    .click();
  const card = page
    .getByRole("region", { name: "고칠 것" })
    .getByRole("listitem")
    .filter({ hasText: "GitHub" });
  await expect(
    card.getByRole("button", { name: "접근 가능한 계정으로 바꾸기" }),
  ).toBeFocused();
});

test("the account list also confirms before switching to another GitHub user", async ({
  page,
}) => {
  await patchBridge(page, otherUserPatch);
  const connections = await openConnections(page);
  await connections
    .getByRole("button", { name: "계정 바꾸기 · orbit-developer" })
    .click();
  const list = connections.getByRole("region", { name: "GitHub 계정" });
  const use = list.getByRole("button", { name: "orbit-work 사용" });
  await use.click();
  const confirm = list.getByRole("group", { name: "orbit-work 사용" });
  await expect(confirm).toContainText("@orbit-work");
  await confirm.getByRole("button", { name: "취소", exact: true }).click();
  await expect(use).toBeFocused();
  await expect(page.locator("html")).not.toHaveAttribute("data-bind");
  await use.click();
  await confirm.getByRole("button", { name: "바꾸기", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute(
    "data-bind",
    '["orbit-work",true]',
  );
});

test("fix cards stay readable and reachable in a narrow window", async ({
  page,
}) => {
  await page.setViewportSize({ width: 360, height: 640 });
  const connections = await openConnections(
    page,
    "?view=dashboard&demo=issues",
  );
  const fixes = connections.getByRole("region", { name: "고칠 것" });
  await expect(fixes.getByRole("listitem")).toHaveCount(2);
  const primary = fixes.getByRole("button", {
    name: "접근 가능한 계정으로 바꾸기",
  });
  await expect(primary).toBeInViewport();
  expect(
    await fixes.evaluate(
      (element) => element.scrollWidth <= element.clientWidth,
    ),
  ).toBe(true);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({ path: "artifacts/connection-fixes-360.png" });
});
