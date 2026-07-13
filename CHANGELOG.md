# Changelog

## Unreleased

### Added

- Added Tauri-native file selection and drag-and-drop intake so enabled local operations receive validated absolute source paths.
- Added a read-only Rust path inspection boundary that records file name, extension, size, and source kind without reading file contents.
- Added a backend planning boundary for future Preview 0.2 image tools.
- Added a backend detection boundary for a future bundled image-engine sidecar.
- Added a first-party Rust `image-engine` macOS Apple Silicon sidecar with real local JPG/JPEG, PNG, and WebP conversion.
- Added a guarded Simplified Chinese image conversion panel with batch selection and JPG, PNG, or WebP output choices.
- Added collision-safe `converted` output planning, timeout handling, process cleanup, diagnostics capture, and output image validation.
- Applied JPEG and WebP orientation metadata to decoded pixels before output encoding without copying stale orientation metadata.
- Added a Rust backend task registry for real qpdf and image-engine jobs, with async Tauri command boundaries and stable task IDs.
- Added process-backed cancellation, terminal cancelled-state arbitration, and partial-output cleanup for running local conversions.
- Added task-owned temporary output workspaces and validated atomic no-overwrite publication for qpdf and image-engine results.
- Restricted failure and cancellation cleanup to task-owned temporary paths so pre-existing or concurrently created final files are never deleted.
- Added three-second per-engine startup smoke-check timeouts with child-process cleanup and fail-open main-window handoff.
- Stored startup engine timeout and failure diagnostics for the existing engine status and error panels.
- Bundled qpdf and image-engine license/notice resources into the installed application.
- Added a manifest-driven pre-bundle verification gate for required sidecars, SHA-256 digests, executable permissions, and license files.
- Added a credential-free macOS Developer ID signing, notarization, stapling, and Gatekeeper verification checklist.

### Unchanged

- Browser-only file input remains a metadata-preview fallback and cannot enable real PDF or image operations.
- AVIF, TIFF/TIF, HEIC, image compression, resizing, metadata removal, and images-to-PDF remain disabled.
- No LibreOffice, PDFium, libvips, Sharp, ImageMagick, fonts, upload, cloud, server-side conversion, or telemetry is added.
- Existing qpdf PDF merge, split, rotate, and page extraction behavior is unchanged.
- Existing qpdf argument plans and operation semantics are unchanged; only execution scheduling and cancellation ownership moved to the backend task boundary.

## Preview 0.1.1 - qpdf macOS Apple Silicon Chinese UI

LocalConvert Desktop by 田宸宇.

Slogan: 让可能发生在这儿。

Preview 0.1.1 tags the current stable Simplified Chinese UI build. It keeps the existing qpdf-backed PDF tools unchanged and focuses on Chinese user-facing copy for the local desktop utility experience.

### Changed

- Localized the visible app interface to Simplified Chinese.
- Kept the branded startup splash screen with creator attribution.
- Kept the splash slogan as `让可能发生在这儿。`.
- Kept the startup splash minimum display time at 1800ms.
- Kept qpdf PDF merge, split, rotate, and page extraction behavior unchanged.

### Unchanged

- qpdf remains the only bundled engine.
- No Office conversion, image conversion, PDF preview, cloud upload, server-side conversion, telemetry, or new conversion category is added in this preview.

## Preview 0.1 - qpdf macOS Apple Silicon

LocalConvert Desktop by 田宸宇.

Slogan: 让可能发生在这儿。

Preview 0.1 is the first local PDF tools preview for macOS Apple Silicon. It focuses on a small, reliable qpdf-backed feature set and keeps conversion local to the user's computer.

### Added

- Branded startup splash screen with creator attribution.
- macOS Apple Silicon build.
- Bundled qpdf 12.3.2 sidecar for local PDF structure operations.
- Local PDF merge.
- Local PDF split.
- Local PDF page rotation.
- Local PDF page extraction.
- Local-only processing with no upload and no cloud dependency.
- Output behavior that preserves source files and avoids modifying originals.

### Known Limitations

- qpdf is currently bundled only for macOS Apple Silicon.
- Windows bundled qpdf engine is not available yet.
- Linux bundled qpdf engine is not available yet.
- Office conversion is not enabled yet.
- Image conversion is not enabled yet.
- PDF preview, PDF rasterization, and PDF-to-image are not enabled yet.

### Verification

- Manual release test checklist: `docs/manual-test-preview-0.1.md`
