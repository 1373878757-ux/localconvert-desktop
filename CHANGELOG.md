# Changelog

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
