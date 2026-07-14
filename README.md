# LocalConvert Desktop

by 田宸宇

Slogan: 让可能发生在这儿。

## Project Overview

LocalConvert Desktop is a desktop-first file conversion app for people who want reliable local conversion without sending files to a cloud service.

The app is planned as a Tauri v2 desktop application with a React and TypeScript frontend, Rust backend commands, and bundled sidecar conversion engines. Conversion work runs on the user's machine. The app should not require a server, account, cloud storage, or internet connection to convert supported files after installation.

This README is the source of truth for the project. Before any code is written or changed, update this document first when the intended product behavior, conversion scope, architecture, privacy model, output rules, or build expectations change.

The first implementation should focus on a dependable v1 conversion set, clear task status, safe output handling, and predictable local execution rather than broad format coverage.

## Product Principles

- Local by default: files are processed on the user's device.
- No cloud upload: source files are not uploaded for conversion.
- No server dependency: conversion does not depend on a hosted backend.
- Offline conversion: supported conversions should work without internet access after installation.
- Bundled engines: runtime users should not manually install LibreOffice, qpdf, Poppler, PDFium, image libraries, or other conversion dependencies.
- Preserve originals: original files are never overwritten by default.
- Clear outputs: converted files are written to a predictable local folder.
- Transparent failures: failed tasks should show useful error information without retaining hidden source copies.
- Safe process execution: sidecar tools are launched with argument arrays, explicit paths, timeouts, and cleanup.
- README-first development: implementation work must follow this README, and scope changes must be documented here before code changes.
- Strict local scope: do not implement video conversion, audio conversion, cloud upload, or server-side conversion.

## Key Features

- Tauri-native file selection and drag-and-drop intake with real local paths for enabled desktop operations.
- Batch conversion queue with visible task status.
- Office-to-PDF conversion for common Office documents.
- Image format conversion, compression, resizing, and metadata removal.
- Multiple images combined into a single PDF.
- PDF merge, split, page extraction, rotation, rasterization, and preview support.
- Output folder opening from completed tasks.
- Retry and cancellation for queued or running tasks where safe.
- Success and failure summaries after batch work.
- Failure logs that capture command context and engine output without copying or retaining source files.

## Supported Conversions

The v1 scope should prioritize these conversion groups:

| Category | Supported operations |
| --- | --- |
| Office to PDF | Convert DOC, DOCX, PPT, PPTX, XLS, XLSX, ODT, ODS, and ODP files to PDF using LibreOffice headless mode. |
| Images | Convert common image formats, apply compression presets, resize images, and remove EXIF metadata. |
| Images to PDF | Combine multiple images into one PDF in user-selected order. |
| PDF structure | Merge PDFs, split PDFs, extract page ranges, and rotate pages. |
| PDF rasterization | Convert PDF pages to images and generate previews. |

Format support should be expanded only when the local engine path, output validation, and packaging story are reliable.

Preview 0.3 keeps the Preview 0.2 local image conversion matrix and adds local image resizing for JPG/JPEG, PNG, and WebP on macOS Apple Silicon. Resize supports fitting within maximum width and height, width-only resizing with automatic height, and height-only resizing with automatic width. Aspect ratio is always preserved, the output keeps the source format and extension, and smaller images are never upscaled. When requested bounds are larger than the source, the engine writes a collision-safe output at the original oriented pixel dimensions and reports that no resize was required. Requested dimensions are limited to 16,384 pixels per edge and the resulting image is limited to 64,000,000 pixels.

Preview 0.4 adds local same-format image compression and size optimization for JPG/JPEG, PNG, and WebP on macOS Apple Silicon. JPEG accepts quality values from 40 through 95 and defaults to 82. WebP accepts quality values from 40 through 95 and defaults to 80. PNG uses lossless optimization only and has no lossy quality control. Compression always keeps the source format and extension, applies supported orientation metadata before encoding, and proposes a collision-safe filename containing `compressed`, such as `report compressed.jpg`. If the task-owned encoded result is not smaller than the source file, the backend does not publish a final output and reports `压缩后未变小，未生成新文件`. This preview does not guarantee that every source can be made smaller.

