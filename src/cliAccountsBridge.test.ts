import { beforeEach, expect, test, vi } from "vitest";
const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri: () => true }));
beforeEach(() => invoke.mockReset());

test("CLI account binding rejects unsafe usernames before IPC", async () => {
  const { bridge } = await import("./bridge");
  for (const username of ["", "--token", "user/name", "user\nname"]) {
    await expect(
      bridge.connectGithubCliAccount("github", "github:local", username),
    ).rejects.toThrow();
  }
  expect(invoke).not.toHaveBeenCalled();
});

test("CLI username and connection are explicit in the binding and response", async () => {
  const { bridge } = await import("./bridge");
  const account = {
    id: "github:local",
    provider: "github",
    label: "Example",
    mode: "local",
    cliUsername: "example-user",
    profile: "42",
  };
  invoke.mockResolvedValue(account);
  await expect(
    bridge.connectGithubCliAccount("github", "github:local", "example-user"),
  ).resolves.toEqual(account);
  expect(invoke).toHaveBeenCalledWith("connect_github_cli_account", {
    provider: "github",
    connectionId: "github:local",
    username: "example-user",
    allowIdentityChange: false,
  });
  await bridge.connectGithubCliAccount(
    "github",
    "github:local",
    "example-user",
    true,
  );
  expect(invoke).toHaveBeenLastCalledWith("connect_github_cli_account", {
    provider: "github",
    connectionId: "github:local",
    username: "example-user",
    allowIdentityChange: true,
  });
  invoke.mockResolvedValue(["valid-user", "invalid user"]);
  await expect(bridge.githubCliAccounts()).rejects.toThrow();
});
