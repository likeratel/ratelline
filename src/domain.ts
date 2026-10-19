import { z } from "zod";

export const PROVIDERS = [
  "github",
  "vercel",
  "aws",
  "github-deployments",
  "railway",
  "netlify",
  "render",
  "cloudflare-pages",
  "cloudflare-workers",
  "heroku",
  "fly",
  "gitlab",
  "bitbucket",
  "digitalocean",
  "gcloud",
  "azure",
  "eas",
  "firebase",
  "supabase",
  "fastlane",
] as const;
export const providerSchema = z.enum(PROVIDERS);
export const displayScopeSchema = z.enum(["mine", "all"]);
export const statusSchema = z.enum([
  "queued",
  "running",
  "waiting",
  "success",
  "failed",
  "canceled",
  "skipped",
  "unknown",
]);
const optionalText = z.string().nullish();
const date = z
  .string()
  .refine((value) => Number.isFinite(Date.parse(value)), "유효하지 않은 날짜");
export const accountSchema = z.object({
  id: z.string().min(1),
  provider: providerSchema,
  label: z.string().min(1),
  mode: z.enum(["local", "isolated"]),
  profile: optionalText,
  cliUsername: optionalText,
});
export const targetSchema = z.object({
  id: z.string().min(1),
  connectionId: z.string().min(1),
  provider: providerSchema,
  name: z.string(),
  description: optionalText,
  metadata: z.record(z.string(), z.string()),
});
export const panelSettingsSchema = z.object({
  providers: z
    .array(providerSchema)
    .max(PROVIDERS.length)
    .default(["github", "vercel", "aws"]),
  hiddenTargets: z.array(z.string().min(1).max(1600)).max(500).default([]),
  showActive: z.boolean().default(true),
  showCompleted: z.boolean().default(true),
  rowLimit: z.number().int().min(1).max(10).default(5),
  pinnedTargets: z.array(z.string().min(1).max(1600)).max(500).default([]),
  failureFirst: z.boolean().default(false),
});
export type PanelSettings = z.infer<typeof panelSettingsSchema>;

export const languageSchema = z.enum(["system", "ko", "en", "ja", "zh-CN"]);
export const notificationRuleSchema = z.object({
  targetKey: z.string().min(1).max(1600),
  enabled: z.boolean(),
  scope: z.enum(["mine", "all"]),
  statuses: z
    .array(z.enum(["success", "failed", "canceled"]))
    .max(3)
    .refine((items) => new Set(items).size === items.length),
});
export const preferencesSchema = z.object({
  accountAliases: z
    .record(z.string(), z.string().trim().min(1).max(80))
    .default({}),
  language: languageSchema.default("system"),
  checkUpdatesAutomatically: z.boolean().default(true),
  animateMenuBarIcon: z.boolean().default(true),
  notifications: z
    .object({
      enabled: z.boolean().default(false),
      rules: z
        .array(notificationRuleSchema)
        .max(500)
        .refine(
          (items) =>
            new Set(items.map((item) => item.targetKey)).size === items.length,
        )
        .default([]),
    })
    .prefault({}),
});
export type Preferences = z.infer<typeof preferencesSchema>;
export type NotificationRule = z.infer<typeof notificationRuleSchema>;

export const displayScopesSchema = z.object({
  providers: z.partialRecord(providerSchema, displayScopeSchema).default({}),
  targets: z
    .record(z.string().min(1).max(1600), displayScopeSchema)
    .default({}),
});
export type DisplayScopes = z.infer<typeof displayScopesSchema>;

const activityIdentifier = z
  .string()
  .refine(
    (value) =>
      value.trim().length > 0 &&
      !value.includes("\0") &&
      new TextEncoder().encode(value).length <= 512,
    "Invalid activity identifier",
  );
export const activityKeySchema = z.object({
  provider: providerSchema,
  targetId: activityIdentifier,
  activityId: activityIdentifier,
});
export type ActivityKey = z.infer<typeof activityKeySchema>;
export const activityKeyString = (key: ActivityKey) =>
  JSON.stringify([key.provider, key.targetId, key.activityId]);
export const activityIdentity = (activity: Activity): ActivityKey => ({
  provider: activity.provider,
  targetId: activity.targetId ?? "",
  activityId: activity.id,
});
export const isHiddenActivity = (activity: Activity, settings: Settings) =>
  settings.hiddenActivities.some(
    (key) =>
      key.provider === activity.provider &&
      key.targetId === activity.targetId &&
      key.activityId === activity.id,
  );