A first-party Rust `image-engine` sidecar performs conversion, resizing, and compression locally and is enabled only after its startup version and self-check validation pass. Real operations require Tauri-native local paths; browser-only `File` tasks remain metadata previews. Image batches fail closed when any selected item has no native path, uses an unsupported format, has invalid dimensions or quality, is cancelled, or is already running. The Rust backend validates requests, creates the source-adjacent `converted` folder only at execution time, refuses overwrites, launches the sidecar with argument arrays, captures diagnostics, and validates the output before reporting success. JPEG and WebP orientation metadata exposed by the current decoder is applied to pixel data before conversion, resizing, or compression, and stale orientation metadata is not copied to the output. AVIF, TIFF/TIF, and HEIC remain disabled; metadata removal and images-to-PDF are not enabled yet. The current engine does not decode HEIC or apply HEIC orientation metadata.

## Platform Matrix

LocalConvert Desktop should support a full desktop platform matrix at the architecture level, but v1 delivery only enables platforms whose bundled engine assets are available and verified.

Desktop full edition platforms:

| Platform key | Status | Notes |
| --- | --- | --- |
| `windows-x86_64` | v1 enabled | First-priority Windows x64 desktop build. |
| `windows-aarch64` | Future | Desktop full edition only when all engine assets are available. |
| `macos-aarch64` | v1 enabled | Second-priority macOS Apple Silicon desktop build. |
| `macos-x86_64` | Future | Desktop full edition only when all engine assets are available. |
| `linux-x86_64` | Future | Desktop full edition only when all engine assets are available. |

Future mobile lite edition platforms:

| Platform key | Status | Notes |
| --- | --- | --- |
| `android-aarch64` | Future lite only | Do not promise bundled desktop engines or LibreOffice-based Office conversion. |
| `ios-aarch64` | Future lite only | Do not promise bundled desktop engines or LibreOffice-based Office conversion. |

The full bundled engine edition is desktop-only. Android and iOS are future lite editions and must not be described as supporting LibreOffice-based Office conversion.

## Architecture

LocalConvert Desktop should use a layered desktop architecture:

1. The React and TypeScript frontend manages file selection, queue display, options, progress, cancellation, retry, summaries, and output-folder actions.
2. Tauri v2 exposes Rust backend commands for validated conversion requests and filesystem operations.
3. Rust commands normalize paths, create output destinations, register real work by `taskId`, run blocking sidecar execution on Tauri's background blocking pool, enforce timeouts and cancellation, capture stdout and stderr, validate outputs, and return structured task results.
4. Bundled conversion engines perform format-specific work as sidecar binaries or packaged runtime assets.

The frontend should not directly shell out to conversion tools. It should call Tauri commands with structured request data. The Rust side should be responsible for process safety, path handling, engine discovery, cleanup, and validation.

## Bundled Conversion Engines

Runtime installers should include the conversion engines required for the supported v1 feature set.

| Engine | Responsibility | Minimum v1 asset types |
| --- | --- | --- |
| LibreOffice headless | Office-to-PDF conversion for DOC, DOCX, PPT, PPTX, XLS, XLSX, ODT, ODS, and ODP. | `sidecar`, `runtime-folder` |
| qpdf | PDF structure operations such as merge, split, page extraction, and rotation. | `sidecar` |
| PDFium | PDF rasterization, page-to-image conversion, thumbnails, and previews. | `library` |
| image-engine | Image conversion, compression, resizing, and EXIF removal. | `sidecar` |

The exact packaging layout can vary by platform, but the app should resolve engines from its own bundled resources instead of expecting users to install command-line tools manually.

Engine asset structure:

```text
src-tauri/binaries/<platform>/
src-tauri/resources/engines/<engine>/<platform>/
src-tauri/resources/fonts/
src-tauri/resources/licenses/
src-tauri/engine-manifest.json
src-tauri/scripts/fetch-engines.mjs
src-tauri/scripts/verify-engines.mjs
src-tauri/scripts/prepare-sidecars.mjs
```

Asset placement rules:

- Executable sidecars go under `src-tauri/binaries/<platform>/`.
- Large runtime folders go under `src-tauri/resources/engines/<engine>/<platform>/`.
- Fonts go under `src-tauri/resources/fonts/`.
- Licenses and third-party notices go under `src-tauri/resources/licenses/`.
- Each engine asset must be recorded in `src-tauri/engine-manifest.json` with `name`, `version`, `platform`, `type`, `source`, `sha256`, `license`, and `destination`.
- v1 release packaging must fail clearly when required engine assets for `windows-x86_64` or `macos-aarch64` are missing.
- Future desktop platforms remain disabled until all required engine assets are present and verified.
- Mobile lite platforms must not include the full bundled desktop engine set.

