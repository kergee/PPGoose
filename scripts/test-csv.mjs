import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const source = fs.readFileSync(new URL("../src/lib/csv.ts", import.meta.url), "utf8");
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ES2022 } }).outputText;
const { csvCell, compressionCsv } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`);

assert.equal(csvCell('a,"b"\n中文'), '"a,""b""\n中文"');
for (const value of ["=HYPERLINK(1)", "+cmd", "@SUM(1)", "-formula", "\t=1", " \n@1"]) {
  assert.ok(csvCell(value).startsWith('"\''), value);
}
assert.equal(csvCell(-5), '"-5"');
assert.equal(csvCell(0), '"0"');
assert.equal(csvCell(undefined), '""');
assert.equal(csvCell(false), '"false"');

const csv = compressionCsv([
  { id:"1", name:"=风险,图片.png", path:"C:\\中文 空格\\图.png", ext:"png", originalSize:100, compressedSize:120,
    status:"done", elapsedMs:0, width:16, height:8, optionsUsed:{ quality:0, outputMode:"subfolder", pngStrategy:"lossless" } },
  { id:"2", name:"失败.png", path:"D:\\失败.png", ext:"png", originalSize:0, status:"error", error:'错误 "示例"\n请重试' },
]);
assert.ok(csv.startsWith("\uFEFF"));
assert.ok(csv.endsWith("\r\n"));
assert.ok(csv.includes('"-20"')); // A numeric saving can be negative after conversion.
assert.ok(csv.includes('"0","lossless"')); // Auto quality is recorded, not omitted.
assert.ok(csv.includes('"\'=风险,图片.png"'));
assert.ok(csv.includes('"错误 ""示例""\n请重试"'));
assert.ok(!csv.includes("NaN") && !csv.includes("undefined"));
console.log("CSV contract checks passed: BOM, quoting, formulas, Chinese paths, errors, zero values and negative savings.");
