import { z } from "zod";
import { issueKindSchema } from "./domain";
export const githubProviderSchema = z.enum(["github", "github-deployments"]);
export type GithubProvider = z.infer<typeof githubProviderSchema>;
export const isGithubProvider = (value: string): value is GithubProvider =>
  value === "github" || value === "github-deployments";
export const githubUsernameSchema = z
  .string()
  .min(1)
  .max(39)
  .regex(/^[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?$/)
  .refine((username) => !username.includes("--"));
export const githubCliBindingSchema = z.object({
  provider: githubProviderSchema,
  connectionId: z.string().min(1).max(128),
  username: githubUsernameSchema,
  allowIdentityChange: z.boolean(),
});
export const githubCliAccountSchema = z.object({
  login: githubUsernameSchema,
  active: z.boolean(),
  state: z.enum(["ok", "signedOut", "error"]),
});
export type GithubCliAccount = z.infer<typeof githubCliAccountSchema>;
export const githubAccountAccessSchema = githubCliAccountSchema.extend({
  canAccessAll: z.boolean(),
  inaccessibleTargets: z.array(z.string().min(1).max(512)).max(500),
  checkFailed: issueKindSchema.nullish(),
  identityMismatch: z.boolean(),
  usedByConnectionId: z.string().min(1).max(128).nullish(),
});
export type GithubAccountAccess = z.infer<typeof githubAccountAccessSchema>;
export const connectionIdSchema = z
  .string()
  .min(1)
  .max(128)
  .regex(/^[A-Za-z0-9:-]+$/);

/**
 * Accounts that can read every target of a connection and are free to use.
 * An account of a different GitHub user than the saved one (`identityMismatch`)
 * may be chosen only after the user confirms the identity change.
 */
export const accessibleAccounts = (results: readonly GithubAccountAccess[]) =>
  results.filter(
    (item) =>
      item.state === "ok" &&
      !item.usedByConnectionId &&
      !item.checkFailed &&
      item.inaccessibleTargets.length === 0 &&
      (item.canAccessAll || item.identityMismatch),
  );
