#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const args = process.argv.slice(2);
const schemaOnly = args.includes("--schema-only");
const platformArg = args.find((arg) => arg.startsWith("--platform="));

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const srcTauriDir = path.resolve(scriptDir, "..");
const repoRoot = path.resolve(srcTauriDir, "..");
const manifestPath = path.join(srcTauriDir, "engine-manifest.json");

function readManifest() {
  return JSON.parse(fs.readFileSync(manifestPath, "utf8"));
}

function resolveDestination(destination) {
  return path.isAbsolute(destination)
    ? destination
    : path.resolve(repoRoot, destination);
}

function sha256File(filePath) {
  const hash = crypto.createHash("sha256");
  hash.update(fs.readFileSync(filePath));
  return hash.digest("hex");
}

function isExecutable(filePath) {
  try {
    fs.accessSync(filePath, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function validateAssetRecord(asset, manifest, errors) {
  for (const field of manifest.assetRecordRequiredFields) {
    if (!asset[field]) {
      errors.push(`Asset record is missing required field: ${field}`);
    }
  }

  const desktopPlatforms = manifest.platformMatrix.desktopFullEdition;
  const mobilePlatforms = manifest.platformMatrix.mobileLiteEditionFutureOnly;
  const allPlatforms = new Set([...desktopPlatforms, ...mobilePlatforms]);
  if (asset.platform && !allPlatforms.has(asset.platform)) {
    errors.push(`Unknown asset platform: ${asset.platform}`);
  }

  if (mobilePlatforms.includes(asset.platform)) {
    errors.push(`Full bundled engine assets are desktop-only: ${asset.platform}`);
  }

  if (asset.type && !manifest.allowedAssetTypes.includes(asset.type)) {
    errors.push(`Unknown asset type for ${asset.name}: ${asset.type}`);
  }

  if (asset.sha256 && !/^[a-f0-9]{64}$/i.test(asset.sha256)) {
    errors.push(`Invalid sha256 for ${asset.name} on ${asset.platform}`);
  }
}

function verifyAssetFile(asset, errors, warnings) {
  const destination = resolveDestination(asset.destination);
  if (!fs.existsSync(destination)) {
    errors.push(`Missing asset: ${asset.destination}`);
    return;
  }

  const stat = fs.statSync(destination);
  if (asset.type === "runtime-folder") {
    if (!stat.isDirectory()) {
      errors.push(`Expected runtime folder: ${asset.destination}`);
      return;
    }
    if (fs.readdirSync(destination).length === 0) {
      errors.push(`Runtime folder is empty: ${asset.destination}`);
    }
    warnings.push(`Folder hash is registry metadata only for ${asset.destination}; verify the source archive before unpacking.`);
    return;
  }

  if (!stat.isFile()) {
    errors.push(`Expected file asset: ${asset.destination}`);
    return;
  }

  const actualHash = sha256File(destination);
  if (actualHash.toLowerCase() !== asset.sha256.toLowerCase()) {
    errors.push(`sha256 mismatch for ${asset.destination}`);
  }

  const executablePlatform = asset.platform.startsWith("macos-") || asset.platform.startsWith("linux-");
  if (asset.type === "sidecar" && executablePlatform && !isExecutable(destination)) {
    errors.push(`Sidecar is not executable: ${asset.destination}`);
  }
}

function requiredPlatforms(manifest) {
  if (!platformArg) {
    return manifest.v1EnabledPlatforms;
  }

  const platform = platformArg.slice("--platform=".length);
  return [platform];
}

function verifyRequiredCoverage(manifest, platforms, errors) {
  const assets = Array.isArray(manifest.assets) ? manifest.assets : [];
  for (const platform of platforms) {
    for (const engine of manifest.requiredEngines) {
      for (const type of engine.requiredAssetTypes || []) {
        const hasAsset = assets.some((asset) => (
          asset.platform === platform &&
          asset.name === engine.name &&
          asset.type === type
        ));
        if (!hasAsset) {
          errors.push(`Missing ${engine.name} ${type} asset record for ${platform}`);
        }
      }
    }
  }
}

function main() {
  const manifest = readManifest();
  const errors = [];
  const warnings = [];
  const assets = Array.isArray(manifest.assets) ? manifest.assets : [];

  if (manifest.rules.fullBundledEngineEdition !== "desktop-only") {
    errors.push("Manifest must keep fullBundledEngineEdition as desktop-only.");
  }

  for (const asset of assets) {
    validateAssetRecord(asset, manifest, errors);
  }

  if (!schemaOnly) {
    verifyRequiredCoverage(manifest, requiredPlatforms(manifest), errors);
    for (const asset of assets) {
      verifyAssetFile(asset, errors, warnings);
    }
  }

  for (const warning of warnings) {
    console.warn(`warning: ${warning}`);
  }

  if (errors.length > 0) {
    for (const error of errors) {
      console.error(`error: ${error}`);
    }
    process.exit(1);
  }

  console.log(schemaOnly ? "Engine manifest schema check passed." : "Engine verification passed.");
}

main();
