import { expect, test } from "@playwright/test";

test.use({ viewport: { width: 420, height: 540 } });

test("an open settings view reconciles another window's account removal", async ({
  page,
}) => {
  await page.route(/\/src\/bridge\.ts(?:\?.*)?$/, async (route) => {
    const response = await route.fetch();
    const source = (await response.text()).replace(
      "if (isPreview) return () => {};",
      `if (isPreview) {
        const update = (event) => { preview = event.detail; handler(preview); };
        window.addEventListener("test-snapshot", update);
        return () => window.removeEventListener("test-snapshot", update);
      }`,
    );
    await route.fulfill({ response, body: source });
  });
  await page.goto("/?view=dashboard");
  await page.getByRole("button", { name: "설정", exact: true }).click();
  const connections = page.getByRole("tabpanel", { name: "서비스 연결" });
  await connections.getByRole("checkbox").uncheck();
  await page.evaluate(async () => {
    const modulePath = "/src/demo.ts";
    const { demoSnapshot } = await import(modulePath);
    const snapshot = demoSnapshot();
    window.dispatchEvent(
      new CustomEvent("test-snapshot", {
        detail: {
          ...snapshot,
          settings: {
            ...snapshot.settings,
            accounts: snapshot.settings.accounts.filter(
              (item: { provider: string }) => item.provider !== "vercel",
            ),
            targets: snapshot.settings.targets.filter(
              (item: { provider: string }) => item.provider !== "vercel",
            ),
          },
        },
      }),
    );
  });
  await expect(
    connections.getByLabel("연결할 계정").locator("option"),
  ).toHaveCount(1);
  await expect(connections.locator(".target-option input")).toBeChecked();
  await page.getByRole("button", { name: "전체 서비스", exact: true }).click();
  await page.getByRole("button", { name: "설정", exact: true }).click();
  await expect(
    connections.getByLabel("연결할 계정").locator("option"),
  ).toHaveCount(1);
});

test("default view is a compact summary with dashboard entry and no settings controls", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.getByRole("main", { name: "실행 요약" })).toBeVisible();
  await expect(page.locator(".summary-grid")).toHaveCount(0);
  await expect(
    page
      .getByRole("region", { name: "진행 중", exact: true })
      .locator(".panel-run"),
  ).toHaveCount(2);
  await expect(page.locator(".panel-scope")).toContainText(
    "내 실행 · 3/3개 대상",
  );
  await expect(page.getByLabel("서비스 필터")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "설정" })).toHaveCount(0);
  await page.screenshot({ path: "artifacts/ratelline-panel.png" });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  await page.getByRole("button", { name: "대시보드 열기" }).click();
  await expect(page).toHaveURL(/view=dashboard/);
  await expect(
    page.getByRole("heading", { name: "실행 현황", exact: true }),
  ).toBeVisible();
});

test("connection controls remain usable in a narrow dashboard", async ({
  page,
}) => {
  await page.goto("/?view=dashboard");
  await page.getByRole("button", { name: "설정", exact: true }).click();
  const connections = page.getByRole("tabpanel", { name: "서비스 연결" });
  await expect(connections.locator(".connections-settings")).toBeVisible();
  await connections.getByLabel("서비스", { exact: true }).selectOption("aws");
  await expect(
    page.getByRole("textbox", { name: "프로필", exact: true }),
  ).toHaveValue("development");
  await expect(
    connections.getByRole("button", { name: "설정 저장" }),
  ).toHaveCount(0);
  expect(
    await connections.evaluate(
      (element) => element.scrollWidth <= element.clientWidth,
    ),
  ).toBe(true);
  await page.screenshot({ path: "artifacts/ratelline-panel-settings.png" });
});

test("adds a separate account without inheriting another account's selected targets", async ({
  page,
}) => {
  await page.goto("/?view=dashboard");
  await page.getByRole("button", { name: "설정", exact: true }).click();
  const connections = page.getByRole("tabpanel", { name: "서비스 연결" });
  await connections
    .getByRole("button", { name: "계정 추가", exact: true })
    .click();
  await connections
    .getByRole("textbox", { name: "계정 이름", exact: true })
    .fill("회사");
  await connections
    .getByRole("button", { name: "계정 연결 추가", exact: true })
    .click();
  await expect(connections.getByLabel("연결할 계정")).toContainText("회사");
  await connections.getByRole("button", { name: "연결 및 대상 확인" }).click();
  await expect(connections.locator(".target-option input")).not.toBeChecked();
  await connections.locator(".target-option input").check();
  await page.getByRole("button", { name: "전체 서비스", exact: true }).click();
  await expect(page.getByLabel("계정 필터")).toContainText("회사");
  await page
    .getByLabel("계정 필터")
    .selectOption({ label: "GitHub Actions · 회사" });
  await expect(page.locator(".activity-row")).toHaveCount(0);
});

