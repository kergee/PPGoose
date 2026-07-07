# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

## Project

PPGoose is a cross-platform desktop image compressor (Tauri 2 + React/TypeScript frontend + Rust backend). All compression runs locally. Input formats: PNG, JPG, GIF, WebP. AVIF is output-only (WebP → AVIF conversion via ravif) because no portable AVIF decoder is available — image's `avif-native` feature needs system dav1d. UI text is in Chinese.

## Commands

```bash
npm install            # install frontend deps
npm run tauri dev      # run the app in dev mode (starts Vite + compiles Rust)
npm run tauri build    # build release bundles (output: src-tauri/target/release/bundle/)
npm run build          # typecheck (tsc) + build frontend only
```

Rust-only checks (faster than a full tauri dev cycle):

```bash
cd src-tauri && cargo check
cd src-tauri && cargo test               # unit tests for the compress modules
cd src-tauri && cargo test flat_art      # run a single test by name substring
```

No linters are configured. Building requires a C compiler (mozjpeg and libwebp compile bundled C sources); on Windows that means Visual C++ Build Tools.

## Architecture

The app is split across an IPC boundary. Data shapes must stay in sync on both sides — Rust structs use `#[serde(rename_all = "camelCase")]`, so the TypeScript types are camelCase:

- **Rust commands**: `src-tauri/src/lib.rs` defines all Tauri commands (`scan_paths`, `compress_file`, `compress_batch`, `pick_files`, `pick_folder`, `pick_folders`) and registers them in `run()`.
- **TS bindings**: `src/lib/tauri.ts` wraps each command with `invoke()`; `src/types/index.ts` mirrors the Rust types (`CompressOptions`, `CompressResult`, `ProgressEvent`). Adding a command or changing an option requires touching lib.rs, tauri.ts, and types/index.ts together.
- **State**: `src/store/useStore.ts` (Zustand) holds the file list, options, and compression flow. Components in `src/components/` are thin views over this store.

### Compression flow

1. Frontend sends dropped/picked paths to `scan_paths`, which recursively walks folders and returns supported image files with sizes (`FileMeta`), deduplicated across the batch.
2. `startCompression` in the store subscribes to the `compress://progress` Tauri event, then invokes `compress_batch` with `[(id, path)]` pairs.
3. `compress_batch` runs the rayon batch inside `spawn_blocking` (keep it off the async runtime); each file emits one `ProgressEvent` (result or error) keyed by the frontend-generated file id.

### Compression engine (`src-tauri/src/compress/`)

`mod.rs` is the dispatcher: it detects format by extension, resolves conversion targets (WebP → PNG/JPEG/AVIF), calls the per-format module, and resolves the output path (overwrite / `compressed/` subfolder / custom dir, with optional filename suffix). Key invariants live here:

- `quality: 0` means auto and maps to 80.
- Never inflate: if the compressed result is larger than the original *and the format is unchanged*, the original bytes are written instead.
- Output is written atomically (temp file + rename) so Overwrite mode can't corrupt the original on a mid-write failure.
- All decode paths go through `decode_oriented`/`decode_with_meta` in `mod.rs`, which apply EXIF orientation (the metadata itself is dropped on re-encode, so pixels must be rotated) and extract the ICC profile.
- Smart quality (`smartQuality`, 极致模式): `smart.rs` binary-searches encoder quality for the smallest output with DSSIM ≤ 0.0015 vs the source. Applies to JPEG/WebP outputs only when quality is 0 (auto), never to lossless WebP sources.
- Animated WebP is rejected in `compress_file` (every re-encode path would flatten it to one frame).
- One file per format in `compress/` (`png.rs`, `jpeg.rs`, `gif.rs`, `webp.rs`, `avif.rs`), each exposing `compress(data, quality) -> Result<Vec<u8>>` (GIF takes no quality). Per-format quality behavior:
  - PNG: imagequant runs with a minimum-quality floor (`q - 25`), so photos/gradients fail quantization and fall back to lossless oxipng — that's the "lossy for flat art, lossless for photos" mechanism. Candidates (quantized, oxipng-lossless, oxipng-on-quantized) are compared and the smallest wins.
  - JPEG: mozjpeg progressive mode, grayscale sources stay single-channel, quality ≥ 90 disables chroma subsampling (4:4:4), ICC profile carried over via APP2 markers.
  - WebP: lossless sources (VP8L chunk) are re-encoded losslessly, never silently degraded.
  - GIF: encoded with gifski (writer runs on a separate thread — its channel deadlocks otherwise); preserves the original loop count (read via the `gif` crate directly, image's decoder doesn't expose it).

Supported input extensions are hardcoded in two places: `SUPPORTED_EXTS` in `lib.rs` and `ImageFormat::from_path` in `compress/mod.rs` — update both when adding a format.

### Build notes

- `ravif` is built with `default-features = false` to avoid a NASM dependency (slower AVIF encoding; see comment in `src-tauri/Cargo.toml` to enable asm).
- Release profile is heavily size-optimized (LTO, `opt-level = "s"`), so release builds are slow. Do NOT add `panic = "abort"`: mozjpeg reports errors via panics and `jpeg.rs` relies on `catch_unwind`.
- `.github/workflows/release.yml` builds installers for macOS/Windows/Linux on tag push.
