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
  recordId?: string;
  overwritten?: boolean;
  warning?: string;
  resized?: boolean;
  width?: number;
  height?: number;
  targetSize?: number;
  keptOriginal?: boolean;
  elapsedMs?: number;
  optionsUsed?: CompressOptions;
}

export type ConvertTarget = "png" | "jpeg" | "avif";
export type PngStrategy = "balanced" | "detail" | "lossless";
export type JpegStrategy = "balanced" | "detail" | "compact";

export interface CompressOptions {
  quality: number;       // 0 = auto
  outputMode: "overwrite" | "subfolder" | "custom";
  customDir?: string;
  suffix?: string;
  /** If set, WebP files are converted to this format instead of compressed */
  convertWebpTo?: ConvertTarget | null;
  /** 极致模式: perceptual-quality search (slower, usually smaller). Only applies when quality is 0 (auto) */
  smartQuality?: boolean;
  /** PNG: balanced palette / high-detail palette / lossless pixels. */
  pngStrategy?: PngStrategy;
  jpegStrategy?: JpegStrategy;
  targetSizeKb?: number;
  maxEdge?: number;
  allowResize?: boolean;
  qualityFloor?: number;
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
  recordId: string;
  overwritten: boolean;
  warning?: string | null;
  resized: boolean;
  width: number;
  height: number;
  targetSize?: number | null;
  keptOriginal: boolean;
}

export interface HistoryRecord {
  id: string;
  createdAt: number;
  inputPath: string;
  outputPath: string;
  originalSize: number;
  compressedSize: number;
  originalHash: string;
  outputHash: string;
  backupPath: string | null;
  overwritten: boolean;
  state: "prepared" | "done" | "restored" | "failed";
}

export interface ProgressEvent {
  fileId: string;
  result?: CompressResult;
  error?: string;
  elapsedMs: number;
}

export interface ContextMenuStatus {
  supported: boolean;
  enabled: boolean;
}

export interface PreviewInfo {
  beforePath: string;
  afterPath: string;
  originalSize: number;
  compressedSize: number;
  firstFrameOnly: boolean;
}