## Privacy and Security Model

LocalConvert Desktop is designed around local file privacy:

- Source files stay on the user's machine.
- Supported conversions do not require internet access after installation.
- No account, cloud workspace, or online storage is required.
- The app should not upload source files, converted files, logs, thumbnails, or metadata.
- Failure logs should include useful diagnostics such as engine name, exit code, timeout state, sanitized arguments, stdout, and stderr.
- Failure logs must not retain hidden copies of source documents.
- Temporary files should be scoped to a task and cleaned after completion, cancellation, timeout, or failure.
- Real conversion engines must write only into a task-owned temporary directory created inside the final output filesystem; they must never write directly to the user-visible final path.
- After validation, Rust must publish outputs with atomic no-overwrite semantics. If a final path appears before publication, the task must fail without replacing or deleting that file.
- Failure and cancellation cleanup must remove only task-owned temporary paths, never an arbitrary final output path.

Process execution must follow these rules:

- Use argument-array command execution.
- Do not use `shell=true`.
- Pass file paths as arguments, not by string-concatenating shell commands.
- Do not concatenate shell command strings.
- Do not call conversion binaries directly from the frontend.
- The frontend must call Rust backend commands only.
- Rust backend commands must spawn sidecars using argument arrays.
- Support paths containing spaces, CJK characters, long filenames, and platform-specific path separators.
- Use explicit sidecar paths resolved from the application bundle.
- Apply per-task timeouts.
- Clean up child processes and temporary directories after timeout or cancellation.
- Capture stdout and stderr for diagnostics.
- Validate expected output files before marking a task as successful.

## Default Output Rules

By default, converted files should be written to a `converted` folder next to the source file.

Example:

```text
/Users/example/Documents/report.docx
/Users/example/Documents/converted/report.pdf
```

The app should create the `converted` folder when it does not exist.

Original files must not be overwritten by default. If the target filename already exists, the app should auto-increment the output filename:

```text
report.pdf
report (1).pdf
report (2).pdf
```

This rule applies to single-file conversions, batch conversions, and generated files such as combined PDFs or exported PDF pages.

## Task Queue Behavior

The conversion queue should make task state easy to understand and recover from.

Expected task states:

- Pending
- Running
- Succeeded
- Failed
- Cancelled
- Retrying

Expected queue behavior:

- Each task should show its source file, requested operation, output path, status, and error summary when applicable.
- Batch work should continue when one task fails unless the user cancels the batch.
- Users should be able to retry failed tasks.
- Users should be able to cancel pending tasks.
- Running-task cancellation should terminate the child process when supported by the active engine.
- Real conversion jobs must be tracked in a Rust backend registry so frontend state is not the authority for process lifetime.
- A cancelled backend task must remain cancelled even if a sidecar completion result arrives late, and task-owned partial outputs must be removed.
- Running tasks must be cancelled before they can be removed from the visible queue.
- Completed tasks should offer an action to open the output folder.
- Batch completion should show a success and failure summary.

## Office Conversion Notes

Office-to-PDF conversion should use LibreOffice in headless mode through the bundled runtime.

Implementation notes:

- Use an isolated LibreOffice user profile per task or worker to avoid shared profile locks and user-machine configuration drift.
- Pass paths through argument arrays and avoid shell invocation.
- Enforce a conversion timeout.
- Capture stdout and stderr.
- Clean up temporary profiles and intermediate files.
- Validate the output PDF before reporting success.
- Treat a zero exit code without a readable output PDF as a failure.
- Surface clear guidance when a document cannot be converted because of corruption, unsupported content, password protection, or engine failure.

Office rendering can differ from the source application's native output. The v1 goal is reliable local conversion with clear failure handling, not pixel-perfect parity for every Office feature.

## Development Setup

