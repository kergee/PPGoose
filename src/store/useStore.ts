import { create } from "zustand";
import { CompressOptions, FileItem, ProgressEvent } from "../types";
import { tauriApi } from "../lib/tauri";
import { listen } from "@tauri-apps/api/event";

// Simple uuid fallback (avoid extra dep)
let _counter = 0;
const genId = () => `${Date.now()}-${++_counter}`;

interface Store {
  files: FileItem[];
  options: CompressOptions;
  isCompressing: boolean;

  // Actions
  addPaths: (paths: string[]) => Promise<void>;
  startCompression: () => Promise<void>;
  clearAll: () => void;
  clearDone: () => void;
  updateOptions: (patch: Partial<CompressOptions>) => void;
  removeFile: (id: string) => void;
}

export const useStore = create<Store>((set, get) => ({
  files: [],
  options: {
    quality: 0,
    outputMode: "overwrite",
    customDir: undefined,
    suffix: undefined,
    convertWebpTo: null,
  },
  isCompressing: false,

  addPaths: async (paths) => {
    const { files: scanned } = await tauriApi.scanPaths(paths);
    if (scanned.length === 0) return;

    const existing = new Set(get().files.map((f) => f.path));
    const newItems: FileItem[] = scanned
      .filter((p) => !existing.has(p))
      .map((p) => {
        const parts = p.replace(/\\/g, "/").split("/");
        const name = parts[parts.length - 1];
        const ext = name.split(".").pop()?.toLowerCase() ?? "";
        return {
          id: genId(),
          path: p,
          name,
          ext,
          originalSize: 0,
          status: "pending",
        };
      });

    if (newItems.length === 0) return;

    // Fetch file sizes asynchronously
    set((s) => ({ files: [...s.files, ...newItems] }));

    // Get sizes via compress_file dry-run is too slow;
    // read size via file metadata command instead
    // For now, size will be filled in after compression.
  },

  startCompression: async () => {
    const { files, options } = get();
    const pending = files.filter((f) => f.status === "pending");
    if (pending.length === 0) return;

    set({ isCompressing: true });

    // Mark all as compressing
    set((s) => ({
      files: s.files.map((f) =>
        f.status === "pending" ? { ...f, status: "compressing" } : f
      ),
    }));

    // Subscribe to progress events BEFORE starting batch
    const unlisten = await listen<ProgressEvent>(
      "compress://progress",
      ({ payload }) => {
        set((s) => ({
          files: s.files.map((f) => {
            if (f.id !== payload.fileId) return f;
            if (payload.result) {
              return {
                ...f,
                status: "done",
                originalSize: payload.result.originalSize,
                compressedSize: payload.result.compressedSize,
                outputPath: payload.result.outputPath,
              };
            }
            return { ...f, status: "error", error: payload.error };
          }),
        }));
      }
    );

    try {
      const fileIds: [string, string][] = pending.map((f) => [f.id, f.path]);
      await tauriApi.compressBatch(fileIds, options);
    } finally {
      unlisten();
      set({ isCompressing: false });
    }
  },

  clearAll: () => set({ files: [] }),
  clearDone: () =>
    set((s) => ({ files: s.files.filter((f) => f.status !== "done") })),
  removeFile: (id) =>
    set((s) => ({ files: s.files.filter((f) => f.id !== id) })),
  updateOptions: (patch) =>
    set((s) => ({ options: { ...s.options, ...patch } })),
}));