export const settingsSchema = z.object({
  hiddenActivities: z.array(activityKeySchema).max(10000).default([]),
  scope: displayScopeSchema,
  displayScopes: displayScopesSchema.prefault({}),
  historyHours: z.union([z.literal(24), z.literal(168)]),
  targets: z.array(targetSchema),
  accounts: z.array(accountSchema),
  panel: panelSettingsSchema.prefault({}),
  preferences: preferencesSchema.prefault({}),
});
export const connectionSchema = z.object({
  connectionId: z.string(),
  provider: providerSchema,
  status: z.enum([
    "connected",
    "signed-out",
    "missing-cli",
    "error",
    "unconfigured",
  ]),
  account: optionalText,
  message: optionalText,
  cliVersion: optionalText,
});
export const activitySchema = z.object({
  id: z.string(),
  connectionId: z.string().min(1),
  targetId: z.string().optional(),
  provider: providerSchema,
  kind: z.string(),
  title: z.string(),
  workflowName: optionalText,
  project: z.string(),
  branch: optionalText,
  url: z.string(),
  status: statusSchema,
  rawStatus: z.string(),
  actor: optionalText,
  ownership: z.enum(["mine", "other", "unknown"]),
  startedAt: date,
  finishedAt: date.nullish(),
  updatedAt: date,
});
export const issueSchema = z.object({
  connectionId: optionalText,
  provider: providerSchema,
  message: z.string(),
  targetId: optionalText,
});
export const targetHistorySchema = z.object({
  connectionId: z.string(),
  targetId: z.string(),
  provider: providerSchema,
  nextCursor: z.string().nullish(),
  checkedAt: date,
  generation: z.number().int().nonnegative(),
});
export type TargetHistory = z.infer<typeof targetHistorySchema>;
export const queryHealthSchema = z.object({
  provider: providerSchema,
  connectionId: z.string().min(1),
  targetId: z.string().min(1),
  status: z.enum(["checking", "success", "partial", "error"]),
  lastAttemptAt: date,
  lastSuccessfulAt: date.nullish(),
});
export type QueryHealth = z.infer<typeof queryHealthSchema>;
export const snapshotSchema = z.object({
  settings: settingsSchema,
  settingsError: z.string().nullable().optional(),
  connections: z.array(connectionSchema),
  activities: z.array(activitySchema),
  issues: z.array(issueSchema),
  refreshing: z.boolean(),
  history: z.array(targetHistorySchema).default([]),
  pendingTargets: z.array(z.string()).default([]),
  queryHealth: z.array(queryHealthSchema).default([]),
  pollIntervalSeconds: z.number().int().positive().max(3600).default(120),
  lastUpdatedAt: date.nullish(),
});
export const discoverySchema = z.object({
  connection: connectionSchema,
  targets: z.array(targetSchema),
  awsProfiles: z.array(z.string()),
  issues: z.array(issueSchema),
});
export const loginSchema = z.object({
  connectionId: z.string(),
  provider: providerSchema,
  state: z.enum(["waiting", "complete", "error"]),
  message: z.string(),
  url: optionalText,
  code: optionalText,
});
export type Provider = z.infer<typeof providerSchema>;
export type Account = z.infer<typeof accountSchema>;
export type Activity = z.infer<typeof activitySchema>;
export type Target = z.infer<typeof targetSchema>;
export type Settings = z.infer<typeof settingsSchema>;
export type Snapshot = z.infer<typeof snapshotSchema>;
export type Connection = z.infer<typeof connectionSchema>;
export type Discovery = z.infer<typeof discoverySchema>;
export type LoginStatus = z.infer<typeof loginSchema>;

