//! Descriptor-relative Linux I/O. Every directory component is opened with
//! O_NOFOLLOW; renames stay anchored to the directory checked during planning.
use std::{
    ffi::OsString,
    fs::{File, Permissions},
    io::{self, Read, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Component, Path},
    sync::atomic::{AtomicU64, Ordering},
};

use rustix::fs::{self, AtFlags, Mode, OFlags};
use serde::{Deserialize, Serialize};

const LIMIT: u64 = 4 * 1024 * 1024;
static COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub bytes: Vec<u8>,
    pub mode: u32,
}

#[derive(Debug)]
pub struct Dir(File);

impl Dir {
    pub fn open(path: &Path, create: bool) -> io::Result<Self> {
        if !path.is_absolute() {
            return Err(io::Error::other("path must be absolute"));
        }
        let mut fd = fs::open(
            "/",
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        for component in path.components() {
            match component {
                Component::RootDir => continue,
                Component::Normal(name) => {
                    let flags =
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
                    match fs::openat(&fd, name, flags, Mode::empty()) {
                        Ok(next) => fd = next,
                        Err(rustix::io::Errno::NOENT) if create => {
                            match fs::mkdirat(&fd, name, Mode::from_raw_mode(0o700)) {
                                Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                                Err(e) => return Err(e.into()),
                            }
                            fd = fs::openat(&fd, name, flags, Mode::empty())?;
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
                _ => return Err(io::Error::other("path must not contain '..' or '.'")),
            }
        }
        Ok(Self(fd.into()))
    }

    pub fn parent(path: &Path) -> io::Result<(Self, OsString)> {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("missing parent directory"))?;
        let name = path
            .file_name()
            .ok_or_else(|| io::Error::other("missing filename"))?;
        Ok((Self::open(parent, false)?, name.to_owned()))
    }

    pub fn check_writable(&self) -> io::Result<()> {
        let meta = self.0.metadata()?;
        if meta.uid() != rustix::process::getuid().as_raw()
            || meta.mode() & 0o022 != 0
            || meta.mode() & 0o300 != 0o300
        {
            return Err(io::Error::other(
                "destination directory must be user-owned with owner write/execute permissions, and not group/world writable",
            ));
        }
        Ok(())
    }

    pub fn read(&self, name: &std::ffi::OsStr) -> io::Result<Option<Snapshot>> {
        let fd = match fs::openat(
            &self.0,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let file = File::from(fd);
        let meta = file.metadata()?;
        if !meta.is_file() || meta.len() > LIMIT {
            return Err(io::Error::other(
                "expected a regular file no larger than 4 MiB",
            ));
        }
        let mut bytes = Vec::new();
        file.take(LIMIT + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > LIMIT {
            return Err(io::Error::other("file grew beyond 4 MiB"));
        }
        Ok(Some(Snapshot {
            bytes,
            mode: meta.mode() & 0o777,
        }))
    }

    pub fn check_target(&self, name: &std::ffi::OsStr) -> io::Result<()> {
        match fs::statat(&self.0, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) => {
                if fs::FileType::from_raw_mode(stat.st_mode) != fs::FileType::RegularFile
                    || stat.st_nlink != 1
                    || stat.st_uid != rustix::process::getuid().as_raw()
                    || stat.st_mode & 0o7000 != 0
                {
                    return Err(io::Error::other(
                        "destination must be a user-owned regular file, without symlinks, hardlinks or special permission bits",
                    ));
                }
                Ok(())
            }
            Err(rustix::io::Errno::NOENT) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn stage<'a>(&'a self, bytes: &[u8], mode: u32) -> io::Result<Staged<'a>> {
        self.check_writable()?;
        for _ in 0..100 {
            let name = OsString::from(format!(
                ".chroma-{}-{}.tmp",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let fd = match fs::openat(
                &self.0,
                &name,
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o600),
            ) {
                Ok(fd) => fd,
                Err(rustix::io::Errno::EXIST) => continue,
                Err(e) => return Err(e.into()),
            };
            let staged = Staged {
                dir: self,
                name,
                consumed: false,
            };
            let mut file = File::from(fd);
            file.write_all(bytes)?;
            file.set_permissions(Permissions::from_mode(mode & 0o777))?;
            file.sync_all()?;
            return Ok(staged);
        }
        Err(io::Error::other("cannot allocate temporary file"))
    }

    pub fn remove(&self, name: &std::ffi::OsStr) -> io::Result<()> {
        self.check_target(name)?;
        match fs::unlinkat(&self.0, name, AtFlags::empty()) {
            Ok(()) | Err(rustix::io::Errno::NOENT) => {}
            Err(e) => return Err(e.into()),
        }
        self.0.sync_all()
    }

    pub fn lock(&self) -> io::Result<File> {
        self.check_writable()?;
        self.check_target(std::ffi::OsStr::new("lock"))?;
        let fd = fs::openat(
            &self.0,
            "lock",
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::from_raw_mode(0o600),
        )?;
        fs::flock(&fd, fs::FlockOperation::NonBlockingLockExclusive)?;
        Ok(fd.into())
    }
}

pub struct Staged<'a> {
    dir: &'a Dir,
    name: OsString,
    consumed: bool,
}

impl Staged<'_> {
    pub fn commit_new(mut self, destination: &std::ffi::OsStr) -> io::Result<()> {
        fs::renameat_with(
            &self.dir.0,
            &self.name,
            &self.dir.0,
            destination,
            fs::RenameFlags::NOREPLACE,
        )?;
        self.consumed = true;
        self.dir.0.sync_all()
    }

    pub fn commit(mut self, destination: &std::ffi::OsStr) -> io::Result<()> {
        self.dir.check_target(destination)?;
        fs::renameat(&self.dir.0, &self.name, &self.dir.0, destination)?;
        self.consumed = true;
        self.dir.0.sync_all()
    }
}

impl Drop for Staged<'_> {
    fn drop(&mut self) {
        if !self.consumed {
            let _ = fs::unlinkat(&self.dir.0, &self.name, AtFlags::empty());
        }
    }
}

pub fn read_optional(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match Dir::parent(path) {
        Ok((dir, name)) => Ok(dir.read(&name)?.map(|s| s.bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn write(path: &Path, bytes: &[u8], replace: bool) -> io::Result<()> {
    let (dir, name) = Dir::parent(path)?;
    dir.check_target(&name)?;
    let old = dir.read(&name)?;
    if old.is_some() && !replace {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "file exists; explicit --force is required",
        ));
    }
    let mode = old.as_ref().map_or(0o600, |s| s.mode);
    let staged = dir.stage(bytes, mode)?;
    if replace {
        staged.commit(&name)
    } else {
        staged.commit_new(&name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staged_files_are_private_and_removed_when_abandoned() {
        let root = tempfile::tempdir().unwrap();
        let dir = Dir::open(root.path(), false).unwrap();
        {
            let _staged = dir.stage(b"uncommitted", 0o600).unwrap();
            let path = std::fs::read_dir(root.path())
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            assert_eq!(std::fs::metadata(path).unwrap().mode() & 0o777, 0o600);
        }
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn exclusive_commit_refuses_a_file_created_after_staging() {
        let root = tempfile::tempdir().unwrap();
        let dir = Dir::open(root.path(), false).unwrap();
        let staged = dir.stage(b"generated", 0o600).unwrap();
        std::fs::write(root.path().join("target"), b"concurrent user edit").unwrap();
        assert!(staged.commit_new(std::ffi::OsStr::new("target")).is_err());
        assert_eq!(
            std::fs::read(root.path().join("target")).unwrap(),
            b"concurrent user edit"
        );
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
