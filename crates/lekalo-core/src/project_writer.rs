//! One OS-held, project-wide mutation lock shared by every Model writer
//! (issue #73 r6, review findings devin F2 + omp F2).
//!
//! The research contract (step 7) requires "Serialize mutation across
//! change/migrate/generate/adoption writers". Every mutating service entry
//! acquires this lock before its own service-local lock or journal, so a
//! writer from one service can never interleave source replacements with
//! another service's stage -> verify -> finalize window.
//!
//! The persistent leaf is never unlinked: unlinking after unlocking permits
//! a contender to lock an old inode while another creates a new one. No
//! holder metadata is written, and acquisition never follows a link: the
//! open itself carries the no-follow/neutral-open flag and the opened
//! handle is verified to be a regular, reparse-free, singly linked leaf,
//! so a link planted in any check -> open window is refused by the open,
//! never followed (review finding omp F5).

use std::path::Path;

use crate::change::ChangeError;

/// The project-wide mutation lock leaf (under `.lekalo/cache`), owned by
/// the shared [`WriterLock`]; tests anchor the services' behavior on it.
#[cfg(test)]
pub(crate) const LOCK_PATH: &str = ".lekalo/cache/writer.lock";

/// Which writer family is entering the shared lock: its own durable
/// journal is resolved by its own recovery path after exclusivity is
/// held, so the sibling-barrier check skips it (a family never refuses
/// to recover its own interruption).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MutationFamily {
    Change,
    Migration,
    Format,
    Generate,
    Adoption,
}

/// Why a mutating-writers entry refused: the shared lock itself, or a
/// sibling service's durable recovery barrier checked under it. Each
/// writer family projects these onto its own registered rules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MutationRefusal {
    /// A live writer holds the shared project-wide mutation lock.
    LockHeld,
    /// A change transaction awaits recovery (the change family's durable
    /// barrier; denied with `change.recovery-required`).
    ChangeRecovery { transaction: String },
    /// A migration or formatter journal awaits recovery: the registered
    /// sibling-family rule code is carried for exact projection.
    SiblingJournal { code: &'static str },
}

/// The kernel releases exclusivity on drop or process death. Directory
/// handles keep the Windows lock home from being renamed during ownership.
pub(crate) struct WriterLock {
    _file: std::fs::File,
    #[cfg(windows)]
    _directories: Vec<std::fs::File>,
}

impl WriterLock {
    pub(crate) fn acquire(root: &Path) -> Result<Self, crate::change::ChangeError> {
        Self::open(root).map_err(|_| crate::change::ChangeError::PreconditionFailed {
            subject: "writer-lock".to_owned(),
            detail: "held",
        })
    }

    /// The mutating-writers entry: acquire the shared lock, then refuse
    /// while any sibling service's interrupted transaction awaits recovery.
    /// An interrupted change/migration/format journal must never be
    /// overwritten by another service's writer, so the durable-barrier
    /// check runs only AFTER exclusivity is held (a writer that passed its
    /// own load barrier before the journal was published still stops here).
    pub(crate) fn mutation(root: &Path, family: MutationFamily) -> Result<Self, MutationRefusal> {
        let guard = Self::acquire(root).map_err(|_| MutationRefusal::LockHeld)?;
        if family != MutationFamily::Change {
            if let Err(ChangeError::RecoveryRequired { transaction }) =
                crate::change::refuse_while_recovery_pending(root)
            {
                return Err(MutationRefusal::ChangeRecovery { transaction });
            }
        }
        let sibling = |fs: &crate::project_fs::Fs| {
            let migration = (family != MutationFamily::Migration)
                .then(|| crate::versioning::migration_recovery_code(fs))
                .flatten();
            let format = (family != MutationFamily::Format)
                .then(|| crate::format::transaction::recovery_code(fs))
                .flatten();
            // r8 (devin F1/F3 + omp F3): the adoption recovery journals
            // join the sibling-barrier set — before r8 they were in NO
            // barrier set, so a stranded installation serialized against
            // nothing. The adoption family skips its own barrier
            // (recovery is the resolver).
            let adoption = (family != MutationFamily::Adoption)
                .then(|| crate::adoption::recovery_barrier_code(fs))
                .flatten();
            migration.or(format).or(adoption)
        };
        match crate::project_fs::Fs::open(root).map(|fs| sibling(&fs)) {
            Ok(Some(code)) => return Err(MutationRefusal::SiblingJournal { code }),
            Ok(None) => {}
            Err(_) => {
                return Err(MutationRefusal::SiblingJournal {
                    code: "versioning.recovery-required",
                })
            }
        }
        Ok(guard)
    }

