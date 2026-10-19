import { describe, expect, it } from "vitest";
import { demoSnapshot } from "./demo";
import {
  duration,
  latestCompleted,
  sortPanelActivities,
  isActive,
  panelActivities,
  panelSettingsSchema,
  targetKey,
  settingsSchema,
  snapshotSchema,
  visibleActivities,
  type Activity,
} from "./domain";

describe("activity filtering", () => {
  it("uses the latest cross-account state while preserving verified Mine attribution", () => {
    const base = demoSnapshot();
    const cached = {
      ...base.activities[0],
      ownership: "mine" as const,
      status: "running" as const,
      updatedAt: "2026-09-29T00:00:00Z",
    };
    const fresh = {
      ...cached,
      connectionId: "github:work",
      ownership: "other" as const,
      status: "success" as const,
      updatedAt: "2026-09-29T00:10:00Z",
      finishedAt: "2026-09-29T00:10:00Z",
    };
    const now = Date.parse("2026-09-29T00:20:00Z");
    for (const activities of [
      [cached, fresh],
      [fresh, cached],
    ]) {
      const snapshot = { ...base, activities };
      expect(visibleActivities(snapshot, "all", "", now)).toEqual([
        {
          ...cached,
          status: fresh.status,
          rawStatus: fresh.rawStatus,
          finishedAt: fresh.finishedAt,
          updatedAt: fresh.updatedAt,
        },
      ]);
      expect(
        visibleActivities(snapshot, "all", "", now, cached.connectionId),
      ).toEqual([cached]);
      expect(
        visibleActivities(snapshot, "all", "", now, fresh.connectionId),
      ).toEqual([]);
    }
    const tied = { ...cached, updatedAt: fresh.updatedAt };
    expect(
      visibleActivities(
        { ...base, activities: [fresh, tied] },
        "all",
        "",
        now,
      )[0].status,
    ).toBe("success");
  });
  it("combines accounts without double-counting the same execution and prefers verified mine", () => {
    const base = demoSnapshot();
    const run = base.activities[0];
    const other = {
      ...run,
      connectionId: "github:work",
      ownership: "other" as const,
    };
    const snapshot = {
      ...base,
      settings: { ...base.settings, scope: "all" as const },
      activities: [other, run],
    };
    const combined = visibleActivities(snapshot, "all", "");
    expect(combined).toHaveLength(1);
    expect(combined[0].connectionId).toBe("github:local");
    expect(
      visibleActivities(snapshot, "all", "", undefined, "github:work"),
    ).toEqual([other]);
    expect(
      visibleActivities(
        { ...snapshot, settings: { ...snapshot.settings, scope: "mine" } },
        "all",
        "",
        undefined,
        "github:work",
      ),
    ).toEqual([]);
  });
  it("preserves grouping, attribution, and tie order across complex duplicates", () => {
    const base = demoSnapshot();
    const verified = {
      ...base.activities[0],
      id: "shared-run",
      title: "verified release",
      status: "running" as const,
      rawStatus: "running",
      updatedAt: "2026-09-29T00:00:00Z",
      finishedAt: null,
    };
    const freshOther = {
      ...verified,
      connectionId: "github:work",
      title: "other account title",
      ownership: "other" as const,
      status: "success" as const,
      rawStatus: "completed",
      updatedAt: "2026-09-29T00:05:00Z",
      finishedAt: "2026-09-29T00:05:00Z",
    };
    const sameIdOtherTarget = {
      ...verified,
      targetId: "other-target",
      title: "separate target",
      status: "failed" as const,
      rawStatus: "failure",
      updatedAt: freshOther.updatedAt,
      finishedAt: freshOther.finishedAt,
    };
    const tiedActive = {
      ...verified,
      id: "tied-run",
      title: "tie",
      updatedAt: "2026-09-29T00:04:00Z",
    };
    const tiedTerminal = {
      ...tiedActive,
      status: "success" as const,
      rawStatus: "completed",
      finishedAt: tiedActive.updatedAt,
    };
    const snapshot = {
      ...base,
      settings: { ...base.settings, scope: "mine" as const },
      activities: [
        verified,
        sameIdOtherTarget,
        freshOther,
        tiedActive,
        tiedTerminal,
      ],
    };

    expect(
      visibleActivities(
        snapshot,
        "all",
        "",
        Date.parse("2026-09-29T00:10:00Z"),
      ),
    ).toEqual([
      {
        ...verified,
        status: freshOther.status,
        rawStatus: freshOther.rawStatus,
        updatedAt: freshOther.updatedAt,
        finishedAt: freshOther.finishedAt,
      },
      sameIdOtherTarget,
      tiedTerminal,
    ]);
    expect(
      visibleActivities(
        snapshot,
        "all",
        "verified release",
        Date.parse("2026-09-29T00:10:00Z"),
      ),
    ).toEqual([
      {
        ...verified,
        status: freshOther.status,
        rawStatus: freshOther.rawStatus,
        updatedAt: freshOther.updatedAt,
        finishedAt: freshOther.finishedAt,
      },
    ]);
  });
  it("groups 5,000 distinct activities within a bounded synthetic budget", () => {
    const base = demoSnapshot();
    const template = base.activities[0];
    const updatedAt = "2026-09-29T00:00:00Z";
    const activities = Array.from({ length: 5_000 }, (_, index) => ({
      ...template,
      id: `synthetic-${index}`,
      project: `project-${index}`,
      title: `release-${index}`,
      status: "success" as const,
      rawStatus: "completed",
      updatedAt,
      finishedAt: updatedAt,
    }));
    const snapshot = {
      ...base,
      settings: { ...base.settings, scope: "all" as const },
      activities,
    };

    const startedAt = performance.now();
    const visible = visibleActivities(
      snapshot,
      "all",
      "",
      Date.parse("2026-09-29T00:10:00Z"),
    );
    const elapsedMs = performance.now() - startedAt;

    expect(visible).toHaveLength(5_000);
    expect(visible[0].id).toBe("synthetic-0");
    expect(visible[visible.length - 1]?.id).toBe("synthetic-4999");
    expect(
      elapsedMs,
      `grouping 5,000 synthetic activities took ${elapsedMs.toFixed(1)}ms`,
    ).toBeLessThan(1_000);

    const latestStartedAt = performance.now();
    const completed = latestCompleted(activities);
    const latestElapsedMs = performance.now() - latestStartedAt;
    expect(completed).toHaveLength(5_000);
    expect(
      latestElapsedMs,
      `grouping 5,000 completed activities took ${latestElapsedMs.toFixed(1)}ms`,
    ).toBeLessThan(1_000);
  });
  it("mine excludes other users and unknown ownership", () => {
    expect(
      visibleActivities(demoSnapshot(), "all", "").every(
        (item) => item.ownership === "mine",
      ),
    ).toBe(true);
  });
  it("all includes other users and unknown ownership without changing the source", () => {
    const original = demoSnapshot();
    const ids = original.activities.map((item) => item.id);
    const all = visibleActivities(
      { ...original, settings: { ...original.settings, scope: "all" } },
      "all",
      "",
    );
    expect(all.some((item) => item.ownership === "other")).toBe(true);
    expect(all.some((item) => item.ownership === "unknown")).toBe(true);
    expect(original.activities.map((item) => item.id)).toEqual(ids);
  });
  it("combines provider and text filters", () => {
    expect(
      visibleActivities(demoSnapshot(), "github", "production").map(
        (item) => item.id,
      ),
    ).toEqual(["g1"]);
    expect(visibleActivities(demoSnapshot(), "vercel", "production")).toEqual(
      [],
    );
  });
  it("retains long-running observations instead of silently hiding them", () => {
    const snapshot = demoSnapshot();
    const old = {
      ...snapshot.activities[0],
      startedAt: "2020-01-01T00:00:00Z",
      updatedAt: "2020-01-01T00:00:00Z",
    };
    expect(
      visibleActivities({ ...snapshot, activities: [old] }, "all", ""),
    ).toHaveLength(1);
  });
  it("uses the latest update rather than creation or completion time for recent runs", () => {
    const snapshot = demoSnapshot();
    const now = Date.parse("2026-09-30T12:00:00Z");
    const item = {
      ...snapshot.activities[0],
      status: "success" as const,
      startedAt: "2020-01-01T00:00:00Z",
      finishedAt: "2020-01-02T00:00:00Z",
      updatedAt: "2026-09-30T11:00:00Z",
    };
    expect(
      visibleActivities({ ...snapshot, activities: [item] }, "all", "", now),
    ).toHaveLength(1);
  });
  it("orders visible and grouped completed runs by their latest update", () => {
    const snapshot = demoSnapshot();
    const latestUpdate = {
      ...snapshot.activities[0],
      id: "latest-update",
      status: "success" as const,
      finishedAt: "2026-09-28T12:00:00Z",
      updatedAt: "2026-09-30T11:00:00Z",
    };
    const latestFinish = {
      ...latestUpdate,
      id: "latest-finish",
      finishedAt: "2026-09-30T11:30:00Z",
      updatedAt: "2026-09-30T10:00:00Z",
    };
    const now = Date.parse("2026-09-30T12:00:00Z");

    expect(
      visibleActivities(
        { ...snapshot, activities: [latestFinish, latestUpdate] },
        "all",
        "",
        now,
      ).map((item) => item.id),
    ).toEqual(["latest-update", "latest-finish"]);
    expect(latestCompleted([latestFinish, latestUpdate])).toEqual([
      latestUpdate,
    ]);
  });
  it("excludes old terminal updates, allows longer history", () => {
    const snapshot = demoSnapshot();
    const updatedAt = new Date(Date.now() - 48 * 3600_000).toISOString();
    const changed = {
      ...snapshot,
      activities: [
        { ...snapshot.activities[0], status: "success" as const, updatedAt },
      ],
    };
    expect(visibleActivities(changed, "all", "")).toEqual([]);
    expect(
      visibleActivities(
        { ...changed, settings: { ...changed.settings, historyHours: 168 } },
        "all",
        "",
      ),
    ).toHaveLength(1);
  });
  it("does not invent terminal duration when completion time is missing", () => {
    const item: Activity = {
      ...demoSnapshot().activities[0],
      status: "failed",
      finishedAt: null,
    };
    expect(duration(item)).toBe("—");
    expect(isActive(item)).toBe(false);
  });
  it("keeps unknown status visible without claiming it is running", () => {
    const snapshot = demoSnapshot();
    const item: Activity = {
      ...snapshot.activities[0],
      status: "unknown",
      updatedAt: "2020-01-01T00:00:00Z",
      finishedAt: null,
    };
    expect(isActive(item)).toBe(false);
    expect(duration(item)).toBe("—");
    expect(
      visibleActivities({ ...snapshot, activities: [item] }, "all", ""),
    ).toEqual([item]);
  });
});
describe("IPC validation", () => {
  it("accepts a valid snapshot and rejects unknown source states", () => {
    expect(snapshotSchema.safeParse(demoSnapshot()).success).toBe(true);
    expect(
      snapshotSchema.safeParse({
        ...demoSnapshot(),
        settingsError: "Synthetic settings read failure",
      }).success,
    ).toBe(true);
    expect(
      snapshotSchema.safeParse({ ...demoSnapshot(), settingsError: null })
        .success,
    ).toBe(true);
    expect(
      settingsSchema.safeParse({
        scope: "everyone",
        historyHours: 24,
        targets: [],
      }).success,
    ).toBe(false);
    expect(
      settingsSchema.safeParse({ scope: "mine", historyHours: 0, targets: [] })
        .success,
    ).toBe(false);
  });
  it("rejects invalid dates in backend responses", () => {
    const snapshot = demoSnapshot();
    expect(
      snapshotSchema.safeParse({ ...snapshot, lastUpdatedAt: "not a date" })
        .success,
    ).toBe(false);
  });
});

