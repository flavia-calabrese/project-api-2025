use std::{
    collections::{HashMap, HashSet},
    ops::BitOr,
    path::PathBuf,
};

use log::info;
use shared::file_entry::FileEntry;

use crate::{
    Fd, Pid,
    cache::{CachedFile, Inode},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpenFlags(i32);

impl OpenFlags {
    pub const READ: OpenFlags = OpenFlags(0b1); // rappresentazione binaria: primo bit a 1
    pub const WRITE: OpenFlags = OpenFlags(0b10); // rappresentazione binaria: secondo bit a 1
    pub const CREATE: OpenFlags = OpenFlags(libc::O_CREAT);
    pub const TRUNC: OpenFlags = OpenFlags(libc::O_TRUNC);
    pub const APPEND: OpenFlags = OpenFlags(libc::O_APPEND);

    pub const EMPTY: OpenFlags = OpenFlags(0);

    pub const fn contains(self, flag: OpenFlags) -> bool {
        (self.0 & flag.0) != 0
    }

    pub const fn is_read(self) -> bool {
        self.contains(Self::READ)
    }

    pub const fn is_write(self) -> bool {
        self.contains(Self::WRITE)
    }

    pub const fn is_create(self) -> bool {
        self.contains(Self::CREATE)
    }

    pub fn from_flags(mut flags: i32) -> Option<Self> {
        // controlliamo prima le open modes (primi due bit)
        let modes = flags & 0b11;
        let mut open_flags = match modes {
            // 0b00
            libc::O_RDONLY => Self::READ,
            // 0b01
            libc::O_WRONLY => Self::WRITE,
            //0b10
            libc::O_RDWR => Self::READ | Self::WRITE,
            _ => return None,
        };

        flags = flags & !0b11;

        // vedo se il bit corrispondete alla create è settato o meno in flags
        if flags & libc::O_CREAT != 0 {
            // lo setto anche in open_flags
            open_flags = open_flags | Self::CREATE;
            // lo metto a zero in flags -> così che alla fine flags sarà tutto a 0
            flags = flags & !libc::O_CREAT;
        }

        if flags & libc::O_TRUNC != 0 {
            // lo setto anche in open_flags
            open_flags = open_flags | Self::TRUNC;
            // lo metto a zero in flags -> così che alla fine flags sarà tutto a 0
            flags = flags & !libc::O_TRUNC;
        }

        if flags & libc::O_APPEND != 0 {
            // lo setto anche in open_flags
            open_flags = open_flags | Self::APPEND;
            // lo metto a zero in flags -> così che alla fine flags sarà tutto a 0
            flags = flags & !libc::O_APPEND;
        }

        // TODO: fix -> per ora ritorna sempre dei bit sporchi (il 15 esimo)
        /*if flags != 0 {
            info!("flags parsing failed: leftover_flags={:?}", flags);
            return None;
        }
        */

        Some(open_flags)
    }
}

impl BitOr for OpenFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

/// rappresenta lo stato di un file aperto (chi lo ha aperto)
#[derive(Debug, Clone)]
pub struct OpenedFile {
    pub fd: Fd,
    pub ino: Inode,
    pub flags: OpenFlags,
}

/// rappresentazione locale del file
#[derive(Debug, Clone)]
pub struct RfsFile {
    pub file_entry: FileEntry,
    pub file_path: PathBuf,
    pub hard_link: u32,
    pub fds: HashMap<Fd, OpenedFile>,
}

impl From<CachedFile> for RfsFile {
    fn from(value: CachedFile) -> Self {
        Self {
            file_entry: value.file_entry,
            file_path: value.file_path,
            hard_link: 1,
            fds: Default::default(),
        }
    }
}
