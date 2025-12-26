use fuser::{
    FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyDirectory, ReplyEntry, ReplyOpen,
    Request,
};
use libc::{ENODATA, ENOENT};
use log::{LevelFilter, info};
use shared::file_entry::FileEntry;
use std::ffi::OsStr;
use std::time::{Duration, SystemTime};
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
            blocks: (value.size + 511) / 512,
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
            nlink: 1,
            uid: 1000,
            gid: 1000,
            rdev: 0,
            blksize: 512,
            flags: 0,
        })
    }
}

//#[derive(Debug, Clone)]
/*enum XattrNamespace {
    Security,
    System,
    Trusted,
    User,
}*/
struct RemoteFS {
    cache: Cache,
}

impl RemoteFS {
    fn new() -> Self {
        Self {
            cache: Cache::new(),
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
    fn open(&mut self, _req: &Request<'_>, _ino: u64, _flags: i32, reply: ReplyOpen) {
        //dbg!(_req, _ino, _flags, &reply);

        reply.opened(0, 0);
    }
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
        // recupero il file in cache
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
        let file = self.cache.get_file_by_ino(Inode(ino));

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
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _lock_owner: u64,
        reply: fuser::ReplyEmpty,
    ) {
        reply.ok();
    }

    fn write(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: fuser::ReplyWrite,
    ) {
        let offset = offset as usize;
        
        // invece di panic se non si legge il vecchio, inizia con un buffer vuoto
        let mut content = match self.cache.get_file_content(Inode(ino), 0, 0) {
            Ok(_) => {
                // bisogna leggere tutto per file grandi, ora contenuto con offset (0)
                self.cache.get_file_content(Inode(ino), 0, 1024 * 1024).unwrap_or_default()
            },
            Err(_) => Vec::new(), // se il file è nuovo, parte da zero
        };

        // se buffer è abbastanza grande per l'offset
        if offset + data.len() > content.len() {
            content.resize(offset + data.len(), 0);
        }

        // copia i dati nel buffer
        content[offset..offset + data.len()].copy_from_slice(data);

        // invia al server
        match self.cache.write_file_content(Inode(ino), content) {
            Ok(_) => {
                reply.written(data.len() as u32);
            }
            Err(e) => {
                info!("write failed on server: {}", e);
                reply.error(libc::EIO);
            }
        }
    }

    fn setattr(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<fuser::TimeOrNow>,
        _mtime: Option<fuser::TimeOrNow>,  
        _ctime: Option<SystemTime>,  
        _fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        if let Some(new_size) = size {
            // legge il file, lo ridimensiona e lo salva
            match self.cache.get_file_content(Inode(ino), 0, 0) { // prende il file per averne il path
                Ok(mut content) => {
                    content.resize(new_size as usize, 0);
                    if let Err(_) = self.cache.write_file_content(Inode(ino), content) {
                        reply.error(libc::EIO);
                        return;
                    }
                }
                Err(_) => {
                    reply.error(ENOENT);
                    return;
                }
            }
        }

        // restituisce gli attributi aggiornati
        self.getattr(_req, ino, None, reply);
    }

    fn create(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        mode: u32,
        umask: u32,
        _flags: i32,
        reply: fuser::ReplyCreate,
    ) {
        let name_str = name.to_string_lossy().into_owned();
        info!("CREATE: parent={}, name={}", parent, name_str);

        // trova il percorso della cartella genitore
        if let Some(parent_file) = self.cache.get_file_by_ino(Inode(parent)) {
            let mut new_path = parent_file.file_path.clone();
            new_path.push(&name_str);
            
            // crea un FileEntry fittizio per il nuovo file: l'inode viene generato casualmente o sequenzialmente dalla cache
            let new_ino = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos() as u64 % 1_000_000 + 2000; 
            let now = SystemTime::now();
            
            let new_entry = FileEntry {
                ino: new_ino,
                name: name_str.clone(),
                is_dir: false,
                size: 0,
                modified_at: now,
                permissions: mode & !umask, // applica la umask Unix
            };

            // inserisce il nuovo file nella cache locale: in questo modo i successivi getattr/write lo troveranno
            self.cache.add_to_cache(new_path, new_entry.clone());
            
            // risponde a FUSE con successo
            let attr = FileAttrWrapper::from(new_entry).0;
            reply.created(&TTL, &attr, 0, 0, 0);
        } else {
            reply.error(libc::ENOENT);
        }
    }

    fn mknod(
        &mut self,
        _req: &Request<'_>,
        _parent: u64,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        _rdev: u32,
        reply: fuser::ReplyEntry,
    ) {
        // Per semplicità, richiamiamo la logica di creazione
        // o rispondiamo con successo se il file è già stato gestito
        info!("MKNOD: name={:?}", name);
        // Nota: mknod usa ReplyEntry, non ReplyCreate
        // Implementazione speculare a lookup ma per un nuovo file
        reply.error(libc::ENOSYS); // Opzionale: implementa se touch non basta
    }

    fn mkdir(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        let name_str = name.to_string_lossy();

        if let Some(parent_file) = self.cache.get_file_by_ino(Inode(parent)) {
            let mut new_path = parent_file.file_path.clone();
            new_path.push(name_str.as_ref());
            let path_str = new_path.to_str().unwrap_or("");

            match self.cache.api.create_directory(path_str) {
                Ok(_) => {
                    let new_ino = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos() as u64 % 1_000_000 + 5000;

                    let new_entry = FileEntry {
                        ino: new_ino,
                        name: name_str.into_owned(),
                        is_dir: true,
                        size: 4096, // dim standard per le directory
                        modified_at: SystemTime::now(),
                        permissions: mode,
                    };

                    self.cache.add_to_cache(new_path, new_entry.clone());

                    let attr = FileAttrWrapper::from(new_entry).0;
                    reply.entry(&TTL, &attr, 0);
                }
                Err(e) => {
                    info!("Errore mkdir sul server: {:?}", e);
                    reply.error(libc::EIO);
                }
            }
        } else {
            reply.error(ENOENT);        
        }
    }
}

fn main() {
    env_logger::builder().filter_level(LevelFilter::Info).init();
    let mountpoint = "/mnt/remote-fs";
    let remote_fs = RemoteFS::new();

    if !std::path::Path::new(mountpoint).exists() {
        std::fs::create_dir_all(mountpoint).ok(); 
    }   
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
