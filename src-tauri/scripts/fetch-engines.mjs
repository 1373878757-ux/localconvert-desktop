#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

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

function ensureDirForAsset(asset) {
  const destination = resolveDestination(asset.destination);
  const directory = asset.type === "runtime-folder"
    ? destination
    : path.dirname(destination);
  fs.mkdirSync(directory, { recursive: true });
}

function ensureStandardLayout(manifest) {
  const desktopPlatforms = manifest.platformMatrix.desktopFullEdition;
  const engines = manifest.requiredEngines.map((engine) => engine.name);

  for (const platform of desktopPlatforms) {
    fs.mkdirSync(path.join(srcTauriDir, "binaries", platform), { recursive: true });
  }

  for (const engine of engines) {
    for (const platform of desktopPlatforms) {
      fs.mkdirSync(path.join(srcTauriDir, "resources", "engines", engine, platform), { recursive: true });
    }
  }

  fs.mkdirSync(path.join(srcTauriDir, "resources", "fonts"), { recursive: true });
  fs.mkdirSync(path.join(srcTauriDir, "resources", "licenses"), { recursive: true });
}

function main() {
  const manifest = readManifest();
  const assets = Array.isArray(manifest.assets) ? manifest.assets : [];

  ensureStandardLayout(manifest);

  if (assets.length === 0) {
    console.log("No engine assets are recorded in src-tauri/engine-manifest.json.");
    console.log("Standard engine asset directories have been created from the platform matrix.");
    console.log("Add real engine asset records before running release packaging.");
    return;
  }

  for (const asset of assets) {
    ensureDirForAsset(asset);
    console.log(`${asset.platform} ${asset.name}`);
    console.log(`  source: ${asset.source}`);
    console.log(`  destination: ${asset.destination}`);
  }
}

main();
