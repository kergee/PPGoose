import { invoke } from "@tauri-apps/api/core";
import { CompressOptions, CompressResult, FileMeta, HistoryRecord, PreviewInfo, ProgressEvent } from "../types";

export const tauriApi = {
  exportCsv: (csv: string) => invoke<string | null>("export_csv", { csv }),
  takeLaunchPaths: () => invoke<string[]>("take_launch_paths"),
  contextMenuStatus: () => invoke<import("../types").ContextMenuStatus>("context_menu_status"),
  setContextMenu: (enabled: boolean) => invoke<import("../types").ContextMenuStatus>("set_context_menu", { enabled }),
  // Strategy options travel through the same typed IPC contract as the core.
  getPreview: (recordId: string) => invoke<PreviewInfo>("get_preview", { recordId }),
  listHistory: () => invoke<HistoryRecord[]>("list_history"),
  restoreFile: (recordId: string) => invoke<HistoryRecord>("restore_file", { recordId }),
  getDataDirectory: () => invoke<string>("get_data_directory"),
  scanPaths: (paths: string[]) =>
    invoke<{ files: FileMeta[] }>("scan_paths", { paths }),

  compressFile: (path: string, options: CompressOptions) =>
    invoke<CompressResult>("compress_file", { path, options }),

  compressBatch: (
    fileIds: [string, string][],
    options: CompressOptions
  ) => invoke<ProgressEvent[]>("compress_batch", { fileIds, options }),

  pickFiles: () => invoke<string[]>("pick_files"),

  pickFolder: () => invoke<string | null>("pick_folder"),

  pickFolders: () => invoke<string[]>("pick_folders"),
};
