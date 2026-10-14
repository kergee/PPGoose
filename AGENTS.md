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
cd src-tauri && cargo test --manifest-path ../crates/Cargo.toml  # shared core tests
cd src-tauri && cargo test --manifest-path ../crates/Cargo.toml flat_art  # filter core tests
```

No linters are configured. Building requires a C compiler (mozjpeg and libwebp compile bundled C sources); on Windows that means Visual C++ Build Tools.

## Architecture

The app is split across an IPC boundary. Data shapes must stay in sync on both sides — Rust structs use `#[serde(rename_all = "camelCase")]`, so the TypeScript types are camelCase:

- **Rust commands**: `src-tauri/src/lib.rs` defines all Tauri commands (`scan_paths`, `compress_file`, `compress_batch`, `pick_files`, `pick_folder`, `pick_folders`, `list_history`, `restore_file`, `get_data_directory`, `get_preview`) and registers them in `run()`.
- **TS bindings**: `src/lib/tauri.ts` wraps each command with `invoke()`; `src/types/index.ts` mirrors the Rust types (`CompressOptions`, `CompressResult`, `ProgressEvent`). Adding a command or changing an option requires touching lib.rs, tauri.ts, and types/index.ts together.
- **State**: `src/store/useStore.ts` (Zustand) holds the file list, options, and compression flow. Components in `src/components/` are thin views over this store.

### Compression flow

1. Frontend sends dropped/picked paths to `scan_paths`, which recursively walks folders and returns supported image files with sizes (`FileMeta`), deduplicated across the batch.
2. `startCompression` in the store subscribes to the `compress://progress` Tauri event, then invokes `compress_batch` with `[(id, path)]` pairs.
3. `compress_batch` runs the rayon batch inside `spawn_blocking` (keep it off the async runtime); each file emits one `ProgressEvent` (result or error) keyed by the frontend-generated file id. The command also returns all events to reconcile missed/late notifications. The frontend blocks restore/compress overlap and supports retrying failed rows.

### Compression engine (`crates/ppgoose-core/src/`)

`lib.rs` is the dispatcher (the desktop `src-tauri/src/compress/mod.rs` only re-exports `ppgoose_core`): it detects format by extension, resolves conversion targets (WebP → PNG/JPEG/AVIF), calls the per-format module, uses `target.rs` for target-size/resize processing, and resolves the output path (overwrite / `compressed/` subfolder / custom dir, with optional filename suffix). Key invariants live here:

- `quality: 0` means auto; JPEG Compact maps to 67, other non-PNG policies to 80, PNG uses the selected strategy's bounds. `jpegStrategy` is mirrored in TypeScript; Balanced retains prior defaults, Detail/Compact use 4:4:4 and upstream table 3.
- Never inflate: if the compressed result is larger than the original *and the format is unchanged and dimensions were not explicitly reduced*, the original bytes are written instead.
- `file_io.rs` stages, syncs and commits atomically. Only explicit source overwrite may replace a file; other collisions use numbered no-clobber outputs.
- `storage.rs` locks across instances, saves verified original backups and a Prepared journal before overwrite, then records Done. Restore verifies original backup and current output hashes. Tauri manages one Store under app_data_dir. Rust 1.89+ is required for File::lock.
- `target.rs` enforces targetSizeKb × 1024, qualityFloor/manual upper bounds and maxEdge. Automatic resize requires allowResize. Each candidate uses original pixels; PNG Lossless/APNG refuse resize, and lossless WebP retains lossless encoding. Failure writes nothing.
- Preview uses immutable, hash-checked registered cache files; Tauri asset scope is initially empty and only those cache files are allowed. GIF/APNG previews are static first frames.
- Shared decode paths go through `decode_oriented`/`decode_with_meta` in core `lib.rs`, which apply EXIF orientation (the metadata itself is dropped on re-encode, so pixels must be rotated) and extract the ICC profile.
- Smart quality (`smartQuality`, 极致模式): `smart.rs` binary-searches encoder quality for the smallest output with DSSIM ≤ 0.0015 vs the source. Applies to JPEG/WebP outputs only when quality is 0 (auto), never to lossless WebP sources.
- Animated WebP is rejected in `compress_file` (every re-encode path would flatten it to one frame).
- One file per format in core `src/` (`png.rs`, `jpeg.rs`, `gif.rs`, `webp.rs`, `avif.rs`), each exposing `compress(data, quality) -> Result<Vec<u8>>`; PNG and JPEG additionally expose `compress_with_strategy`. Per-format quality behavior:
  - JPEG: mozjpeg progressive mode, grayscale sources stay single-channel, quality ≥ 90 disables chroma subsampling (4:4:4), ICC profile carried over via APP2 markers.
  - WebP: lossless sources (VP8L chunk) are re-encoded losslessly, never silently degraded.
  - GIF: compare gifski with an independent lossless delta candidate for opaque animations with at most 256 colours. Delta encoding uses changed rectangles, shared palettes and compact local palettes. Validate dimensions, frame delays/count and loop metadata before selecting a gifski result. The writer runs on a separate thread; GIF loop count is read with the `gif` crate.

Supported input extensions are hardcoded in two places: `SUPPORTED_EXTS` in `lib.rs` and `ImageFormat::from_path` in `crates/ppgoose-core/src/lib.rs` — update both when adding a format.

### Build notes

- `ravif` is built with `default-features = false` in `crates/ppgoose-core/Cargo.toml` to avoid a NASM dependency (slower AVIF encoding; add `asm` alongside `threading` only when the build environment provides NASM).
- Release profile is heavily size-optimized (LTO, `opt-level = "s"`), so release builds are slow. Do NOT add `panic = "abort"`: mozjpeg reports errors via panics and `jpeg.rs` relies on `catch_unwind`.
- `.github/workflows/release.yml` builds installers for macOS/Windows/Linux on tag push.
- `.github/workflows/core.yml` runs core tests/builds on Windows/macOS/Linux and checks frontend/desktop compilation on Windows for source pushes and PRs.

### Independent core / CMS

- `crates/Cargo.toml` is a separate workspace (core + JSON CLI), with its own Cargo.lock. `src-tauri` stays independently buildable with a path dependency.
- Core-only checks: `cargo test --manifest-path crates/Cargo.toml --locked`; build CLI: `cargo build --manifest-path crates/Cargo.toml --release -p ppgoose-cli --locked`.
- `compress_bytes` uses EncodingOptions, detects content, returns bytes/format/metadata, and writes nothing. File calls retain persistent Store transactions.
- JSON CLI protocol v1 reads one request to EOF and returns one response; partial options default to subfolder. Rust/desktop CompressOptions defaults still overwrite.
- WebP uses fallible advanced encoding with exact lossless RGBA and ICC; profiled sources refuse AVIF because current ravif cannot preserve/transform ICC.
- Public documentation describes PPGoose APIs and upstream codec parameters. Private working notes belong in ignored local-notes/. See docs/CMS-独立压缩核心.txt.


### Reports and Windows shell integration

- `src/lib/csv.ts` builds BOM UTF-8 CSV from actual per-row options/timing; use `npm run test:csv`. The native export command uses a save dialog and a synced temporary file.
- `src-tauri/src/shell.rs` implements explicit `--add` launch paths, a draining startup queue, and HKCU-only owned shell verbs. The single-instance plugin must be registered first.
- Frontend subscribes before draining launch paths. A shell launch never starts compression automatically.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked` includes isolated Windows registry roundtrip and launch queue tests. Tests must not modify real Explorer menu keys.
- Desktop builds declare Rust 1.90+; independent core remains 1.89+. Registry code is Windows-only.
