export type FileStatus = "pending" | "compressing" | "done" | "error";

export interface FileItem {
  id: string;
  path: string;
  name: string;
  ext: string;
  originalSize: number;
  compressedSize?: number;
  outputPath?: string;
  status: FileStatus;
  error?: string;
}

export type ConvertTarget = "png" | "jpeg" | "avif";
export type ConvertAvifTarget = "png" | "jpeg" | "webP";

export interface CompressOptions {
  quality: number;       // 0 = auto
  outputMode: "overwrite" | "subfolder" | "custom";
  customDir?: string;
  suffix?: string;
  /** If set, WebP files are converted to this format instead of compressed */
  convertWebpTo?: ConvertTarget | null;
  /** If set, AVIF files are converted to this format instead of compressed */
  convertAvifTo?: ConvertAvifTarget | null;
}

export interface CompressResult {
  inputPath: string;
  outputPath: string;
  originalSize: number;
  compressedSize: number;
}

export interface ProgressEvent {
  fileId: string;
  result?: CompressResult;
  error?: string;
}