This repository includes a Tauri v2, React, and TypeScript desktop app for LocalConvert Desktop. The current Preview 0.4 implementation includes a Simplified Chinese UI, branded startup splash screen, Tauri-native file selection and drag-and-drop intake, a local task queue, backend output path planning, the bundled macOS Apple Silicon qpdf sidecar, startup engine self-checks, real local PDF merge, split, page extraction, and rotate execution, and real local JPG/JPEG, PNG, and WebP conversion, resizing, and same-format compression through the first-party Rust `image-engine` sidecar. Each startup engine smoke check has a three-second timeout; a failed, timed-out, or panicking check is stored as a local startup error and never prevents the main window from opening after the splash minimum display time. Real qpdf and image-engine jobs run through an asynchronous Rust task registry with task-ID status arbitration, child-process cancellation, task-owned temporary outputs, validated atomic no-overwrite publication, and scoped cleanup. Compression outputs are published only when they are smaller than their source. Native desktop intake records validated absolute paths and reads filesystem metadata only; browser-only `File` fallback tasks remain metadata previews and cannot run real conversion operations. AVIF, TIFF/TIF, HEIC, metadata removal, images-to-PDF, Office conversion, and PDFium rasterization remain disabled until intentionally enabled in later implementation steps.

Development machines need the normal Tauri v2 toolchain requirements for the target platform, including Node.js, npm, Rust 1.89 or newer, Cargo, and platform-specific build dependencies. Rust 1.89 is required by the pinned WebP encoder used to build the first-party `image-engine` sidecar.

Install dependencies:

```bash
npm install
```

Run the desktop app in development mode:

```bash
npm run tauri dev
```

Runtime users should not run these commands and should not install conversion engines manually. They should install the packaged desktop app, which includes the required sidecar engines.

## Build and Packaging

Build the desktop app with the generic Tauri build command:

```bash
npm run tauri build
```

Unsigned local development and test builds remain supported. Developer ID signing and Apple notarization are opt-in release steps supplied through the local Keychain and environment variables; no signing credentials belong in this repository. Follow the [macOS signing and notarization workflow](docs/macos-signing-notarization.md) before distributing a macOS DMG outside the Mac App Store.

Packaging should focus on a complete offline installer:

- Use Tauri `externalBin` for executable sidecars.
- Use Tauri `resources` for LibreOffice runtime folders, PDFium libraries, fonts, and license files.
- Bundle sidecar conversion engines into the app package.
- Bundle every license, notice, and build notice required by each included sidecar under the installed application's `licenses` resource directory.
- Resolve engine paths from the installed application bundle.
- Include all runtime files needed for supported v1 conversions.
- Run `cargo clippy --all-targets -- -D warnings` from `src-tauri` before release packaging.
- Run `src-tauri/scripts/verify-engines.mjs` as a required Tauri pre-bundle gate; fail when a current bundled asset, SHA-256 digest, executable permission, or required license file is invalid.
- Keep current bundled build requirements separate from future planned engines so an unavailable future engine does not fail today's supported package.
- Run `src-tauri/scripts/prepare-sidecars.mjs` before release packaging.
- Keep startup engine self-checks bounded by per-engine timeouts and fail open to the main workbench with visible local diagnostics.
- Verify required files exist before enabling conversion actions.
- Verify executable permission on macOS and Linux sidecars.
- Verify engine version commands where available.
- Show clear local errors when bundled engines are missing or invalid.
- Verify that conversions work on a clean machine without manually installed conversion tools.
- Verify that conversions work while the machine is offline.
- Keep installer behavior platform-appropriate for enabled desktop targets.

The packaging process should include a post-install smoke test pass that confirms the app can run supported conversions without network access.

## Smoke Tests

Before a release, verify these scenarios on a clean install.

Preview 0.1 manual release checklist: `docs/manual-test-preview-0.1.md`.

Current qpdf PDF tools checklist:

- Merge two or more PDFs and confirm a single output PDF is written to the source-adjacent `converted` folder.
- Split one multi-page PDF and confirm the split output PDFs are written only to `converted`.
- Rotate one PDF left 90, right 90, and 180 degrees, confirming each output PDF is readable.
- Extract selected pages such as `1,3,5-7` and confirm the output page count matches the selected pages.
- Repeat at least one operation with a Chinese filename.
- Repeat at least one operation from a folder path containing spaces.
- Repeat at least one operation when the planned output name already exists and confirm auto-incremented collision naming.
- Hash or otherwise compare source files before and after each operation and confirm sources are unchanged.
- Confirm Office, image metadata tools, PDF rasterization, and preview tools remain disabled until their bundled engines or execution paths are intentionally added.

Current image conversion checklist:

