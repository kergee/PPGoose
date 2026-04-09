import { useStore } from "../store/useStore";
import { FileRow } from "./FileRow";

export function FileList() {
  const files = useStore((s) => s.files);

  if (files.length === 0) return null;

  return (
    <div className="flex-1 overflow-y-auto rounded-xl bg-surface-1 border border-surface-3">
      {/* Header */}
      <div className="flex items-center gap-3 px-4 py-2 border-b border-surface-3 sticky top-0 bg-surface-1 z-10">
        <span className="w-10" />
        <span className="flex-1 text-[11px] font-medium text-neutral-500 uppercase tracking-wide">文件</span>
        <span className="text-[11px] text-neutral-500 w-20 text-right">原始</span>
        <span className="w-6" />
        <span className="text-[11px] text-neutral-500 w-20 text-right">压缩后</span>
        <span className="text-[11px] text-neutral-500 w-12 text-right">节省</span>
        <span className="w-6" />
        <span className="w-6" />
      </div>

      {/* Rows */}
      <div className="py-1">
        {files.map((f) => (
          <FileRow key={f.id} file={f} />
        ))}
      </div>
    </div>
  );
}
