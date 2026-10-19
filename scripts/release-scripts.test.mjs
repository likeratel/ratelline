import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import {
  assertAppleSignature,
  assertDeveloperIdSignature,
  findReleaseArtifacts,
} from "./release-artifacts.mjs";
import { releaseConfig } from "./release-config.mjs";
import {
  matrixForScope,
  validateReleaseEnvironment,
  validateReleaseVersions,
} from "./release-preflight.mjs";

const pfx = Buffer.from([0x30, 0x82, 0x01, 0x00, 0x01]).toString("base64");
const privateKey =
  "-----BEGIN PRIVATE KEY-----\nsynthetic-test-data\n-----END PRIVATE KEY-----";
const common = {
  TAURI_SIGNING_PRIVATE_KEY: "synthetic-updater-private-key",
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "synthetic-password",
  RATELLINE_UPDATER_PUBLIC_KEY: "synthetic-updater-public-key",
};
const mac = {
  ...common,
  APPLE_CERTIFICATE: pfx,
  APPLE_CERTIFICATE_PASSWORD: "synthetic-password",
  APPLE_SIGNING_IDENTITY:
    "Developer ID Application: Example Person (ABC1234567)",
  APPLE_TEAM_ID: "ABC1234567",
  APPLE_API_ISSUER: "12345678-1234-1234-1234-123456789abc",
  APPLE_API_KEY: "KEY1234567",
  APPLE_API_PRIVATE_KEY: privateKey,
};
const windows = {
  ...common,
  WINDOWS_CERTIFICATE: pfx,
  WINDOWS_CERTIFICATE_PASSWORD: "synthetic-password",
  WINDOWS_CERTIFICATE_THUMBPRINT: "0123456789abcdef0123456789abcdef01234567",
  WINDOWS_TIMESTAMP_URL: "https://timestamp.example.test/",
};

test("release scopes produce only their requested platform jobs", () => {
  assert.equal(matrixForScope("macos").include.length, 2);
  assert.equal(matrixForScope("windows").include.length, 1);
  assert.equal(matrixForScope("macos-and-windows").include.length, 3);
  assert.throws(() => matrixForScope("linux"), /Unsupported release scope/);
});

test("preflight fails closed without naming or printing secret values", () => {
  assert.deepEqual(validateReleaseEnvironment("macos", mac), ["macos"]);
  assert.deepEqual(validateReleaseEnvironment("windows", windows), ["windows"]);
  assert.throws(
    () =>
      validateReleaseEnvironment("macos", {
        ...mac,
        APPLE_CERTIFICATE_PASSWORD: "",
      }),
    (error) => {
      assert.match(error.message, /APPLE_CERTIFICATE_PASSWORD is required/);
      assert.doesNotMatch(error.message, /synthetic-password/);
      return true;
    },
  );
});

test("Windows release overlay enables updater and timestamped SHA-256 signing", () => {
  assert.deepEqual(releaseConfig("windows", windows), {
    bundle: {
      createUpdaterArtifacts: true,
      windows: {
        certificateThumbprint: "0123456789ABCDEF0123456789ABCDEF01234567",
        digestAlgorithm: "sha256",
        timestampUrl: "https://timestamp.example.test/",
        tsp: true,
      },
    },
  });
  assert.throws(
    () =>
      releaseConfig("windows", {
        ...windows,
        WINDOWS_TIMESTAMP_URL: "http://timestamp.test",
      }),
    /must use HTTPS/,
  );
});

test("version check requires one semantic version across all manifests", () => {
  const root = mkdtempSync(join(tmpdir(), "ratelline-release-version-"));
  try {
    mkdirSync(join(root, "src-tauri"));
    writeFileSync(join(root, "package.json"), '{"version":"1.2.3"}');
    writeFileSync(
      join(root, "src-tauri", "tauri.conf.json"),
      '{"version":"1.2.3"}',
    );
    writeFileSync(
      join(root, "src-tauri", "Cargo.toml"),
      '[package]\nversion = "1.2.3"\n',
    );
    assert.equal(validateReleaseVersions(root), "1.2.3");
    writeFileSync(
      join(root, "src-tauri", "tauri.conf.json"),
      '{"version":"1.2.4"}',
    );
    assert.throws(() => validateReleaseVersions(root), /versions differ/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("artifact discovery does not descend into app bundles", () => {
  const root = mkdtempSync(join(tmpdir(), "ratelline-release-artifacts-"));
  try {
    mkdirSync(join(root, "macos", "ratelline.app", "Contents"), {
      recursive: true,
    });
    mkdirSync(join(root, "dmg"), { recursive: true });
    mkdirSync(join(root, "nsis"), { recursive: true });
    writeFileSync(join(root, "dmg", "ratelline.dmg"), "synthetic");
    writeFileSync(join(root, "nsis", "ratelline-setup.exe"), "synthetic");
    writeFileSync(
      join(root, "macos", "ratelline.app", "Contents", "nested.exe"),
      "synthetic",
    );
    const artifacts = findReleaseArtifacts(root);
    assert.equal(artifacts.apps.length, 1);
    assert.equal(artifacts.dmgs.length, 1);
    assert.equal(artifacts.windowsInstallers.length, 1);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("Apple signature evidence rejects ad-hoc or mismatched identities", () => {
  const valid = [
    "Authority=Developer ID Application: Example Person (ABC1234567)",
    "TeamIdentifier=ABC1234567",
    "Timestamp=Oct 4, 2026 at 12:00:00 PM",
    "Runtime Version=26.0.0",
  ].join("\n");
  const expected = {
    identity: mac.APPLE_SIGNING_IDENTITY,
    teamId: mac.APPLE_TEAM_ID,
    requireRuntime: true,
    label: "test app",
  };
  assert.doesNotThrow(() => assertAppleSignature(valid, expected));
  assert.doesNotThrow(() =>
    assertDeveloperIdSignature(valid, {
      requireRuntime: true,
      label: "test app",
    }),
  );
  assert.throws(
    () => assertAppleSignature(`${valid}\nSignature=adhoc`, expected),
    /ad-hoc/,
  );
  assert.throws(
    () =>
      assertAppleSignature(
        valid.replace("Example Person", "Another Person"),
        expected,
      ),
    /wrong signing identity/,
  );
});
