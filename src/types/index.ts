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

export interface CompressOptions {
  quality: number;       // 0 = auto
  outputMode: "overwrite" | "subfolder" | "custom";
  customDir?: string;
  suffix?: string;
  /** If set, WebP files are converted to this format instead of compressed */
  convertWebpTo?: ConvertTarget | null;
  /** 极致模式: perceptual-quality search (slower, usually smaller). Only applies when quality is 0 (auto) */
  smartQuality?: boolean;
}

/** A scanned input file with its on-disk size */
export interface FileMeta {
  path: string;
  size: number;
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
