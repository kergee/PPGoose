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
  isRestoring: boolean;

  // Actions
  addPaths: (paths: string[]) => Promise<void>;
  startCompression: () => Promise<void>;
  clearAll: () => void;
  clearDone: () => void;
  updateOptions: (patch: Partial<CompressOptions>) => void;
  removeFile: (id: string) => void;
  restoreFile: (recordId: string) => Promise<void>;
  message: string;
}

export const useStore = create<Store>((set, get) => ({
  files: [],
  options: {
    quality: 0,
    outputMode: "overwrite",
    customDir: undefined,
    suffix: undefined,
    convertWebpTo: null,
    smartQuality: false,
    pngStrategy: "detail",
    jpegStrategy: "balanced",
    allowResize: false,
    qualityFloor: 40,
  },
  isCompressing: false,
  isRestoring: false,
  message: "",
  restoreFile: async (recordId) => {
    if (get().isCompressing || get().isRestoring) throw new Error("请等待当前操作完成");
    set({isRestoring:true,message:""});
    try { await tauriApi.restoreFile(recordId); }
    catch (error) { set({message:String(error)}); throw error; }
    finally { set({isRestoring:false}); }
    set((s) => ({message:"原图已恢复",files:s.files.map((f)=>f.recordId===recordId ? {
      ...f,status:"pending",compressedSize:undefined,outputPath:undefined,recordId:undefined,overwritten:false,warning:undefined,error:undefined,resized:undefined,width:undefined,height:undefined,targetSize:undefined,keptOriginal:undefined,elapsedMs:undefined,optionsUsed:undefined,
    }:f)}));
  },

  addPaths: async (paths) => {
    const { files: scanned } = await tauriApi.scanPaths(paths);
    if (scanned.length === 0) return;

    // Dedupe against files already in the list AND within this batch
    const seen = new Set(get().files.map((f) => f.path));
    const newItems: FileItem[] = [];
    for (const meta of scanned) {
      if (seen.has(meta.path)) continue;
      seen.add(meta.path);
      const parts = meta.path.replace(/\\/g, "/").split("/");
      const name = parts[parts.length - 1];
      const ext = name.split(".").pop()?.toLowerCase() ?? "";
      newItems.push({
        id: genId(),
        path: meta.path,
        name,
        ext,
        originalSize: meta.size,
        status: "pending",
      });
    }

    if (newItems.length === 0) return;
    set((s) => ({ files: [...s.files, ...newItems] }));
  },

  startCompression: async () => {
    if (get().isCompressing || get().isRestoring) return;
    const { files, options } = get();
    const pending = files.filter((f) => f.status === "pending" || f.status === "error");
    if (pending.length === 0) return;

    set({ isCompressing: true, message:"" });

    // Mark all as compressing
    set((s) => ({
      files: s.files.map((f) =>
        f.status === "pending" || f.status === "error" ? { ...f, status: "compressing", error:undefined, elapsedMs:undefined, optionsUsed:{...options} } : f
      ),
    }));

    // Subscribe to progress events BEFORE starting batch
    let unlisten: (() => void) | undefined;
    const receive = (payload: ProgressEvent) => {
      set((s) => ({files:s.files.map((f) => {
        if(f.id!==payload.fileId)return f;
        if(payload.result) {const r=payload.result;return {...f,status:"done",originalSize:r.originalSize,compressedSize:r.compressedSize,outputPath:r.outputPath,
          elapsedMs:payload.elapsedMs,recordId:r.recordId,overwritten:r.overwritten,warning:r.warning??undefined,resized:r.resized,width:r.width,height:r.height,targetSize:r.targetSize??undefined,keptOriginal:r.keptOriginal};}
        return {...f,status:"error",error:payload.error,elapsedMs:payload.elapsedMs};
      })}));
    };
    try {
    unlisten = await listen<ProgressEvent>(
      "compress://progress",
      ({payload})=>receive(payload)
    );

      const fileIds: [string, string][] = pending.map((f) => [f.id, f.path]);
      const results = await tauriApi.compressBatch(fileIds, options);
      results.forEach(receive);
    } catch (error) {
      set((s)=>({message:String(error),files:s.files.map((f)=>f.status==="compressing" ? {...f,status:"error",error:String(error)}:f)}));
    } finally {
      unlisten?.();
      set({ isCompressing: false });
    }
  },

  clearAll: () => {if(!get().isCompressing && !get().isRestoring)set({ files: [] });},
  clearDone: () =>
    set((s) => ({ files: s.files.filter((f) => f.status !== "done") })),
  removeFile: (id) =>
    {if(!get().isCompressing && !get().isRestoring)set((s) => ({ files: s.files.filter((f) => f.id !== id) }));},
  updateOptions: (patch) =>
    set((s) => ({ options: { ...s.options, ...patch } })),
}));
