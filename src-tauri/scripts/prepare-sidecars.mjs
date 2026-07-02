#!/usr/bin/env node

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

function sidecarTargetPath(asset) {
  const source = resolveDestination(asset.destination);
  const extension = asset.platform.startsWith("windows-") ? ".exe" : "";
  const triple = targetTriples[asset.platform];
  if (!triple) {
    throw new Error(`No target triple mapping for ${asset.platform}`);
  }

  const parsed = path.parse(source);
  const baseName = parsed.ext === ".exe" ? parsed.name : parsed.base;
  return path.join(parsed.dir, `${baseName}-${triple}${extension}`);
}

function main() {
  const manifest = readManifest();
  const sidecars = (manifest.assets || []).filter((asset) => asset.type === "sidecar");

  if (sidecars.length === 0) {
    console.log("No sidecar assets are recorded in src-tauri/engine-manifest.json.");
    return;
  }

  for (const asset of sidecars) {
    const source = resolveDestination(asset.destination);
    const target = sidecarTargetPath(asset);

    if (!fs.existsSync(source)) {
      throw new Error(`Missing sidecar source: ${asset.destination}`);
    }

    if (source === target) {
      console.log(`Already prepared: ${path.relative(repoRoot, target)}`);
      continue;
    }

    fs.copyFileSync(source, target);
    console.log(`Prepared sidecar: ${path.relative(repoRoot, target)}`);
  }
}

main();
