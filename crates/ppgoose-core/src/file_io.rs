use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tempfile::{Builder, NamedTempFile};

pub fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

pub fn hash_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut state = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        state.update(&buffer[..read]);
    }
    Ok(format!("{:x}", state.finalize()))
}

pub fn stage(folder: &Path, bytes: &[u8]) -> Result<NamedTempFile> {
    std::fs::create_dir_all(folder)?;
    let mut file = Builder::new()
        .prefix(".ppgoose-")
        .suffix(".tmp")
        .tempfile_in(folder)?;
    file.write_all(bytes)
        .context("无法写入临时文件，原文件未覆盖")?;
    file.as_file().sync_all()?;
    Ok(file)
}

/// Replace only when explicitly requested. Otherwise claim a unique destination
/// with the OS's no-clobber operation, including races with other processes.
pub fn commit(staged: NamedTempFile, desired: &Path, replace: bool) -> Result<PathBuf> {
    commit_recorded(staged, desired, replace, |_| Ok(()))
}

pub fn commit_recorded(
    mut staged: NamedTempFile,
    desired: &Path,
    replace: bool,
    mut before: impl FnMut(&Path) -> Result<()>,
) -> Result<PathBuf> {
    if replace {
        before(desired)?;
        staged
            .persist(desired)
            .map_err(|error| error.error)
            .context("无法替换原图，原文件仍保留")?;
        return Ok(desired.to_path_buf());
    }
    let stem = desired.file_stem().unwrap_or_default().to_string_lossy();
    let ext = desired
        .extension()
        .map(|v| format!(".{}", v.to_string_lossy()))
        .unwrap_or_default();
    for number in 0..100000 {
        let target = if number == 0 {
            desired.to_path_buf()
        } else {
            desired.with_file_name(format!("{stem} ({number}){ext}"))
        };
        before(&target)?;
        match staged.persist_noclobber(&target) {
            Ok(_) => return Ok(target),
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                staged = error.file
            }
            Err(error) => return Err(error.error.into()),
        }
    }
    anyhow::bail!("同名文件太多，未写入输出")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_same_name_outputs_never_clobber() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("same.png");
        std::fs::write(&path, b"existing").unwrap();
        let handles: Vec<_> = (0..8)
            .map(|n| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let bytes = format!("file-{n}");
                    let temp = stage(path.parent().unwrap(), bytes.as_bytes()).unwrap();
                    let output = commit(temp, &path, false).unwrap();
                    assert_eq!(std::fs::read(&output).unwrap(), bytes.as_bytes());
                    output
                })
            })
            .collect();
        let outputs: std::collections::HashSet<_> =
            handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(outputs.len(), 8);
        assert_eq!(std::fs::read(&path).unwrap(), b"existing");
    }

    #[test]
    fn failed_commit_preserves_destination_and_cleans_temp() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("existing-directory");
        std::fs::create_dir(&target).unwrap();
        let staged = stage(dir.path(), b"new contents").unwrap();
        let temporary = staged.path().to_owned();
        assert!(commit(staged, &target, true).is_err());
        assert!(target.is_dir());
        assert!(!temporary.exists());
    }
}