export function effectiveDisplayScope(
  settings: Settings,
  source: {
    provider: Provider;
    connectionId?: string;
    targetId?: string;
  },
): "mine" | "all" {
  const targetScope =
    source.connectionId && source.targetId
      ? settings.displayScopes.targets[
          `${source.provider}:${source.connectionId}:${source.targetId}`
        ]
      : undefined;
  return (
    targetScope ??
    settings.displayScopes.providers[source.provider] ??
    settings.scope
  );
}
export const PROVIDER_NAMES: Record<Provider, string> = {
  github: "GitHub Actions",
  vercel: "Vercel",
  aws: "AWS",
  "github-deployments": "GitHub Deployments",
  railway: "Railway",
  netlify: "Netlify",
  render: "Render",
  "cloudflare-pages": "Cloudflare Pages",
  "cloudflare-workers": "Cloudflare Workers",
  heroku: "Heroku",
  fly: "Fly.io",
  gitlab: "GitLab",
  bitbucket: "Bitbucket",
  digitalocean: "DigitalOcean",
  gcloud: "Google Cloud",
  azure: "Azure App Service",
  eas: "Expo / EAS",
  firebase: "Firebase",
  supabase: "Supabase",
  fastlane: "fastlane",
};
export const STATUS_LABELS: Record<Activity["status"], string> = {
  queued: "대기 중",
  running: "실행 중",
  waiting: "승인 대기",
  success: "성공",
  failed: "실패",
  canceled: "취소",
  skipped: "건너뜀",
  unknown: "상태 미확인",
};
export const isActive = (activity: Activity) =>
  ["queued", "running", "waiting"].includes(activity.status) &&
  !(
    activity.provider === "vercel" &&
    activity.rawStatus.toUpperCase() === "BLOCKED"
  );

export const isVisibleActivity = (activity: Activity, _now = Date.now()) =>
  !(
    activity.provider === "vercel" &&
    activity.rawStatus.toUpperCase() === "BLOCKED"
  );

type DecoratedActivity = {
  activity: Activity;
  eligible: boolean;
  index: number;
  key: string;
};

type ActivityGroup = {
  eligibleMine: boolean;
  firstIndex: number;
  key: string;
  latest: Activity;
  representative?: Activity;
};

function activityGroup(item: DecoratedActivity): ActivityGroup {
  return {
    eligibleMine: item.eligible && item.activity.ownership === "mine",
    firstIndex: item.index,
    key: item.key,
    latest: item.activity,
    representative: item.eligible ? item.activity : undefined,
  };
}

function addToActivityGroup(
  group: ActivityGroup,
  item: DecoratedActivity,
): ActivityGroup {
  return {
    ...group,
    eligibleMine:
      group.eligibleMine ||
      (item.eligible && item.activity.ownership === "mine"),
    latest: newerActivity(group.latest, item.activity),
    representative: item.eligible
      ? group.representative
        ? newerActivity(group.representative, item.activity)
        : item.activity
      : group.representative,
  };
}

function reconcileActivities(
  activities: Activity[],
  settings: Settings,
): Array<{ activity: Activity; firstIndex: number }> {
  const decorated = activities
    .map((activity, index) => ({
      activity,
      eligible:
        effectiveDisplayScope(settings, activity) === "all" ||
        activity.ownership === "mine",
      index,
      key: JSON.stringify([
        activity.provider,
        activity.targetId ?? "",
        activity.id,
      ]),
    }))
    .sort((a, b) =>
      a.key === b.key ? a.index - b.index : a.key < b.key ? -1 : 1,
    );
  const groups: ActivityGroup[] = [];
  let current: ActivityGroup | undefined;
  for (const item of decorated) {
    if (!current || current.key !== item.key) {
      if (current) groups.push(current);
      current = activityGroup(item);
    } else {
      current = addToActivityGroup(current, item);
    }
  }
  if (current) groups.push(current);

  return groups.flatMap(
    ({ eligibleMine, firstIndex, latest, representative }) =>
      representative
        ? [
            {
              activity: {
                ...representative,
                status: latest.status,
                rawStatus: latest.rawStatus,
                finishedAt: latest.finishedAt,
                updatedAt: latest.updatedAt,
                ownership: eligibleMine
                  ? ("mine" as const)
                  : representative.ownership,
              },
              firstIndex,
            },
          ]
        : [],
  );
}

export function visibleActivities(
  snapshot: Snapshot,
  provider: Provider | "all",
  search: string,
  now = Date.now(),
  connectionId = "all",
) {
  const query = search.trim().toLocaleLowerCase();
  const cutoff = now - snapshot.settings.historyHours * 60 * 60 * 1000;
  const candidates = snapshot.activities.filter(
    (activity) =>
      !isHiddenActivity(activity, snapshot.settings) &&
      (connectionId === "all" || activity.connectionId === connectionId) &&
      (provider === "all" || activity.provider === provider),
  );
  const reconciled = reconcileActivities(candidates, snapshot.settings);
  const filtered = reconciled.filter(
    ({ activity }) =>
      isVisibleActivity(activity, now) &&
      (isActive(activity) ||
        activity.status === "unknown" ||
        Date.parse(activity.updatedAt) >= cutoff) &&
      (!query ||
        [
          activity.title,
          activity.project,
          activity.branch,
          activity.actor,
          activity.workflowName,
        ].some((part) => part?.toLocaleLowerCase().includes(query))),
  );
  return filtered
    .sort(
      (a, b) =>
        Date.parse(b.activity.updatedAt) - Date.parse(a.activity.updatedAt) ||
        a.firstIndex - b.firstIndex,
    )
    .map(({ activity }) => activity);
}

