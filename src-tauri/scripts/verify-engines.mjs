#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const targetTriples = {
  "windows-x86_64": "x86_64-pc-windows-msvc",
  "windows-aarch64": "aarch64-pc-windows-msvc",
  "macos-aarch64": "aarch64-apple-darwin",
  "macos-x86_64": "x86_64-apple-darwin",
  "linux-x86_64": "x86_64-unknown-linux-gnu"
};

const scriptPath = fileURLToPath(import.meta.url);
const scriptDir = path.dirname(scriptPath);
const srcTauriDir = path.resolve(scriptDir, "..");
const defaultRepoRoot = path.resolve(srcTauriDir, "..");
const defaultManifestPath = path.join(srcTauriDir, "engine-manifest.json");
const defaultTauriConfigPath = path.join(srcTauriDir, "tauri.conf.json");

function readManifest(manifestPath = defaultManifestPath) {
  return JSON.parse(fs.readFileSync(manifestPath, "utf8"));
}

function resolveDestination(destination, repoRoot) {
  return path.isAbsolute(destination)
    ? destination
    : path.resolve(repoRoot, destination);
}

export function sha256File(filePath) {
  const hash = crypto.createHash("sha256");
  hash.update(fs.readFileSync(filePath));
  return hash.digest("hex");
}

export function sidecarTargetPath(asset, repoRoot) {
  const source = resolveDestination(asset.destination, repoRoot);
  const triple = targetTriples[asset.platform];
  if (!triple) {
    return null;
  }

  const extension = asset.platform.startsWith("windows-") ? ".exe" : "";
  const parsed = path.parse(source);
  const baseName = parsed.ext === ".exe" ? parsed.name : parsed.base;
  return path.join(parsed.dir, `${baseName}-${triple}${extension}`);
}

