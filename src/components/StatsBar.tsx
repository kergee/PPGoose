import { useStore } from "../store/useStore";

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

export function StatsBar() {
  const files = useStore((s) => s.files);
  const isCompressing = useStore((s) => s.isCompressing);
  const clearAll = useStore((s) => s.clearAll);
  const clearDone = useStore((s) => s.clearDone);
  const startCompression = useStore((s) => s.startCompression);

  const done = files.filter((f) => f.status === "done");
  const pending = files.filter((f) => f.status === "pending");
  const total = files.length;

  const totalOriginal = done.reduce((a, f) => a + (f.originalSize ?? 0), 0);
  const totalCompressed = done.reduce((a, f) => a + (f.compressedSize ?? 0), 0);
  const saved = totalOriginal - totalCompressed;
  const savedPct = totalOriginal > 0
    ? ((saved / totalOriginal) * 100).toFixed(1)
    : "0";

  if (total === 0) return null;

  return (
    <div className="flex items-center gap-4 px-4 py-3 rounded-xl bg-surface-1 border border-surface-3">
      {/* Stats */}
      <div className="flex items-center gap-6 flex-1">
        <Stat label="文件数" value={`${total}`} />
        {done.length > 0 && (
          <>
            <Stat label="已完成" value={`${done.length}`} accent />
            <Stat label="节省空间" value={formatSize(saved)} accent />
            <Stat label="压缩率" value={`${savedPct}%`} accent />
          </>
        )}
      </div>

      {/* Actions */}
      <div className="flex items-center gap-2">
        {pending.length > 0 && !isCompressing && (
          <button
            onClick={startCompression}
            className="px-4 py-1.5 rounded-lg bg-goose-600 hover:bg-goose-500 text-white text-sm font-medium transition-colors"
          >
            开始压缩 {pending.length > 0 && `(${pending.length})`}
          </button>
        )}
        {isCompressing && (
          <button disabled className="px-4 py-1.5 rounded-lg bg-goose-700/50 text-goose-300 text-sm font-medium cursor-wait">
            压缩中…
          </button>
        )}
        {done.length > 0 && !isCompressing && (
          <button
            onClick={clearDone}
            className="px-3 py-1.5 rounded-lg bg-surface-2 hover:bg-surface-3 text-neutral-400 text-sm transition-colors"
          >
            清除已完成
          </button>
        )}
        <button
          onClick={clearAll}
          className="px-3 py-1.5 rounded-lg bg-surface-2 hover:bg-surface-3 text-neutral-400 text-sm transition-colors"
        >
          全部清除
        </button>
      </div>
    </div>
  );
}

function Stat({ label, value, accent }: { label: string; value: string; accent?: boolean }) {
  return (
    <div className="flex flex-col">
      <span className="text-[10px] text-neutral-500 uppercase tracking-wide">{label}</span>
      <span className={`text-sm font-semibold tabular-nums ${accent ? "text-goose-400" : "text-neutral-200"}`}>
        {value}
      </span>
    </div>
  );
}
