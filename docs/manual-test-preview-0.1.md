# LocalConvert Desktop Preview 0.1 Manual Test Checklist

LocalConvert Desktop by 田宸宇.

Slogan: 让可能，发生在这儿。

This checklist verifies Preview 0.1 on macOS Apple Silicon with bundled qpdf 12.3.2. Run tests offline where possible and use disposable PDF copies for operation checks.

## Environment

- Platform: macOS Apple Silicon.
- Build: LocalConvert Desktop Preview 0.1.
- Bundled engine: qpdf 12.3.2.
- Network: disconnect internet or block network access during privacy checks.

## Install And Startup

- [ ] Install or open the packaged app.
- [ ] Confirm the startup splash appears before the main workbench.
- [ ] Confirm the splash shows `LocalConvert Desktop`.
- [ ] Confirm the splash shows `by 田宸宇`.
- [ ] Confirm the splash shows `让可能，发生在这儿。`.
- [ ] Confirm the splash includes `No upload. Files stay on this computer.`
- [ ] Confirm the main workbench appears after startup initialization.

## Engine Self-Check

- [ ] Confirm the engine status panel reports bundled qpdf as available.
- [ ] Confirm the qpdf message references the bundled qpdf sidecar and version 12.3.2.
- [ ] Confirm Office, image conversion, PDF preview, and PDF-to-image remain disabled.

## PDF Operations

- [ ] Merge two PDFs and confirm one merged PDF is written to a `converted` folder.
- [ ] Split a multi-page PDF and confirm split files are written to `converted`.
- [ ] Rotate a PDF and confirm the rotated output PDF is written to `converted`.
- [ ] Extract selected pages, such as `1,3,5-7`, and confirm the output PDF is written to `converted`.
- [ ] Repeat at least one operation with a Chinese filename.
- [ ] Repeat at least one operation from a path containing spaces.
- [ ] Repeat at least one operation when the planned output already exists and confirm collision-safe naming.

## Source Safety

- [ ] Record the source PDF hash before each operation.
- [ ] Run the operation.
- [ ] Record the source PDF hash after each operation.
- [ ] Confirm the source hash is unchanged.
- [ ] Confirm source files are not modified, moved, deleted, or overwritten.

## Local Privacy

- [ ] Run the app while offline.
- [ ] Confirm PDF tools still work offline.
- [ ] Confirm no upload prompt appears.
- [ ] Confirm no cloud account, server URL, or online storage requirement appears.
- [ ] Confirm generated outputs are written locally only.

## Release Result

- [ ] All required Preview 0.1 checks passed.
- [ ] Any failed check is documented with the source filename, operation, expected result, actual result, and error log text.