describe("status panel policy", () => {
  it("excludes blocked even from old cached waiting states without hiding other approvals", () => {
    const base = demoSnapshot();
    const blocked: Activity = {
      ...base.activities[1],
      status: "waiting",
      rawStatus: "BLOCKED",
    };
    const approval: Activity = {
      ...base.activities[0],
      status: "waiting",
      rawStatus: "waiting",
    };
    expect(
      visibleActivities(
        { ...base, activities: [blocked, approval] },
        "all",
        "",
      ),
    ).toEqual([approval]);
    expect(isActive(blocked)).toBe(false);
  });
  it("keeps active observations across the 24-hour boundary in either history range", () => {
    const base = demoSnapshot();
    const now = Date.parse("2026-09-29T12:00:00Z");
    for (const status of ["queued", "running", "waiting"] as const) {
      for (const historyHours of [24, 168] as const) {
        const item = {
          ...base.activities[0],
          status,
          startedAt: "2026-09-28T12:00:00Z",
          updatedAt: new Date(now).toISOString(),
        };
        const snapshot = {
          ...base,
          settings: { ...base.settings, historyHours },
          activities: [item],
        };
        expect(visibleActivities(snapshot, "all", "", now)).toEqual([item]);
        expect(visibleActivities(snapshot, "all", "", now + 1)).toEqual([item]);
      }
    }
  });
  it("filters panel services and account-specific projects without changing the dashboard", () => {
    const base = demoSnapshot();
    const settings = {
      ...base.settings,
      panel: {
        ...base.settings.panel,
        providers: ["github"] as const,
        hiddenTargets: [targetKey(base.settings.targets[0])],
      },
    };
    const snapshot = {
      ...base,
      settings: {
        ...settings,
        panel: { ...settings.panel, providers: [...settings.panel.providers] },
      },
    };
    expect(panelActivities(snapshot)).toEqual([]);
    expect(visibleActivities(snapshot, "all", "")).toEqual(
      visibleActivities(base, "all", ""),
    );
  });
  it("defaults old settings and rejects invalid limits", () => {
    const { panel: _panel, ...legacy } = demoSnapshot().settings;
    expect(settingsSchema.parse(legacy).panel).toEqual(
      panelSettingsSchema.parse({}),
    );
    for (const rowLimit of [0, 11, 1.5]) {
      expect(panelSettingsSchema.safeParse({ rowLimit }).success).toBe(false);
    }
  });
});

