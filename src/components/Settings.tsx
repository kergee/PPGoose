import { useState } from "react";
import { useStore } from "../store/useStore";
import { tauriApi } from "../lib/tauri";

export function Settings() {
  const [open, setOpen] = useState(false);
  const options = useStore((s) => s.options);
  const updateOptions = useStore((s) => s.updateOptions);

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
          <div className="absolute right-0 top-10 z-20 w-80 bg-surface-2 border border-surface-3 rounded-xl shadow-2xl p-4 animate-slide-up">
            <h3 className="text-sm font-semibold text-neutral-200 mb-4">压缩设置</h3>

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
                0 = 自动选择最优参数（推荐）
              </p>
            </div>

            {/* Output mode */}
            <div className="mb-4">
              <label className="block text-xs text-neutral-400 mb-2">输出方式</label>
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
                  { value: "jpeg", label: "→ JPEG（体积最小，不含透明）" },
                  { value: "avif", label: "→ AVIF（最新格式，~50% 更小）" },
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
          </div>
        </>
      )}
    </div>
  );
}
