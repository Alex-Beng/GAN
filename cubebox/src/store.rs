//! LittleFS-backed queue of finished records.
//!
//! One JSON file per attempt, named `rec_<zero-padded key>.json`. Files are
//! written as soon as an attempt finishes and deleted only after the server has
//! accepted them, so a power loss or a WiFi outage loses nothing.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

pub struct Store {
    root: PathBuf,
    /// Hard cap on queued files; the oldest are dropped when exceeded.
    max_files: usize,
}

impl Store {
    pub fn new(root: &str) -> Self {
        Self {
            root: PathBuf::from(root),
            max_files: 2000,
        }
    }

    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(&self.root)
            .map_err(|e| anyhow!("create {}: {e}", self.root.display()))
    }

    fn path_for(&self, key: u64) -> PathBuf {
        self.root.join(format!("rec_{key:010}.json"))
    }

    /// Persist one record. Uses write-to-temp + rename so a torn write can not
    /// leave a half JSON file behind.
    pub fn write(&self, key: u64, json: &str) -> Result<()> {
        self.ensure()?;
        let final_path = self.path_for(key);
        let tmp = self.root.join(format!(".rec_{key:010}.tmp"));
        {
            let mut f = fs::File::create(&tmp)
                .map_err(|e| anyhow!("create {}: {e}", tmp.display()))?;
            f.write_all(json.as_bytes())
                .map_err(|e| anyhow!("write {}: {e}", tmp.display()))?;
            f.sync_all().ok();
        }
        fs::rename(&tmp, &final_path)
            .map_err(|e| anyhow!("rename {}: {e}", final_path.display()))?;
        self.prune();
        Ok(())
    }

    /// Queued files, oldest first.
    pub fn files(&self) -> Vec<(u64, PathBuf)> {
        let Ok(rd) = fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut out: Vec<(u64, PathBuf)> = rd
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name();
                let name = name.to_string_lossy();
                let key = name
                    .strip_prefix("rec_")
                    .and_then(|s| s.strip_suffix(".json"))
                    .and_then(|s| s.parse::<u64>().ok())?;
                Some((key, e.path()))
            })
            .collect();
        out.sort_by_key(|(k, _)| *k);
        out
    }

    pub fn count(&self) -> usize {
        self.files().len()
    }

    pub fn read(&self, path: &Path) -> Result<String> {
        fs::read_to_string(path).map_err(|e| anyhow!("read {}: {e}", path.display()))
    }

    pub fn remove(&self, path: &Path) -> Result<()> {
        fs::remove_file(path).map_err(|e| anyhow!("remove {}: {e}", path.display()))
    }

    /// Drop the oldest files beyond `max_files`.
    fn prune(&self) {
        let files = self.files();
        if files.len() <= self.max_files {
            return;
        }
        let excess = files.len() - self.max_files;
        for (_, path) in files.iter().take(excess) {
            let _ = fs::remove_file(path);
            log::warn!("[store] queue full, dropped {}", path.display());
        }
    }
}
