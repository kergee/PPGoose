use serde::Serialize;
use std::{ffi::OsString, path::Path, sync::Mutex};

#[derive(Default)]
pub struct LaunchQueue(Mutex<Vec<String>>);
impl LaunchQueue {
    pub fn enqueue(&self, paths: Vec<String>) {
        let mut queue = self.0.lock().unwrap_or_else(|e| e.into_inner());
        for path in paths {
            if !queue.contains(&path) {
                queue.push(path);
            }
        }
    }
    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

/// Only explicit --add arguments are imported; ordinary startup adds nothing.
pub fn launch_paths(args: impl IntoIterator<Item = OsString>, cwd: &Path) -> Vec<String> {
    let mut args = args.into_iter();
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--add")) {
        return Vec::new();
    }
    args.filter_map(|arg| {
        let path = std::path::PathBuf::from(arg);
        let path = if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        };
        path.canonicalize()
            .ok()
            .filter(|p| p.is_file() || p.is_dir())
            .map(|p| p.to_string_lossy().into_owned())
    })
    .collect()
}

#[tauri::command]
pub fn take_launch_paths(queue: tauri::State<'_, LaunchQueue>) -> Vec<String> {
    queue.take()
}

#[derive(Serialize)]
pub struct ContextMenuStatus {
    supported: bool,
    enabled: bool,
}

#[tauri::command]
pub fn context_menu_status() -> Result<ContextMenuStatus, String> {
    #[cfg(windows)]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let root = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        Ok(ContextMenuStatus {
            supported: true,
            enabled: registry::enabled(&root, &exe),
        })
    }
    #[cfg(not(windows))]
    {
        Ok(ContextMenuStatus {
            supported: false,
            enabled: false,
        })
    }
}

#[tauri::command]
pub async fn set_context_menu(enabled: bool) -> Result<ContextMenuStatus, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let root = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
            if enabled {
                registry::install(&root, &exe)
            } else {
                registry::remove(&root)
            }
            .map_err(|e| e.to_string())?;
            Ok(ContextMenuStatus {
                supported: true,
                enabled: registry::enabled(&root, &exe),
            })
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err("此入口仅支持 Windows".into())
    }
}

#[cfg(windows)]
mod registry {
    use std::{io, path::Path};
    use winreg::RegKey;
    const VERB: &str = "PPGoose.AddImages";
    fn keys() -> Vec<String> {
        [".png", ".jpg", ".jpeg", ".gif", ".webp"]
            .iter()
            .map(|ext| format!("Software\\Classes\\SystemFileAssociations\\{ext}\\shell\\{VERB}"))
            .chain(std::iter::once(format!(
                "Software\\Classes\\Directory\\shell\\{VERB}"
            )))
            .collect()
    }
    fn command(exe: &Path) -> String {
        format!("\"{}\" --add \"%1\"", exe.display())
    }
    pub fn enabled(root: &RegKey, exe: &Path) -> bool {
        keys().iter().all(|key| {
            root.open_subkey(format!("{key}\\command"))
                .and_then(|k| k.get_value::<String, _>(""))
                .is_ok_and(|value| value == command(exe))
        })
    }
    pub fn install(root: &RegKey, exe: &Path) -> io::Result<()> {
        for key in keys() {
            let (verb, _) = root.create_subkey(&key)?;
            verb.set_value("", &"添加到 PPGoose")?;
            verb.set_value("Icon", &format!("\"{}\",0", exe.display()))?;
            verb.set_value("MultiSelectModel", &"Document")?;
            let (cmd, _) = verb.create_subkey("command")?;
            cmd.set_value("", &command(exe))?;
        }
        Ok(())
    }
    pub fn remove(root: &RegKey) -> io::Result<()> {
        for key in keys() {
            match root.delete_subkey_all(key) {
                Ok(()) => (),
                Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    #[cfg(test)]
    #[test]
    fn isolated_registration_roundtrip_preserves_unicode_and_other_verbs() {
        let hive = RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        let name = format!(
            "Software\\PPGoose\\Tests\\{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let (root, _) = hive.create_subkey(&name).unwrap();
        let exe = Path::new(r"C:\工具 空格 🪿\PPGoose.exe");
        install(&root, exe).unwrap();
        assert!(enabled(&root, exe));
        assert!(!enabled(&root, Path::new(r"C:\different\PPGoose.exe")));
        root.create_subkey("Software\\Classes\\Directory\\shell\\OtherApp")
            .unwrap();
        remove(&root).unwrap();
        assert!(!enabled(&root, exe));
        assert!(root
            .open_subkey("Software\\Classes\\Directory\\shell\\OtherApp")
            .is_ok());
        remove(&root).unwrap();
        drop(root);
        hive.delete_subkey_all(name).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arguments_and_queue_preserve_paths_without_auto_compression() {
        let dir = tempfile::tempdir().unwrap();
        let name = "中文 空格 & 🪿.png";
        std::fs::write(dir.path().join(name), b"source").unwrap();
        let args = vec![
            OsString::from("--add"),
            OsString::from(name),
            OsString::from("missing.png"),
        ];
        let paths = launch_paths(args, dir.path());
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with(name));
        assert!(launch_paths([OsString::from(name)], dir.path()).is_empty());
        let queue = LaunchQueue::default();
        queue.enqueue(paths.clone());
        queue.enqueue(paths.clone());
        assert_eq!(queue.take(), paths);
        assert!(queue.take().is_empty());
    }
}
