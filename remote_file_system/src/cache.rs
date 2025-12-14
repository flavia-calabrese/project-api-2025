use std::{collections::HashMap, path::PathBuf, str::FromStr, time::SystemTime};

use shared::file_entry::FileEntry;

use crate::api::Api;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Inode(pub u64);

#[derive(Debug, Clone)]
pub struct CachedFile {
    pub file_entry: FileEntry,
    pub file_path: PathBuf,
}

#[derive(Debug)]
pub struct Cache {
    files: HashMap<Inode, CachedFile>,
    api: Api,
}

impl Cache {
    const FILE_ROOT: FileEntry = FileEntry {
        ino: 1,
        name: String::new(),
        is_dir: true,
        size: 1,
        modified_at: SystemTime::UNIX_EPOCH,
        permissions: 0o755,
    };

    /// inizializza:
    /// - files con la root già inseirta
    /// - api
    pub fn new() -> Self {
        let remote_fs_root: CachedFile = CachedFile {
            file_entry: Self::FILE_ROOT,
            file_path: PathBuf::from_str("/").unwrap(),
        };

        let mut files = HashMap::new();
        files.insert(Inode(1), remote_fs_root);

        Self {
            files: files,
            api: Api::new(),
        }
    }

    pub fn list_dir(&mut self, path: &str) -> reqwest::Result<Vec<FileEntry>> {
        // chiamo api
        let entries = self.api.list_dir(path)?;

        // salvo in cache
        for entry in &entries {
            let ino = Inode(entry.ino);
            let path = PathBuf::from_str(path).unwrap().join(&entry.name);
            let cached_file = CachedFile {
                file_entry: entry.clone(),
                file_path: path,
            };
            self.files.insert(ino, cached_file);
        }

        Ok(entries)
    }

    pub fn get_file_by_ino(&self, ino: Inode) -> Option<CachedFile> {
        self.files.get(&ino).cloned()
    }
}
