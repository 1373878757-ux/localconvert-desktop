# Changelog

## Unreleased

### Preview 0.6.1 - Task Result Usability

- Aligned npm, Cargo, and Tauri application version metadata to `0.6.1`.
- Added safe system-file-manager navigation for published task outputs and the last successfully exported CSV or JSON report.
- Added write-only clipboard handling for concise failed, unsupported, and skipped task summaries, with raw metadata payload lines excluded.
- Added confirmed in-memory task-history cleanup that never deletes source files, outputs, reports, logs, or temporary directories.
- Improved the Chinese task empty state and missing-path feedback while keeping creator attribution intact.
- Kept report serialization, qpdf, image conversion, resize, compression, metadata cleanup, bundled engines, and local-only guarantees unchanged.

### Preview 0.6.0 - Local Task Report Export

- Aligned npm, Cargo, and Tauri application version metadata to `0.6.0`.
- Added fully local CSV and pretty-printed JSON export for completed task results through the native save dialog.
- Added task report fields for operation type, local source/output paths, result status, timestamps, duration, available byte counts, savings, and user-facing result summaries.
- Added explicit report statuses for success, failure, cancellation, skipped metadata cleanup, not-smaller compression, and unsupported results.
- Added strict Rust-side report schemas and serialization tests, including CSV escaping, stable status names, savings formatting, and rejection of undeclared raw EXIF/GPS/XMP payload fields.
- Added a compact Chinese report export panel with timestamped filenames, success/cancellation/error feedback, and a warning that exported reports may contain local paths.
- Added only the minimal `dialog:allow-save` capability; report contents are written by the Rust backend without adding a frontend filesystem plugin.
- Kept PDF and image operation behavior, bundled engines, cancellation, atomic output publication, and local-only guarantees unchanged.

### Preview 0.5.1 - macOS Preview Install Guidance

- Aligned npm, Cargo, and Tauri application version metadata to `0.5.1`.
- Added a user-facing macOS Apple Silicon preview install guide with official GitHub Release download guidance, SHA-256 verification, and cautious Gatekeeper first-launch steps.
- Added concise local-processing and current-preview limitation sections without changing conversion behavior.
- Updated the in-app About panel with version, creator attribution, pure-local processing, unsigned/not-notarized status, and GitHub Release checksum reminders.
- Clarified low-risk user-facing messages for unsupported HEIC operations, metadata-free cleanup results, and compression results that are not smaller.
- Added a future GitHub prerelease checklist covering build, smoke-test level, DMG SHA-256, signing/notarization status, and GitHub asset digest comparison.
- Kept qpdf, image conversion, resize, compression, metadata cleanup, task cancellation, and atomic output behavior unchanged; added no engines or network capability.

### Preview 0.5 - Image Metadata Privacy Cleanup Preview

- Aligned application version metadata to `0.5.0` for the Preview 0.5 development line.
- Added best-effort local metadata cleanup for JPG/JPEG, PNG, and WebP through the existing first-party Rust `image-engine`.
- Added container-level removal for common JPEG EXIF/XMP/IPTC/comment segments, PNG EXIF/text/time chunks, and WebP EXIF/XMP chunks while preserving encoded pixel payloads where feasible.
- Added visual orientation normalization for JPEG files that depend on EXIF orientation; this necessary path uses a documented JPEG quality 95 re-encode before stale orientation metadata is removed.
- Added collision-safe `cleaned` output naming, no-output behavior when no removable metadata is found, backend cancellation, task-owned temporary output, validation, and atomic no-overwrite publication.
- Added a guarded Simplified Chinese Preview 0.5 metadata cleanup panel with explicit best-effort and non-forensic limitations.
- Kept HEIC, RAW, GIF, TIFF, PDF, Office, video, and audio metadata cleanup disabled.
- Added no new sidecar or Rust dependency and kept existing image conversion, resize, compression, and qpdf behavior unchanged.

### Preview 0.4 - Image Compression Preview

- Aligned application version metadata to `0.4.0` for the Preview 0.4 development line.
- Added local same-format JPEG, PNG, and WebP compression through the existing first-party Rust `image-engine`.
- Added JPEG quality 40-95 with default 82, WebP quality 40-95 with default 80, and lossless-only PNG optimization.
- Added `compressed` output naming, task-owned temporary encoding, cancellation, validation, and atomic no-overwrite publication.
- Added fail-closed behavior that does not publish a final file when the encoded result is not smaller than its source.
- Preserved supported EXIF orientation normalization before compression and kept HEIC blocked.
- Added the maintained Rust `webpx` encoder API with statically linked libwebp 1.6.0 for lossy WebP quality control, with packaged license notices and manifest verification.
- Kept existing image conversion, image resize, and qpdf PDF operation behavior unchanged.

### Preview 0.3 - Image Resize Preview

- Aligned application version metadata to `0.3.0` for the Preview 0.3 development line.
- Added local aspect-ratio-preserving resize planning for JPG/JPEG, PNG, and WebP using the existing first-party Rust `image-engine`.
- Added fit-within-bounds, width-only, and height-only modes with no upscaling.
- Added explicit 16,384-pixel edge and 64,000,000-pixel result limits.
- Kept real resize execution behind native local paths, the backend task registry, cancellation, task-owned temporary outputs, validation, and atomic no-overwrite publication.
- Kept HEIC, AVIF, TIFF/TIF, compression, metadata removal, and images-to-PDF disabled.
- Kept existing image conversion and qpdf PDF operation behavior unchanged.

## Preview 0.2.0 - Minimal Image Conversion

- Aligned application version metadata to `0.2.0` for the Preview 0.2 development line.
- Formalized the enabled local conversion matrix: JPG/JPEG to PNG or WebP, PNG to JPG or WebP, and WebP to JPG or PNG.
- Kept real image conversion behind Tauri-native local paths, the asynchronous backend task registry, cancellation, and atomic no-overwrite output finalization.
- Added explicit fail-closed guidance for same-format targets, browser-preview tasks, cancelled or running tasks, HEIC, and other unsupported formats.
- Added acceptance coverage for the enabled codecs, HEIC rejection, image-task cancellation, source preservation, and collision-safe output publication.
- Kept AVIF, TIFF/TIF, HEIC, compression, resizing, metadata removal, and images-to-PDF disabled.
- Kept existing qpdf behavior unchanged and added no upload, cloud, server, telemetry, or network capability.

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
