# PPGoose 🪿

> 免费开源的跨平台本地图片压缩工具


PPGoose 整合开源图片编码器，按格式与所选策略比较压缩候选，提供自动质量、目标大小和手动参数。压缩收益取决于图片内容。

---

本次 0.2.0 功能与验证范围见 [更新记录](docs/更新记录-0.2.0.txt)。

## 功能特性

| 特性 | 说明 |
|------|------|
| **格式支持** | 压缩 PNG · JPG · GIF · WebP，可输出 **AVIF**（AVIF 暂不支持作为输入） |
| **自动选参** | 分析图片色彩复杂度，自动在无损/有损之间选择最优策略 |
| **PNG 策略** | 细节优先（默认）· 均衡压缩 · 像素无损；保护 APNG 与 16-bit PNG |
| **JPEG 策略** | 均衡（保留默认）· 细节优先 · 紧凑压缩（自动 67 / 4:4:4 / 上游表 3） |
| **CMS 独立核心** | 无 Tauri 依赖的 Rust 库与 JSON CLI；桌面端共用同一实现 |
| **GIF 差帧优化** | 对适合的动画生成像素无损变化区域候选，与 gifski 择优；校验时长和循环 |
| **极致模式** | 按感知质量（DSSIM）二分搜索最低可用编码质量，JPEG/WebP 自动质量时可选，收益取决于图片 |
| **色彩保真** | 自动修正 EXIF 方向；PNG/JPEG/静态 WebP 保留 ICC，PNG 缩放保留色彩标签（显示效果仍取决于查看器色彩管理） |
| **批量 + 子文件夹** | 拖入文件夹后递归扫描所有支持格式 |
| **并行压缩** | Rust 多线程，充分利用多核 |
| **输出方式** | 覆盖原文件 / 保存到 `compressed/` 子文件夹 / 自定义目录 |
| **写入保护** | 临时文件同步后原子提交；另存同名自动编号，失败不留下半成品 |
| **备份恢复** | 覆盖前自动保存原图与 SHA-256 记录；文件行或“备份与恢复”入口可恢复，重启后仍有效 |
| **图片对比** | 左右滑块、原图 100% 和尺寸显示；透明棋盘格，动图比较首帧 |
| **目标大小** | 设置 KB 上限、最低编码质量、最长边；可选允许自动缩小，未达目标报错并保留原图 |
| **文件名后缀** | 可选添加后缀（如 `_min`），避免覆盖原文件 |
| **WebP 格式转换** | WebP 真正编码为 PNG / JPEG / AVIF；PNG 保留解码像素，JPEG 透明区域使用白底 |
| **压缩统计** | 实时显示总节省空间与压缩率 |
| **纯本地** | 全部在本机完成，不上传任何文件 |
| **跨平台** | macOS · Windows · Linux |

## 压缩引擎

| 格式 | 引擎 | 策略 |
|------|------|------|
| PNG | `oxipng` + `imagequant` | 细节优先借鉴高质量量化上限，保留最低质量保护；均衡或像素无损可选，多候选择小 |
| JPG | `mozjpeg` | 渐进式 + Huffman 优化，自动修正 EXIF 方向，灰度图保持单通道；细节/研究策略全档 4:4:4 与上游表 3 |
| WebP | `libwebp` | 无损源保留完整 RGBA 与 ICC，有损源按 quality 重编码；可转换为 PNG / JPEG / AVIF；动画 WebP 暂不支持 |
| GIF | Rust 差帧编码 + `gifski` | 共享/局部调色板与变化矩形候选，和 gifski 择优；校验循环次数与帧延迟 |
| AVIF | `ravif` | 纯 Rust AV1 编码（仅输出），支持透明，实际体积和速度取决于图片 |

## 界面截图

*深色主题 · 拖入即用 · 实时进度*

