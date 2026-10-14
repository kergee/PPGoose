mod compress;
mod shell;

use compress::storage::{Preview, Record, Store};
// CompressOptions (including PNG/JPEG strategy) is shared with the CMS core.
use compress::{CompressOptions, CompressResult};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use tauri::Emitter;
use tauri::{Manager, State};
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
    pub elapsed_ms: u64,
}

// ── Commands ───────────────────────────────────────────────────────────────

fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|x| x.to_str())
        .map(|x| SUPPORTED_EXTS.contains(&x.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn add_file(path: &Path, seen: &mut HashSet<String>, files: &mut Vec<FileMeta>) {
    let Ok(path) = path.canonicalize() else {
        return;
    };
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
async fn compress_file(
    path: String,
    options: CompressOptions,
    store: State<'_, Store>,
) -> Result<CompressResult, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        compress::compress_file(Path::new(&path), &options, &store).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
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
    store: State<'_, Store>,
) -> Result<Vec<ProgressEvent>, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        file_ids
            .par_iter()
            .map(|(id, path)| {
                let started = std::time::Instant::now();
                let outcome = compress::compress_file(Path::new(path), &options, &store);
                let elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                let event = match outcome {
                    Ok(result) => ProgressEvent {
                        file_id: id.clone(),
                        result: Some(result),
                        error: None,
                        elapsed_ms,
                    },
                    Err(e) => ProgressEvent {
                        file_id: id.clone(),
                        result: None,
                        error: Some(e.to_string()),
                        elapsed_ms,
                    },
                };

                let _ = app.emit("compress://progress", &event);
                event
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_history(store: State<'_, Store>) -> Result<Vec<Record>, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || store.records().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn restore_file(record_id: String, store: State<'_, Store>) -> Result<Record, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        store.restore(&record_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_data_directory(store: State<'_, Store>) -> String {
    store.root.to_string_lossy().into_owned()
}

#[tauri::command]
async fn get_preview(
    app: tauri::AppHandle,
    record_id: String,
    store: State<'_, Store>,
) -> Result<Preview, String> {
    let store = store.inner().clone();
    let preview = tauri::async_runtime::spawn_blocking(move || {
        store.preview(&record_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    app.asset_protocol_scope()
        .allow_file(&preview.before_path)
        .map_err(|e| e.to_string())?;
    app.asset_protocol_scope()
        .allow_file(&preview.after_path)
        .map_err(|e| e.to_string())?;
    Ok(preview)
}

/// The user picks the destination with the native save dialog.
#[tauri::command]
async fn export_csv(app: tauri::AppHandle, csv: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    if csv.len() > 16 * 1024 * 1024 {
        return Err("报告超过 16 MiB，请分批导出".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let Some(destination) = app
            .dialog()
            .file()
            .add_filter("CSV", &["csv"])
            .set_file_name("PPGoose-压缩报告.csv")
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = destination.into_path().map_err(|e| e.to_string())?;
        let parent = path.parent().ok_or("报告目录不存在")?;
        use std::io::Write;
        let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        staged
            .write_all(csv.as_bytes())
            .map_err(|e| e.to_string())?;
        staged.as_file().sync_all().map_err(|e| e.to_string())?;
        staged.persist(&path).map_err(|e| e.to_string())?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|e| e.to_string())?
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

    Ok(path
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned()))
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
    let queue = shell::LaunchQueue::default();
    queue.enqueue(shell::launch_paths(
        std::env::args_os().skip(1),
        &std::env::current_dir().unwrap_or_default(),
    ));
    tauri::Builder::default()
        .manage(queue)
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            let paths = shell::launch_paths(
                args.into_iter().skip(1).map(std::ffi::OsString::from),
                Path::new(&cwd),
            );
            app.state::<shell::LaunchQueue>().enqueue(paths);
            let _ = app.emit("shell://paths", ());
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            app.manage(Store::new(app.path().app_data_dir()?));
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            shell::take_launch_paths,
            shell::context_menu_status,
            shell::set_context_menu,
            export_csv,
            scan_paths,
            compress_file,
            compress_batch,
            pick_files,
            pick_folder,
            pick_folders,
            list_history,
            restore_file,
            get_data_directory,
            get_preview,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PPGoose")
}
