import { invoke } from "@tauri-apps/api/core";
import { CompressOptions, CompressResult } from "../types";

export const tauriApi = {
  scanPaths: (paths: string[]) =>
    invoke<{ files: string[] }>("scan_paths", { paths }),

  compressFile: (path: string, options: CompressOptions) =>
    invoke<CompressResult>("compress_file", { path, options }),

  compressBatch: (
    fileIds: [string, string][],
    options: CompressOptions
  ) => invoke<void>("compress_batch", { fileIds, options }),

  pickFiles: () => invoke<string[]>("pick_files"),

  pickFolder: () => invoke<string | null>("pick_folder"),

  pickFolders: () => invoke<string[]>("pick_folders"),
};
