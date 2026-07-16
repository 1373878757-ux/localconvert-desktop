# GitHub Pages 发布与验证

本指南用于把 LocalConvert Desktop 的静态展示页从仓库 `main` 分支的 `/docs` 目录发布到 GitHub Pages，并在发布后完成基础验证。

预期公开地址：<https://1373878757-ux.github.io/localconvert-desktop/>

## 发布前提

- 页面入口已经存在于 [`docs/index.html`](index.html)。
- 样式和截图均使用 `docs/` 内的相对本地路径。
- GitHub Pages 必须在仓库设置中显式启用。
- 如果仓库是 Private，当前 GitHub 套餐必须支持私有仓库 Pages。若 **Settings > Pages** 无法启用并提示套餐不支持，需要先决定公开仓库、升级套餐，或改用独立的公开展示仓库；这不是页面代码故障。

## 手动启用

1. 打开 GitHub 仓库：<https://github.com/1373878757-ux/localconvert-desktop>。
2. 进入 **Settings**。
3. 在侧边栏进入 **Pages**。
4. 在 **Build and deployment** 下，将 **Source** 选择为 **Deploy from a branch**。
5. 将 **Branch** 选择为 `main`。
6. 将目录选择为 `/docs`。
7. 点击 **Save**。
8. 等待 GitHub Pages 部署完成。首次部署和 HTTPS 证书配置可能需要几分钟。
9. 打开：<https://1373878757-ux.github.io/localconvert-desktop/>。

## 发布后检查

- [ ] 页面通过 HTTPS 正常加载。
- [ ] 页面标题和首屏中的 `LocalConvert Desktop` 正常显示。
- [ ] `by 田宸宇` 正常显示。
- [ ] `Preview 0.8.0` 正常显示。
- [ ] 以下六张真实应用截图均可加载：主工作台、PDF 工具、图片工具、任务结果、报告导出和 About 面板。
- [ ] GitHub Release 下载链接可打开。
- [ ] README、项目展示和一页作品集简介链接可打开。
- [ ] 下载区显示的 SHA-256 为：

  ```text
  9791adbd9ea15fdb2b860a39cae56a419a1d8ff66099b2dc643d4192e17ebaa5
  ```

- [ ] 移动端页面没有横向溢出。
- [ ] 浏览器控制台没有错误。
- [ ] 页面没有外部脚本、CDN、跟踪代码、远程字体或远程图片。

本地 DMG 可使用以下命令复核：

```bash
shasum -a 256 "LocalConvert.Desktop_0.8.0_aarch64.dmg"
```

计算结果必须与上方 SHA-256 以及 GitHub Release 中对应资源的摘要一致。不一致时不要打开文件。

## 故障排查

### 页面显示 404

等待几分钟后刷新，并确认仓库 **Settings > Pages** 已保存 `main` 和 `/docs`。如果设置页提示当前套餐不支持该仓库，请先处理仓库可见性或套餐前提，不要把 404 当作页面代码失败。

### 截图无法加载

检查 [`docs/index.html`](index.html) 中的截图引用是否仍为相对于 `docs/` 的路径，例如：

```text
assets/screenshots/main-window.png
```

同时确认对应文件已经提交到 `main` 分支。

### 显示了错误页面或旧内容

确认 Pages 来源是 `main` 分支的 `/docs` 目录，并等待最新部署完成。必要时在 Pages 设置页检查最近一次部署状态。

### HTTPS 警告

首次发布后可能仍在配置证书。等待证书签发完成后再刷新，不要改用不安全的 HTTP 链接。

## 更新页面

后续修改 `docs/index.html`、`docs/assets/site.css` 或截图并推送到 `main` 后，GitHub Pages 会重新部署。发布前仍应检查相对路径、移动端布局和外部资源边界。
