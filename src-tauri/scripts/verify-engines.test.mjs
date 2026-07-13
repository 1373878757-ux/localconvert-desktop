import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  sha256File,
  sidecarTargetPath,
  verifyManifest
} from "./verify-engines.mjs";

const platform = "macos-aarch64";

function writeExecutable(filePath, contents) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, contents);
  fs.chmodSync(filePath, 0o755);
}

function createFixture(testContext) {
  const repoRoot = fs.mkdtempSync(
    path.join(os.tmpdir(), "localconvert-engine-verification-")
  );
  testContext.after(() => fs.rmSync(repoRoot, { recursive: true, force: true }));

  const qpdfDestination = "src-tauri/binaries/macos-aarch64/qpdf";
  const imageDestination = "src-tauri/binaries/macos-aarch64/image-engine";
  const qpdfPath = path.join(repoRoot, qpdfDestination);
  const imagePath = path.join(repoRoot, imageDestination);
  writeExecutable(qpdfPath, "qpdf-test-binary");
  writeExecutable(imagePath, "image-engine-test-binary");

  const assets = [
    {
      name: "qpdf",
      version: "12.3.2",
      platform,
      type: "sidecar",
      source: "test fixture",
      sha256: sha256File(qpdfPath),
      license: "Apache-2.0 test fixture",
      destination: qpdfDestination
    },
    {
      name: "image-engine",
      version: "0.3.0-preview.0",
      platform,
      type: "sidecar",
      source: "test fixture",
      sha256: sha256File(imagePath),
      license: "First-party test fixture",
      destination: imageDestination
    }
  ];

  for (const asset of assets) {
    const source = path.join(repoRoot, asset.destination);
    const prepared = sidecarTargetPath(asset, repoRoot);
    fs.copyFileSync(source, prepared);
    fs.chmodSync(prepared, 0o755);
  }

  const qpdfLicense =
    "src-tauri/resources/licenses/qpdf-12.3.2-LICENSE.txt";
  const imageNotice =
    "src-tauri/resources/licenses/image-engine-macos-aarch64-BUILD-NOTICE.txt";
  fs.mkdirSync(
    path.join(repoRoot, "src-tauri/resources/licenses"),
    { recursive: true }
  );
  fs.writeFileSync(path.join(repoRoot, qpdfLicense), "qpdf license");
  fs.writeFileSync(path.join(repoRoot, imageNotice), "image-engine notice");

  const manifest = {
    schemaVersion: 1,
    platformMatrix: {
      desktopFullEdition: [platform],
      mobileLiteEditionFutureOnly: []
    },
    rules: {
      fullBundledEngineEdition: "desktop-only"
    },
    buildVerification: {
      requiredBundledAssets: {
        [platform]: [
          {
            name: "qpdf",
            type: "sidecar",
            requiredLicenseFiles: [qpdfLicense]
          },
          {
            name: "image-engine",
            type: "sidecar",
            requiredLicenseFiles: [imageNotice]
          }
        ]
      },
      plannedOnlyEngines: ["libreoffice", "pdfium"]
    },
    assetRecordRequiredFields: [
      "name",
      "version",
      "platform",
      "type",
      "source",
      "sha256",
      "license",
      "destination"
    ],
    allowedAssetTypes: ["sidecar", "runtime-folder", "library", "font", "license"],
    assets
  };
  const tauriConfig = {
    bundle: {
      externalBin: [
        "binaries/macos-aarch64/qpdf",
        "binaries/macos-aarch64/image-engine"
      ],
      resources: {
        "engine-manifest.json": "engine-manifest.json",
        "resources/licenses/": "licenses/"
      }
    }
  };

  return {
    repoRoot,
    manifest,
    tauriConfig,
    qpdfPath,
    imagePath,
    qpdfLicense: path.join(repoRoot, qpdfLicense),
    imageNotice: path.join(repoRoot, imageNotice),
    preparedImagePath: sidecarTargetPath(assets[1], repoRoot)
  };
}

test("validates the manifest schema", (testContext) => {
  const fixture = createFixture(testContext);
  const result = verifyManifest({
    manifest: fixture.manifest,
    repoRoot: fixture.repoRoot,
    platform,
    schemaOnly: true,
    tauriConfig: fixture.tauriConfig
  });

  assert.deepEqual(result.errors, []);
});

test("accepts required assets, matching hashes, and required licenses", (testContext) => {
  const fixture = createFixture(testContext);
  const result = verifyManifest({
    manifest: fixture.manifest,
    repoRoot: fixture.repoRoot,
    platform,
    tauriConfig: fixture.tauriConfig
  });

  assert.deepEqual(result.errors, []);
  assert.equal(result.assetCount, 2);
  assert.equal(result.licenseCount, 2);
});

test("rejects a missing required bundled asset", (testContext) => {
  const fixture = createFixture(testContext);
  fs.rmSync(fixture.qpdfPath);

  const result = verifyManifest({
    manifest: fixture.manifest,
    repoRoot: fixture.repoRoot,
    platform,
    tauriConfig: fixture.tauriConfig
  });

  assert.ok(result.errors.some((error) => error.includes("Missing asset")));
});

test("rejects a missing required license file", (testContext) => {
  const fixture = createFixture(testContext);
  fs.rmSync(fixture.imageNotice);

  const result = verifyManifest({
    manifest: fixture.manifest,
    repoRoot: fixture.repoRoot,
    platform,
    tauriConfig: fixture.tauriConfig
  });

  assert.ok(
    result.errors.some((error) => error.includes("Missing required license file"))
  );
});

test("rejects a qpdf SHA-256 mismatch", (testContext) => {
  const fixture = createFixture(testContext);
  fs.writeFileSync(fixture.qpdfPath, "changed-qpdf-binary");

  const result = verifyManifest({
    manifest: fixture.manifest,
    repoRoot: fixture.repoRoot,
    platform,
    tauriConfig: fixture.tauriConfig
  });

  assert.ok(
    result.errors.some(
      (error) => error.includes("qpdf asset") && error.includes("sha256 mismatch")
    )
  );
});

test("rejects an image-engine prepared-sidecar SHA-256 mismatch", (testContext) => {
  const fixture = createFixture(testContext);
  fs.writeFileSync(fixture.preparedImagePath, "changed-image-engine-binary");

  const result = verifyManifest({
    manifest: fixture.manifest,
    repoRoot: fixture.repoRoot,
    platform,
    tauriConfig: fixture.tauriConfig
  });

  assert.ok(
    result.errors.some(
      (error) =>
        error.includes("image-engine prepared sidecar") &&
        error.includes("sha256 mismatch")
    )
  );
});

test("rejects missing Tauri license resource mapping", (testContext) => {
  const fixture = createFixture(testContext);
  delete fixture.tauriConfig.bundle.resources["resources/licenses/"];

  const result = verifyManifest({
    manifest: fixture.manifest,
    repoRoot: fixture.repoRoot,
    platform,
    tauriConfig: fixture.tauriConfig
  });

  assert.ok(
    result.errors.some((error) => error.includes("packaged licenses/ directory"))
  );
});
