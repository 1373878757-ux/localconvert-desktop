# LocalConvert Desktop

**by 田宸宇**

> 让可能发生在这儿。

面向普通办公用户的纯本地桌面文件转换工具。文件在本机处理，不上传、不依赖服务器、不采集遥测，默认不会覆盖原文件。

**当前稳定预览： [Preview 0.6.1](https://github.com/1373878757-ux/localconvert-desktop/releases/tag/preview-0.6.1-task-usability-macos)** · macOS Apple Silicon

[下载与安装](#macos-download-and-install) · [当前功能](#key-features) · [一页作品集简介](docs/portfolio-brief.md) · [项目展示](docs/showcase.md) · [60 秒演示脚本](docs/demo-script.md)

> 当前 DMG 为未签名或 ad-hoc 签名的预览构建，尚未完成 Apple Developer ID 公证。请只从官方 GitHub Releases 页面下载，并在打开前核对 SHA-256。

## Project Page

项目静态展示页计划发布于：<https://1373878757-ux.github.io/localconvert-desktop/>

页面源码位于 [`docs/index.html`](docs/index.html)，不使用远程脚本、字体、图片或分析服务。GitHub Pages 尚需在仓库 **Settings > Pages** 中选择 `main` 分支和 `/docs` 目录后才会生效；本文不假设该站点已经启用。

## Project Overview

LocalConvert Desktop 适合需要处理 PDF 和常见图片、但不希望把文件交给在线转换网站的办公用户。下载安装后，当前已支持 PDF 结构操作、JPG/PNG/WebP 转换与处理，以及 CSV/JSON 任务报告导出。

应用采用 Tauri v2、React、TypeScript 与 Rust 后端命令，并随安装包内置 qpdf 和第一方 Rust `image-engine` sidecar。受支持的转换不需要账户、云存储、服务器或网络连接。

本 README 是项目事实来源。产品行为、转换范围、架构、隐私模型、输出规则或构建要求发生变化时，应先更新本文档，再修改实现。

当前项目优先保证可靠的本地执行、清晰的任务状态和安全输出，而不是追求宽泛但不稳定的格式覆盖。

### What This App Does Locally

- 文件由安装包内置的本地引擎处理。
- 源文件和输出文件不会上传。
- 转换不依赖托管服务器，也不要求登录。
- 应用不包含遥测。
- 原文件不会被覆盖；结果写入源文件旁的 `converted` 文件夹，并自动处理重名。

## Product Principles

- **纯本地**：文件始终在用户设备上处理。
- **无上传**：源文件、输出文件和原始元数据不会发送到云端。
- **无服务器依赖**：受支持的转换在安装后可离线运行。
- **内置引擎**：运行时用户不需要手动安装 qpdf 或图片处理依赖。
- **保护原文件**：默认输出到源文件旁的 `converted` 文件夹，绝不就地覆盖。
- **失败透明**：失败任务提供可读错误信息，不暗中保留源文件副本。
- **安全执行**：sidecar 使用明确路径和参数数组启动，并具备超时、取消与清理机制。
- **README 优先**：范围变化先更新本文档，再进入实现。
- **范围克制**：不开发视频、音频、云上传或服务端转换。

## Key Features

Preview 0.6.1 当前提供以下能力：

| 功能 | 当前状态 | 说明 |
| --- | --- | --- |
| PDF 合并 | 可用 | 合并两个或更多本地 PDF。 |
| PDF 拆分 | 可用 | 将多页 PDF 拆分为独立输出。 |
| PDF 旋转 | 可用 | 支持左转 90°、右转 90° 和 180°。 |
| PDF 页面提取 | 可用 | 支持 `1,3,5-7` 等页面范围。 |
| JPG/PNG/WebP 转换 | 可用 | 支持 JPG/JPEG、PNG、WebP 之间的已启用方向。 |
| JPG/PNG/WebP 改尺寸 | 可用 | 等比缩放，支持适应范围、仅宽度、仅高度，默认不放大。 |
| JPEG/WebP/PNG 压缩 | 可用 | JPEG/WebP 质量压缩，PNG 无损优化；未变小时不发布输出。 |
| JPG/PNG/WebP 元数据清理 | 可用 | 尽力移除常见隐私元数据，不宣称法证级清理。 |
| CSV/JSON 报告导出 | 可用 | 导出任务状态与结果元数据，不包含文件内容。 |
| 打开输出/报告位置 | 可用 | 在 Finder 中定位已发布输出或最近导出的报告。 |
| 复制错误摘要 | 可用 | 仅复制精简的人类可读错误，不复制原始元数据载荷。 |
| 清空任务记录 | 可用 | 二次确认后只清空当前界面记录，不删除任何文件。 |

任务由 Tauri 原生文件选择或拖放导入，真实操作使用本地绝对路径。队列显示等待、处理中、完成、失败和取消状态，并支持安全取消、失败重试、原子输出和防覆盖发布。

## Supported Conversions

### Supported Formats

| 类别 | 当前格式 | 当前操作 |
| --- | --- | --- |
| PDF 工具 | PDF | 合并、拆分、旋转、页面提取 |
| 图片格式转换 | JPG/JPEG、PNG、WebP | JPG/JPEG → PNG/WebP；PNG → JPG/WebP；WebP → JPG/PNG |
| 图片改尺寸 | JPG/JPEG、PNG、WebP | 适应最大宽高、仅宽度、仅高度，始终保持宽高比 |
| 图片压缩 | JPG/JPEG、PNG、WebP | JPEG/WebP 质量压缩；PNG 无损优化 |
| 元数据清理 | JPG/JPEG、PNG、WebP | 尽力移除常见 EXIF/GPS/XMP/IPTC、PNG 文本/时间和 WebP 元数据块 |
| 任务报告 | CSV、JSON | 导出任务状态、路径、时间、体积和结果摘要 |

### Current Behavior

- 真实任务必须来自 Tauri 原生本地路径；浏览器 `File` 仅用于元数据预览。
- PDF 操作由内置 qpdf 执行；图片操作由第一方 Rust `image-engine` 执行。
- 图片改尺寸支持适应范围、仅宽度和仅高度，默认不放大；单边上限为 16,384 像素，结果上限为 64,000,000 像素。
- JPEG 质量范围为 40–95，默认 82；WebP 质量范围为 40–95，默认 80；PNG 只做无损优化。
- 压缩结果未小于源文件时不发布新文件，并报告 `压缩后未变小，未生成新文件`。
- 元数据清理保持源格式；未发现可清理内容时不发布新文件，并报告 `未发现可清理的元数据，未生成新文件`。
- JPEG/WebP 支持的方向元数据会在转换、改尺寸或压缩前固化；需要方向变换的 JPEG 清理会以质量 95 重编码。
- 图片批次采用 fail-closed：任一任务无原生路径、格式不受支持或参数无效时，整批不会启动。
- 报告可能包含本地路径，但不包含文件内容或原始 EXIF/GPS/XMP/IPTC 载荷。
- 成功输出先写入任务专属临时位置，验证后再以无覆盖方式发布到 `converted` 文件夹。

格式范围只会在本地引擎、输出验证、取消清理、许可证和打包链路均可靠时扩大。

## Not Yet Supported

- HEIC 转换、改尺寸、压缩、元数据清理及方向处理。
- GIF、TIFF/TIF、RAW 图片处理。
- DOC/DOCX、PPT/PPTX、XLS/XLSX 等 Office 文档转换。
- 多图合成 PDF、PDF 预览和 PDF 转图片。
- Windows x64 公开预览构建。
- Intel macOS 构建。
- 视频和音频转换不在当前产品目标内。

## macOS Download and Install

当前公开预览仅面向 macOS Apple Silicon，安装包尚未完成 Developer ID 签名与 Apple 公证，首次打开时可能出现 Gatekeeper 提示。

1. 只从 [Preview 0.6.1 GitHub Release](https://github.com/1373878757-ux/localconvert-desktop/releases/tag/preview-0.6.1-task-usability-macos) 下载 DMG，不使用第三方转载文件。
2. 在终端计算下载文件的 SHA-256：

   ```bash
   shasum -a 256 "LocalConvert.Desktop_0.6.1_aarch64.dmg"
   ```

3. 将结果与 GitHub Release 页面显示的 asset digest 对比；不一致时立即停止，不要打开文件。
4. 校验通过后挂载 DMG，将 `LocalConvert Desktop.app` 拖入 Applications。
5. 如果 Gatekeeper 因开发者身份无法验证而阻止打开，可在 Finder 中按住 Control 点击该应用，选择 **打开**，再次核对提示后只为这个已校验应用确认打开。

不要全局关闭 Gatekeeper，也不要对来源不明的应用移除隔离属性。项目的发布签名准备见 [macOS signing and notarization workflow](docs/macos-signing-notarization.md)。

## Privacy and Safety

- 文件在本机处理，转换不上传，也不依赖服务器。
- 受支持的转换不要求网络连接、账户或云存储。
- 应用不采集遥测。
- 原文件不会被覆盖；输出写入源文件旁的 `converted` 文件夹，并使用安全递增命名。
- 任务报告可能包含本机文件路径，分享前应先检查。
- 报告不包含文件内容，也不包含原始 EXIF、GPS、XMP 或 IPTC 载荷。
- 元数据清理是最佳努力的隐私辅助功能，不是法证级彻底清除；重要文件在分享前仍应自行复核。
- 失败、超时和取消只清理任务自有临时文件，不删除任意最终输出或源文件。

## Technical Stack

| 层级 | 技术与职责 |
| --- | --- |
| 桌面外壳 | Tauri v2：窗口、原生文件入口、打包与 sidecar 管理 |
| 前端 | React + TypeScript：中文 UI、拖放、任务队列、结果和报告交互 |
| 后端 | Rust Tauri commands：路径校验、后台任务、取消、超时、日志与原子输出 |
| PDF 引擎 | 内置 qpdf sidecar：合并、拆分、旋转、页面提取 |
| 图片引擎 | 第一方 Rust `image-engine` sidecar：转换、改尺寸、压缩、元数据清理 |
| 资产验证 | manifest 驱动的 SHA-256、可执行权限和许可证构建门禁 |

前端不会直接调用转换二进制；所有真实文件操作都经过 Rust 后端命令和参数数组边界。

## Release Timeline

| 版本 | 里程碑 |
| --- | --- |
| 0.1.x | 建立 qpdf PDF 工具、可靠本地执行与输出安全基础。 |
| 0.2.0 | 增加 JPG/PNG/WebP 图片格式转换。 |
| 0.3.0 | 增加保持宽高比的图片改尺寸。 |
| 0.4.0 | 增加 JPEG/WebP 质量压缩与 PNG 无损优化。 |
| 0.5.0 | 增加 JPG/PNG/WebP 元数据隐私清理。 |
| 0.5.1 | 完善 macOS 安装、Gatekeeper 与 SHA-256 指引。 |
| 0.6.0 | 增加 CSV/JSON 批量任务报告导出。 |
| **0.6.1** | 增加输出/报告定位、精简错误复制、历史清理确认和空状态优化。 |

## Screenshots

以下截图来自真实的 Preview 0.6.1 macOS 应用，并使用 `/private/tmp/LocalConvert Demo 0.6.1` 中的合成演示文件；不包含个人文档、用户名或真实图片元数据。

### Main Workspace

![LocalConvert Desktop Preview 0.6.1 主工作台](docs/assets/screenshots/main-window.png)

| PDF tools | Image tools |
| --- | --- |
| ![已选择两个合成 PDF 的本地 qpdf 工具](docs/assets/screenshots/pdf-tools.png) | ![JPG、PNG、WebP 图片工具](docs/assets/screenshots/image-tools.png) |

| Task results | Report export |
| --- | --- |
| ![使用合成文件生成的本地任务结果](docs/assets/screenshots/task-results.png) | ![CSV 和 JSON 任务报告导出面板](docs/assets/screenshots/report-export.png) |

### About

![Preview 0.6.1、作者与未公证提示](docs/assets/screenshots/about-panel.png)

更多展示说明见 [项目展示文档](docs/showcase.md)，录制流程见 [60 秒演示脚本](docs/demo-script.md)。

## Platform Matrix

LocalConvert Desktop 保留完整桌面平台矩阵的架构方向，但当前只发布已具备完整、可验证引擎资产的 macOS Apple Silicon 预览构建。

Desktop full edition platforms:

| Platform key | Status | Notes |
| --- | --- | --- |
| `windows-x86_64` | Planned v1 | First-priority future Windows build; no public preview is available yet. |
| `windows-aarch64` | Future | Desktop full edition only when all engine assets are available. |
| `macos-aarch64` | Current preview | Preview 0.6.1 is available with bundled qpdf and image-engine assets. |
| `macos-x86_64` | Future | Desktop full edition only when all engine assets are available. |
| `linux-x86_64` | Future | Desktop full edition only when all engine assets are available. |

Future mobile lite edition platforms:

| Platform key | Status | Notes |
| --- | --- | --- |
| `android-aarch64` | Future lite only | Do not promise bundled desktop engines or LibreOffice-based Office conversion. |
| `ios-aarch64` | Future lite only | Do not promise bundled desktop engines or LibreOffice-based Office conversion. |

The full bundled engine edition is desktop-only. Android and iOS are future lite editions and must not be described as supporting LibreOffice-based Office conversion.

## Architecture

LocalConvert Desktop 对当前已启用操作采用以下分层桌面架构，并为后续本地引擎保留相同边界：

1. The React and TypeScript frontend manages file selection, queue display, options, progress, cancellation, retry, summaries, and output-folder actions.
2. Tauri v2 exposes Rust backend commands for validated conversion requests and filesystem operations.
3. Rust commands normalize paths, create output destinations, register real work by `taskId`, run blocking sidecar execution on Tauri's background blocking pool, enforce timeouts and cancellation, capture stdout and stderr, validate outputs, and return structured task results.
4. Bundled conversion engines perform format-specific work as sidecar binaries or packaged runtime assets.

The frontend should not directly shell out to conversion tools. It should call Tauri commands with structured request data. The Rust side should be responsible for process safety, path handling, engine discovery, cleanup, and validation.

## Bundled Conversion Engines

当前公开安装包只内置已经启用并通过资产校验的 qpdf 与第一方 `image-engine`。LibreOffice 和 PDFium 保留为未来桌面完整版方向，在真实运行时、许可证和打包链路完成前不会对用户宣称可用。

| Engine | Status | Responsibility | Asset types |
| --- | --- | --- | --- |
| qpdf | Current | PDF merge, split, page extraction, and rotation. | `sidecar` |
| image-engine | Current | Image conversion, compression, resizing, and best-effort metadata/privacy cleanup. | `sidecar` |
| LibreOffice headless | Planned | Future Office-to-PDF conversion for DOC, DOCX, PPT, PPTX, XLS, XLSX, ODT, ODS, and ODP. | `sidecar`, `runtime-folder` |
| PDFium | Planned | Future PDF rasterization, page-to-image conversion, thumbnails, and previews. | `library` |

The exact packaging layout can vary by platform, but enabled operations must resolve engines from the app's own bundled resources instead of expecting users to install command-line tools manually.

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
- A release build must fail clearly when required engine assets for its enabled target platform are missing. The current public profile requires `macos-aarch64`; `windows-x86_64` remains planned.
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
- The most recently exported report should offer the same local location action after a successful export; cancelling the save dialog must not change it.
- Failed, unsupported, and skipped results may copy only a concise human-readable error summary, never file contents or raw metadata payloads.
- Clearing task history must require confirmation and affect only the current in-memory UI records, not any local files or logs.
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

This repository includes a Tauri v2, React, and TypeScript desktop app for LocalConvert Desktop. The current Preview 0.6.1 implementation includes a Simplified Chinese UI, branded startup splash screen, Tauri-native file selection and drag-and-drop intake, a local task queue, local CSV/JSON task report export, safe local output/report location navigation, concise error-summary copying, confirmed in-memory history cleanup, backend output path planning, the bundled macOS Apple Silicon qpdf sidecar, startup engine self-checks, real local PDF merge, split, page extraction, and rotate execution, and real local JPG/JPEG, PNG, and WebP conversion, resizing, same-format compression, and best-effort metadata cleanup through the first-party Rust `image-engine` sidecar. Each startup engine smoke check has a three-second timeout; a failed, timed-out, or panicking check is stored as a local startup error and never prevents the main window from opening after the splash minimum display time. Real qpdf and image-engine jobs run through an asynchronous Rust task registry with task-ID status arbitration, child-process cancellation, task-owned temporary outputs, validated atomic no-overwrite publication, and scoped cleanup. Compression outputs are published only when they are smaller than their source, and metadata cleanup outputs are published only when removable metadata is found. Native desktop intake records validated absolute paths and reads filesystem metadata only; browser-only `File` fallback tasks remain metadata previews and cannot run real conversion operations. Report export writes only the task metadata already held by the UI through a Rust backend command; it does not read source file contents or export raw image metadata. Local navigation validates that a remembered absolute path still exists before revealing it, and clipboard access is write-only for a filtered summary. AVIF, TIFF/TIF, HEIC, GIF, RAW, images-to-PDF, Office conversion, and PDFium rasterization remain disabled until intentionally enabled in later implementation steps.

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

### macOS Preview Install Guide

用户安装步骤、Gatekeeper 安全提示和 Preview 0.6.1 SHA-256 命令见前文 [macOS Download and Install](#macos-download-and-install)。发布人员应遵循 [macOS signing and notarization workflow](docs/macos-signing-notarization.md)，且不得将证书、密码或 Apple API 凭据提交到仓库。

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

### Future GitHub Prerelease Checklist

1. Confirm the intended version is aligned across npm, Cargo, and Tauri metadata, then build the `.app` and DMG.
2. Run the agreed smoke-test level for the change; use the heavy review matrix before creating a GitHub Release.
3. Record the SHA-256 of the exact DMG that passed smoke validation and do not silently replace it with a rebuilt container.
4. State clearly whether the artifact is Developer ID signed, notarized, and stapled; never imply that an ad-hoc build is notarized.
5. Create the prerelease from the validated tag and upload only the validated DMG.
6. Compare the GitHub asset digest and size with the local validated artifact before considering the prerelease complete.
7. Export one CSV and one JSON task report, confirm both are valid UTF-8 text, and verify they contain local paths but no file contents or raw image metadata payloads.
8. Reveal one published output and the last exported report, copy one concise task error, then clear history and confirm that no local file was deleted.

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
- Confirm Office, unsupported image metadata formats, PDF rasterization, and preview tools remain disabled until their bundled engines or execution paths are intentionally added.

Current image conversion checklist:

- Convert PNG to JPG and confirm a non-empty output is written to the source-adjacent `converted` folder.
- Convert JPG/JPEG to WebP and WebP to PNG.
- Convert JPEG fixtures with EXIF orientations 1, 6, 3, and 8 and confirm the output dimensions and visible pixel orientation are normalized.
- Repeat a conversion with a Chinese filename and a path containing spaces.
- Create an output collision and confirm the backend selects an incremented name instead of overwriting it.
- Hash or otherwise compare the source image before and after conversion and confirm it is unchanged.
- Confirm AVIF, TIFF/TIF, HEIC, GIF, RAW, and images-to-PDF remain disabled.

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

Current image metadata cleanup checklist:

- Clean a JPEG containing EXIF, GPS, camera/device, XMP, or IPTC metadata and confirm the supported metadata is absent from the output.
- Clean a JPEG with non-default EXIF orientation and confirm its visible orientation and dimensions are normalized before metadata is removed; note that this path requires a high-quality JPEG re-encode.
- Clean PNG text metadata and WebP EXIF/XMP chunks and confirm dimensions and decoded pixels remain unchanged.
- Confirm outputs keep the source format, use `cleaned` collision-safe names, and never overwrite existing files.
- Use an image with no removable metadata and confirm the task reports `未发现可清理的元数据，未生成新文件` without publishing a final output.
- Confirm HEIC, GIF, TIFF, RAW, PDF, Office, video, and audio metadata cleanup remain unavailable.
- Confirm cancellation, failure, and unchanged results leave no final output or `.localconvert-task-*` directory, and confirm source hashes remain unchanged.
- Treat cleanup as best effort: confirm the UI does not claim forensic-grade or complete removal of every private vendor field.

Current task report export checklist:

- Complete at least one successful task and create failed and cancelled results where practical.
- Export CSV and JSON reports through the native save dialog using the timestamped default filenames.
- Confirm JSON uses a top-level `appVersion`, `generatedAt`, and `tasks` object structure and is pretty printed.
- Confirm CSV has a header row, remains valid UTF-8, and correctly escapes commas, quotes, carriage returns, and line feeds.
- Confirm available byte savings and duration fields are represented consistently.
- Confirm cancelling the save dialog is reported as a cancelled export, not as a task failure, and does not clear task history.
- Confirm reports may contain local source/output paths but contain no source file bytes and no raw EXIF, GPS, XMP, or IPTC values.

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

- 继续加固现有 PDF、图片与任务报告流程，并推进 macOS Developer ID 签名和公证。
- 为 Windows x64 准备可验证的 qpdf、image-engine、许可证与安装包资产，再发布首个 Windows 预览版。
- 仅在 LibreOffice 运行时、中文字体、隔离 profile、超时和打包验证完整后启用 Office 转 PDF。
- 在 PDFium 本地资产和页面验证链路可靠后增加 PDF 预览与 PDF 转图片。
- 在排序、页面尺寸和原子输出规则完整后增加多图合成 PDF。
- 只在解码、方向、内存上限、许可证和跨平台资产均经过验证后扩展图片格式。
- 持续执行干净安装、离线、取消、防覆盖、许可证和资产摘要验证。

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