describe("panel activity order and settings migration", () => {
  it("sorts persisted pinned and failure-first settings by updated time without mutating the source", () => {
    const snapshot = demoSnapshot();
    const activities = [
      {
        ...snapshot.activities[0],
        id: "oldest",
        status: "failed" as const,
        updatedAt: "2026-09-28T12:00:00Z",
      },
      {
        ...snapshot.activities[0],
        id: "newest",
        provider: snapshot.settings.targets[1].provider,
        connectionId: snapshot.settings.targets[1].connectionId,
        targetId: snapshot.settings.targets[1].id,
        updatedAt: "2026-09-30T12:00:00Z",
      },
      {
        ...snapshot.activities[0],
        id: "middle",
        updatedAt: "2026-09-29T12:00:00Z",
      },
    ];
    const original = [...activities];
    const legacyPanel = panelSettingsSchema.parse({
      ...snapshot.settings.panel,
      pinnedTargets: [targetKey(snapshot.settings.targets[0])],
      failureFirst: true,
    });
    const sorted = sortPanelActivities(activities, legacyPanel);

    expect(sorted.map((item) => item.id)).toEqual([
      "newest",
      "middle",
      "oldest",
    ]);
    expect(activities).toEqual(original);
    expect(sorted).not.toBe(activities);
  });
  it("defaults legacy settings to system language, no alerts, and no pins", () => {
    const snapshot = demoSnapshot();
    const { preferences: _preferences, ...legacy } = snapshot.settings;
    const {
      pinnedTargets: _pins,
      failureFirst: _failures,
      ...panel
    } = legacy.panel;
    const migrated = settingsSchema.parse({ ...legacy, panel });
    expect(migrated.preferences.language).toBe("system");
    expect(migrated.preferences.notifications.enabled).toBe(false);
    expect(migrated.preferences.animateMenuBarIcon).toBe(true);
    expect(migrated.panel.pinnedTargets).toEqual([]);
  });
  it("rejects unknown languages and duplicate notification rules", () => {
    const snapshot = demoSnapshot();
    expect(
      settingsSchema.safeParse({
        ...snapshot.settings,
        preferences: { language: "not-a-language" },
      }).success,
    ).toBe(false);
    const rule = {
      targetKey: targetKey(snapshot.settings.targets[0]),
      enabled: true,
      scope: "all",
      statuses: ["failed"],
    };
    expect(
      settingsSchema.safeParse({
        ...snapshot.settings,
        preferences: { notifications: { rules: [rule, rule] } },
      }).success,
    ).toBe(false);
  });
});
