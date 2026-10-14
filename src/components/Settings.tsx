import { useEffect, useState } from "react";
import type { ContextMenuStatus } from "../types";
import { useStore } from "../store/useStore";
import { tauriApi } from "../lib/tauri";

export function Settings() {
  const [menu, setMenu] = useState<ContextMenuStatus>();
  const [menuBusy, setMenuBusy] = useState(false);
  const [menuError, setMenuError] = useState("");
  const [open, setOpen] = useState(false);
  const options = useStore((s) => s.options);
  const updateOptions = useStore((s) => s.updateOptions);
  const compressing = useStore((s) => s.isCompressing || s.isRestoring);
  useEffect(() => {
    if (open) tauriApi.contextMenuStatus().then(setMenu).catch((e) => setMenuError(String(e)));
  }, [open]);

  const pickFolder = async () => {
    const folder = await tauriApi.pickFolder();
    if (folder) updateOptions({ customDir: folder });
  };

  return (
    <div className="relative">
      <button
        onClick={() => setOpen((v) => !v)}
        className={`p-2 rounded-lg transition-colors ${
          open ? "bg-surface-3 text-neutral-200" : "bg-surface-1 hover:bg-surface-2 text-neutral-500"
        }`}
        title="设置"
      >
        <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
        </svg>
      </button>

      {open && (
        <>
          {/* Backdrop */}
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} />

          {/* Panel */}
          <div className="absolute right-0 top-10 z-20 w-80 max-h-[calc(100vh-5rem)] overflow-y-auto bg-surface-2 border border-surface-3 rounded-xl shadow-2xl p-4 animate-slide-up">
            <h3 className="text-sm font-semibold text-neutral-200 mb-4">压缩设置</h3>
            <fieldset disabled={compressing}>
            {menu?.supported && <div className="mb-4">
              <label className="flex gap-2 items-center text-xs text-neutral-300">
                <input type="checkbox" checked={menu.enabled} disabled={menuBusy}
                  onChange={async (e) => {
                    const enabled = e.target.checked;
                    setMenuBusy(true); setMenuError("");
                    try { setMenu(await tauriApi.setContextMenu(enabled)); }
                    catch (error) {
                      setMenuError(String(error));
                      try { setMenu(await tauriApi.contextMenuStatus()); } catch { /* Keep the original error. */ }
                    } finally { setMenuBusy(false); }
                  }} className="accent-goose-500" />Windows 右键入口
              </label>
              <p className="text-xs text-neutral-600 mt-1">图片或文件夹右键 → 添加到 PPGoose。Windows 11 可能位于“显示更多选项”。仅加入队列，随后手动开始压缩。</p>
              <p className="text-xs text-neutral-600 mt-1">移动 exe 后重新开启；删除便携版前先关闭此入口。</p>
            </div>}
            {menuError && <p role="alert" className="text-xs text-red-400 mb-4">{menuError}</p>}

            {/* Quality */}
            <div className="mb-4">
              <label className="block text-xs text-neutral-400 mb-2">
                质量
                <span className="ml-2 text-goose-400 font-mono">
                  {options.quality === 0 ? "自动" : options.quality}
                </span>
              </label>
              <div className="flex items-center gap-2">
                <span className="text-xs text-neutral-600">低</span>
                <input
                  type="range" min={0} max={100}
                  value={options.quality}
                  onChange={(e) => updateOptions({ quality: +e.target.value })}
                  className="flex-1 accent-goose-500"
                />
                <span className="text-xs text-neutral-600">高</span>
              </div>
              <p className="text-xs text-neutral-600 mt-1">
                0 = 使用自动策略；手动质量只调整有损压缩
              </p>

              {options.quality === 0 && !options.targetSizeKb && (
                <label className="flex items-start gap-2 mt-2 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={options.smartQuality ?? false}
                    onChange={(e) => updateOptions({ smartQuality: e.target.checked })}
                    className="accent-goose-500 mt-0.5"
                  />
                  <span className="text-xs text-neutral-300">
                    极致模式
                    <span className="block text-neutral-600">
                      按感知质量（DSSIM）搜索最低可用质量，更慢但通常更小
                    </span>
                  </span>
                </label>
              )}
            </div>

            <div className="mb-4 space-y-2">
              <label className="block text-xs text-neutral-400">目标大小（KB）
                <input type="number" min="1" max="102400" step="1" value={options.targetSizeKb ?? ""}
                  onChange={(e)=>updateOptions({targetSizeKb:Number(e.target.value)||undefined,smartQuality:false})}
                  placeholder="留空不限制大小" className="block w-full mt-1 bg-surface-1 border border-surface-3 rounded-lg px-2 py-1.5 text-neutral-200" />
              </label>
              <label className="block text-xs text-neutral-400">最长边（像素）
                <input type="number" min="1" max="30000" step="1" value={options.maxEdge ?? ""}
                  onChange={(e)=>updateOptions({maxEdge:Number(e.target.value)||undefined})}
                  placeholder="留空保持尺寸" className="block w-full mt-1 bg-surface-1 border border-surface-3 rounded-lg px-2 py-1.5 text-neutral-200" />
              </label>
              {options.targetSizeKb && <>
                <label className="block text-xs text-neutral-400">最低编码质量
                  <input type="number" min="1" max="100" step="1" value={options.qualityFloor ?? 40}
                    onChange={(e)=>updateOptions({qualityFloor:Number(e.target.value)})}
                    className="block w-full mt-1 bg-surface-1 border border-surface-3 rounded-lg px-2 py-1.5 text-neutral-200" />
                </label>
                <label className="flex gap-2 items-center text-xs text-neutral-300"><input type="checkbox" checked={options.allowResize ?? false}
                  onChange={(e)=>updateOptions({allowResize:e.target.checked})} className="accent-goose-500" />必要时允许等比例缩小尺寸</label>
              </>}
              <p className="text-xs text-neutral-600">1 KB = 1024 B。达不到目标时会报错，原图保留。像素无损 PNG 和 APNG 保持原尺寸。</p>
            </div>

            <div className="mb-4">
              <label className="block text-xs text-neutral-400 mb-2">PNG 策略</label>
              <select
                value={options.pngStrategy ?? "detail"}
                onChange={(e) => updateOptions({ pngStrategy: e.target.value as "balanced" | "detail" | "lossless" })}
                className="w-full bg-surface-1 border border-surface-3 rounded-lg px-2 py-1.5 text-xs text-neutral-200"
              >
                <option value="detail">细节优先（推荐）</option>
                <option value="balanced">均衡压缩</option>
                <option value="lossless">像素无损</option>
              </select>
              <p className="text-xs text-neutral-600 mt-1">
                细节优先适合文字截图，仍可能有少量颜色变化。像素无损保留原像素。
              </p>
            </div>

            <div className="mb-4">
              <label className="block text-xs text-neutral-400 mb-2">JPEG 策略</label>
              <select
                value={options.jpegStrategy ?? "balanced"}
                onChange={(e) => updateOptions({ jpegStrategy: e.target.value as "balanced" | "detail" | "compact" })}
                className="w-full bg-surface-1 border border-surface-3 rounded-lg px-2 py-1.5 text-xs text-neutral-200"
              >
                <option value="balanced">均衡压缩（默认）</option>
                <option value="detail">细节优先（保留色彩边缘）</option>
                <option value="compact">紧凑压缩（自动质量 67）</option>
              </select>
              <p className="text-xs text-neutral-600 mt-1">
                细节优先和紧凑压缩保留更多色彩细节，文件可能更大。紧凑压缩仍为有损压缩；极致模式或目标大小会搜索质量。
              </p>
            </div>

            {/* Output mode */}
            <div className="mb-4">
              <label className="block text-xs text-neutral-400 mb-2">输出方式</label>
              <p className="text-xs text-neutral-600 mb-2">覆盖前自动备份；另存遇到同名文件自动编号。</p>
              <div className="flex flex-col gap-1.5">
                {[
                  { value: "overwrite", label: "覆盖原文件" },
                  { value: "subfolder", label: "保存到 compressed/ 子文件夹" },
                  { value: "custom",    label: "自定义目录" },
                ].map((opt) => (
                  <label key={opt.value} className="flex items-center gap-2 cursor-pointer">
                    <input
                      type="radio"
                      name="outputMode"
                      value={opt.value}
                      checked={options.outputMode === opt.value}
                      onChange={() => updateOptions({ outputMode: opt.value as any })}
                      className="accent-goose-500"
                    />
                    <span className="text-xs text-neutral-300">{opt.label}</span>
                  </label>
                ))}
              </div>

              {options.outputMode === "custom" && (
                <div className="mt-2 flex items-center gap-2">
                  <span className="text-xs text-neutral-500 flex-1 truncate">
                    {options.customDir ?? "未选择"}
                  </span>
                  <button
                    onClick={pickFolder}
                    className="text-xs px-2 py-1 bg-surface-3 hover:bg-surface-4 rounded text-neutral-300 transition-colors"
                  >
                    选择…
                  </button>
                </div>
              )}
            </div>

            {/* Suffix */}
            <div className="mb-4">
              <label className="block text-xs text-neutral-400 mb-1">
                文件名后缀
                <span className="ml-1 text-neutral-600">（可选，如 _min）</span>
              </label>
              <input
                type="text"
                value={options.suffix ?? ""}
                onChange={(e) => updateOptions({ suffix: e.target.value || undefined })}
                placeholder="留空则不加后缀"
                className="w-full bg-surface-1 border border-surface-3 rounded-lg px-3 py-1.5 text-xs text-neutral-200 placeholder-neutral-600 focus:outline-none focus:border-goose-600"
              />
            </div>

            {/* WebP conversion */}
            <div className="pt-3 border-t border-surface-3">
              <label className="block text-xs text-neutral-400 mb-2">
                WebP 转换目标格式
              </label>
              <div className="flex flex-col gap-1.5">
                {[
                  { value: null,   label: "不转换（直接压缩）" },
                  { value: "png",  label: "→ PNG（无损，兼容性最好）" },
                  { value: "jpeg", label: "→ JPEG（透明区域使用白底）" },
                  { value: "avif", label: "→ AVIF（支持透明）" },
                ].map((opt) => (
                  <label key={String(opt.value)} className="flex items-center gap-2 cursor-pointer">
                    <input
                      type="radio"
                      name="convertWebpTo"
                      checked={(options.convertWebpTo ?? null) === opt.value}
                      onChange={() => updateOptions({ convertWebpTo: opt.value as any })}
                      className="accent-goose-500"
                    />
                    <span className="text-xs text-neutral-300">{opt.label}</span>
                  </label>
                ))}
              </div>
              <p className="text-xs text-neutral-600 mt-2">
                仅对 .webp 文件生效，其他格式不受影响
              </p>
            </div>
            </fieldset>
          </div>
        </>
      )}
    </div>
  );
}
