
Use --work-directory for new outputs; it must not exist. Source samples are
read-only. Report records bytes/timing/hashes, not an equal-quality ranking.
"""
import argparse
from datetime import datetime, timedelta, timezone
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import time


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--samples", type=Path, required=True)
    parser.add_argument("--work-directory", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    work = args.work_directory.resolve()
    work.mkdir(parents=True, exist_ok=False)
    exe = work / args.executable.name
    shutil.copy2(args.executable, exe)
    state = work / "持久记录"
    results = []

    def invoke(operation: str, **values) -> dict:
        request = {"protocolVersion": 1, "operation": operation, "dataDirectory": str(state), **values}
        completed = subprocess.run([str(exe)], input=json.dumps(request, ensure_ascii=False).encode("utf-8"),
                                   capture_output=True, timeout=120, shell=False)
        response = json.loads(completed.stdout)
        if completed.returncode or response.get("ok") is not True or response.get("protocolVersion") != 1:
            raise RuntimeError(response)
        return response["result"]

    samples = sorted(path for path in args.samples.resolve().iterdir() if path.suffix.lower() in {".png", ".jpg", ".jpeg", ".gif", ".webp"})
    before = {str(path): sha(path) for path in samples}
    for path in samples:
        strategies = ["default"] + (["detail", "compact"] if path.suffix.lower() in {".jpg", ".jpeg"} else [])
        for strategy in strategies:
            options = {"outputMode": "custom", "customDir": str(work / "输出" / strategy)}
            if strategy != "default":
                options["jpegStrategy"] = strategy
            started = time.perf_counter()
            result = invoke("compress", inputPath=str(path), options=options)
            elapsed = time.perf_counter() - started
            output = Path(result["outputPath"])
            assert output.is_file() and output.stat().st_size == result["compressedSize"]
            assert result["compressedSize"] <= result["originalSize"] and not result["overwritten"]
            assert sha(path) == before[str(path)]
            results.append({"sample": path.name, "sourceSha256": before[str(path)], "strategy": strategy,
                            "elapsedSeconds": round(elapsed, 4), "outputSha256": sha(output), **result})

    # Test an explicit target on a copy and restore it through another process.
    original = next(path for path in samples if path.suffix.lower() in {".jpg", ".jpeg"})
    source = work / "恢复测试 中文 🪿.jpg"
    shutil.copy2(original, source)
    expected_hash = sha(source)
    result = invoke("compress", inputPath=str(source), options={"outputMode": "overwrite", "maxEdge": 32,
                    "targetSizeKb": 4, "qualityFloor": 40})
    assert result["resized"] and result["overwritten"] and result["compressedSize"] <= 4096
    restored = invoke("restore", recordId=result["recordId"])
    assert restored["state"] == "restored" and sha(source) == expected_hash
    history = invoke("listHistory")
    assert any(row["id"] == result["recordId"] and row["state"] == "restored" for row in history)
    assert all(sha(Path(path)) == digest for path, digest in before.items())
    report = {
        "verifiedAt": datetime.now(timezone(timedelta(hours=8))).isoformat(),
        "executableSha256": sha(exe), "isolatedExecutable": str(exe),
        "sourceDirectory": str(args.samples.resolve()), "sampleCount": len(samples),
        "compressionChecks": len(results), "sourceHashesUnchanged": True,
        "crossProcessRestoreVerified": True, "results": results,
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"sampleCount": len(samples), "compressionChecks": len(results),
                      "restored": True, "report": str(args.report.resolve()), "executableSha256": sha(exe)}, ensure_ascii=False))


if __name__ == "__main__":
    main()
