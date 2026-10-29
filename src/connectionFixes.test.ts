import { describe, expect, it } from "vitest";
import { connectionFixes, fixAction, groupIssues } from "./connectionFixes";
import {
  connectionFixCopy,
  connectionFixCopyEntries,
  connectionFixCopyKeys,
} from "./connectionFixCopy";
import { demoIssueSnapshot, demoSnapshot } from "./demo";
import { ISSUE_KINDS, issueSchema, type Issue } from "./domain";

const issue = (overrides: Partial<Issue>): Issue => ({
  provider: "github",
  connectionId: "github:local",
  targetId: "github:orbit/web",
  message: "Synthetic failure",
  kind: "other",
  ...overrides,
});

describe("connection fixes", () => {
  it("lists nothing for a healthy snapshot", () => {
    expect(connectionFixes(demoSnapshot())).toEqual([]);
  });

  it("puts the account choice before a sign-in and counts affected targets", () => {
    const fixes = connectionFixes(demoIssueSnapshot());
    expect(
      fixes.map((fix) => [fix.connectionId, fix.kind, fix.action]),
    ).toEqual([
      ["github:local", "needsAccount", "chooseAccount"],
      ["vercel:local", "signedOut", "providerSignIn"],
    ]);
    expect(fixes[0].affectedTargets).toBe(1);
  });

  it("offers one primary action per cause", () => {
    expect(fixAction("github", "noAccess")).toBe("chooseAccount");
    expect(fixAction("github-deployments", "needsAccount")).toBe(
      "chooseAccount",
    );
    expect(fixAction("github", "signedOut")).toBe("githubSignIn");
    expect(fixAction("vercel", "signedOut")).toBe("providerSignIn");
    expect(fixAction("vercel", "noAccess")).toBe("providerSwitchAccount");
    expect(fixAction("railway", "missingCli")).toBe("installCli");
    for (const kind of ["rateLimited", "network", "timedOut"] as const)
      expect(fixAction("github", kind)).toBe("retry");
  });

  it("reports a connected account's classified query failure but not notices", () => {
    const snapshot = {
      ...demoSnapshot(),
      issues: [
        issue({ kind: "noAccess" }),
        issue({ kind: "other", message: "History limit reached." }),
      ],
    };
    const [fix] = connectionFixes(snapshot);
    expect(fix).toMatchObject({
      connectionId: "github:local",
      kind: "noAccess",
      affectedTargets: 1,
    });
    expect(
      connectionFixes({
        ...demoSnapshot(),
        issues: [issue({ kind: "other" })],
      }),
    ).toEqual([]);
  });
});

describe("issue grouping", () => {
  it("collapses one cause across statuses and targets into one row", () => {
    const groups = groupIssues([
      issue({ kind: "noAccess", targetId: "a", message: "recent" }),
      issue({ kind: "noAccess", targetId: "a", message: "active" }),
      issue({ kind: "noAccess", targetId: "b", message: "recent" }),
      issue({ kind: "other", message: "limit" }),
      issue({ kind: "other", message: "unsupported response" }),
    ]);
    expect(groups.map((group) => [group.kind, group.targetIds])).toEqual([
      ["noAccess", ["a", "b"]],
      ["other", ["github:orbit/web"]],
      ["other", ["github:orbit/web"]],
    ]);
  });

  it("parses issues without a kind from older backends as other", () => {
    expect(
      issueSchema.parse({ provider: "github", message: "legacy" }).kind,
    ).toBe("other");
    expect(
      issueSchema.parse({
        provider: "github",
        message: "future",
        kind: "somethingNew",
      }).kind,
    ).toBe("other");
  });
});

describe("connection fix copy", () => {
  it("has four non-empty translations for every key and cause", () => {
    for (const key of connectionFixCopyKeys) {
      const entries = connectionFixCopyEntries(key);
      expect(entries).toHaveLength(4);
      for (const entry of entries) expect(entry.trim()).not.toBe("");
    }
    for (const kind of ISSUE_KINDS)
      expect(connectionFixCopyKeys).toContain(kind);
    expect(connectionFixCopy("en", "affected", { count: 3 })).toBe(
      "3 affected targets",
    );
  });
});
