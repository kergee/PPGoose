import { DropZone } from "./components/DropZone";
import { FileList } from "./components/FileList";
import { StatsBar } from "./components/StatsBar";
import { Settings } from "./components/Settings";
import { useStore } from "./store/useStore";

export default function App() {
  const fileCount = useStore((s) => s.files.length);

  return (
    <div className="flex flex-col h-screen bg-surface-0 text-neutral-200 select-none overflow-hidden">
      {/* Titlebar */}
      <header
        className="flex items-center justify-between px-4 h-11 border-b border-surface-2 shrink-0"
        data-tauri-drag-region
      >
        <div className="flex items-center gap-2" data-tauri-drag-region>
          {/* Goose logo */}
          <svg width="20" height="20" viewBox="0 0 52 52" fill="none">
            <ellipse cx="26" cy="34" rx="14" ry="10" fill="#22c55e33" />
            <circle cx="36" cy="20" r="8" fill="#22c55e" />
            <ellipse cx="43" cy="18" rx="5" ry="3" fill="#4ade80" />
            <circle cx="38" cy="18" r="1.5" fill="white" />
            <circle cx="38.6" cy="17.5" r="0.6" fill="#000" />
            <path d="M26 24 Q28 18 36 20" stroke="#22c55e" strokeWidth="3" strokeLinecap="round" fill="none" />
          </svg>
          <span className="text-sm font-semibold text-neutral-100">PPGoose</span>
          <span className="text-xs text-neutral-600 font-mono">v0.1</span>
        </div>

        <Settings />
      </header>

      {/* Main content */}
      <main className="flex flex-col flex-1 gap-3 p-4 overflow-hidden">
        {/* Drop zone — shrinks when files are present */}
        <div className={`shrink-0 transition-all duration-300 ${fileCount > 0 ? "h-28" : "flex-1"}`}>
          <DropZone />
        </div>

        {/* File list */}
        {fileCount > 0 && <FileList />}

        {/* Stats & action bar */}
        <StatsBar />
      </main>
    </div>
  );
}
