use serde::{Deserialize, Serialize};
use std::os::unix::fs::MetadataExt;
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub ino: u64,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_at: SystemTime,
    pub permissions: u32, // Permessi in formato Unix (es. 0o755)
}

impl FileEntry {
    // Metodo per convertire i metadati del file system locale in FileEntry
    pub fn from_metadata(name: String, metadata: std::fs::Metadata) -> Self {
        FileEntry {
            ino: metadata.ino(),
            name,
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            modified_at: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            permissions: metadata.mode(), // Metodo di estensione di Unix
        }
    }
}
