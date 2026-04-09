import { useEffect, useRef, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { tauriApi } from "../lib/tauri";
import { useStore } from "../store/useStore";

export function DropZone() {
  const addPaths = useStore((s) => s.addPaths);
  const [isDragging, setIsDragging] = useState(false);
  const appWindow = useRef(getCurrentWebviewWindow());

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    appWindow.current.onDragDropEvent((event) => {
      if (event.payload.type === "over") {
        setIsDragging(true);
      } else if (event.payload.type === "drop") {
        setIsDragging(false);
        const paths = (event.payload as any).paths as string[];
        if (paths?.length) addPaths(paths);
      } else {
        setIsDragging(false);
      }
    }).then((fn) => { unlisten = fn; });

    return () => { unlisten?.(); };
  }, [addPaths]);

  const handlePickFiles = async (e: React.MouseEvent) => {
    e.stopPropagation();
    const paths = await tauriApi.pickFiles();
    if (paths.length) addPaths(paths);
  };

  const handlePickFolder = async (e: React.MouseEvent) => {
    e.stopPropagation();
    const paths = await tauriApi.pickFolders();
    if (paths.length) addPaths(paths);
  };

  return (
    <div
      className={`
        flex flex-col items-center justify-center gap-3
        w-full rounded-xl border-2 border-dashed
        transition-all duration-200 select-none
        ${isDragging
          ? "border-goose-500 bg-goose-500/10 scale-[1.01]"
          : "border-surface-3 bg-surface-1"
        }
      `}
      style={{ minHeight: 180 }}
    >
      <GooseIcon dragging={isDragging} />
      <div className="text-center">
        <p className="text-sm font-medium text-neutral-300">
          {isDragging ? "松手即可添加" : "将图片或文件夹拖到这里"}
        </p>
        <p className="text-xs text-neutral-500 mt-1">
          支持 PNG · JPG · GIF · WebP，自动递归子文件夹
        </p>
      </div>
      <div className="flex gap-2 mt-1">
        <button
          onClick={handlePickFiles}
          className="px-3 py-1.5 text-xs rounded-lg bg-surface-2 hover:bg-surface-3 text-neutral-300 hover:text-neutral-100 border border-surface-3 hover:border-goose-600 transition-all cursor-pointer"
        >
          选择文件
        </button>
        <button
          onClick={handlePickFolder}
          className="px-3 py-1.5 text-xs rounded-lg bg-surface-2 hover:bg-surface-3 text-neutral-300 hover:text-neutral-100 border border-surface-3 hover:border-goose-600 transition-all cursor-pointer"
        >
          选择文件夹
        </button>
      </div>
    </div>
  );
}

function GooseIcon({ dragging }: { dragging: boolean }) {
  return (
    <div className={`transition-transform duration-300 ${dragging ? "scale-110" : ""}`}>
      <svg width="52" height="52" viewBox="0 0 52 52" fill="none">
        {/* Simple goose silhouette */}
        <ellipse cx="26" cy="34" rx="14" ry="10" fill={dragging ? "#22c55e" : "#2a2a2a"} />
        <ellipse cx="26" cy="34" rx="14" ry="10" fill={dragging ? "#22c55e33" : "#1f1f1f"} />
        <circle cx="36" cy="20" r="8" fill={dragging ? "#22c55e" : "#333"} />
        <ellipse cx="43" cy="18" rx="5" ry="3" fill={dragging ? "#4ade80" : "#404040"} />
        <circle cx="38" cy="18" r="1.5" fill={dragging ? "#fff" : "#aaa"} />
        <circle cx="38.6" cy="17.5" r="0.6" fill="#000" />
        <path
          d="M26 24 Q28 18 36 20"
          stroke={dragging ? "#22c55e" : "#333"}
          strokeWidth="3"
          strokeLinecap="round"
          fill="none"
        />
      </svg>
    </div>
  );
}