test("clears CLI account selection per connection and preserves other targets on removal", async ({
  page,
}) => {
  await page.goto("/?view=dashboard");
  await page.getByRole("button", { name: "설정", exact: true }).click();
  const connections = page.getByRole("tabpanel", { name: "서비스 연결" });
  await expect(
    connections.getByRole("button", { name: "계정 바꾸기 · orbit-developer" }),
  ).toBeVisible();
  await connections
    .getByRole("button", { name: "계정 추가", exact: true })
    .click();
  await connections
    .getByRole("textbox", { name: "계정 이름", exact: true })
    .fill("회사");
  await connections
    .getByRole("button", { name: "계정 연결 추가", exact: true })
    .click();
  const accounts = connections.getByRole("region", { name: "GitHub 계정" });
  await expect(
    accounts.getByText("아직 계정을 고르지 않았습니다.", { exact: false }),
  ).toBeVisible();
  await expect(accounts.getByText("다른 연결에서 사용 중")).toBeVisible();
  await expect(
    accounts.getByRole("button", { name: "orbit-developer 사용" }),
  ).toBeDisabled();
  await connections.getByRole("button", { name: "연결 및 대상 확인" }).click();
  await connections.locator(".target-option input").check();
  await accounts.getByRole("button", { name: "orbit-work 사용" }).click();
  await expect(
    connections.getByRole("button", { name: "계정 바꾸기 · orbit-work" }),
  ).toBeVisible();
  await connections.getByLabel("연결할 계정").selectOption("github:local");
  await expect(
    connections.getByRole("button", { name: "계정 바꾸기 · orbit-developer" }),
  ).toBeVisible();
  await expect(connections.locator(".target-option input")).toBeChecked();
  await connections.getByLabel("연결할 계정").selectOption({ label: "회사" });
  await connections.getByRole("button", { name: "이 계정 연결 제거" }).click();
  await expect(connections.getByLabel("연결할 계정")).toContainText("회사");
  await connections.getByLabel("연결할 계정").selectOption("github:local");
  await expect(
    connections.getByRole("button", { name: "연결 제거", exact: true }),
  ).toHaveCount(0);
  await connections.getByLabel("연결할 계정").selectOption({ label: "회사" });
  await expect(
    connections.getByRole("button", { name: "연결 제거", exact: true }),
  ).toHaveCount(0);
  await connections.getByRole("button", { name: "이 계정 연결 제거" }).click();
  await connections.getByRole("button", { name: "취소", exact: true }).click();
  await expect(connections.getByLabel("연결할 계정")).toContainText("회사");
  await connections.getByRole("button", { name: "이 계정 연결 제거" }).click();
  await connections
    .getByRole("button", { name: "연결 제거", exact: true })
    .click();
  await expect(connections.getByLabel("연결할 계정")).toHaveValue(
    "github:local",
  );
  await expect(connections.locator(".target-option input")).toBeChecked();
  await page.getByRole("button", { name: "전체 서비스", exact: true }).click();
  await expect(page.getByLabel("계정 필터")).not.toContainText("회사");
});

