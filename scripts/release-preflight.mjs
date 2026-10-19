import { appendFileSync, existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const defaultRepositoryRoot = resolve(scriptDirectory, "..");

const matrices = Object.freeze({
  macos: Object.freeze([
    Object.freeze({
      name: "macOS Intel",
      runner: "macos-15-intel",
      platform: "macos",
      target: "x86_64-apple-darwin",
      args: "--target x86_64-apple-darwin",
    }),
    Object.freeze({
      name: "macOS Apple Silicon",
      runner: "macos-15",
      platform: "macos",
      target: "aarch64-apple-darwin",
      args: "--target aarch64-apple-darwin",
    }),
  ]),
  windows: Object.freeze([
    Object.freeze({
      name: "Windows x64",
      runner: "windows-2022",
      platform: "windows",
      target: "x86_64-pc-windows-msvc",
      args: "--target x86_64-pc-windows-msvc",
    }),
  ]),
});

const commonRequired = Object.freeze([
  "TAURI_SIGNING_PRIVATE_KEY",
  "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
  "RATELLINE_UPDATER_PUBLIC_KEY",
]);

const platformRequired = Object.freeze({
  macos: Object.freeze([
    "APPLE_CERTIFICATE",
    "APPLE_CERTIFICATE_PASSWORD",
    "APPLE_SIGNING_IDENTITY",
    "APPLE_TEAM_ID",
    "APPLE_API_ISSUER",
    "APPLE_API_KEY",
  ]),
  windows: Object.freeze([
    "WINDOWS_CERTIFICATE",
    "WINDOWS_CERTIFICATE_PASSWORD",
    "WINDOWS_CERTIFICATE_THUMBPRINT",
    "WINDOWS_TIMESTAMP_URL",
  ]),
});

function nonempty(value) {
  return typeof value === "string" && value.trim().length > 0;
}

function selectedPlatforms(scope) {
  if (scope === "macos-and-windows") {
    return ["macos", "windows"];
  }
  if (scope === "macos" || scope === "windows") {
    return [scope];
  }
  throw new Error(`Unsupported release scope: ${scope || "(empty)"}`);
}

function requireNames(names, environment, errors) {
  for (const name of names) {
    if (!nonempty(environment[name])) {
      errors.push(`${name} is required`);
    }
  }
}

function validBase64Pfx(value) {
  if (!nonempty(value)) {
    return false;
  }
  const normalized = value.replace(/\s/g, "");
  if (
    !/^[A-Za-z0-9+/]+={0,2}$/.test(normalized) ||
    normalized.length % 4 !== 0
  ) {
    return false;
  }
  const decoded = Buffer.from(normalized, "base64");
  return decoded.length > 4 && decoded[0] === 0x30;
}

function applePrivateKey(environment) {
  if (nonempty(environment.APPLE_API_PRIVATE_KEY)) {
    return environment.APPLE_API_PRIVATE_KEY;
  }
  if (
    nonempty(environment.APPLE_API_KEY_PATH) &&
    existsSync(environment.APPLE_API_KEY_PATH)
  ) {
    return readFileSync(environment.APPLE_API_KEY_PATH, "utf8");
  }
  return "";
}

function validateMacEnvironment(environment, errors) {
  if (
    nonempty(environment.APPLE_CERTIFICATE) &&
    !validBase64Pfx(environment.APPLE_CERTIFICATE)
  ) {
    errors.push("APPLE_CERTIFICATE must contain base64-encoded PKCS#12 data");
  }
  if (
    nonempty(environment.APPLE_SIGNING_IDENTITY) &&
    !environment.APPLE_SIGNING_IDENTITY.startsWith("Developer ID Application:")
  ) {
    errors.push(
      "APPLE_SIGNING_IDENTITY must name a Developer ID Application certificate",
    );
  }
  if (
    nonempty(environment.APPLE_TEAM_ID) &&
    !/^[A-Z0-9]{10}$/.test(environment.APPLE_TEAM_ID)
  ) {
    errors.push("APPLE_TEAM_ID must be a 10-character Apple team identifier");
  }
  if (
    nonempty(environment.APPLE_SIGNING_IDENTITY) &&
    nonempty(environment.APPLE_TEAM_ID) &&
    !environment.APPLE_SIGNING_IDENTITY.includes(
      `(${environment.APPLE_TEAM_ID})`,
    )
  ) {
    errors.push("APPLE_SIGNING_IDENTITY does not match APPLE_TEAM_ID");
  }
  if (
    nonempty(environment.APPLE_API_ISSUER) &&
    !/^[0-9a-f-]{36}$/i.test(environment.APPLE_API_ISSUER)
  ) {
    errors.push("APPLE_API_ISSUER must be an App Store Connect issuer UUID");
  }
  if (
    nonempty(environment.APPLE_API_KEY) &&
    !/^[A-Z0-9]{10}$/.test(environment.APPLE_API_KEY)
  ) {
    errors.push(
      "APPLE_API_KEY must be a 10-character App Store Connect key ID",
    );
  }
  const privateKey = applePrivateKey(environment).trim();
  if (!privateKey) {
    errors.push("APPLE_API_PRIVATE_KEY or APPLE_API_KEY_PATH is required");
  } else if (
    !/^-----BEGIN PRIVATE KEY-----[\s\S]+-----END PRIVATE KEY-----$/.test(
      privateKey,
    )
  ) {
    errors.push("The Apple API private key must be PEM-encoded PKCS#8 data");
  }
}

function validateWindowsEnvironment(environment, errors) {
  if (
    nonempty(environment.WINDOWS_CERTIFICATE) &&
    !validBase64Pfx(environment.WINDOWS_CERTIFICATE)
  ) {
    errors.push("WINDOWS_CERTIFICATE must contain base64-encoded PKCS#12 data");
  }
  if (
    nonempty(environment.WINDOWS_CERTIFICATE_THUMBPRINT) &&
    !/^[A-F0-9]{40}$/i.test(
      environment.WINDOWS_CERTIFICATE_THUMBPRINT.replace(/\s/g, ""),
    )
  ) {
    errors.push(
      "WINDOWS_CERTIFICATE_THUMBPRINT must be a 40-character SHA-1 thumbprint",
    );
  }
  if (nonempty(environment.WINDOWS_TIMESTAMP_URL)) {
    try {
      const url = new URL(environment.WINDOWS_TIMESTAMP_URL);
      if (url.protocol !== "https:") {
        errors.push("WINDOWS_TIMESTAMP_URL must use HTTPS");
      }
    } catch {
      errors.push("WINDOWS_TIMESTAMP_URL must be a valid URL");
    }
  }
}

export function matrixForScope(scope) {
  const include = selectedPlatforms(scope).flatMap(
    (platform) => matrices[platform],
  );
  return { include };
}

export function validateReleaseEnvironment(scope, environment) {
  const platforms = selectedPlatforms(scope);
  const errors = [];
  requireNames(commonRequired, environment, errors);
  for (const platform of platforms) {
    requireNames(platformRequired[platform], environment, errors);
  }
  if (platforms.includes("macos")) {
    validateMacEnvironment(environment, errors);
  }
  if (platforms.includes("windows")) {
    validateWindowsEnvironment(environment, errors);
  }
  if (errors.length > 0) {
    throw new Error(`Release preflight failed:\n- ${errors.join("\n- ")}`);
  }
  return platforms;
}

function cargoPackageVersion(contents) {
  const packageSection = contents.match(/\[package\]([\s\S]*?)(?=\n\[|$)/);
  const version = packageSection?.[1].match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  if (!version) {
    throw new Error("Could not read src-tauri/Cargo.toml package version");
  }
  return version;
}

export function validateReleaseVersions(
  repositoryRoot = defaultRepositoryRoot,
) {
  const packageJson = JSON.parse(
    readFileSync(resolve(repositoryRoot, "package.json"), "utf8"),
  );
  const tauriConfig = JSON.parse(
    readFileSync(
      resolve(repositoryRoot, "src-tauri", "tauri.conf.json"),
      "utf8",
    ),
  );
  const cargoVersion = cargoPackageVersion(
    readFileSync(resolve(repositoryRoot, "src-tauri", "Cargo.toml"), "utf8"),
  );
  const versions = [packageJson.version, tauriConfig.version, cargoVersion];
  if (versions.some((version) => version !== versions[0])) {
    throw new Error(
      "Release versions differ across package.json, tauri.conf.json, and Cargo.toml",
    );
  }
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(versions[0])) {
    throw new Error("Release version must be a valid semantic version");
  }
  return versions[0];
}

function argumentValue(name) {
  const index = process.argv.indexOf(name);
  return index === -1 ? undefined : process.argv[index + 1];
}

export function runPreflight({
  scope,
  environment = process.env,
  repositoryRoot = defaultRepositoryRoot,
  githubOutput,
}) {
  const platforms = validateReleaseEnvironment(scope, environment);
  const version = validateReleaseVersions(repositoryRoot);
  if (githubOutput) {
    appendFileSync(
      githubOutput,
      `matrix=${JSON.stringify(matrixForScope(scope))}\n`,
      "utf8",
    );
  }
  return { platforms, version };
}

const isMain =
  process.argv[1] &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url;
if (isMain) {
  try {
    const result = runPreflight({
      scope: argumentValue("--scope"),
      githubOutput: argumentValue("--github-output"),
    });
    process.stdout.write(
      `Release preflight passed for ${result.platforms.join(" and ")} at version ${result.version}.\n`,
    );
  } catch (error) {
    process.stderr.write(
      `${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 1;
  }
}
