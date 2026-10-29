import { expect, test, type Page } from "@playwright/test";

async function scenario(page: Page, patch: string) {
  await page.route(/\/src\/bridge\.ts(?:\?.*)?$/, async (route) => {
    const response = await route.fetch();
    await route.fulfill({
      response,
      body: `${await response.text()}\n${patch}`,
    });
  });
  await page.goto("/?view=dashboard");
}

test("a service without selected projects explains the next step in that service", async ({
  page,
}) => {
  await page.setViewportSize({ width: 720, height: 520 });
  await scenario(
    page,
    `preview = {...preview, settings: {...preview.settings, targets: preview.settings.targets.filter(t => t.provider !== 'vercel')}};`,
  );
  await page.getByRole("button", { name: /^Vercel/ }).click();
  await expect(
    page.getByRole("heading", {
      name: "이 서비스에서 모니터링할 프로젝트를 선택하세요",
    }),
  ).toBeVisible();
  await expect(
    page.locator(".monitor-summary, .activity-section, .toolbar"),
  ).toHaveCount(0);
  const action = page.getByRole("button", {
    name: "프로젝트 선택",
    exact: true,
  });
  const box = (await action.boundingBox())!;
  expect(box.y + box.height).toBeLessThan(520);
  await page.screenshot({
    path: "artifacts/ux-service-empty-720.png",
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "프로젝트 선택", exact: true })
    .click();
  await expect(page.getByLabel("서비스", { exact: true })).toHaveValue(
    "vercel",
  );
  await expect(page.locator(".project-selection")).toBeVisible();
});

test("service and account totals only count projects being monitored in the current view", async ({
  page,
}) => {
  await scenario(
    page,
    `preview = {...preview, settings: {...preview.settings, accounts: [...preview.settings.accounts, {...preview.settings.accounts[0], id:'github:unused', label:'Unused account'}]}};`,
  );
  await expect(page.locator(".connection-health-summary")).toHaveText(
    "계정 연결 3/3",
  );
  await page.getByRole("button", { name: /^Vercel/ }).click();
  await expect(page.locator(".connection-health-summary")).toHaveText(
    "계정 연결 1/1",
  );
  await expect(page.locator(".monitor-summary button")).toContainText(
    "1개 대상 모니터링",
  );
  await page.locator(".service-health > summary").click();
  await expect(page.locator(".service-health-row")).toHaveCount(1);
  await expect(page.locator(".service-health-row")).toContainText("Vercel");
});

test("failed connections expose recovery immediately and sort the failing account first", async ({
  page,
}) => {
  await page.setViewportSize({ width: 720, height: 520 });
  await scenario(
    page,
    `preview = {...preview, connections:preview.connections.map(c=>c.provider==='vercel'?{...c,status:'signed-out',message:'Synthetic expired session'}:c)};`,
  );
  const health = page.locator(".service-health");
  await expect(health).toHaveAttribute("open", "");
  await expect(health.locator(".service-health-row").first()).toContainText(
    "Vercel",
  );
  const reconnect = health.getByRole("button", { name: "브라우저로 로그인" });
  await expect(reconnect).toBeVisible();
  await page.screenshot({
    path: "artifacts/ux-recovery-720.png",
    fullPage: true,
  });
  await reconnect.click();
  await expect(page.getByLabel("서비스", { exact: true })).toHaveValue(
    "vercel",
  );
});

test("project management keeps the filtered account across All services and multi-account views", async ({
  page,
}) => {
  await scenario(
    page,
    `
    const secondary = {...preview.settings.accounts[0], id:'github:secondary', label:'Example secondary', mode:'isolated'};
    preview = {...preview, settings:{...preview.settings, accounts:[...preview.settings.accounts,secondary], targets:[...preview.settings.targets,{...preview.settings.targets[0], connectionId:secondary.id}]}};
  `,
  );
  await page.getByLabel("계정 필터").selectOption("vercel:local");
  await page.locator(".monitor-summary button").click();
  await expect(page.getByLabel("서비스", { exact: true })).toHaveValue(
    "vercel",
  );
  await expect(page.getByLabel("연결할 계정", { exact: true })).toHaveValue(
    "vercel:local",
  );
  await page.getByRole("button", { name: /^GitHub Actions/ }).click();
  await page.getByLabel("계정 필터").selectOption("github:secondary");
  await page.locator(".scope-management button").click();
  await expect(page.getByLabel("서비스", { exact: true })).toHaveValue(
    "github",
  );
  await expect(page.getByLabel("연결할 계정", { exact: true })).toHaveValue(
    "github:secondary",
  );
});

test("search and account empty state has a keyboard recovery without changing stored display choices", async ({
  page,
}) => {
  await page.goto("/?view=dashboard");
  await page.getByRole("button", { name: /^GitHub Actions/ }).click();
  await page.getByLabel("서비스 기본 표시").selectOption("all");
  await page.getByLabel("계정 필터").selectOption("github:local");
  await page
    .getByLabel("실행 검색", { exact: true })
    .fill("no-such-deployment");
  await expect(
    page.getByRole("heading", { name: "검색 조건에 맞는 실행이 없습니다" }),
  ).toBeVisible();
  await expect(page.locator(".activity-section")).toHaveCount(0);
  const reset = page.getByRole("button", { name: "검색·계정 필터 해제" });
  await reset.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByLabel("실행 검색", { exact: true })).toBeFocused();
  await expect(page.getByLabel("계정 필터")).toHaveValue("all");
  await expect(page.getByLabel("서비스 기본 표시")).toHaveValue("all");
  await expect(
    page.getByRole("heading", { name: "GitHub Actions", exact: true }),
  ).toBeVisible();
  await expect(page.locator(".activity-row")).not.toHaveCount(0);
});

for (const width of [1080, 720, 360]) {
  test(`no-results guidance wraps consistently at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 780 });
    await page.goto("/?view=dashboard");
    await page
      .getByLabel("실행 검색", { exact: true })
      .fill("example-long-search-".repeat(8));
    const empty = page.locator(".filtered-runs-empty");
    await expect(empty).toBeVisible();
    const dimensions = await page.evaluate(() => ({
      viewport: innerWidth,
      scroll: document.documentElement.scrollWidth,
    }));
    expect(dimensions.scroll).toBeLessThanOrEqual(dimensions.viewport);
    await page.screenshot({
      path: `artifacts/ux-empty-${width}.png`,
      fullPage: true,
    });
  });
}
