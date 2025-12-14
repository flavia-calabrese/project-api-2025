use fuser::{
    FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyDirectory, ReplyEntry, Request,
};
use libc::ENOENT;
use log::{LevelFilter, info};
use shared::file_entry::FileEntry;
use std::{
    ffi::OsStr,
    time::Duration,
};
mod api;
mod cache;

use crate::cache::{Cache, Inode};

const TTL: Duration = Duration::from_secs(1); // 1 second

struct FileAttrWrapper(FileAttr);

impl From<FileEntry> for FileAttrWrapper {
    fn from(value: FileEntry) -> Self {
        Self(FileAttr {
            ino: value.ino,
            size: value.size,
            blocks: 512 / value.size,
            atime: value.modified_at,
            mtime: value.modified_at,
            ctime: value.modified_at,
            crtime: value.modified_at,
            kind: if value.is_dir {
                FileType::Directory
            } else {
                FileType::RegularFile
            },
            perm: value.permissions as u16,
            nlink: 0,
            uid: 1000,
            gid: 1000,
            rdev: 0,
            blksize: 512,
            flags: 0,
        })
    }
}

struct RemoteFS {
    cache: Cache,
}

impl RemoteFS {
    fn new() -> Self {
        Self {
            cache: Cache::new(),
        }
    }
}

impl Filesystem for RemoteFS {
    /*fn open(&mut self, _req: &Request<'_>, _ino: u64, _flags: i32, reply: ReplyOpen) {
        dbg!(_req, _ino, _flags, &reply);

        reply.opened(0, 0);
    }
    */
    fn getattr(&mut self, _req: &Request<'_>, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        // prendo il file in cache
        let file = self.cache.get_file_by_ino(Inode(ino));
        match file {
            Some(file) => {
                // trasformo da FileEntry in FileAttr
                let file_attr = FileAttrWrapper::from(file.file_entry).0;
                reply.attr(&TTL, &file_attr);
            }
            None => reply.error(ENOENT),
        }
    }

    /// data una dir con `inode = parent`, restituisce un FileAttr di un file con nome `name`
    fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let file = self.cache.get_file_by_ino(Inode(parent));

        let Some(file) = file else {
            info!("file not found: {:?}", Inode(parent));
            reply.error(ENOENT);
            return;
        };
        // se list_dir ritorna Ok -> il suo contenuto viene salvato in entries, altrimenti esegue il branch Err()
        let entries = match self.cache.list_dir(file.file_path.to_str().unwrap()) {
            Ok(entries) => entries,
            Err(err) => {
                info!("list_dir failed: {:?}", err);
                reply.error(ENOENT);
                return;
            }
        };

        // recupero il file con nome name
        let file = entries
            .into_iter()
            .find(|f| f.name == name.to_string_lossy());

        match file {
            Some(f) => {
                let inode = FileAttrWrapper::from(f).0;
                reply.entry(&TTL, &inode, 0);
            }
            None => reply.error(ENOENT),
        }
    }

    /// legge il contenuto di una cartella con inode `ino`
    fn readdir(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let file = self.cache.get_file_by_ino(Inode(ino));

        let Some(file) = file else {
            info!("file not found: {:?}", Inode(ino));
            reply.error(ENOENT);
            return;
        };
        // se list_dir ritorna Ok -> il suo contenuto viene salvato in entries, altrimenti esegue il branch Err()
        let mut entries = match self.cache.list_dir(file.file_path.to_str().unwrap()) {
            Ok(entries) => entries,
            Err(err) => {
                info!("list_dir failed: {:?}", err);
                reply.error(ENOENT);
                return;
            }
        };

        // sort entries by inode
        entries.sort_by_key(|e| e.ino);

        for (index, entry) in entries.into_iter().enumerate().skip(offset as usize) {
            let kind = if entry.is_dir {
                FileType::Directory
            } else {
                FileType::RegularFile
            };
            let name = OsStr::new(&entry.name);
            let buffer_full = reply.add(entry.ino, (index + 1) as i64, kind, name);

            if buffer_full {
                break;
            }
        }
        reply.ok();
    }
}

fn main() {
    env_logger::builder().filter_level(LevelFilter::Info).init();
    let mountpoint = "/mnt/remote-fs";
    let remote_fs = RemoteFS::new();

    std::fs::create_dir_all(mountpoint).unwrap();
    println!("Mounting RemoteFS at {}", mountpoint);

    fuser::mount2(
        remote_fs,
        mountpoint,
        &[
            MountOption::FSName("remote_fs".to_string()),
            MountOption::AutoUnmount,
        ],
    )
    .unwrap();
}