test("saved panel preferences affect the panel while preserving dashboard rows", async ({
  page,
}) => {
  // Switch the mounted view without reloading the synthetic in-memory backend.
  await page.route(/\/src\/main\.tsx(?:\?.*)?$/, async (route) => {
    const response = await route.fetch();
    const source = (await response.text()).replace(
      'ReactDOM.createRoot(document.getElementById("root"))',
      '(window.__testRoot = ReactDOM.createRoot(document.getElementById("root")))',
    );
    await route.fulfill({
      response,
      body: `${source}\nwindow.__showPanel = async () => { const { TrayPanel } = await import('/src/components/TrayPanel.tsx'); window.__testRoot.render(React.createElement(TrayPanel)); };`,
    });
  });
  await page.goto("/?view=dashboard");
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          typeof (window as unknown as { __showPanel?: () => void })
            .__showPanel,
      ),
    )
    .toBe("function");
  await expect(page.locator(".app-shell")).toBeVisible();
  const dashboardCount = await page.locator(".activity-row").count();
  await page.getByRole("button", { name: "설정", exact: true }).click();
  await page.getByRole("tab", { name: "메뉴 막대", exact: true }).click();
  const menuBar = page.getByRole("tabpanel", { name: "메뉴 막대" });
  await menuBar
    .getByRole("group", { name: "표시할 서비스" })
    .getByLabel("Vercel", { exact: true })
    .uncheck();
  await menuBar.getByLabel("최근 종료 표시", { exact: true }).uncheck();
  await menuBar.getByLabel("섹션별 표시 건수").selectOption("1");
  await page.getByRole("button", { name: /전체 서비스/ }).click();
  await expect(page.locator(".activity-row")).toHaveCount(dashboardCount);
  await page.getByRole("button", { name: "설정", exact: true }).click();
  await page.getByRole("tab", { name: "메뉴 막대", exact: true }).click();
  await expect(menuBar.getByLabel("섹션별 표시 건수")).toHaveValue("1");
  await expect(
    menuBar.getByLabel("최근 종료 표시", { exact: true }),
  ).not.toBeChecked();
  await page.screenshot({
    path: "artifacts/ratelline-panel-preferences.png",
    fullPage: true,
  });
  await page.evaluate(() =>
    (window as unknown as { __showPanel: () => void }).__showPanel(),
  );
  await expect(page.getByRole("main", { name: "실행 요약" })).toBeVisible();
  await expect(page.locator(".panel-run")).toHaveCount(1);
  await expect(
    page.getByText("Deploy production", { exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("region", { name: "최근 24시간" })).toHaveCount(
    0,
  );
});

test("a menu bar view reconciles a newer provider snapshot", async ({
  page,
}) => {
  await page.route(/\/src\/bridge\.ts(?:\?.*)?$/, async (route) => {
    const response = await route.fetch();
    const source = await response.text();
    const snapshotMarker = "let preview = demoSnapshot();";
    const subscriptionMarker = "if (isPreview) return () => {};";
    expect(source).toContain(snapshotMarker);
    expect(source).toContain(subscriptionMarker);
    await route.fulfill({
      response,
      body: source
        .replace(
          snapshotMarker,
          `${snapshotMarker}
          preview = { ...preview, settings: { ...preview.settings, panel: { ...preview.settings.panel, providers: preview.settings.panel.providers.filter((provider) => provider !== "aws") } } };`,
        )
        .replace(
          subscriptionMarker,
          `if (isPreview) {
          const update = (event) => { preview = event.detail; handler(preview); };
          window.addEventListener("test-panel-snapshot", update);
          return () => window.removeEventListener("test-panel-snapshot", update);
        }`,
        ),
    });
  });
  await page.goto("/?view=dashboard");
  await page.getByRole("button", { name: "설정", exact: true }).click();
  await page.getByRole("tab", { name: "메뉴 막대", exact: true }).click();
  const menuBar = page.getByRole("tabpanel", { name: "메뉴 막대" });
  await expect(menuBar.getByLabel("AWS", { exact: true })).not.toBeChecked();
  await menuBar.getByLabel("GitHub Actions", { exact: true }).uncheck();
  await page.evaluate(async () => {
    const { demoSnapshot } = await import("/src/demo.ts");
    const snapshot = demoSnapshot();
    window.dispatchEvent(
      new CustomEvent("test-panel-snapshot", {
        detail: {
          ...snapshot,
          settings: {
            ...snapshot.settings,
            panel: { ...snapshot.settings.panel, providers: ["github", "aws"] },
          },
        },
      }),
    );
  });
  await expect(
    menuBar.getByLabel("GitHub Actions", { exact: true }),
  ).toBeChecked();
  await expect(menuBar.getByLabel("AWS", { exact: true })).toBeChecked();
});