- Convert PNG to JPG and confirm a non-empty output is written to the source-adjacent `converted` folder.
- Convert JPG/JPEG to WebP and WebP to PNG.
- Convert JPEG fixtures with EXIF orientations 1, 6, 3, and 8 and confirm the output dimensions and visible pixel orientation are normalized.
- Repeat a conversion with a Chinese filename and a path containing spaces.
- Create an output collision and confirm the backend selects an incremented name instead of overwriting it.
- Hash or otherwise compare the source image before and after conversion and confirm it is unchanged.
- Confirm AVIF, TIFF/TIF, HEIC, metadata removal, and images-to-PDF remain disabled.

Current image resize checklist:

- Resize a JPEG to fit within a maximum width and height and confirm the aspect ratio is preserved.
- Resize a PNG by width only and a WebP by height only, confirming the missing dimension is calculated automatically.
- Resize an oriented JPEG and confirm orientation is applied before the target dimensions are calculated.
- Request bounds larger than the source and confirm the output keeps the original oriented dimensions instead of upscaling.
- Reject zero, negative, non-integer, non-numeric, over-16,384, and over-64,000,000-pixel requests before starting the sidecar.
- Confirm HEIC resize remains blocked before an output folder or process is created.
- Confirm cancellation leaves no partial final output, collisions do not overwrite existing files, and the source hash remains unchanged.

Current image compression checklist:

- Compress JPEG at quality 82 and another value between 40 and 95; confirm the output remains JPEG and orientation is normalized before encoding.
- Compress WebP at quality 80 and another value between 40 and 95; confirm the output remains WebP.
- Run PNG lossless optimization and confirm the output remains PNG.
- Reject JPEG or WebP quality values below 40, above 95, or not expressed as whole numbers before starting the sidecar.
- Confirm HEIC and unsupported formats are rejected before an output folder or process is created.
- Confirm published filenames include `compressed`, use collision numbering when needed, never overwrite an existing file, and leave the source hash unchanged.
- Use a source that cannot be made smaller and confirm the task reports `压缩后未变小，未生成新文件` without publishing a final output.
- Confirm cancellation leaves no final output or task-owned temporary directory.

Office-to-PDF:

- Convert a DOCX file to PDF.
- Convert a PPTX file to PDF.
- Convert an XLSX file to PDF.
- Confirm each output PDF exists, is readable, and is written to the default `converted` folder.

Image conversion:

- Convert between common image formats.
- Apply compression presets.
- Resize images.
- Remove EXIF metadata.
- Confirm source images are unchanged.

Images to PDF:

- Select multiple images.
- Preserve the selected order.
- Generate a single PDF.
- Confirm filename collision handling works.

PDF operations:

- Merge multiple PDFs.
- Split a PDF.
- Extract a page range.
- Rotate pages.
- Convert PDF pages to images.
- Generate previews or thumbnails.

Queue behavior:

- Confirm status transitions for pending, running, succeeded, failed, cancelled, and retrying tasks.
- Retry a failed task.
- Cancel a pending task.
- Cancel a running task and confirm child-process cleanup.
- Open the output folder for a successful task.
- Review the success and failure summary after a batch.

Privacy checks:

- Convert files while offline.
- Confirm no upload is attempted.
- Confirm no server is required.
- Confirm no hidden source copies remain after success, failure, timeout, or cancellation.
- Confirm failure logs contain diagnostics but do not retain source documents.

Path handling:

- Convert files from folders with spaces in their names.
- Convert files from folders with CJK characters in their names.
- Convert files with long filenames.
- Confirm outputs are created with safe auto-incremented names.

## Roadmap

- Implement the v1 local conversion queue and default output rules.
- Add reliable Office-to-PDF conversion with isolated LibreOffice execution.
- Expand the current image conversion, resize, and compression preview with EXIF removal and images-to-PDF.
- Add PDF merge, split, extraction, rotation, rasterization, and previews.
- Add detailed per-task logs and batch summaries.
- Add cross-platform packaging with bundled sidecar engines.
- Add clean-install and offline release validation.
- Expand supported formats only after the bundled local engine path is proven reliable.

## Non-Goals

The following are not current goals:

- Video conversion. Do not implement it.
- Audio conversion. Do not implement it.
- Cloud conversion or cloud upload. Do not implement it.
- Server-side conversion or hosted conversion workers. Do not implement them.
- Accounts or sign-in.
- Online storage.
- Collaboration features.
- Automatic source deletion.
- In-place replacement of original files.
- Requiring runtime users to install conversion engines manually.
