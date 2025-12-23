use std::{collections::HashMap, path::PathBuf, str::FromStr, time::SystemTime};

use log::info;
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

    pub fn get_file_content(
        &self,
        ino: Inode,
        offset: u64,
        size: u32,
    ) -> Result<Vec<u8>, std::io::Error> {
        let Some(file) = self.files.get(&ino).cloned() else {
            info!("file not found");
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "file not found",
            ));
        };
        let path = file.file_path.to_str().unwrap();
        let content = self.api.read_file_contents(path, offset, size);
        //dbg!(&content);
        content
    }

    pub fn write_file_content (
        &self, 
        ino: Inode, 
        data: Vec<u8>
    ) -> Result<(), std::io::Error> {
        if let Some(file) = self.files.get(&ino) {
            let path_str = file.file_path.to_str().unwrap_or("");
            self.api.write_file_contents(path_str, data)
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"))
        }
    }

    pub fn add_to_cache(&mut self, path: PathBuf, entry: FileEntry) {
        let ino = Inode(entry.ino);
        self.files.insert(ino, CachedFile {
            file_path: path,
            file_entry: entry,
        });
    }
}
