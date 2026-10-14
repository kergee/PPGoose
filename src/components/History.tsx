import { useEffect, useState } from "react";
import { tauriApi } from "../lib/tauri";
import { HistoryRecord } from "../types";
import { useStore } from "../store/useStore";

export function History({ onClose }: { onClose: () => void }) {
  const [records, setRecords] = useState<HistoryRecord[]>([]);
  const [directory, setDirectory] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState("");
  const [loading, setLoading] = useState(true);
  const restoreFile = useStore((s) => s.restoreFile);
  const compressing = useStore((s) => s.isCompressing || s.isRestoring);
  const reload = async () => {
    const [rows, path] = await Promise.all([tauriApi.listHistory(), tauriApi.getDataDirectory()]);
    setRecords(rows.filter((r) => r.overwritten && r.backupPath));
    setDirectory(path);
  };
  useEffect(() => { reload().catch((e) => setError(String(e))).finally(() => setLoading(false)); }, []);
  const restore = async (id: string) => {
    setBusy(id); setError("");
    try { await restoreFile(id); await reload(); }
    catch (e) { setError(String(e)); }
    finally { setBusy(""); }
  };
  return (
    <div className="fixed inset-0 z-40 bg-black/70 flex items-center justify-center p-4" onClick={onClose}>
      <section role="dialog" aria-modal="true" aria-label="备份与恢复"
        className="w-full max-w-3xl max-h-[90vh] bg-surface-1 border border-surface-3 rounded-xl flex flex-col" onClick={(e) => e.stopPropagation()}>
        <header className="p-4 border-b border-surface-3 flex justify-between items-center">
          <h2 className="text-base font-semibold">备份与恢复</h2>
          <button onClick={onClose} aria-label="关闭备份窗口" className="px-2 text-neutral-400">✕</button>
        </header>
        <div className="p-4 text-xs text-neutral-400 space-y-2 shrink-0">
          <p>覆盖原图前自动备份，重启后仍可恢复。多次覆盖同一文件时，从最新记录依次恢复。</p>
          <p className="break-all select-text">数据目录：{directory || "读取中…"}</p>
          {error && <p role="alert" className="text-red-400 select-text">{error}</p>}
        </div>
        <div className="overflow-y-auto px-4 pb-4 min-h-0 space-y-2">
          {loading && <p className="text-sm text-neutral-500">正在读取记录…</p>}
          {!loading && records.length === 0 && <p className="text-sm text-neutral-500">还没有覆盖备份。另存输出不会修改原图。</p>}
          {records.map((r) => (
            <div key={r.id} className="bg-surface-2 rounded-lg p-3 flex gap-3 items-center">
              <div className="flex-1 min-w-0">
                <p className="text-sm truncate" title={r.inputPath}>{r.inputPath.replace(/\\/g, "/").split("/").pop()}</p>
                <p className="text-xs text-neutral-500 mt-1">{new Date(r.createdAt * 1000).toLocaleString()} · {(r.originalSize / 1024).toFixed(1)} KB · {r.state === "restored" ? "已恢复" : r.state === "done" ? "可恢复" : "待核验"}</p>
                <p className="text-xs text-neutral-600 truncate select-text" title={r.inputPath}>{r.inputPath}</p>
              </div>
              <button disabled={!!busy || compressing || r.state === "restored"} onClick={() => restore(r.id)}
                className="text-xs px-3 py-2 bg-goose-700 rounded-lg disabled:opacity-40">
                {busy === r.id ? "恢复中…" : r.state === "restored" ? "已恢复" : "恢复原图"}
              </button>
            </div>
          ))}
        </div>
        <p className="p-4 border-t border-surface-3 text-xs text-neutral-500">恢复会校验备份和当前文件。当前文件有其他改动时，会停止恢复；备份继续保留。</p>
      </section>
    </div>
  );
}
