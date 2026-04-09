import { FileItem } from "../types";
import { useStore } from "../store/useStore";

const EXT_COLORS: Record<string, string> = {
  png:  "bg-blue-500/20 text-blue-300",
  jpg:  "bg-orange-500/20 text-orange-300",
  jpeg: "bg-orange-500/20 text-orange-300",
  gif:  "bg-purple-500/20 text-purple-300",
  webp: "bg-teal-500/20 text-teal-300",
};

function formatSize(bytes: number): string {
  if (bytes === 0) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

function savingsPct(orig: number, comp: number): string {
  if (!orig || !comp) return "";
  const pct = ((1 - comp / orig) * 100).toFixed(0);
  return `${pct}%`;
}

interface Props {
  file: FileItem;
}

export function FileRow({ file }: Props) {
  const removeFile = useStore((s) => s.removeFile);
  const savings = file.compressedSize
    ? savingsPct(file.originalSize, file.compressedSize)
    : "";

  const isGood = file.compressedSize && file.compressedSize < file.originalSize;

  return (
    <div className="flex items-center gap-3 px-4 py-2.5 hover:bg-surface-2/60 group rounded-lg transition-colors animate-fade-in">
      {/* Extension badge */}
      <span className={`text-[10px] font-bold uppercase px-1.5 py-0.5 rounded ${EXT_COLORS[file.ext] ?? "bg-surface-3 text-neutral-400"}`}>
        {file.ext}
      </span>

      {/* Filename */}
      <span className="flex-1 text-sm text-neutral-200 truncate" title={file.path}>
        {file.name}
      </span>

      {/* Original size */}
      <span className="text-xs text-neutral-500 w-20 text-right tabular-nums">
        {formatSize(file.originalSize)}
      </span>

      {/* Arrow */}
      <span className="text-neutral-600 text-xs">→</span>

      {/* Compressed size */}
      <span className={`text-xs w-20 text-right tabular-nums ${
        file.status === "done" ? (isGood ? "text-goose-400" : "text-neutral-400") : "text-neutral-600"
      }`}>
        {file.status === "done" ? formatSize(file.compressedSize!) : "—"}
      </span>

      {/* Savings */}
      <span className={`text-xs font-semibold w-12 text-right tabular-nums ${
        isGood ? "text-goose-400" : "text-neutral-600"
      }`}>
        {savings || "—"}
      </span>

      {/* Status indicator */}
      <div className="w-6 flex justify-center">
        {file.status === "pending" && (
          <span className="w-1.5 h-1.5 rounded-full bg-surface-4" />
        )}
        {file.status === "compressing" && (
          <svg className="w-4 h-4 animate-spin-slow text-goose-500" viewBox="0 0 24 24" fill="none">
            <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="3" strokeDasharray="40 20" />
          </svg>
        )}
        {file.status === "done" && (
          <svg className="w-4 h-4 text-goose-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
            <polyline points="20 6 9 17 4 12" />
          </svg>
        )}
        {file.status === "error" && (
          <span title={file.error} className="w-4 h-4 text-red-400 text-sm flex items-center justify-center">✕</span>
        )}
      </div>

      {/* Remove button (hover only) */}
      <button
        onClick={() => removeFile(file.id)}
        className="opacity-0 group-hover:opacity-100 transition-opacity text-neutral-600 hover:text-neutral-300 text-xs"
        title="移除"
      >
        ✕
      </button>
    </div>
  );
}
