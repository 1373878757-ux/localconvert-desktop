# LocalConvert Desktop 本地文件转换工具

**by 田宸宇**

> 纯本地、无上传、无服务器、无遥测的桌面文件处理工具。

**当前稳定版本： [Preview 0.8.0](https://github.com/1373878757-ux/localconvert-desktop/releases/tag/preview-0.8.0-output-rules-macos)** · macOS Apple Silicon

LocalConvert Desktop 面向需要批量处理 PDF 和常见图片、同时重视文件隐私的办公用户与个人用户。它把 PDF 结构操作、JPG/PNG/WebP 图片处理、任务队列和结果报告整合进一个本地桌面工作台；文件不上传、不经过转换服务器，原文件默认不会被覆盖。纯本地设计既减少敏感文档离开设备的风险，也让已支持的处理能力可以离线运行。

![LocalConvert Desktop Preview 0.6.1 主工作台](assets/screenshots/main-window.png)

## 核心能力

- **PDF 工具**：合并、拆分、旋转和抽取指定页面。
- **图片格式转换**：JPG/JPEG、PNG、WebP 之间的已启用方向。
- **图片改尺寸**：适应范围、仅宽度、仅高度，保持宽高比且默认不放大。
- **图片压缩与优化**：JPEG/WebP 质量压缩与 PNG 无损优化；结果未变小时不发布文件。
- **元数据隐私清理**：尽力清理 JPG/PNG/WebP 中常见的隐私元数据。
- **批量任务报告**：导出 CSV/JSON，记录成功、失败、取消、跳过等结果。
- **任务体验**：真实后台取消、打开输出/报告位置、复制精简错误、确认后清空界面记录。
- **本地偏好与预设**：记住常用参数；五个内置预设只填充表单，不自动处理文件。
- **输出位置与命名**：支持默认 `converted`、同源目录、每次询问和记住的自定义目录，以及安全前缀、后缀和日期时间令牌。

## 隐私与安全设计

- 文件在本机处理，不上传、不依赖转换服务器，也不采集遥测。
- 引擎先写入任务专属临时位置，验证后再原子发布到当前安全输出位置。
- 原文件不覆盖；输出重名时安全递增，取消或失败只清理任务自有临时文件。
- 报告可能包含本地路径，但不包含文件内容或原始 EXIF/GPS/XMP/IPTC 数据。

## 技术实现

| 层级 | 实现 |
| --- | --- |
| 桌面与前端 | Tauri v2、React、TypeScript |
| 本地后端 | Rust backend commands、后台任务注册表、超时与真实取消 |
| PDF 引擎 | 内置 qpdf sidecar |
| 图片引擎 | 第一方 Rust `image-engine` sidecar |
| 输出安全 | 参数数组执行、任务临时目录、输出验证、原子防覆盖发布 |

## 工程质量

- Rust 单元与集成测试覆盖请求校验、任务状态、取消、输出安全和引擎行为。
- `cargo clippy --all-targets -- -D warnings` 作为零警告质量门禁。
- manifest 驱动的 engine verifier 检查引擎资产、SHA-256、权限和许可证。
- 发布候选记录精确 DMG SHA-256，并在 GitHub prerelease 上传后核对 asset digest 与大小。

## 当前限制

- 当前公开预览仅面向 macOS Apple Silicon，使用未签名或 ad-hoc 签名构建，尚未完成 Developer ID 公证。
- HEIC 转换、改尺寸、压缩、元数据清理和方向处理尚未启用。
- Office 文档转换尚未启用；视频和音频转换不在当前产品目标内。
- 元数据清理是最佳努力的隐私辅助功能，不是法证级彻底清除。

## 展示截图

| 场景 | 真实截图 |
| --- | --- |
| 主工作台 | [main-window.png](assets/screenshots/main-window.png) |
| PDF 工具 | [pdf-tools.png](assets/screenshots/pdf-tools.png) |
| 图片工具 | [image-tools.png](assets/screenshots/image-tools.png) |
| 任务结果 | [task-results.png](assets/screenshots/task-results.png) |
| 报告导出 | [report-export.png](assets/screenshots/report-export.png) |
| About 与版本状态 | [about-panel.png](assets/screenshots/about-panel.png) |

截图来自真实 Preview 0.6.1 应用，并使用安全合成文件，不包含个人文档或真实隐私元数据。

## 一句话作品集介绍

- **技术版**：使用 Tauri v2、React、TypeScript 与 Rust 构建的纯本地文件转换工具，通过 qpdf 和第一方图片 sidecar 实现可取消、可验证、原子防覆盖的桌面处理流程。
- **产品版**：一款面向办公用户的本地 PDF 与图片处理工具，不上传文件、不依赖服务器，并用清晰的任务队列管理批量结果。
- **简历版**：独立设计并实现 LocalConvert Desktop，完成 PDF/图片处理、后台任务取消、安全输出、引擎资产校验及 macOS prerelease 发布链路。

## 60 秒讲解稿

LocalConvert Desktop 是我为重视隐私的办公用户设计的一款纯本地文件转换工具。用户可以把 PDF、JPG、PNG 或 WebP 拖进桌面工作台，在本机完成 PDF 合并、拆分、旋转、抽取页，以及图片转换、改尺寸、压缩和元数据清理。前端使用 React 和 TypeScript，Tauri 的 Rust 后端负责路径校验、任务队列、超时、真实取消和原子防覆盖输出，qpdf 与第一方 image-engine 随应用打包。Preview 0.8.0 会在本机记住常用参数，并提供只填充表单、不自动执行的图片预设，以及默认 `converted`、同源目录、每次询问和记住的自定义输出目录。处理过程不上传文件、不依赖服务器，也不采集遥测；CSV/JSON 报告只记录任务结果，不包含文件内容或原始隐私元数据。当前 Preview 0.8.0 面向 macOS Apple Silicon，仍是未完成 Developer ID 公证的预览构建，HEIC 和 Office 等能力尚未启用。

更完整的产品与架构说明见 [项目展示](showcase.md)，录制节奏见 [60 秒演示脚本](demo-script.md)。
