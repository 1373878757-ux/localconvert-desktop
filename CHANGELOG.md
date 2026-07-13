# Changelog

## Unreleased

No changes yet.

## Preview 0.1.2 - Reliability Release Candidate

LocalConvert Desktop by 田宸宇.

Slogan: 让可能发生在这儿。

Preview 0.1.2 consolidates the reliability, packaging, and release-readiness work completed after Preview 0.1.1. This preparation changes version metadata and release documentation only; it does not alter the existing qpdf or image-engine operation semantics.

### Reliability

- Replaced reliance on browser `File.path` with Tauri-native file selection and drag-and-drop intake that provides validated absolute local paths. The read-only intake boundary records file name, extension, size, and source kind without reading file contents.
- Moved real qpdf and image-engine jobs behind an asynchronous Rust task registry with stable task IDs, status arbitration, child-process cancellation, and terminal cancelled-state protection.
- Added task-owned temporary output workspaces, output validation, and atomic no-overwrite publication. Failure and cancellation cleanup is restricted to task-owned paths and cannot delete pre-existing or concurrently created final files.
- Added three-second per-engine startup self-check timeouts with child-process cleanup, stored diagnostics, and fail-open handoff so the main window still appears after a failed, timed-out, or panicking check.
- Bundled required qpdf and image-engine license/notice resources and enforced a manifest-driven pre-bundle gate for required assets, SHA-256 digests, executable permissions, and license files.
- Made `cargo clippy --all-targets -- -D warnings` pass with zero warnings and added it to the documented release validation sequence.
- Applied supported JPEG and WebP orientation metadata to decoded pixels before encoding, without copying stale orientation metadata that could rotate an output twice.
- Added a credential-free macOS Developer ID signing, notarization, stapling, sidecar-signing, and Gatekeeper verification workflow without making unsigned local builds depend on Apple credentials.

### Included Since Preview 0.1.1

- Added the planning and detection boundaries for future image operations.
- Added the first-party Rust `image-engine` macOS Apple Silicon sidecar with real local JPG/JPEG, PNG, and WebP conversion.
- Added a guarded Simplified Chinese image conversion panel with batch selection and JPG, PNG, or WebP output choices.
- Added collision-safe `converted` output planning, timeout handling, process cleanup, diagnostics capture, and output image validation.

### Unchanged

- Existing qpdf PDF merge, split, rotate, and page extraction behavior remains unchanged.
- Existing enabled image conversion behavior remains unchanged by this release-candidate preparation.
- Browser-only file input remains a metadata-preview fallback and cannot enable real PDF or image operations.
- AVIF, TIFF/TIF, HEIC, image compression, resizing, metadata removal, and images-to-PDF remain disabled.
- No LibreOffice, PDFium, libvips, Sharp, ImageMagick, fonts, upload, cloud, server-side conversion, or telemetry is added.
- No GitHub Release or release tag is created by this preparation step.

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
