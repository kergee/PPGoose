use super::file_io;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum RecordState {
    Prepared,
    Done,
    Restored,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub id: String,
    pub created_at: u64,
    pub input_path: String,
    pub output_path: String,
    pub original_size: u64,
    pub compressed_size: u64,
    pub original_hash: String,
    pub output_hash: String,
    pub backup_path: Option<String>,
    pub overwritten: bool,
    pub state: RecordState,
}

#[derive(Debug, Clone)]
pub struct Store {
    pub root: PathBuf,
}

pub struct Committed {
    pub record: Record,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub before_path: String,
    pub after_path: String,
    pub original_size: u64,
    pub compressed_size: u64,
    pub first_frame_only: bool,
}

impl Store {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn lock(&self) -> Result<File> {
        std::fs::create_dir_all(&self.root)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.root.join("history.lock"))?;
        // Released by dropping the handle; works across app instances as well.
        file.lock()?;
        Ok(file)
    }

    fn read(&self) -> Result<Vec<Record>> {
        let path = self.root.join("history.json");
        if !path.exists() {
            return Ok(Vec::new());
        }
        serde_json::from_slice(&std::fs::read(&path)?)
            .context("备份记录无法读取，已停止覆盖操作。请保留数据目录")
    }

    fn save(&self, records: &[Record]) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(records)?;
        let staged = file_io::stage(&self.root, &bytes)?;
        file_io::commit(staged, &self.root.join("history.json"), true)?;
        Ok(())
    }

    pub fn records(&self) -> Result<Vec<Record>> {
        let _lock = self.lock()?;
        let mut rows = self.read()?;
        rows.reverse();
        Ok(rows)
    }

    pub fn preview(&self, id: &str) -> Result<Preview> {
        let _lock = self.lock()?;
        let rows = self.read()?;
        let record = rows
            .iter()
            .find(|r| r.id == id)
            .context("图片记录不存在，请重新压缩")?;
        anyhow::ensure!(
            record.state != RecordState::Restored && record.state != RecordState::Failed,
            "这条记录已经恢复或处理未完成"
        );
        let before = Path::new(record.backup_path.as_deref().unwrap_or(&record.input_path));
        if record.backup_path.is_some() {
            anyhow::ensure!(
                before
                    .canonicalize()?
                    .starts_with(self.root.join("backups").canonicalize()?),
                "备份路径异常"
            );
        }
        let after = Path::new(&record.output_path);
        let before_bytes = std::fs::read(before)?;
        let after_bytes = std::fs::read(after)?;
        anyhow::ensure!(
            file_io::hash(&before_bytes) == record.original_hash,
            "原图或备份已变化，无法显示当时的对比"
        );
        anyhow::ensure!(
            file_io::hash(&after_bytes) == record.output_hash,
            "输出文件已变化，无法显示当时的对比"
        );
        let (before_path, before_static) =
            self.cache_preview(&record.id, "before", &before_bytes)?;
        let (after_path, after_static) = self.cache_preview(&record.id, "after", &after_bytes)?;
        Ok(Preview {
            before_path,
            after_path,
            original_size: record.original_size,
            compressed_size: record.compressed_size,
            first_frame_only: before_static || after_static,
        })
    }

    fn cache_preview(&self, id: &str, side: &str, data: &[u8]) -> Result<(String, bool)> {
        let format = image::guess_format(data)?;
        let animated = format == image::ImageFormat::Gif
            || (format == image::ImageFormat::Png
                && png::Decoder::new(data)
                    .read_info()?
                    .info()
                    .animation_control
                    .is_some());
        let (bytes, ext) = if animated {
            let (image, icc) = super::decode_with_meta(data)?;
            let mut bytes = Vec::new();
            use image::ImageEncoder;
            let mut encoder = image::codecs::png::PngEncoder::new(&mut bytes);
            if let Some(icc) = icc {
                encoder.set_icc_profile(icc)?;
            }
            encoder.write_image(
                image.as_bytes(),
                image.width(),
                image.height(),
                image.color().into(),
            )?;
            (bytes, "png")
        } else {
            let ext = match format {
                image::ImageFormat::Png => "png",
                image::ImageFormat::Jpeg => "jpg",
                image::ImageFormat::WebP => "webp",
                image::ImageFormat::Avif => "avif",
                _ => anyhow::bail!("此格式无法预览"),
            };
            (data.to_vec(), ext)
        };
        let dir = self.root.join("previews");
        let path = dir.join(format!("{id}-{side}.{ext}"));
        if !path.exists() || file_io::hash_file(&path)? != file_io::hash(&bytes) {
            file_io::commit(file_io::stage(&dir, &bytes)?, &path, true)?;
        }
        Ok((path.to_string_lossy().into_owned(), animated))
    }

    pub fn commit(
        &self,
        input: &Path,
        desired: &Path,
        original: &[u8],
        output: &[u8],
        replace: bool,
    ) -> Result<Committed> {
        let staged = file_io::stage(desired.parent().unwrap(), output)?;
        let _lock = self.lock()?;
        let mut rows = self.read()?;
        let original_hash = file_io::hash(original);
        anyhow::ensure!(
            file_io::hash_file(input)? == original_hash,
            "原图已被其他程序修改，未覆盖"
        );
        let id = loop {
            let reserved = tempfile::Builder::new()
                .prefix("record-")
                .tempfile_in(&self.root)?;
            let id = reserved
                .path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if !rows.iter().any(|r| r.id == id) {
                break id;
            }
        };
        let mut record = Record {
            id,
            created_at: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            input_path: input.to_string_lossy().into_owned(),
            output_path: desired.to_string_lossy().into_owned(),
            original_size: original.len() as u64,
            compressed_size: output.len() as u64,
            original_hash,
            output_hash: file_io::hash(output),
            backup_path: None,
            overwritten: replace,
            state: RecordState::Prepared,
        };
        if replace && original == output {
            // Already optimal: don't replace an identical original or create a backup.
            record.state = RecordState::Done;
            record.overwritten = false;
            rows.push(record.clone());
            self.save(&rows)?;
            return Ok(Committed {
                record,
                warning: None,
            });
        }
        if replace {
            let dir = self.root.join("backups");
            let ext = input.extension().unwrap_or_default().to_string_lossy();
            let path = dir.join(format!("{}.{}", record.id, ext));
            let temp = file_io::stage(&dir, original)?;
            let backup = file_io::commit(temp, &path, false)?;
            anyhow::ensure!(
                file_io::hash_file(&backup)? == record.original_hash,
                "备份校验失败，原图未覆盖"
            );
            record.backup_path = Some(backup.to_string_lossy().into_owned());
        }
        rows.push(record.clone());
        let index = rows.len() - 1;
        let result = file_io::commit_recorded(staged, desired, replace, |destination| {
            anyhow::ensure!(
                file_io::hash_file(input)? == record.original_hash,
                "原图已被其他程序修改，未覆盖"
            );
            record.output_path = destination.to_string_lossy().into_owned();
            rows[index] = record.clone();
            self.save(&rows)
        });
        match result {
            Ok(path) => {
                record.output_path = path.to_string_lossy().into_owned();
                record.state = RecordState::Done;
                rows[index] = record.clone();
                let warning = self
                    .save(&rows)
                    .err()
                    .map(|error| format!("输出已保存，恢复记录待确认：{error}"));
                Ok(Committed { record, warning })
            }
            Err(error) => {
                record.state = RecordState::Failed;
                rows[index] = record;
                let _ = self.save(&rows);
                Err(error)
            }
        }
    }

    pub fn restore(&self, id: &str) -> Result<Record> {
        let _lock = self.lock()?;
        let mut rows = self.read()?;
        let record = rows
            .iter_mut()
            .find(|r| r.id == id)
            .context("备份记录不存在")?;
        anyhow::ensure!(
            record.overwritten && record.state != RecordState::Restored,
            "这条记录没有待恢复的原图"
        );
        let backup = Path::new(record.backup_path.as_deref().context("这条记录没有备份")?);
        let root = self.root.join("backups").canonicalize()?;
        let canonical = backup.canonicalize()?;
        anyhow::ensure!(canonical.starts_with(&root), "备份路径异常，未恢复");
        let bytes = std::fs::read(&canonical)?;
        anyhow::ensure!(
            file_io::hash(&bytes) == record.original_hash,
            "备份校验失败，未恢复"
        );
        let target = Path::new(&record.input_path);
        if target.exists() {
            let current = file_io::hash_file(target)?;
            if current == record.original_hash {
                record.state = RecordState::Restored;
                let result = record.clone();
                self.save(&rows)?;
                return Ok(result);
            }
            anyhow::ensure!(
                current == record.output_hash,
                "当前文件已被其他程序修改，未覆盖这些改动。请先保留当前文件"
            );
        }
        let temp = file_io::stage(target.parent().context("原文件目录不存在")?, &bytes)?;
        file_io::commit(temp, target, true)?;
        record.state = RecordState::Restored;
        let result = record.clone();
        self.save(&rows)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Store, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.png");
        std::fs::write(&path, b"original").unwrap();
        let store = Store::new(dir.path().join("data"));
        (dir, store, path)
    }
    #[test]
    fn overwrite_restores_exact_bytes_after_restart() {
        let (_dir, store, path) = fixture();
        let result = store
            .commit(&path, &path, b"original", b"compressed", true)
            .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"compressed");
        Store::new(store.root.clone())
            .restore(&result.record.id)
            .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
    }
    #[test]
    fn modified_file_and_damaged_backup_are_not_restored() {
        let (_dir, store, path) = fixture();
        let record = store
            .commit(&path, &path, b"original", b"compressed", true)
            .unwrap()
            .record;
        std::fs::write(&path, b"user edits").unwrap();
        assert!(store.restore(&record.id).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"user edits");
        std::fs::write(&path, b"compressed").unwrap();
        std::fs::write(record.backup_path.unwrap(), b"damaged").unwrap();
        assert!(store.restore(&record.id).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"compressed");
    }
    #[test]
    fn prepared_record_survives_interrupted_finalization() {
        let (_dir, store, path) = fixture();
        let record = store
            .commit(&path, &path, b"original", b"compressed", true)
            .unwrap()
            .record;
        let mut rows = store.read().unwrap();
        rows[0].state = RecordState::Prepared;
        store.save(&rows).unwrap();
        Store::new(store.root.clone()).restore(&record.id).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
    }
    #[test]
    fn concurrent_stores_keep_all_records() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("data");
        let jobs: Vec<_> = (0..8)
            .map(|n| {
                let root = root.clone();
                let path = dir.path().join(format!("{n}.png"));
                std::fs::write(&path, b"original").unwrap();
                std::thread::spawn(move || {
                    Store::new(root)
                        .commit(&path, &path, b"original", b"smaller", true)
                        .unwrap()
                        .record
                        .id
                })
            })
            .collect();
        for h in jobs {
            h.join().unwrap();
        }
        assert_eq!(Store::new(root).records().unwrap().len(), 8);
    }
    #[test]
    fn corrupt_history_stops_overwrite() {
        let (_dir, store, path) = fixture();
        std::fs::create_dir_all(&store.root).unwrap();
        std::fs::write(store.root.join("history.json"), b"broken").unwrap();
        assert!(store
            .commit(&path, &path, b"original", b"compressed", true)
            .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
    }
    #[test]
    fn preview_uses_backup_and_refuses_unrelated_paths_or_changed_outputs() {
        let (_dir, store, path) = fixture();
        let mut image = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(4, 4, image::Rgb([250, 0, 0]))
            .write_to(&mut image, image::ImageFormat::Png)
            .unwrap();
        let original = image.into_inner();
        std::fs::write(&path, &original).unwrap();
        let mut image = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(4, 4, image::Rgb([0, 250, 0]))
            .write_to(&mut image, image::ImageFormat::Png)
            .unwrap();
        let compressed = image.into_inner();
        let record = store
            .commit(&path, &path, &original, &compressed, true)
            .unwrap()
            .record;
        let preview = store.preview(&record.id).unwrap();
        assert_eq!(std::fs::read(preview.before_path).unwrap(), original);
        assert_eq!(std::fs::read(preview.after_path).unwrap(), compressed);
        assert!(store.preview("../../arbitrary").is_err());
        std::fs::write(&path, b"external edit").unwrap();
        assert!(store.preview(&record.id).is_err());
    }
}
