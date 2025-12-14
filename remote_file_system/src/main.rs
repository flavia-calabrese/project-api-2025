use fuser::{
    FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyDirectory, ReplyEntry, Request,
};
use libc::ENOENT;
use log::LevelFilter;
use shared::file_entry::FileEntry;
use std::{
    ffi::OsStr,
    time::{Duration, UNIX_EPOCH},
};
mod api;

use crate::api::Api;

const TTL: Duration = Duration::from_secs(1); // 1 second
const REMOTE_FS_BASE_DIR: FileAttr = FileAttr {
    ino: 1,
    size: 0,
    blocks: 0,
    atime: UNIX_EPOCH, // 1970-01-01 00:00:00
    mtime: UNIX_EPOCH,
    ctime: UNIX_EPOCH,
    crtime: UNIX_EPOCH,
    kind: FileType::Directory,
    perm: 0o755,
    nlink: 2,
    uid: 501,
    gid: 20,
    rdev: 0,
    flags: 0,
    blksize: 512,
};

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
    api: Api,
}

impl RemoteFS {
    fn new() -> Self {
        Self { api: Api::new() }
    }
}

impl Filesystem for RemoteFS {
    /*fn open(&mut self, _req: &Request<'_>, _ino: u64, _flags: i32, reply: ReplyOpen) {
        dbg!(_req, _ino, _flags, &reply);

        reply.opened(0, 0);
    }
    */
    fn getattr(&mut self, _req: &Request<'_>, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        match ino {
            1 => reply.attr(&TTL, &REMOTE_FS_BASE_DIR),
            _ => reply.error(ENOENT),
        }
    }

    fn lookup(&mut self, _req: &Request<'_>, _parent: u64, name: &OsStr, reply: ReplyEntry) {
        // se api ritorna Ok -> il suo contenuto viene salvato in entries, altrimenti esegue il blocco else
        let Ok(entries) = self.api.list_dir("/") else {
            reply.error(ENOENT);
            return;
        };

        dbg!(&entries);

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

    fn readdir(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let Ok(mut entries) = self.api.list_dir("/") else {
            reply.error(ENOENT);
            return;
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
