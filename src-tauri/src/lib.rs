mod compress;

use compress::{CompressOptions, CompressResult};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::Emitter;
use walkdir::WalkDir;

// ── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub files: Vec<String>,
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

/// Scan a list of paths (files or folders) and return all supported image files.
#[tauri::command]
fn scan_paths(paths: Vec<String>) -> ScanResult {
    let supported_exts = ["png", "jpg", "jpeg", "gif", "webp", "avif"];

    let mut files: Vec<String> = paths
        .iter()
        .flat_map(|p| {
            let path = Path::new(p);
            if path.is_dir() {
                WalkDir::new(path)
                    .follow_links(true)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .and_then(|x| x.to_str())
                            .map(|x| supported_exts.contains(&x.to_lowercase().as_str()))
                            .unwrap_or(false)
                    })
                    .map(|e| e.path().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
            } else if path.is_file() {
                let ext = path
                    .extension()
                    .and_then(|x| x.to_str())
                    .unwrap_or("");
                if supported_exts.contains(&ext.to_lowercase().as_str()) {
                    vec![p.clone()]
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        })
        .collect();

    // Deduplicate while preserving order
    files.dedup();

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
#[tauri::command]
async fn compress_batch(
    app: tauri::AppHandle,
    file_ids: Vec<(String, String)>, // [(id, path)]
    options: CompressOptions,
) -> Result<(), String> {
    let app = app.clone();
    let options = options.clone();

    // Process in parallel using rayon
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

    Ok(())
}

/// Open a native file/folder picker and return selected paths.
#[tauri::command]
async fn pick_files(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let paths = app
        .dialog()
        .file()
        .add_filter(
            "Images",
            &["png", "jpg", "jpeg", "gif", "webp"],
        )
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
        .expect("error while running PPGoose");
}