test("summary rows show loaded platform logos alongside their status", async ({
  page,
}) => {
  await page.goto("/?view=panel");
  const rows = page.locator(".panel-run");
  await expect(rows.first()).toBeVisible();
  for (const row of await rows.all()) {
    const provider = row.getByRole("img");
    await expect(provider).toHaveAttribute("aria-label", /GitHub|Vercel|AWS/);
    await expect(provider.locator("img")).toHaveAttribute(
      "src",
      /^\/providers\/(github|vercel|aws)\.svg$/,
    );
    await expect
      .poll(() =>
        provider
          .locator("img")
          .evaluate(
            (image) =>
              (image as HTMLImageElement).complete &&
              (image as HTMLImageElement).naturalWidth > 0,
          ),
      )
      .toBe(true);
    await expect(row.locator(".run-symbol")).toBeVisible();
    await expect(row.locator(".run-symbol")).toHaveAttribute("width", "16");
    await expect(row.locator(".panel-provider .run-symbol")).toHaveCount(0);
  }
  await expect(page.locator(".panel-run-meta")).not.toContainText(["VC "]);
});

test("explains when all monitored targets are excluded from the panel", async ({
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
      preview = { ...preview, settings: { ...preview.settings, panel: { ...preview.settings.panel, providers: [] } } };`,
      ),
    });
  });
  await page.goto("/");
  await expect(page.locator(".panel-scope")).toContainText("0/3개 대상");
  await expect(
    page.getByText(/표시할 대상이 모두 제외되어 있습니다/),
  ).toBeVisible();
  await expect(page.locator(".panel-run")).toHaveCount(0);
});

test("panel shows actors alongside long project names and marks missing actors", async ({
  page,
}) => {
  await page.route(/\/src\/bridge\.ts(?:\?.*)?$/, async (route) => {
    const response = await route.fetch();
    const source = (await response.text()).replace(
      "if (isPreview) return () => {};",
      `if (isPreview) {
        const update = (event) => { preview = event.detail; handler(preview); };
        window.addEventListener("test-snapshot", update);
        return () => window.removeEventListener("test-snapshot", update);
      }`,
    );
    await route.fulfill({ response, body: source });
  });
  await page.goto("/");
  await expect(page.locator(".panel-run-actor").first()).toContainText("alex");
  await page.evaluate(async () => {
    const modulePath = "/src/demo.ts";
    const { demoSnapshot } = await import(modulePath);
    const snapshot = demoSnapshot();
    window.dispatchEvent(
      new CustomEvent("test-snapshot", {
        detail: {
          ...snapshot,
          settings: { ...snapshot.settings, scope: "all" },
          activities: snapshot.activities.map(
            (item: Record<string, unknown>, index: number) => ({
              ...item,
              project: "example-organization/" + "long-project-name".repeat(8),
              branch: "feature/" + "long-branch".repeat(8),
              actor:
                index === 0 ? "example-developer" : index === 1 ? "   " : null,
              ownership: index === 0 ? "mine" : "unknown",
            }),
          ),
        },
      }),
    );
  });
  const actor = page.locator('.panel-run-actor[title="example-developer"]');
  await expect(actor).toBeVisible();
  await expect(
    page
      .locator(".panel-run-actor")
      .filter({ hasText: "실행자 미확인" })
      .first(),
  ).toBeVisible();
  const bounds = await actor.boundingBox();
  expect(bounds?.width).toBeGreaterThan(50);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
});

test("panel preserves workflow names without repeating titles or overflowing", async ({
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
      preview = { ...preview, activities: preview.activities.map((item) => ({
        ...item,
        workflowName: item.id === "g1" ? "Deploy production workflow with a very long name" : item.id === "g2" ? item.title : null,
      })) };
    `,
      ),
    });
  });
  await page.goto("/");
  const row = page
    .locator(".panel-run")
    .filter({ has: page.locator(".panel-run-workflow") });
  await expect(row).toHaveCount(1);
  await expect(row.locator(".panel-run-workflow")).toHaveText(
    "Deploy production workflow with a very long name",
  );
  await expect(row.locator(".panel-run-title")).toHaveText("Deploy production");
  await expect(row.locator(".panel-run-heading")).toHaveAttribute(
    "title",
    "Deploy production workflow with a very long name · Deploy production",
  );
  await expect(row.locator(".panel-run-actor")).toContainText("alex");
  const duplicate = page.locator(".panel-run").filter({
    has: page.locator(".panel-run-title", { hasText: "Test & typecheck" }),
  });
  await expect(duplicate).toHaveCount(1);
  await expect(duplicate.locator(".panel-run-workflow")).toHaveCount(0);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({ path: "artifacts/ratelline-panel-workflow.png" });
});
