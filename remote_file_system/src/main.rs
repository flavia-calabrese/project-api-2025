use fuser::{
    FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyDirectory, ReplyEntry, ReplyOpen,
    Request,
};
use libc::{EBADF, EINVAL, ENODATA, ENOENT, pid_t};
use log::{LevelFilter, info, warn};
use reqwest::Error;
use shared::file_entry::FileEntry;
use std::{
    collections::HashMap,
    ffi::OsStr,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
mod api;
mod cache;
mod file;

use crate::{
    cache::{Cache, Inode},
    file::{OpenFlags, OpenedFile, RfsFile},
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fd(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pid(pub u32);

struct RemoteFS {
    cache: Cache,
    rfs_files: HashMap<Inode, RfsFile>,
    last_fd: AtomicU64,
}

impl RemoteFS {
    fn new() -> Self {
        Self {
            cache: Cache::new(),
            rfs_files: HashMap::new(),
            last_fd: AtomicU64::new(3),
        }
    }

    /// ritorna una referenza mutabile a un RfsFile
    fn get_file_by_ino(&mut self, inode: Inode) -> Option<&mut RfsFile> {
        // prendo il file dalla cache
        let file = self.cache.get_file_by_ino(inode)?;
        // prendo o inserisco il file in RemoteFS
        let rfs_file = self.rfs_files.entry(inode).or_insert(file.clone().into());
        // aggiorno il file entry con quello letto in cache
        rfs_file.file_entry = file.file_entry;
        Some(rfs_file)
    }
    fn remove_local_file(&mut self, inode: Inode) -> Result<(), Error> {
        let removed = self.rfs_files.remove_entry(&inode);
        let path = match removed {
            Some(removed) => removed.1.file_path.to_str().unwrap().to_string(),
            None => {
                warn!("ino={:?} not found", inode);
                return Ok(());
            }
        };

        let res = self.cache.delete_file_or_directory(&path, inode);
        res
    }
    fn alloc_new_fd(&self) -> Fd {
        let last_fd = self.last_fd.fetch_add(1, Ordering::SeqCst);
        return Fd(last_fd + 1);
    }
    // dato un inode e un process, restituisce la entry associata con un nuovo file descriptor
    fn open(&mut self, inode: Inode, flags: OpenFlags) -> OpenedFile {
        let entry = OpenedFile {
            fd: self.alloc_new_fd(),
            ino: inode,
            flags,
        };
        let rfs_file = self.get_file_by_ino(inode).unwrap();
        rfs_file.fds.insert(entry.fd, entry.clone());
        entry
    }

    /// rimuove il file descriptor fd dalla lista dei fd di una coppia (processo, inode)
    ///
    /// se il file non ha altri fd e non ha link, allora elimina anche il file
    ///
    fn release(&mut self, inode: Inode, fd: Fd) -> bool {
        //let opened_files = self.fds.entry((inode, pid)).or_default();
        info!("relese ino={:?}, fd={:?}", inode, fd);
        let rfs_file = self.get_file_by_ino(inode).unwrap();

        let file = rfs_file.fds.remove_entry(&(fd));

        let (removed_key, removed_val) = match file {
            Some(f) => f,
            None => return false,
        };

        // ci sono altri fd per inode?
        let no_fds = rfs_file.fds.is_empty();

        // ci sono dei link?
        if no_fds && rfs_file.hard_link == 0 {
            let res = self.remove_local_file(inode);
            match res {
                Ok(_) => {}
                Err(_) => {
                    let rfs_file = self.get_file_by_ino(inode).unwrap();
                    rfs_file.fds.insert(removed_key, removed_val);
                    return false;
                }
            };
        }

        true
    }

    fn unlink(&mut self, inode: Inode) -> Option<()> {
        // recupero il file dato l'ino
        let rfs_file = self.get_file_by_ino(inode);

        let Some(rfs_file) = rfs_file else {
            return None;
        };
        // decremento il numero di link
        if rfs_file.hard_link == 0 {
            // TODO: va ritornato un errore -> EBADF
            return None;
        }
        rfs_file.hard_link -= 1;
        // se non ci sono fd e il numero di link == 0 -> elimino il file
        let no_fds = rfs_file.fds.is_empty();

        // ci sono dei link?
        if no_fds && rfs_file.hard_link == 0 {
            let res = self.remove_local_file(inode);
            match res {
                Ok(_) => {}
                Err(_) => {
                    return None;
                }
            };
        }
        return Some(());
    }

    fn get_file_in_dir(&mut self, parent: Inode, name: &OsStr) -> Option<FileEntry> {
        // recupero il file dalla cache
        let file = self.cache.get_file_by_ino(parent);

        let Some(file) = file else {
            warn!("file not found: {:?}", parent);
            return None;
        };
        // se list_dir ritorna Ok -> il suo contenuto viene salvato in entries, altrimenti esegue il branch Err()
        let entries = match self.cache.list_dir(file.file_path.to_str().unwrap()) {
            Ok(entries) => entries,
            Err(err) => {
                warn!("list_dir failed: {:?}", err);
                return None;
            }
        };

        // recupero il file con nome name
        let file = entries
            .into_iter()
            .find(|f| f.name == name.to_string_lossy());

        // prendo l'ino del file
        let ino = match file {
            Some(file) => file.ino,
            None => {
                warn!("get_file_in_dir file with name={:?} not found", name);
                return None;
            }
        };

        // cerco il file per ino tra i file locali (RfsFile)
        let file = self.get_file_by_ino(Inode(ino));

        match file {
            // se il numero di hardlink è > 0 ritorno il file
            // altrimenti none -> il file non è più raggiungibile
            Some(file) if file.hard_link > 0 => Some(file.file_entry.clone()),
            None => None,
            _ => None,
        }
    }
    /*    pub fn check_access(
            file_uid: u32,
            file_gid: u32,
            file_mode: u16,
            uid: u32,
            gid: u32,
            mut access_mask: i32,
        ) -> bool {
            // F_OK tests for existence of file
            if access_mask == libc::F_OK {
                return true;
            }
            let file_mode = i32::from(file_mode);

            // root is allowed to read & write anything
            if uid == 0 {
                // root only allowed to exec if one of the X bits is set
                access_mask &= libc::X_OK;
                access_mask -= access_mask & (file_mode >> 6);
                access_mask -= access_mask & (file_mode >> 3);
                access_mask -= access_mask & file_mode;
                return access_mask == 0;
            }

            if uid == file_uid {
                access_mask -= access_mask & (file_mode >> 6);
            } else if gid == file_gid {
                access_mask -= access_mask & (file_mode >> 3);
            } else {
                access_mask -= access_mask & file_mode;
            }

            return access_mask == 0;
        }
    */

    /*fn parse_xattr_namespace(key: &[u8]) -> Result<XattrNamespace, c_int> {
        let user = b"user.";
        if key.len() < user.len() {
            return Err(libc::ENOTSUP);
        }
        if key[..user.len()].eq(user) {
            return Ok(XattrNamespace::User);
        }

        let system = b"system.";
        if key.len() < system.len() {
            return Err(libc::ENOTSUP);
        }
        if key[..system.len()].eq(system) {
            return Ok(XattrNamespace::System);
        }

        let trusted = b"trusted.";
        if key.len() < trusted.len() {
            return Err(libc::ENOTSUP);
        }
        if key[..trusted.len()].eq(trusted) {
            return Ok(XattrNamespace::Trusted);
        }

        let security = b"security";
        if key.len() < security.len() {
            return Err(libc::ENOTSUP);
        }
        if key[..security.len()].eq(security) {
            return Ok(XattrNamespace::Security);
        }

        return Err(libc::ENOTSUP);
    }*/

    /*fn xattr_access_check(
        key: &[u8],
        access_mask: i32,
        inode_attrs: &FileEntry,
        request: &Request<'_>,
    ) -> Result<(), c_int> {
        match Self::parse_xattr_namespace(key)? {
            XattrNamespace::Security => {
                if access_mask != libc::R_OK && request.uid() != 0 {
                    return Err(libc::EPERM);
                }
            }
            XattrNamespace::Trusted => {
                if request.uid() != 0 {
                    return Err(libc::EPERM);
                }
            }
            XattrNamespace::System => {
                if key.eq(b"system.posix_acl_access") {
                    if !Self::check_access(
                        request.uid(),
                        request.gid(),
                        inode_attrs.permissions as u16,
                        request.uid(),
                        request.gid(),
                        access_mask,
                    ) {
                        return Err(libc::EPERM);
                    }
                } else if request.uid() != 0 {
                    return Err(libc::EPERM);
                }
            }
            XattrNamespace::User => {
                if !Self::check_access(
                    request.uid(),
                    request.gid(),
                    inode_attrs.permissions as u16,
                    request.uid(),
                    request.gid(),
                    access_mask,
                ) {
                    return Err(libc::EPERM);
                }
            }
        }

        Ok(())
    }*/
}

impl Filesystem for RemoteFS {
    fn open(&mut self, req: &Request<'_>, ino: u64, flags: i32, reply: ReplyOpen) {
        info!("open called with ino={}, pid={}", ino, req.pid());
        let Some(open_flags) = OpenFlags::from_flags(flags) else {
            info!("open failed: ino={:?} flags={:?}", ino, flags);
            reply.error(EINVAL);
            return;
        };

        let opened_file = self.open(Inode(ino), open_flags);
        reply.opened(opened_file.fd.0, 0);
    }

    fn release(
        &mut self,
        req: &Request<'_>,
        ino: u64,
        fh: u64,
        flags: i32,
        lock_owner: Option<u64>,
        flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        info!(
            "release ino={:?}, pid={:?}, fd={:?} flags={} lock_owner={:?} flush={}",
            ino,
            req.pid(),
            fh,
            flags,
            lock_owner,
            flush,
        );

        let res = self.release(Inode(ino), Fd(fh));
        if !res {
            reply.error(EBADF);
            return;
        }

        reply.ok();
    }

    fn getattr(&mut self, _req: &Request<'_>, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        // prendo il file in cache
        let rfs_file = self.get_file_by_ino(Inode(ino));
        match rfs_file {
            Some(file) => {
                // trasformo da FileEntry in FileAttr
                let file_attr = FileAttrWrapper::from(file.file_entry.clone()).0;
                reply.attr(&TTL, &file_attr);
            }
            None => reply.error(ENOENT),
        }
    }

    /// data una dir con `inode = parent`, restituisce un FileAttr di un file con nome `name`
    fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let file = self.get_file_in_dir(Inode(parent), name);

        match file {
            Some(f) => {
                let inode = FileAttrWrapper::from(f).0;
                reply.entry(&TTL, &inode, 0);
            }
            None => {
                info!("lookup failed: parent={:?} name={:?}", parent, name);
                reply.error(ENOENT)
            }
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
        // recupero il file in cache
        let file = self.get_file_by_ino(Inode(ino)).cloned();

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

    /// fornisce metadati extra per il file con inode = ino
    /// => al momento non supportiamo metadati extra
    fn getxattr(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _name: &OsStr,
        _size: u32,
        reply: fuser::ReplyXattr,
    ) {
        reply.error(ENODATA);
    }

    /// controlla i permessi (NON compie alcuna azione sul file)
    fn access(&mut self, _req: &Request<'_>, ino: u64, mask: i32, reply: fuser::ReplyEmpty) {
        // recupero il file in cache
        let file = self.get_file_by_ino(Inode(ino));

        let Some(file) = file else {
            info!("file not found: {:?}", Inode(ino));
            reply.error(ENOENT);
            return;
        };
        //dbg!("sono nella access");
        //dbg!(&file);
        let perm = file.file_entry.permissions;

        // permesso di lettura -> R_OK
        if (mask & libc::R_OK != 0) && perm & 0o444 == 0 {
            reply.error(libc::EACCES);
            return;
        }
        // permesso di scrittura
        if (mask & libc::W_OK != 0) && perm & 0o222 == 0 {
            reply.error(libc::EACCES);
            return;
        }
        // permesso di esecuzione
        if (mask & libc::X_OK != 0) && perm & 0o111 == 0 {
            reply.error(libc::EACCES);
            return;
        }
        reply.ok();
    }

    fn read(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: fuser::ReplyData,
    ) {
        println!("READ ino={} offset={} size={}", ino, offset, size);
        let file_content = self.cache.get_file_content(Inode(ino), offset as u64, size);
        match file_content {
            Err(err) => {
                info!("read failed: {:?}", err);
                reply.error(ENOENT);
            }
            Ok(file_content) => {
                let content = file_content.as_slice();
                reply.data(content);
            }
        }
    }

    // viene chiamata quando un file descriptor viene chiuso (anche se il file resta aperto da altri processi)
    fn flush(
        &mut self,
        req: &Request<'_>,
        ino: u64,
        fh: u64,
        lock_owner: u64,
        reply: fuser::ReplyEmpty,
    ) {
        info!(
            "flush ino={:?}, pid={:?}, fd={:?} lock_owner={}",
            ino,
            req.pid(),
            fh,
            lock_owner
        );
        reply.ok();
    }

    fn unlink(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: fuser::ReplyEmpty) {
        info!("unlink parent={:?}, name={:?}", parent, name);
        let file = self.get_file_in_dir(Inode(parent), name);
        let Some(file) = file else {
            info!("file not found: {:?}", parent);
            reply.error(ENOENT);
            return;
        };
        let res = self.unlink(Inode(file.ino));
        match res {
            None => {
                warn!("unlink failed");
                reply.error(ENOENT);
            }
            Some(_) => {
                reply.ok();
            }
        }
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