function isExecutable(filePath) {
  try {
    fs.accessSync(filePath, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function normalizeOs(value) {
  const normalized = String(value || "").toLowerCase();
  if (normalized === "darwin" || normalized === "macos") return "macos";
  if (normalized === "win32" || normalized === "windows") return "windows";
  if (normalized === "linux") return "linux";
  if (normalized === "android") return "android";
  if (normalized === "ios") return "ios";
  return normalized || "unknown";
}

function normalizeArch(value) {
  const normalized = String(value || "").toLowerCase();
  if (normalized === "arm64" || normalized === "aarch64") return "aarch64";
  if (normalized === "x64" || normalized === "x86_64") return "x86_64";
  return normalized || "unknown";
}

export function currentPlatformKey(environment = process.env) {
  const os = normalizeOs(environment.TAURI_ENV_PLATFORM || process.platform);
  const arch = normalizeArch(environment.TAURI_ENV_ARCH || process.arch);
  return `${os}-${arch}`;
}

function validateAssetRecord(asset, manifest, errors) {
  for (const field of manifest.assetRecordRequiredFields || []) {
    if (!asset[field]) {
      errors.push(`Asset record is missing required field: ${field}`);
    }
  }

  const desktopPlatforms = manifest.platformMatrix?.desktopFullEdition || [];
  const mobilePlatforms = manifest.platformMatrix?.mobileLiteEditionFutureOnly || [];
  const allPlatforms = new Set([...desktopPlatforms, ...mobilePlatforms]);
  if (asset.platform && !allPlatforms.has(asset.platform)) {
    errors.push(`Unknown asset platform: ${asset.platform}`);
  }

  if (mobilePlatforms.includes(asset.platform)) {
    errors.push(`Full bundled engine assets are desktop-only: ${asset.platform}`);
  }

  if (asset.type && !(manifest.allowedAssetTypes || []).includes(asset.type)) {
    errors.push(`Unknown asset type for ${asset.name}: ${asset.type}`);
  }

  if (asset.sha256 && !/^[a-f0-9]{64}$/i.test(asset.sha256)) {
    errors.push(`Invalid sha256 for ${asset.name} on ${asset.platform}`);
  }
}

function validateBuildProfiles(manifest, errors) {
  const profiles = manifest.buildVerification?.requiredBundledAssets;
  const plannedOnlyEngines = manifest.buildVerification?.plannedOnlyEngines;
  if (!profiles || typeof profiles !== "object" || Array.isArray(profiles)) {
    errors.push("Manifest buildVerification.requiredBundledAssets must be an object.");
    return;
  }
  if (!Array.isArray(plannedOnlyEngines)) {
    errors.push("Manifest buildVerification.plannedOnlyEngines must be an array.");
    return;
  }

  const desktopPlatforms = new Set(manifest.platformMatrix?.desktopFullEdition || []);
  const assets = Array.isArray(manifest.assets) ? manifest.assets : [];
  for (const [platform, requirements] of Object.entries(profiles)) {
    if (!desktopPlatforms.has(platform)) {
      errors.push(`Bundled build profile is not a desktop platform: ${platform}`);
    }
    if (!Array.isArray(requirements) || requirements.length === 0) {
      errors.push(`Bundled build profile has no required assets: ${platform}`);
      continue;
    }

    for (const requirement of requirements) {
      if (!requirement.name || !requirement.type) {
        errors.push(`Bundled requirement is missing name or type for ${platform}`);
        continue;
      }
      if (plannedOnlyEngines.includes(requirement.name)) {
        errors.push(`Planned-only engine cannot be required for this build: ${requirement.name}`);
      }
      if (
        !Array.isArray(requirement.requiredLicenseFiles) ||
        requirement.requiredLicenseFiles.length === 0
      ) {
        errors.push(
          `Bundled requirement has no required license files: ${requirement.name} on ${platform}`
        );
      } else {
        for (const licensePath of requirement.requiredLicenseFiles) {
          if (
            typeof licensePath !== "string" ||
            !licensePath.startsWith("src-tauri/resources/licenses/")
          ) {
            errors.push(
              `Required license must be under src-tauri/resources/licenses: ${requirement.name}`
            );
          }
        }
      }

      const matchingAssets = assets.filter(
        (asset) =>
          asset.platform === platform &&
          asset.name === requirement.name &&
          asset.type === requirement.type
      );
      if (matchingAssets.length !== 1) {
        errors.push(
          `Expected exactly one ${requirement.name} ${requirement.type} asset record for ${platform}`
        );
      }
    }
  }
}

function validateManifestSchema(manifest, errors) {
  if (!manifest || typeof manifest !== "object") {
    errors.push("Engine manifest must be a JSON object.");
    return;
  }
  if (manifest.rules?.fullBundledEngineEdition !== "desktop-only") {
    errors.push("Manifest must keep fullBundledEngineEdition as desktop-only.");
  }
  if (!Array.isArray(manifest.assetRecordRequiredFields)) {
    errors.push("Manifest assetRecordRequiredFields must be an array.");
  }
  if (!Array.isArray(manifest.allowedAssetTypes)) {
    errors.push("Manifest allowedAssetTypes must be an array.");
  }
  if (!Array.isArray(manifest.assets)) {
    errors.push("Manifest assets must be an array.");
    return;
  }

  for (const asset of manifest.assets) {
    validateAssetRecord(asset, manifest, errors);
  }
  validateBuildProfiles(manifest, errors);
}

function verifyHashedFile(asset, filePath, label, errors) {
  if (!fs.existsSync(filePath)) {
    errors.push(`Missing ${label}: ${path.relative(defaultRepoRoot, filePath) || filePath}`);
    return;
  }

  const stat = fs.statSync(filePath);
  if (!stat.isFile()) {
    errors.push(`Expected file ${label}: ${filePath}`);
    return;
  }
  if (!/^[a-f0-9]{64}$/i.test(asset.sha256 || "")) {
    return;
  }

  const actualHash = sha256File(filePath);
  if (actualHash.toLowerCase() !== asset.sha256.toLowerCase()) {
    errors.push(`sha256 mismatch for ${asset.name} ${label}: ${filePath}`);
  }

  const executablePlatform =
    asset.platform.startsWith("macos-") || asset.platform.startsWith("linux-");
  if (asset.type === "sidecar" && executablePlatform && !isExecutable(filePath)) {
    errors.push(`Sidecar is not executable: ${filePath}`);
  }
}

function verifyAsset(asset, repoRoot, errors) {
  const source = resolveDestination(asset.destination, repoRoot);
  verifyHashedFile(asset, source, "asset", errors);

  if (asset.type !== "sidecar") {
    return;
  }
  const prepared = sidecarTargetPath(asset, repoRoot);
  if (!prepared) {
    errors.push(`No Tauri target triple mapping for ${asset.platform}`);
    return;
  }
  if (prepared !== source) {
    verifyHashedFile(asset, prepared, "prepared sidecar", errors);
  }
}

function verifyLicenseFile(licensePath, repoRoot, errors) {
  const resolved = resolveDestination(licensePath, repoRoot);
  if (!fs.existsSync(resolved)) {
    errors.push(`Missing required license file: ${licensePath}`);
    return;
  }

  const stat = fs.statSync(resolved);
  if (!stat.isFile()) {
    errors.push(`Required license path is not a file: ${licensePath}`);
  } else if (stat.size === 0) {
    errors.push(`Required license file is empty: ${licensePath}`);
  }
}

function verifyBuildProfile(manifest, platform, repoRoot, errors, warnings) {
  const profiles = manifest.buildVerification?.requiredBundledAssets || {};
  const requirements = profiles[platform];
  if (!requirements) {
    warnings.push(
      `No current bundled-engine build profile for ${platform}; future planned engines were not required.`
    );
    return { assetCount: 0, licenseCount: 0 };
  }

  const assets = Array.isArray(manifest.assets) ? manifest.assets : [];
  const verifiedAssets = new Set();
  const verifiedLicenses = new Set();
  for (const requirement of requirements) {
    const asset = assets.find(
      (candidate) =>
        candidate.platform === platform &&
        candidate.name === requirement.name &&
        candidate.type === requirement.type
    );
    if (!asset) {
      continue;
    }

    const assetKey = `${asset.platform}:${asset.name}:${asset.type}`;
    if (!verifiedAssets.has(assetKey)) {
      verifyAsset(asset, repoRoot, errors);
      verifiedAssets.add(assetKey);
    }

    for (const licensePath of requirement.requiredLicenseFiles || []) {
      if (!verifiedLicenses.has(licensePath)) {
        verifyLicenseFile(licensePath, repoRoot, errors);
        verifiedLicenses.add(licensePath);
      }
    }
  }

  const planned = manifest.buildVerification?.plannedOnlyEngines || [];
  if (planned.length > 0) {
    warnings.push(`Future planned engines skipped by this build profile: ${planned.join(", ")}`);
  }

  return {
    assetCount: verifiedAssets.size,
    licenseCount: verifiedLicenses.size
  };
}

function verifyPackagingConfiguration(manifest, platform, tauriConfig, errors) {
  const requirements =
    manifest.buildVerification?.requiredBundledAssets?.[platform];
  if (!requirements) {
    return;
  }

  const externalBins = tauriConfig?.bundle?.externalBin;
  if (!Array.isArray(externalBins)) {
    errors.push("Tauri bundle.externalBin must list required sidecars.");
  } else {
    for (const requirement of requirements) {
      if (requirement.type !== "sidecar") continue;
      const asset = manifest.assets.find(
        (candidate) =>
          candidate.platform === platform &&
          candidate.name === requirement.name &&
          candidate.type === requirement.type
      );
      if (!asset) continue;

      const expected = asset.destination.replace(/^src-tauri\//, "");
      if (!externalBins.includes(expected)) {
        errors.push(
          `Required sidecar is not registered in Tauri externalBin: ${expected}`
        );
      }
    }
  }

  const resources = tauriConfig?.bundle?.resources;
  if (
    !resources ||
    Array.isArray(resources) ||
    resources["resources/licenses/"] !== "licenses/"
  ) {
    errors.push(
      "Tauri resources must map resources/licenses/ to the packaged licenses/ directory."
    );
  }
  if (
    !resources ||
    Array.isArray(resources) ||
    resources["engine-manifest.json"] !== "engine-manifest.json"
  ) {
    errors.push("Tauri resources must package engine-manifest.json.");
  }
}

export function verifyManifest({
  manifest,
  repoRoot = defaultRepoRoot,
  platform = currentPlatformKey(),
  schemaOnly = false,
  tauriConfig = null
}) {
  const errors = [];
  const warnings = [];
  validateManifestSchema(manifest, errors);

  let summary = { assetCount: 0, licenseCount: 0 };
  if (!schemaOnly && errors.length === 0) {
    summary = verifyBuildProfile(manifest, platform, repoRoot, errors, warnings);
    if (tauriConfig) {
      verifyPackagingConfiguration(manifest, platform, tauriConfig, errors);
    }
  }

  return { errors, warnings, platform, schemaOnly, ...summary };
}

function parseOptions(args) {
  const schemaOnly = args.includes("--schema-only");
  const inlinePlatform = args.find((argument) => argument.startsWith("--platform="));
  const platformIndex = args.indexOf("--platform");
  const platform = inlinePlatform
    ? inlinePlatform.slice("--platform=".length)
    : platformIndex >= 0
      ? args[platformIndex + 1]
      : currentPlatformKey();
  return { schemaOnly, platform };
}

function main() {
  const options = parseOptions(process.argv.slice(2));
  const manifest = readManifest();
  const tauriConfig = JSON.parse(fs.readFileSync(defaultTauriConfigPath, "utf8"));
  const result = verifyManifest({ manifest, tauriConfig, ...options });

  for (const warning of result.warnings) {
    console.warn(`warning: ${warning}`);
  }
  if (result.errors.length > 0) {
    for (const error of result.errors) {
      console.error(`error: ${error}`);
    }
    process.exitCode = 1;
    return;
  }

  if (result.schemaOnly) {
    console.log("Engine manifest schema check passed.");
  } else {
    console.log(
      `Engine verification passed for ${result.platform}: ` +
        `${result.assetCount} required assets, ${result.licenseCount} required license files.`
    );
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === scriptPath) {
  main();
}
