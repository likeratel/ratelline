import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/?view=dashboard");
  await expect(page.locator(".app-shell")).toBeVisible();
});

test("omits workspace placeholders and keeps connection help compact", async ({
  page,
}) => {
  await expect(page.getByText("내 워크스페이스", { exact: true })).toHaveCount(
    0,
  );
  await page.getByText("표시 기준 안내", { exact: true }).click();
  await expect(
    page.getByText(/24시간을 넘겨도 종료가 확인될 때까지 유지됩니다/),
  ).toBeVisible();
  await expect(page.getByText("연결된 프로젝트", { exact: true })).toHaveCount(
    0,
  );
  await page.screenshot({
    path: "artifacts/ratelline-dashboard-ux.png",
    fullPage: true,
  });

  await page.getByRole("button", { name: "설정", exact: true }).click();
  const connections = page.getByRole("tabpanel", { name: "서비스 연결" });

  await expect(
    connections.getByText("YOUR WORKSPACE", { exact: true }),
  ).toHaveCount(0);
  await connections.locator(".authentication-details > summary").click();
  const help = connections.locator("details.connection-help").first();
  await expect(help.locator("summary")).toHaveText("CLI 설치·업데이트 안내");
  await help.locator("summary").click();
  await expect(help).toContainText("gh auth login");
  await page.screenshot({ path: "artifacts/ratelline-dialog-guide-full.png" });
});

test("uses 36px dashboard controls and a custom select indicator", async ({
  page,
}) => {
  const heights = await page
    .locator(
      ".scope-default-choice .segmented, .toolbar .search-box, .toolbar .period-select",
    )
    .evaluateAll((controls) =>
      controls.map((control) =>
        Math.round(control.getBoundingClientRect().height),
      ),
    );
  expect(heights.length).toBeGreaterThanOrEqual(4);
  expect(new Set(heights)).toEqual(new Set([36]));

  for (const select of await page.locator(".toolbar select").all()) {
    await expect(select).toHaveCSS("appearance", "none");
    await expect(select).toHaveCSS("height", "36px");
    expect(
      await select.evaluate(
        (element) => getComputedStyle(element).backgroundImage,
      ),
    ).not.toBe("none");
  }
});

test("keeps a single scrollable settings page with reachable connection actions in a short viewport", async ({
  page,
}) => {
  await page.setViewportSize({ width: 760, height: 420 });
  await page.getByRole("button", { name: "설정", exact: true }).click();

  const connections = page.getByRole("tabpanel", { name: "서비스 연결" });
  await page.screenshot({ path: "artifacts/ratelline-dialog-guide.png" });

  await expect(connections.locator(".provider-tabs")).toHaveCSS(
    "overflow-y",
    "visible",
  );
  expect(
    await connections
      .locator(".provider-tabs")
      .evaluate((catalog) => catalog.scrollHeight <= catalog.clientHeight),
  ).toBe(true);

  await connections.getByRole("button", { name: "서비스 찾아보기" }).click();
  const githubTab = connections.getByRole("tab", {
    name: "GitHub Actions",
    exact: true,
  });
  await githubTab.focus();
  await page.keyboard.press("ArrowRight");
  const vercelTab = connections.getByRole("tab", {
    name: "Vercel",
    exact: true,
  });
  await expect(vercelTab).toBeFocused();
  await expect(vercelTab).toHaveAttribute("aria-selected", "true");

  await expect(
    connections.getByRole("button", { name: "설정 저장", exact: true }),
  ).toHaveCount(0);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({ path: "artifacts/ratelline-dialog-short.png" });
});

test("collapses the empty active section and keeps recent rows readable", async ({
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
      preview = { ...preview, activities: preview.activities.filter(item => item.status === "success" || item.status === "failed") };`,
      ),
    });
  });
  await page.reload();
  const active = page.getByRole("region", { name: "진행 중", exact: true });
  await expect(active).toHaveClass(/compact-empty/);
  expect((await active.boundingBox())!.height).toBeLessThan(70);
  await expect(page.locator(".activity-time").first()).toHaveCSS(
    "font-size",
    "12px",
  );
});

test("prioritizes account actions and opens the service catalog on request", async ({
  page,
}) => {
  await expect(
    page.getByText("로컬 CLI 인증을 이 기기에서만 사용", { exact: true }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "설정", exact: true }).click();
  const connections = page.getByRole("tabpanel", { name: "서비스 연결" });
  await expect(connections.getByLabel("플랫폼 검색")).not.toBeVisible();
  await expect(connections.getByLabel("서비스", { exact: true })).toBeVisible();
  await connections.getByRole("button", { name: "서비스 찾아보기" }).click();
  await expect(connections.getByLabel("플랫폼 검색")).toBeVisible();
  await expect(
    connections.getByRole("tab", { name: "Vercel", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("서비스 추가 또는 변경", { exact: true }),
  ).toHaveCount(0);
});
