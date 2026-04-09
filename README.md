# PPGoose 🪿

> PP鸭的精神续作 · 免费开源的跨平台图片压缩工具

PPGoose 整合了业内最优秀的开源压缩算法，根据每张图片的特征自动选择最优参数，拖入即压，无需手动调参。

---

## 功能特性

| 特性 | 说明 |
|------|------|
| **五格式支持** | PNG · JPG · GIF · WebP · **AVIF** |
| **自动选参** | 分析图片色彩复杂度，自动在无损/有损之间选择最优策略 |
| **批量 + 子文件夹** | 拖入文件夹后递归扫描所有支持格式 |
| **并行压缩** | Rust 多线程，充分利用多核 |
| **输出方式** | 覆盖原文件 / 保存到 `compressed/` 子文件夹 / 自定义目录 |
| **文件名后缀** | 可选添加后缀（如 `_min`），避免覆盖原文件 |
| **WebP 格式转换** | WebP 可一键转为 PNG / JPEG / AVIF，扩展名自动变更 |
| **压缩统计** | 实时显示总节省空间与压缩率 |
| **纯本地** | 全部在本机完成，不上传任何文件 |
| **跨平台** | macOS · Windows · Linux |

## 压缩引擎

| 格式 | 引擎 | 策略 |
|------|------|------|
| PNG | `oxipng` + `imagequant` | 色彩少用有损量化，色彩丰富用无损优化；双路对比取最小 |
| JPG | `mozjpeg` | Mozilla 优化版编码器，同等画质比标准 libjpeg 小 30–40% |
| WebP | `libwebp` | 有损重编码，quality 可调；可转换为 PNG / JPEG / AVIF |
| GIF | `image` crate | 帧重编码 + 色板优化 |
| AVIF | `ravif` | 纯 Rust AV1 编码，同等画质比 JPEG 小 ~50% |

## 界面截图

*深色主题 · 拖入即用 · 实时进度*

```
┌─────────────────────────────────────────────────────┐
│  🪿 PPGoose  v0.1                               ⚙  │
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

## 开发环境

**前置要求：**
- [Rust](https://rustup.rs/) 1.70+
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
| 压缩库 | oxipng · imagequant · mozjpeg · libwebp · ravif · image-rs |

## 路线图

- [x] AVIF 格式支持（`ravif`）
- [x] WebP 转换为 PNG / JPEG / AVIF
- [ ] 压缩前后对比预览（左右滑块）
- [ ] 浅色模式
- [ ] 右键菜单快捷入口（macOS / Windows）
- [ ] 压缩报告导出（CSV）

## 开源协议

MIT License
