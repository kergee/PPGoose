mod compress;

use compress::{CompressOptions, CompressResult};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use tauri::Emitter;
use walkdir::WalkDir;

/// Extensions accepted as input. AVIF is encode-only (no portable decoder),
/// so it is a conversion target but not an input format.
const SUPPORTED_EXTS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];

// ── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMeta {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub files: Vec<FileMeta>,
}

/// Progress event emitted per file during batch compress
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub file_id: String,
    pub result: Option<CompressResult>,
    pub error: Option<String>,
}

// ── Commands ───────────────────────────────────────────────────────────────

fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|x| x.to_str())
        .map(|x| SUPPORTED_EXTS.contains(&x.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn add_file(path: &Path, seen: &mut HashSet<String>, files: &mut Vec<FileMeta>) {
    let key = path.to_string_lossy().into_owned();
    if seen.insert(key.clone()) {
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        files.push(FileMeta { path: key, size });
    }
}

/// Scan a list of paths (files or folders) and return all supported image
/// files with their sizes, deduplicated across the whole batch.
#[tauri::command]
fn scan_paths(paths: Vec<String>) -> ScanResult {
    let mut seen = HashSet::new();
    let mut files = Vec::new();

    for p in &paths {
        let path = Path::new(p);
        if path.is_dir() {
            for entry in WalkDir::new(path)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file() && is_supported(e.path()))
            {
                add_file(entry.path(), &mut seen, &mut files);
            }
        } else if path.is_file() && is_supported(path) {
            add_file(path, &mut seen, &mut files);
        }
    }

    ScanResult { files }
}

/// Compress a single file. Returns result synchronously.
#[tauri::command]
fn compress_file(path: String, options: CompressOptions) -> Result<CompressResult, String> {
    compress::compress_file(Path::new(&path), &options)
        .map_err(|e| e.to_string())
}

/// Compress a batch of files in parallel, emitting per-file progress events.
///
/// Each file emits a `compress://progress` event with `ProgressEvent` payload.
/// The rayon batch runs on a blocking thread so it doesn't stall the async
/// runtime that serves other IPC commands.
#[tauri::command]
async fn compress_batch(
    app: tauri::AppHandle,
    file_ids: Vec<(String, String)>, // [(id, path)]
    options: CompressOptions,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        file_ids.par_iter().for_each(|(id, path)| {
            let event = match compress::compress_file(Path::new(path), &options) {
                Ok(result) => ProgressEvent {
                    file_id: id.clone(),
                    result: Some(result),
                    error: None,
                },
                Err(e) => ProgressEvent {
                    file_id: id.clone(),
                    result: None,
                    error: Some(e.to_string()),
                },
            };

            let _ = app.emit("compress://progress", &event);
        });
    })
    .await
    .map_err(|e| e.to_string())
}

/// Open a native file/folder picker and return selected paths.
#[tauri::command]
async fn pick_files(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let paths = app
        .dialog()
        .file()
        .add_filter("Images", &SUPPORTED_EXTS)
        .blocking_pick_files();

    Ok(paths
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

/// Pick a folder for output.
#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let path = app.dialog().file().blocking_pick_folder();

    Ok(path.and_then(|p| p.into_path().ok()).map(|p| p.to_string_lossy().into_owned()))
}

/// Pick one or more folders to add images from.
#[tauri::command]
async fn pick_folders(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    // macOS doesn't support multi-folder in one dialog; open single folder picker
    // and let the caller call again if needed. Returns 0 or 1 path.
    let path = app.dialog().file().blocking_pick_folder();

    Ok(path
        .and_then(|p| p.into_path().ok())
        .map(|p| vec![p.to_string_lossy().into_owned()])
        .unwrap_or_default())
}

// ── Entry point ────────────────────────────────────────────────────────────

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            scan_paths,
            compress_file,
            compress_batch,
            pick_files,
            pick_folder,
            pick_folders,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PPGoose")
}