```
┌─────────────────────────────────────────────────────┐
│  🪿 PPGoose  v0.2.0                             ⚙  │
├─────────────────────────────────────────────────────┤
│         ┌────────────────────────────────┐          │
│         │   将图片或文件夹拖到这里         │          │
│         │      或点击选择文件             │          │
│         └────────────────────────────────┘          │
├─────────────────────────────────────────────────────┤
│ 文件名           原始     →  压缩后   节省   状态    │
│ photo.jpg       2.3 MB  →  890 KB   61%   ✓       │
│ banner.png      540 KB  →  210 KB   61%   ✓       │
│ logo.webp        89 KB  →   34 KB   62%   ⟳       │
├─────────────────────────────────────────────────────┤
│ 文件数 3 · 已完成 2 · 节省 1.74 MB · 压缩率 61.2%  │
│                                [清除已完成] [全部清除]│
└─────────────────────────────────────────────────────┘
```

## 使用与数据保存

详细操作见 [0.2.0 使用说明](docs/使用说明-0.2.0.txt)。顶部“设置”可设定输出方式与目标大小；完成后，文件行提供“对比”和覆盖时的“恢复”。顶部“备份与恢复”可以查看以前的覆盖记录。

Windows 数据目录为 `%APPDATA%\com.ppgoose.app`，以“备份与恢复”中显示的实际路径为准。`history.json` 保存恢复记录，`backups/` 保存覆盖前原图，`previews/` 保存对比缓存。恢复后备份继续保留。另存文件默认不备份；旧版已覆盖的文件没有自动补建备份。压缩设置和队列当前只保存在本次会话内。

目标大小采用 1 KB = 1024 B，仅返回不超过上限的成功结果；不能保证任何目标都能达到。默认保持尺寸，只有勾选允许缩小或填写最长边才会调整尺寸。像素无损 PNG 和 APNG 不缩小；动画 WebP 仍跳过。WebP 转 PNG 在不缩小时保留解码像素，因此部分很小的目标可能无法实现。

队列栏提供“导出 CSV”，包含当次实际策略、耗时和错误；设置中的 Windows 右键入口将图片/文件夹送入单实例队列。入口仅添加文件，由你手动开始压缩。Windows 11 可在“显示更多选项”中查找。

## 开发环境

独立核心位于 `crates/ppgoose-core`，CMS 可直接调用 `compress_bytes`，或通过 `crates/ppgoose-cli` 的 UTF-8 JSON 协议调用。构建核心无需桌面框架和前端依赖；调用约定与示例见 [CMS 接入说明](docs/CMS-独立压缩核心.txt)。携带 ICC 的源暂拒绝转 AVIF，避免静默丢失色彩解释；JPEG 紧凑压缩不代表同等画质或体积保证。

```powershell
cargo test --manifest-path crates/Cargo.toml --locked
cargo build --manifest-path crates/Cargo.toml --release -p ppgoose-cli --locked
```

默认独立可执行文件在 `crates/target/release/`。

**前置要求：**
- [Rust](https://rustup.rs/) 1.90+（独立压缩核心仍为 1.89+）
- Node.js 18+
- macOS：Xcode Command Line Tools（`xcode-select --install`）
- Windows：[Visual C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
- Linux：`sudo apt install libwebkit2gtk-4.1-dev libssl-dev`

**运行开发模式：**

```bash
git clone <repo>
cd ppgoose
npm install
npm run tauri dev
```

**构建发行版：**

```bash
npm run tauri build
# 产物在 src-tauri/target/release/bundle/
```

## 技术栈

| 层 | 技术 |
|----|------|
| 桌面框架 | [Tauri 2](https://tauri.app/) |
| 前端 | React 18 · TypeScript · Tailwind CSS v3 |
| 状态管理 | Zustand |
| 后端 | Rust + Tokio + Rayon |
| 压缩库 | oxipng · imagequant · mozjpeg · libwebp · ravif · gifski · dssim · image-rs |

## 路线图


- [x] AVIF 输出支持（`ravif` 编码）
- [x] WebP 转换为 PNG / JPEG / AVIF
- [ ] AVIF 输入解码（需引入 dav1d 或等待成熟的纯 Rust 解码器）
- [x] 压缩前后对比预览（左右滑块）
- [x] 原子写入、同名保护、覆盖备份与重启后恢复
- [x] 目标大小控制与明确授权的尺寸缩小
- [ ] 浅色模式
- [x] Windows 图片/文件夹右键入口（设置中开关，单实例加入队列）
- [ ] macOS 快捷入口
- [x] 压缩报告导出（CSV，实际任务配置与耗时）

## 开源协议

MIT License
