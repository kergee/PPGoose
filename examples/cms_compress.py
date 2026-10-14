"""Standard-library CMS subprocess adapter. No shell, HTTP server, or API key."""
import argparse
import json
from pathlib import Path
import subprocess


def call_core(executable: Path, request: dict, timeout: float = 120) -> object:
    """Write one request and close stdin; return result only after confirmed success.

    The CMS must validate paths against its own upload roots and enforce job
    concurrency limits before calling this function. Keep dataDirectory persistent.
    """
    completed = subprocess.run(
        [str(executable.resolve(strict=True))],
        input=json.dumps(request, ensure_ascii=False).encode("utf-8"),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        shell=False,
        check=False,
    )
    try:
        response = json.loads(completed.stdout)
    except (ValueError, UnicodeError) as error:
        raise RuntimeError("Compression core returned invalid JSON") from error
    if response.get("protocolVersion") != 1:
        raise RuntimeError("Unsupported compression core protocol")
    if completed.returncode != 0 or response.get("ok") is not True:
        raise RuntimeError(response.get("error", {}).get("message", "Compression process failed"))
    return response["result"]


def main() -> None:
    parser = argparse.ArgumentParser(description="Call PPGoose compression core from a CMS")
    parser.add_argument("executable", type=Path)
    parser.add_argument("input", type=Path)
    parser.add_argument("data_directory", type=Path)
    parser.add_argument("--output-directory", type=Path)
    parser.add_argument("--target-size-kb", type=int)
    parser.add_argument("--max-edge", type=int)
    parser.add_argument("--allow-resize", action="store_true")
    parser.add_argument("--timeout", type=float, default=120)
    args = parser.parse_args()
    options = {"outputMode": "subfolder", "allowResize": args.allow_resize}
    if args.output_directory:
        options.update(outputMode="custom", customDir=str(args.output_directory.resolve()))
    if args.target_size_kb is not None:
        options["targetSizeKb"] = args.target_size_kb
    if args.max_edge is not None:
        options["maxEdge"] = args.max_edge
    result = call_core(args.executable, {
        "protocolVersion": 1,
        "operation": "compress",
        "inputPath": str(args.input.resolve(strict=True)),
        "dataDirectory": str(args.data_directory.resolve()),
        "options": options,
    }, timeout=args.timeout)
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
