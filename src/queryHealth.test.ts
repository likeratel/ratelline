import { describe, expect, it } from "vitest";
import { demoSnapshot } from "./demo";
import { snapshotSchema, type Snapshot } from "./domain";
import {
  queryStateForTarget,
  shouldRequestRefresh,
  summarizeQueryHealth,
} from "./queryHealth";

const now = Date.parse("2026-10-06T10:00:00Z");
const at = (seconds: number) => new Date(now - seconds * 1000).toISOString();
function fixture(): Snapshot {
  const base = demoSnapshot();
  return {
    ...base,
    lastUpdatedAt: at(0),
    pollIntervalSeconds: 30,
    queryHealth: base.queryHealth.map((row) => ({
      ...row,
      lastAttemptAt: at(5),
      lastSuccessfulAt: at(5),
    })),
  };
}

describe("target query health", () => {
  it("does not let another target's success refresh an old or failed result", () => {
    const base = fixture();
    const snapshot: Snapshot = {
      ...base,
      queryHealth: base.queryHealth.map((row, index) =>
        index === 1
          ? {
              ...row,
              status: "error",
              lastAttemptAt: at(0),
              lastSuccessfulAt: at(3600),
            }
          : row,
      ),
    };
    const summary = summarizeQueryHealth(
      snapshot,
      snapshot.settings.targets,
      now,
    );
    expect(summary).toMatchObject({
      state: "error",
      succeeded: 2,
      total: 3,
      failed: 1,
      lastSuccessfulAt: at(3600),
    });
    expect(
      summarizeQueryHealth(snapshot, [snapshot.settings.targets[0]], now).state,
    ).toBe("success");
    expect(
      queryStateForTarget(snapshot, snapshot.settings.targets[1], now),
    ).toMatchObject({ state: "error", lastSuccessfulAt: at(3600) });
  });
  it("preserves successful time during checking and partial results", () => {
    const base = fixture();
    for (const status of ["checking", "partial"] as const) {
      const snapshot = {
        ...base,
        queryHealth: [
          {
            ...base.queryHealth[0],
            status,
            lastAttemptAt: at(0),
            lastSuccessfulAt: at(400),
          },
        ],
      };
      expect(
        queryStateForTarget(snapshot, base.settings.targets[0], now),
      ).toEqual({
        state: status,
        lastAttemptAt: at(0),
        lastSuccessfulAt: at(400),
      });
    }
  });
  it("uses cadence to mark stale results and keeps legacy snapshots unknown", () => {
    const base = fixture();
    expect(
      queryStateForTarget(base, base.settings.targets[0], now + 55_000).state,
    ).toBe("success");
    expect(
      queryStateForTarget(base, base.settings.targets[0], now + 55_001).state,
    ).toBe("stale");
    const {
      queryHealth: _health,
      pollIntervalSeconds: _cadence,
      ...legacy
    } = base;
    const parsed = snapshotSchema.parse(legacy);
    expect(parsed.queryHealth).toEqual([]);
    expect(parsed.pollIntervalSeconds).toBe(120);
    expect(
      queryStateForTarget(parsed, base.settings.targets[0], now).state,
    ).toBe("unknown");
    expect(
      summarizeQueryHealth(parsed, base.settings.targets, now).lastSuccessfulAt,
    ).toBeNull();
  });
  it("does not let pagination or notification issues overwrite authoritative query success", () => {
    const base = fixture();
    const snapshot = {
      ...base,
      issues: [
        {
          provider: "github" as const,
          connectionId: "github:local",
          targetId: base.settings.targets[0].id,
          message: "Synthetic history lookup failure",
          kind: "other" as const,
        },
      ],
    };
    expect(
      queryStateForTarget(snapshot, base.settings.targets[0], now).state,
    ).toBe("success");
    expect(
      queryStateForTarget(
        { ...snapshot, queryHealth: [] },
        base.settings.targets[0],
        now,
      ).state,
    ).toBe("partial");
  });
  it("matches the complete account and target identity and still exposes connection failures", () => {
    const base = fixture();
    const target = {
      ...base.settings.targets[0],
      connectionId: "github:secondary",
    };
    expect(queryStateForTarget(base, target, now).state).toBe("unknown");
    const snapshot: Snapshot = {
      ...base,
      connections: base.connections.map((row) =>
        row.provider === "github" ? { ...row, status: "signed-out" } : row,
      ),
    };
    expect(
      queryStateForTarget(snapshot, base.settings.targets[0], now).state,
    ).toBe("error");
    expect(
      queryStateForTarget(snapshot, base.settings.targets[1], now).state,
    ).toBe("success");
  });
  it("refreshes on return after twenty seconds even when the last query succeeded", () => {
    const base = fixture();
    expect(shouldRequestRefresh(base, now)).toBe(false);
    expect(shouldRequestRefresh(base, now + 14_999)).toBe(false);
    expect(shouldRequestRefresh(base, now + 15_000)).toBe(true);
    expect(shouldRequestRefresh(base, now + 120_000)).toBe(true);
    expect(
      shouldRequestRefresh({ ...base, refreshing: true }, now + 120_000),
    ).toBe(false);
    const failed: Snapshot = {
      ...base,
      queryHealth: base.queryHealth.map((row) => ({ ...row, status: "error" })),
    };
    expect(shouldRequestRefresh(failed, now)).toBe(false);
    expect(shouldRequestRefresh(failed, now, true)).toBe(true);
    expect(
      shouldRequestRefresh({ ...failed, refreshing: true }, now, true),
    ).toBe(true);
    expect(shouldRequestRefresh(failed, now + 120_000)).toBe(true);
    expect(
      shouldRequestRefresh(
        { ...base, settings: { ...base.settings, targets: [] } },
        now + 120_000,
      ),
    ).toBe(false);
  });
  it.each(["partial", "error"] as const)(
    "keeps healthy targets responsive beside a %s target",
    (status) => {
      const base = fixture();
      const mixed: Snapshot = {
        ...base,
        queryHealth: base.queryHealth.map((row, index) =>
          index === 0 ? { ...row, status } : row,
        ),
      };
      expect(shouldRequestRefresh(mixed, now + 14_999)).toBe(false);
      expect(shouldRequestRefresh(mixed, now + 15_000)).toBe(true);
      expect(shouldRequestRefresh(mixed, now, true)).toBe(true);
      const failed: Snapshot = {
        ...base,
        queryHealth: base.queryHealth.map((row) => ({ ...row, status })),
      };
      expect(shouldRequestRefresh(failed, now + 20_000)).toBe(false);
    },
  );
  it("does not let a history-page update postpone refreshing current activity", () => {
    const base = fixture();
    const snapshot: Snapshot = {
      ...base,
      lastUpdatedAt: at(0),
      queryHealth: base.queryHealth.map((row) => ({
        ...row,
        lastAttemptAt: at(40),
        lastSuccessfulAt: at(40),
      })),
    };
    expect(shouldRequestRefresh(snapshot, now)).toBe(true);
  });
  it("uses each successful target's age when other targets finish later", () => {
    const base = fixture();
    const snapshot: Snapshot = {
      ...base,
      lastUpdatedAt: at(1),
      queryHealth: base.queryHealth.map((row, index) =>
        index === 0
          ? { ...row, lastAttemptAt: at(30), lastSuccessfulAt: at(30) }
          : { ...row, status: "partial", lastAttemptAt: at(1) },
      ),
    };
    expect(shouldRequestRefresh(snapshot, now)).toBe(true);
  });
});