    #[cfg(unix)]
    fn open(root: &Path) -> std::io::Result<Self> {
        use rustix::fs::{flock, mkdirat, openat, FileType, FlockOperation, Mode, OFlags, CWD};
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let mut dir = openat(CWD, root, flags, Mode::empty())?;
        for name in [".lekalo", "cache"] {
            match mkdirat(&dir, name, Mode::RUSR | Mode::WUSR | Mode::XUSR) {
                Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                Err(error) => return Err(error.into()),
            }
            dir = openat(&dir, name, flags, Mode::empty())?;
        }
        let fd = openat(
            &dir,
            "writer.lock",
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::RUSR | Mode::WUSR,
        )?;
        let stat = rustix::fs::fstat(&fd)?;
        if !FileType::from_raw_mode(stat.st_mode).is_file() || stat.st_nlink != 1 {
            return Err(std::io::Error::other("unconfined writer lock"));
        }
        flock(&fd, FlockOperation::NonBlockingLockExclusive)?;
        Ok(Self { _file: fd.into() })
    }

    #[cfg(windows)]
    fn open(root: &Path) -> std::io::Result<Self> {
        use std::os::windows::fs::OpenOptionsExt;
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
            FILE_FLAG_OPEN_REPARSE_POINT,
        };
        fn verify(file: &std::fs::File, directory: bool) -> std::io::Result<()> {
            let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
            if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0
                || info.dwFileAttributes & 0x400 != 0
                || (info.dwFileAttributes & 0x10 != 0) != directory
                || (!directory && info.nNumberOfLinks != 1)
            {
                return Err(std::io::Error::other("unconfined writer lock"));
            }
            Ok(())
        }
        let mut path = root.to_path_buf();
        let mut directories = Vec::new();
        // Share reads/writes, not deletes: each checked parent stays put.
        for name in [None, Some(".lekalo"), Some("cache")] {
            if let Some(name) = name {
                path.push(name);
                match std::fs::create_dir(&path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error),
                }
            }
            let dir = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(3)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&path)?;
            verify(&dir, true)?;
            directories.push(dir);
        }
        path.push("writer.lock");
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)?;
        verify(&file, false)?;
        Ok(Self {
            _file: file,
            _directories: directories,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquisition_is_exclusive_and_persistent() {
        let root = tempfile::tempdir().unwrap();
        let first = WriterLock::acquire(root.path()).unwrap();
        assert!(WriterLock::acquire(root.path()).is_err());
        drop(first);
        assert!(root.path().join(LOCK_PATH).is_file());
        assert!(WriterLock::acquire(root.path()).is_ok());
    }

    #[test]
    fn shared_inode_lock_is_never_written_or_adopted() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".lekalo/cache")).unwrap();
        let victim = outside.path().join("victim");
        std::fs::write(&victim, b"untouched").unwrap();
        std::fs::hard_link(&victim, root.path().join(LOCK_PATH)).unwrap();
        assert!(WriterLock::acquire(root.path()).is_err());
        assert_eq!(std::fs::read(victim).unwrap(), b"untouched");
    }

    // Calls the OS open directly, with no preflight metadata check: a link
    // appearing between a caller's check and open is still not followed.
    #[test]
    fn race_planted_link_is_refused_by_the_open_itself() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".lekalo/cache")).unwrap();
        let victim = outside.path().join("victim");
        std::fs::write(&victim, b"untouched").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&victim, root.path().join(LOCK_PATH)).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&victim, root.path().join(LOCK_PATH)).unwrap();
        assert!(WriterLock::open(root.path()).is_err());
        assert_eq!(std::fs::read(victim).unwrap(), b"untouched");
    }
}
