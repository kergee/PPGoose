import type { FileItem } from "../types";

/** Quote every cell; neutralize spreadsheet formulas in user-controlled text. */
export function csvCell(value: unknown): string {
  let text = value == null ? "" : String(value);
  if (typeof value === "string" && /^[\s\uFEFF]*[=+@-]/u.test(text)) text = `'${text}`;
  return `"${text.replace(/"/g, '""')}"`;
}

/** UTF-8 BOM and CRLF keep Chinese reports compatible with Windows Excel. */
export function compressionCsv(files: FileItem[]): string {
  const header = ["文件名", "原始路径", "输出路径", "状态", "原始字节", "输出字节", "节省字节", "节省比例(%)",
    "宽度", "高度", "已缩小", "保留原字节", "覆盖原图", "目标字节", "耗时毫秒", "质量", "PNG策略", "JPEG策略",
    "极致模式", "转换格式", "允许缩小", "最长边", "最低质量", "输出方式", "后缀", "记录ID", "错误", "提示"];
  const status = { pending: "待处理", compressing: "处理中", done: "完成", error: "失败" };
  const rows = files.map((f) => {
    const o = f.optionsUsed;
    const saved = f.compressedSize == null ? undefined : f.originalSize - f.compressedSize;
    return [f.name, f.path, f.outputPath, status[f.status], f.originalSize, f.compressedSize, saved,
      saved == null || !f.originalSize ? undefined : Number((saved / f.originalSize * 100).toFixed(2)),
      f.width, f.height, f.resized, f.keptOriginal, f.overwritten, f.targetSize, f.elapsedMs,
      o?.quality, o?.pngStrategy, o?.jpegStrategy, o?.smartQuality, o?.convertWebpTo,
      o?.allowResize, o?.maxEdge, o?.qualityFloor, o?.outputMode, o?.suffix, f.recordId, f.error, f.warning];
  });
  return "\uFEFF" + [header, ...rows].map((row) => row.map(csvCell).join(",")).join("\r\n") + "\r\n";
}