function newerActivity(current: Activity, candidate: Activity): Activity {
  const delta = Date.parse(candidate.updatedAt) - Date.parse(current.updatedAt);
  const terminal = (item: Activity) =>
    !isActive(item) && item.status !== "unknown";
  if (delta > 0 || (delta === 0 && terminal(candidate) && !terminal(current))) {
    return candidate;
  }
  if (
    delta === 0 &&
    terminal(candidate) === terminal(current) &&
    candidate.ownership === "mine"
  ) {
    return candidate;
  }
  return current;
}

export const targetKey = (target: Target) =>
  `${target.provider}:${target.connectionId}:${target.id}`;

export function timeAgo(value: string, now = Date.now()): string {
  const seconds = Math.max(0, Math.floor((now - Date.parse(value)) / 1000));
  if (seconds < 60) return "방금";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}분 전`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}시간 전`;
  return `${Math.floor(seconds / 86400)}일 전`;
}

export function duration(activity: Activity, now = Date.now()): string {
  const end = activity.finishedAt
    ? Date.parse(activity.finishedAt)
    : isActive(activity)
      ? now
      : null;
  if (end === null) return "—";
  const seconds = Math.max(
    0,
    Math.floor((end - Date.parse(activity.startedAt)) / 1000),
  );
  if (seconds < 60) return `${seconds}초`;
  return `${Math.floor(seconds / 60)}분 ${seconds % 60}초`;
}

export function panelActivities(snapshot: Snapshot, now = Date.now()) {
  const { panel } = snapshot.settings;
  const hiddenTargets = new Set(panel.hiddenTargets);
  const providerTargets = new Set(
    snapshot.settings.targets
      .filter((target) => !hiddenTargets.has(targetKey(target)))
      .map((target) =>
        JSON.stringify([target.provider, target.connectionId, target.id]),
      ),
  );
  const activities = snapshot.activities.filter(
    (activity) =>
      panel.providers.includes(activity.provider) &&
      providerTargets.has(
        JSON.stringify([
          activity.provider,
          activity.connectionId,
          activity.targetId,
        ]),
      ),
  );
  return sortPanelActivities(
    visibleActivities({ ...snapshot, activities }, "all", "", now),
    panel,
  );
}

export function sortPanelActivities(
  activities: Activity[],
  _panel: PanelSettings,
): Activity[] {
  return [...activities].sort(
    (a, b) => Date.parse(b.updatedAt) - Date.parse(a.updatedAt),
  );
}

export function latestCompleted(activities: Activity[]): Activity[] {
  const decorated = activities
    .filter((item) => !isActive(item) && item.status !== "unknown")
    .map((activity, index) => ({
      activity,
      index,
      // Account and stable target IDs prevent same-name projects from merging.
      key: JSON.stringify([
        activity.provider,
        activity.connectionId,
        activity.targetId,
        activity.kind,
        activity.branch,
        activity.provider === "github" ? activity.title : "",
      ]),
    }))
    .sort((a, b) =>
      a.key === b.key ? a.index - b.index : a.key < b.key ? -1 : 1,
    );
  const groups: Array<{
    firstIndex: number;
    key: string;
    latest: Activity;
  }> = [];
  let current: (typeof groups)[number] | undefined;
  for (const item of decorated) {
    if (!current || current.key !== item.key) {
      if (current) groups.push(current);
      current = {
        firstIndex: item.index,
        key: item.key,
        latest: item.activity,
      };
    } else if (
      Date.parse(item.activity.updatedAt) > Date.parse(current.latest.updatedAt)
    ) {
      current = { ...current, latest: item.activity };
    }
  }
  if (current) groups.push(current);
  return groups
    .sort((a, b) => a.firstIndex - b.firstIndex)
    .map(({ latest }) => latest);
}
