//! Capability-safe project filesystem access for the loader.
//!
//! This layer ports the accepted #4 physical rules (`scripts/check-structure.mjs`)
//! into the loader's own read path and provides descriptor-relative,
//! no-follow reads on Unix and reparse-checked component reads on Windows.
//! It never writes, never reads `.lekalo/**` as model input, and fails
//! closed on every enumeration, metadata, or read error.

use std::path::{Component, Path, PathBuf};

/// Maximum directory depth accepted while scanning a governed tree.
pub const MAX_WALK_DEPTH: usize = 64;

/// Maximum entries accepted across one governed-tree scan.
pub const MAX_WALK_ENTRIES: usize = 10_000;

/// `FILE_ATTRIBUTE_REPARSE_POINT` — every Windows reparse point (symlink,
/// junction, mount point) must be rejected on sight.
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;

/// The optional definition-kind file stems inside a module directory.
pub const KIND_STEMS: [&str; 7] = [
    "entities",
    "commands",
    "queries",
    "policies",
    "events",
    "scenarios",
    "bindings",
];

/// `module.yaml` plus every kind stem: the closed module file set.
pub const MODULE_STEMS: [&str; 8] = [
    "module",
    "entities",
    "commands",
    "queries",
    "policies",
    "events",
    "scenarios",
    "bindings",
];

/// Physical entry classification used by the structure scan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryType {
    File,
    Directory,
    Symlink,
    Special,
}

/// One stable structure reason: a `structure.*` code plus an optional
/// logical project-relative path.
#[derive(Clone, Debug, PartialEq)]
pub struct StructureReason {
    pub code: &'static str,
    pub path: Option<String>,
}

impl StructureReason {
    pub const fn new(code: &'static str) -> Self {
        Self { code, path: None }
    }

    pub fn at(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
}

/// The module directory names and target stems found by validation.
#[derive(Clone, Debug)]
pub struct StructureReport {
    pub modules: Vec<String>,
    pub targets: Vec<String>,
}

/// Terminal outcome of the structure validation pass.
#[derive(Clone, Debug)]
pub enum StructureOutcome {
    Valid(StructureReport),
    Invalid(Vec<StructureReason>),
    Denied(Vec<StructureReason>),
}

/// Fail-closed filesystem error class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsErrorKind {
    /// Any non-missing I/O or metadata failure.
    Io,
    /// The entry does not exist; the only legal "absent" signal.
    NotFound,
    /// A read exceeded its byte limit.
    Limit { max: usize },
}

/// Whether raw metadata identifies a symlink or Windows reparse point.
fn metadata_is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(unix)]
    {
        metadata.file_type().is_symlink()
    }
}

/// Ports `checkPathGrammar`: the closed project-relative path grammar.
///
/// Returns the first stable `structure.*` violation code, or `None` when the
/// path is portable. Classification order matches the checker exactly.
pub fn path_violation(path: &str) -> Option<&'static str> {
    if path.is_empty() {
        return Some("structure.path-empty");
    }
    if !is_nfc(path) {
        return Some("structure.path-not-nfc");
    }
    if path.starts_with('/') || path.starts_with("\\\\") || path.starts_with("//") {
        return Some("structure.path-absolute");
    }
    if starts_with_drive_or_scheme(path) || path.starts_with('~') {
        return Some("structure.path-absolute");
    }
    if path.contains('\\') || contains_percent_escape(path) || path.contains(':') {
        return Some("structure.path-escape");
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment.len() > 64 {
            return Some("structure.path-segment");
        }
        if segment == "." || segment == ".." || segment.ends_with('.') || segment.ends_with(' ') {
            return Some("structure.path-traversal");
        }
        if is_dos_device(segment) {
            return Some("structure.path-device");
        }
        if contains_short_name(segment) {
            return Some("structure.path-short-name");
        }
        if segment.chars().any(|c| c.is_ascii_uppercase()) {
            return Some("structure.path-case");
        }
        if !segment_is_portable(segment) {
            return Some("structure.path-segment");
        }
    }
    None
}

/// Ports `checkSelectionGrammar`: the invocation-relative root selector
/// grammar.
pub fn selection_violation(selector: &str) -> Option<&'static str> {
    if selector.is_empty() {
        return Some("structure.selection-empty");
    }
    if !is_nfc(selector) || selector.chars().any(is_control_char) {
        return Some("structure.selection-segment");
    }
    if selector.starts_with('/')
        || selector.starts_with("\\\\")
        || selector.starts_with("//")
        || selector.starts_with('~')
    {
        return Some("structure.selection-absolute");
    }
    if starts_with_drive_or_scheme(selector) {
        return Some("structure.selection-absolute");
    }
    if selector.contains('\\') || contains_percent_escape(selector) || selector.contains(':') {
        return Some("structure.selection-escape");
    }
    if selector == "." {
        return None;
    }
    let trimmed = selector.strip_prefix("./").unwrap_or(selector);
    for segment in trimmed.split('/') {
        if segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.ends_with('.')
            || segment.ends_with(' ')
        {
            return Some("structure.selection-traversal");
        }
        if is_dos_device(segment) {
            return Some("structure.selection-device");
        }
        if contains_short_name(segment) {
            return Some("structure.selection-short-name");
        }
    }
    None
}

fn starts_with_drive_or_scheme(path: &str) -> bool {
    let mut chars = path.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    let scheme_run = chars
        .by_ref()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '+' || *c == '.' || *c == '-')
        .count();
    // Either `X:` (drive) or `X<run>:` (URI-style scheme).
    path.chars().nth(1) == Some(':') || path.chars().nth(1 + scheme_run) == Some(':')
}

fn is_control_char(character: char) -> bool {
    matches!(character as u32, 0x0000..=0x001F | 0x007F)
}

fn contains_percent_escape(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes
        .windows(3)
        .any(|w| w[0] == b'%' && w[1].is_ascii_hexdigit() && w[2].is_ascii_hexdigit())
}

fn contains_short_name(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    bytes
        .windows(2)
        .any(|w| w[0] == b'~' && w[1].is_ascii_digit())
}

fn segment_is_portable(segment: &str) -> bool {
    let mut chars = segment.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() || first.is_ascii_digit() => {}
        _ => return false,
    }
    chars.count() <= 63
        && segment.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_' || c == '-'
        })
}

/// Unicode NFC acceptance (the raw spelling must already be NFC).
fn is_nfc(text: &str) -> bool {
    use unicode_normalization::UnicodeNormalization;
    text.chars().nfc().eq(text.chars())
}

/// DOS device detection after NFKC compatibility normalization, matching the
/// checker's closed device list (device name with any extension).
fn is_dos_device(segment: &str) -> bool {
    use unicode_normalization::UnicodeNormalization;
    const DOS_DEVICES: [&str; 25] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9", "CONIN$",
        "CONOUT$", "CLOCK$",
    ];
    let normalized: String = segment.chars().nfkc().collect();
    let base = normalized.split('.').next().unwrap_or("");
    let upper = base.to_uppercase();
    DOS_DEVICES.contains(&upper.as_str())
}

/// Split a logical POSIX path into segments (empty logical path = root).
fn logical_segments(logical: &str) -> impl Iterator<Item = &str> {
    logical.split('/').filter(|segment| !segment.is_empty())
}

/// Shared sort helper: entries are ordered by raw byte order of their names.
pub fn sort_names(entries: &mut [(String, EntryType)]) {
    entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
}

#[cfg(unix)]
mod imp {
    use super::{EntryType, FsErrorKind};
    use rustix::fd::OwnedFd;
    use rustix::fs::{openat, statat, AtFlags, Dir, FileType, Mode, OFlags};
    use rustix::io::Errno;
    use std::path::Path;

    /// Root directory handle: an O_NOFOLLOW|O_DIRECTORY descriptor.
    pub struct FsInner {
        root_fd: OwnedFd,
    }

    fn classify(file_type: FileType) -> EntryType {
        if file_type.is_file() {
            EntryType::File
        } else if file_type.is_dir() {
            EntryType::Directory
        } else if file_type.is_symlink() {
            EntryType::Symlink
        } else {
            EntryType::Special
        }
    }

    fn error_of(errno: Errno) -> FsErrorKind {
        if errno == Errno::NOENT {
            FsErrorKind::NotFound
        } else {
            FsErrorKind::Io
        }
    }

    const DIR_OPEN: OFlags = OFlags::RDONLY
        .union(OFlags::DIRECTORY)
        .union(OFlags::NOFOLLOW)
        .union(OFlags::CLOEXEC);

    impl super::Fs {
        pub fn open(root: &Path) -> Result<Self, FsErrorKind> {
            let root_fd =
                openat(rustix::fs::CWD, root, DIR_OPEN, Mode::empty()).map_err(error_of)?;
            Ok(Self {
                inner: FsInner { root_fd },
            })
        }

        /// Open a directory descriptor for a logical path ("" = root).
        ///
        /// Every component is opened `openat`-relative with `O_NOFOLLOW`, so
        /// a component swapped for a symlink mid-walk fails closed instead
        /// of escaping the root.
        fn dir_handle(&self, logical: &str) -> Result<OwnedFd, FsErrorKind> {
            let mut current = self
                .inner
                .root_fd
                .try_clone()
                .map_err(|_| FsErrorKind::Io)?;
            for segment in super::logical_segments(logical) {
                current = openat(&current, segment, DIR_OPEN, Mode::empty()).map_err(error_of)?;
            }
            Ok(current)
        }

        pub fn entry_type(&self, logical_dir: &str, name: &str) -> Result<EntryType, FsErrorKind> {
            let dir = self.dir_handle(logical_dir)?;
            let stat = statat(&dir, name, AtFlags::SYMLINK_NOFOLLOW).map_err(error_of)?;
            Ok(classify(FileType::from_raw_mode(stat.st_mode)))
        }

        pub fn entries(&self, logical_dir: &str) -> Result<Vec<(String, EntryType)>, FsErrorKind> {
            let dir = self.dir_handle(logical_dir)?;
            let mut handle = Dir::read_from(&dir).map_err(error_of)?;
            let mut collected = Vec::new();
            while let Some(entry) = handle.read() {
                let entry = entry.map_err(error_of)?;
                // Raw getdents yields "." and ".."; std read_dir semantics
                // (the shared contract) exclude them.
                let raw = entry.file_name();
                if raw.to_bytes() == b"." || raw.to_bytes() == b".." {
                    continue;
                }
                let name = raw.to_str().map_err(|_| FsErrorKind::Io)?.to_owned();
                let entry_type = match entry.file_type() {
                    FileType::Unknown => {
                        // d_type unknown on this filesystem: resolve with a
                        // no-follow statat instead of guessing.
                        let stat = statat(&dir, entry.file_name(), AtFlags::SYMLINK_NOFOLLOW)
                            .map_err(error_of)?;
                        classify(FileType::from_raw_mode(stat.st_mode))
                    }
                    other => classify(other),
                };
                collected.push((name, entry_type));
            }
            super::sort_names(&mut collected);
            Ok(collected)
        }

        pub fn read_file_opt(
            &self,
            logical_dir: &str,
            name: &str,
            max: usize,
        ) -> Result<Option<Vec<u8>>, FsErrorKind> {
            let dir = self.dir_handle(logical_dir)?;
            let before = statat(&dir, name, AtFlags::SYMLINK_NOFOLLOW).map_err(error_of)?;
            if !FileType::from_raw_mode(before.st_mode).is_file() {
                return Err(FsErrorKind::Io);
            }
            let file = openat(
                &dir,
                name,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(error_of)?;
            let after = rustix::fs::fstat(&file).map_err(error_of)?;
            // Close the preflight/read window: same device, inode, and type.
            if after.st_dev != before.st_dev
                || after.st_ino != before.st_ino
                || !FileType::from_raw_mode(after.st_mode).is_file()
            {
                return Err(FsErrorKind::Io);
            }
            read_limited(std::fs::File::from(file), max).map(Some)
        }
    }

    fn read_limited(file: std::fs::File, max: usize) -> Result<Vec<u8>, FsErrorKind> {
        use std::io::Read;
        let mut buffer = Vec::new();
        file.take((max as u64) + 1)
            .read_to_end(&mut buffer)
            .map_err(|_| FsErrorKind::Io)?;
        if buffer.len() > max {
            return Err(FsErrorKind::Limit { max });
        }
        Ok(buffer)
    }
}

#[cfg(windows)]
mod imp {
    use super::{metadata_is_link, EntryType, FsErrorKind};
    use std::fs;
    use std::io::Read;
    use std::path::{Path, PathBuf};

    pub struct FsInner {
        root: PathBuf,
    }

    fn classify(metadata: &fs::Metadata) -> EntryType {
        if metadata_is_link(metadata) {
            EntryType::Symlink
        } else if metadata.is_dir() {
            EntryType::Directory
        } else if metadata.is_file() {
            EntryType::File
        } else {
            EntryType::Special
        }
    }

    fn error_of(error: &std::io::Error) -> FsErrorKind {
        if error.kind() == std::io::ErrorKind::NotFound {
            FsErrorKind::NotFound
        } else {
            FsErrorKind::Io
        }
    }

    impl FsInner {
        fn physical(&self, logical: &str) -> PathBuf {
            let mut path = self.root.clone();
            for segment in super::logical_segments(logical) {
                path.push(segment);
            }
            path
        }
    }

    impl super::Fs {
        pub fn open(root: &Path) -> Result<Self, FsErrorKind> {
            let metadata = fs::symlink_metadata(root).map_err(|error| error_of(&error))?;
            if classify(&metadata) != EntryType::Directory {
                return Err(FsErrorKind::Io);
            }
            Ok(Self {
                inner: FsInner {
                    root: root.to_path_buf(),
                },
            })
        }

        pub fn entry_type(&self, logical_dir: &str, name: &str) -> Result<EntryType, FsErrorKind> {
            let mut path = self.inner.physical(logical_dir);
            path.push(name);
            let metadata = fs::symlink_metadata(&path).map_err(|error| error_of(&error))?;
            Ok(classify(&metadata))
        }

        pub fn entries(&self, logical_dir: &str) -> Result<Vec<(String, EntryType)>, FsErrorKind> {
            let path = self.inner.physical(logical_dir);
            let read = fs::read_dir(&path).map_err(|error| error_of(&error))?;
            let mut collected = Vec::new();
            for entry in read {
                let entry = entry.map_err(|error| error_of(&error))?;
                let name = entry
                    .file_name()
                    .to_str()
                    .ok_or(FsErrorKind::Io)?
                    .to_owned();
                // std treats junctions as directories; the reparse attribute
                // is the authoritative link signal on Windows.
                let metadata =
                    fs::symlink_metadata(entry.path()).map_err(|error| error_of(&error))?;
                collected.push((name, classify(&metadata)));
            }
            super::sort_names(&mut collected);
            Ok(collected)
        }

        pub fn read_file_opt(
            &self,
            logical_dir: &str,
            name: &str,
            max: usize,
        ) -> Result<Option<Vec<u8>>, FsErrorKind> {
            let mut component_path = self.inner.root.clone();
            for segment in super::logical_segments(logical_dir) {
                component_path.push(segment);
                let metadata =
                    fs::symlink_metadata(&component_path).map_err(|error| error_of(&error))?;
                if classify(&metadata) != EntryType::Directory {
                    return Err(FsErrorKind::Io);
                }
            }
            component_path.push(name);
            let before = fs::symlink_metadata(&component_path).map_err(|error| error_of(&error))?;
            if classify(&before) != EntryType::File {
                return Err(FsErrorKind::Io);
            }
            let file = fs::File::open(&component_path).map_err(|error| error_of(&error))?;
            let after = file.metadata().map_err(|error| error_of(&error))?;
            // Close the preflight/read window as far as the platform permits:
            // same reparse-free regular-file classification and same length.
            if classify(&after) != EntryType::File || after.len() != before.len() {
                return Err(FsErrorKind::Io);
            }
            let mut buffer = Vec::new();
            file.take((max as u64) + 1)
                .read_to_end(&mut buffer)
                .map_err(|_| FsErrorKind::Io)?;
            if buffer.len() > max {
                return Err(FsErrorKind::Limit { max });
            }
            Ok(Some(buffer))
        }
    }
}

use imp::FsInner;

/// The validated-root filesystem capability. All access is logical-path
/// based, no-follow, and fail-closed.
pub struct Fs {
    inner: FsInner,
}

/// Result alias for filesystem operations.
pub type FsResult<T> = Result<T, FsErrorKind>;

impl Fs {
    /// Reject every component of an absolute path that is a symlink, Windows
    /// reparse point, or (on Windows) an 8.3/case alias of a longer name.
    ///
    /// Missing intermediate components surface as
    /// `structure.selection-unreadable` to match the checker's realpath
    /// failure classification.
    fn reject_aliases(absolute: &Path) -> Result<(), StructureReason> {
        let mut prefix = PathBuf::new();
        for component in absolute.components() {
            // Drive/UNC prefixes and the root separator are the trusted
            // anchor; current-directory components carry no information.
            let name: &std::ffi::OsStr = match &component {
                Component::Prefix(_) | Component::RootDir => {
                    prefix.push(component.as_os_str());
                    continue;
                }
                Component::CurDir => continue,
                Component::ParentDir => {
                    return Err(StructureReason::new("structure.selection-traversal"));
                }
                Component::Normal(name) => name,
            };
            prefix.push(name);
            let metadata = std::fs::symlink_metadata(&prefix)
                .map_err(|_| StructureReason::new("structure.selection-unreadable"))?;
            if metadata_is_link(&metadata) {
                return Err(StructureReason::new("structure.selection-alias"));
            }
            #[cfg(windows)]
            {
                // 8.3 short names resolve to a final name that differs from
                // the spelling; the checker's realpath comparison denies it.
                let resolved = std::fs::canonicalize(&prefix)
                    .map_err(|_| StructureReason::new("structure.selection-unreadable"))?;
                if resolved.file_name() != Some(name) {
                    return Err(StructureReason::new("structure.selection-alias"));
                }
            }
        }
        Ok(())
    }

    /// Ports `checkPhysicalSelection` for an invocation-relative selector.
    /// Returns the absolute, alias-free selection directory.
    pub fn check_selection(
        selector: &str,
        missing_code: &'static str,
    ) -> Result<PathBuf, StructureOutcome> {
        let cwd = std::env::current_dir().map_err(|_| {
            StructureOutcome::Invalid(vec![StructureReason::new("structure.selection-unreadable")])
        })?;
        let lexical = if selector == "." {
            cwd
        } else {
            let mut path = cwd;
            for segment in selector.split('/') {
                if segment.is_empty() || segment == "." {
                    continue;
                }
                path.push(segment);
            }
            path
        };
        let metadata = match std::fs::symlink_metadata(&lexical) {
            Ok(metadata) => metadata,
            Err(_) => {
                return Err(StructureOutcome::Invalid(vec![StructureReason::new(
                    missing_code,
                )]))
            }
        };
        if metadata_is_link(&metadata) {
            return Err(StructureOutcome::Denied(vec![StructureReason::new(
                "structure.selection-alias",
            )]));
        }
        if !metadata.is_dir() {
            return Err(StructureOutcome::Invalid(vec![StructureReason::new(
                missing_code,
            )]));
        }
        match Self::reject_aliases(&lexical) {
            Ok(()) => Ok(lexical),
            Err(reason) if reason.code == "structure.selection-unreadable" => {
                Err(StructureOutcome::Invalid(vec![reason]))
            }
            Err(reason) => Err(StructureOutcome::Denied(vec![reason])),
        }
    }

    /// Ports `findRoot`: walk upward from `from` to the nearest marker.
    pub fn find_root(from: &Path) -> Result<Option<PathBuf>, StructureOutcome> {
        let entry_kind =
            |path: &Path| -> Option<std::fs::Metadata> { std::fs::symlink_metadata(path).ok() };
        let mut current = from.to_path_buf();
        loop {
            for candidate in [
                current.join("lekalo"),
                current.join("lekalo").join("project.yaml"),
            ] {
                if entry_kind(&candidate).is_some_and(|metadata| metadata_is_link(&metadata)) {
                    return Err(StructureOutcome::Denied(vec![StructureReason::new(
                        "structure.path-link",
                    )]));
                }
            }
            let lekalo_dir = entry_kind(&current.join("lekalo"));
            let marker = entry_kind(&current.join("lekalo").join("project.yaml"));
            if let (Some(lekalo_dir), Some(marker)) = (lekalo_dir, marker) {
                if lekalo_dir.is_dir() && marker.is_file() {
                    return Ok(Some(current));
                }
            }
            match current.parent() {
                Some(parent) if parent != current => current = parent.to_path_buf(),
                _ => return Ok(None),
            }
        }
    }
}

/// The closed set of canonical `lekalo/` root entries.
const CANONICAL_ROOT_ENTRIES: [&str; 4] =
    ["project.yaml", "modules", "targets", "authorization.yaml"];

/// The closed runtime top-level entries under `.lekalo/`.
const RUNTIME_ENTRIES: [&str; 5] = ["import", "cache", "generated", "consumer", "privacy"];

const RUNTIME_CONSUMER_CHILDREN: [&str; 2] = ["model", "bindings"];
const RUNTIME_PRIVACY_CHILDREN: [&str; 4] = ["exports", "redacted", "aggregates", "decisions"];
const RUNTIME_DECISIONS_CHILDREN: [&str; 3] = ["export", "redaction", "aggregate"];

/// One stop-early failure collected during a governed-tree scan.
type ScanFailure = StructureReason;

struct ScanCounter {
    entries: usize,
}

impl ScanCounter {
    fn new() -> Self {
        Self { entries: 0 }
    }
}

fn runtime_placement_failure(entry_type: EntryType, logical: &str) -> Option<ScanFailure> {
    let runtime = logical.strip_prefix(".lekalo/")?;
    let segments: Vec<&str> = runtime.split('/').collect();
    let top = segments[0];
    if !RUNTIME_ENTRIES.contains(&top) {
        return Some(StructureReason::new("structure.runtime-unexpected-entry").at(logical));
    }
    if segments.len() == 1 {
        return (entry_type != EntryType::Directory)
            .then(|| StructureReason::new("structure.runtime-unexpected-entry").at(logical));
    }
    if matches!(top, "import" | "cache" | "generated") {
        return None;
    }
    if top == "consumer" {
        if !RUNTIME_CONSUMER_CHILDREN.contains(&segments[1]) {
            return Some(StructureReason::new("structure.runtime-unexpected-entry").at(logical));
        }
        return (segments.len() == 2 && entry_type != EntryType::Directory)
            .then(|| StructureReason::new("structure.runtime-unexpected-entry").at(logical));
    }
    if !RUNTIME_PRIVACY_CHILDREN.contains(&segments[1]) {
        return Some(StructureReason::new("structure.runtime-unexpected-entry").at(logical));
    }
    if segments.len() == 2 {
        return (entry_type != EntryType::Directory)
            .then(|| StructureReason::new("structure.runtime-unexpected-entry").at(logical));
    }
    if segments[1] != "decisions" {
        return None;
    }
    if !RUNTIME_DECISIONS_CHILDREN.contains(&segments[2]) {
        return Some(StructureReason::new("structure.runtime-unexpected-entry").at(logical));
    }
    if segments.len() == 3 {
        return (entry_type != EntryType::Directory)
            .then(|| StructureReason::new("structure.runtime-unexpected-entry").at(logical));
    }
    None
}

/// Ports `scanGovernedTree`: fail-closed, sorted, bounded directory scan.
fn scan_governed_tree(
    fs: &Fs,
    dir_logical: &str,
    depth: usize,
    counter: &mut ScanCounter,
    visit: &mut dyn FnMut(EntryType, &str, &str) -> Option<ScanFailure>,
) -> Option<ScanFailure> {
    if depth > MAX_WALK_DEPTH || counter.entries > MAX_WALK_ENTRIES {
        return Some(StructureReason::new("structure.scan-limit").at(dir_logical));
    }
    let entries = match fs.entries(dir_logical) {
        Ok(entries) => entries,
        Err(_) => {
            return Some(StructureReason::new("structure.directory-unreadable").at(dir_logical))
        }
    };
    for (name, entry_type) in entries {
        counter.entries += 1;
        if counter.entries > MAX_WALK_ENTRIES {
            return Some(StructureReason::new("structure.scan-limit").at(dir_logical));
        }
        let logical = format!("{dir_logical}/{name}");
        if entry_type == EntryType::Symlink {
            return Some(StructureReason::new("structure.path-link").at(&logical));
        }
        if entry_type != EntryType::File && entry_type != EntryType::Directory {
            return Some(StructureReason::new("structure.path-special").at(&logical));
        }
        if let Some(code) = path_violation(&name) {
            return Some(StructureReason::new(code).at(&logical));
        }
        if let Some(failure) = visit(entry_type, &name, &logical) {
            return Some(failure);
        }
        if entry_type == EntryType::Directory {
            if let Some(failure) = scan_governed_tree(fs, &logical, depth + 1, counter, visit) {
                return Some(failure);
            }
        }
    }
    None
}

impl Fs {
    /// Validate an absolute project root: alias rejection, then the full
    /// canonical-tree, module, target, lockfile, and runtime checks.
    pub fn validate_project(root: &Path) -> StructureOutcome {
        if let Err(reason) = Self::reject_aliases(root) {
            return classify_failure(reason);
        }
        let fs = match Fs::open(root) {
            Ok(fs) => fs,
            Err(_) => {
                return StructureOutcome::Invalid(vec![StructureReason::new(
                    "structure.root-unreadable",
                )])
            }
        };
        match fs.validate_selected_root() {
            Ok(report) => StructureOutcome::Valid(report),
            Err(failure) => classify_failure(failure),
        }
    }

    /// Validate the already-selected root: canonical tree, modules, targets,
    /// lockfile, and runtime placement, in the checker's order.
    pub(crate) fn validate_selected_root(&self) -> Result<StructureReport, ScanFailure> {
        // lekalo/ must be a real directory.
        let lekalo_type = self.entry_type("", "lekalo").map_err(|_| {
            StructureReason::new("structure.document-missing").at("lekalo/project.yaml")
        })?;
        if lekalo_type == EntryType::Symlink {
            return Err(StructureReason::new("structure.path-link").at("lekalo"));
        }
        if lekalo_type != EntryType::Directory {
            return Err(
                StructureReason::new("structure.document-missing").at("lekalo/project.yaml")
            );
        }

        // Grammar/link/special/nested-root scan of the whole canonical tree.
        let mut counter = ScanCounter::new();
        if let Some(failure) = scan_governed_tree(
            self,
            "lekalo",
            0,
            &mut counter,
            &mut |entry_type, name, logical| {
                if entry_type == EntryType::Directory && name == "lekalo" {
                    return Some(StructureReason::new("structure.nested-root").at(logical));
                }
                None
            },
        ) {
            return Err(failure);
        }

        // project.yaml must be a physical regular file.
        match self.entry_type("lekalo", "project.yaml").map_err(|_| {
            StructureReason::new("structure.document-missing").at("lekalo/project.yaml")
        })? {
            EntryType::File => {}
            _ => {
                return Err(
                    StructureReason::new("structure.document-missing").at("lekalo/project.yaml")
                )
            }
        }

        // Closed root entries.
        for (name, entry_type) in self
            .entries("lekalo")
            .map_err(|_| StructureReason::new("structure.directory-unreadable").at("lekalo"))?
        {
            if !CANONICAL_ROOT_ENTRIES.contains(&name.as_str()) {
                return Err(StructureReason::new("structure.canonical-unexpected-entry")
                    .at(format!("lekalo/{name}")));
            }
            if name == "project.yaml" && entry_type != EntryType::File {
                return Err(
                    StructureReason::new("structure.document-missing").at("lekalo/project.yaml")
                );
            }
            if matches!(name.as_str(), "modules" | "targets") && entry_type != EntryType::Directory
            {
                return Err(StructureReason::new("structure.directory-required")
                    .at(format!("lekalo/{name}")));
            }
            if name == "authorization.yaml" && entry_type != EntryType::File {
                return Err(StructureReason::new("structure.directory-required")
                    .at("lekalo/authorization.yaml"));
            }
        }

        // Modules.
        let mut modules = Vec::new();
        match self.entry_type("lekalo", "modules") {
            Ok(EntryType::Directory) | Err(FsErrorKind::NotFound) => {}
            Ok(_) => {
                return Err(
                    StructureReason::new("structure.directory-required").at("lekalo/modules")
                )
            }
            Err(_) => {
                return Err(
                    StructureReason::new("structure.directory-unreadable").at("lekalo/modules")
                )
            }
        }
        if let Ok(module_dirs) = self.entries("lekalo/modules").map_err(|_| {
            StructureReason::new("structure.directory-unreadable").at("lekalo/modules")
        }) {
            for (name, entry_type) in module_dirs {
                let module_path = format!("lekalo/modules/{name}");
                if entry_type != EntryType::Directory {
                    return Err(StructureReason::new("structure.canonical-unexpected-entry")
                        .at(&module_path));
                }
                modules.push(name.clone());
                match self.entry_type(&module_path, "module.yaml").map_err(|_| {
                    StructureReason::new("structure.document-missing")
                        .at(format!("{module_path}/module.yaml"))
                })? {
                    EntryType::File => {}
                    _ => {
                        return Err(StructureReason::new("structure.document-missing")
                            .at(format!("{module_path}/module.yaml")))
                    }
                }
                for (file, file_type) in self.entries(&module_path).map_err(|_| {
                    StructureReason::new("structure.directory-unreadable").at(&module_path)
                })? {
                    let file_path = format!("{module_path}/{file}");
                    if file_type == EntryType::Directory {
                        return Err(
                            StructureReason::new("structure.module-subdirectory").at(&file_path)
                        );
                    }
                    let stem = file.strip_suffix(".yaml");
                    match stem {
                        Some(stem) if MODULE_STEMS.contains(&stem) => {}
                        _ => {
                            return Err(StructureReason::new(
                                "structure.canonical-unexpected-entry",
                            )
                            .at(&file_path))
                        }
                    }
                }
            }
        }

        // Targets.
        let mut targets = Vec::new();
        if self.entry_type("lekalo", "targets") == Ok(EntryType::Directory) {
            for (name, entry_type) in self.entries("lekalo/targets").map_err(|_| {
                StructureReason::new("structure.directory-unreadable").at("lekalo/targets")
            })? {
                let target_path = format!("lekalo/targets/{name}");
                let legal =
                    entry_type == EntryType::File && name.ends_with(".yaml") && name != ".yaml";
                if !legal {
                    return Err(StructureReason::new("structure.canonical-unexpected-entry")
                        .at(&target_path));
                }
                targets.push(name.trim_end_matches(".yaml").to_owned());
            }
        }

        // Lockfile.
        match self.entry_type("", "lekalo.lock") {
            Ok(EntryType::Symlink) => {
                return Err(StructureReason::new("structure.path-link").at("lekalo.lock"))
            }
            Ok(EntryType::File) | Err(FsErrorKind::NotFound) => {}
            Ok(_) => return Err(StructureReason::new("structure.lock-not-file").at("lekalo.lock")),
            Err(_) => return Err(StructureReason::new("structure.lock-not-file").at("lekalo.lock")),
        }

        // Runtime area (.lekalo): placement validation only; never read.
        match self.entry_type("", ".lekalo") {
            Err(FsErrorKind::NotFound) => {}
            Ok(EntryType::Symlink) => {
                return Err(StructureReason::new("structure.path-link").at(".lekalo"))
            }
            Ok(EntryType::Directory) => {
                let mut runtime_counter = ScanCounter::new();
                if let Some(failure) = scan_governed_tree(
                    self,
                    ".lekalo",
                    0,
                    &mut runtime_counter,
                    &mut |entry_type, name, logical| {
                        if entry_type == EntryType::Directory && name == "lekalo" {
                            return Some(StructureReason::new("structure.nested-root").at(logical));
                        }
                        runtime_placement_failure(entry_type, logical)
                    },
                ) {
                    return Err(failure);
                }
            }
            Ok(_) => {
                return Err(StructureReason::new("structure.runtime-unexpected-entry").at(".lekalo"))
            }
            Err(_) => {
                return Err(StructureReason::new("structure.runtime-unexpected-entry").at(".lekalo"))
            }
        }

        Ok(StructureReport { modules, targets })
    }
}

/// The closed set of structure codes whose refusal is a policy denial
/// (the denied exit class): links, reparse aliases, special files,
/// hostile names, and placement violations. One source of truth for the
/// checker and for downstream services that must reclassify a loader
/// passthrough without weakening the rule identity.
pub(crate) fn structure_policy_code(code: &str) -> Option<&'static str> {
    Some(match code {
        "structure.path-link" => "structure.path-link",
        "structure.path-special" => "structure.path-special",
        "structure.selection-alias" => "structure.selection-alias",
        "structure.nested-root" => "structure.nested-root",
        "structure.canonical-unexpected-entry" => "structure.canonical-unexpected-entry",
        "structure.module-subdirectory" => "structure.module-subdirectory",
        "structure.runtime-unexpected-entry" => "structure.runtime-unexpected-entry",
        _ => return None,
    })
}

/// Map a scan failure onto the accepted outcome classes: links, reparse
/// aliases, and special files are policy denials; everything else is
/// malformed structure.
pub fn classify_failure(failure: ScanFailure) -> StructureOutcome {
    if structure_policy_code(failure.code).is_some() {
        StructureOutcome::Denied(vec![failure])
    } else {
        StructureOutcome::Invalid(vec![failure])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_placement_mirrors_the_checker_containers() {
        assert!(runtime_placement_failure(EntryType::Directory, ".lekalo/import/notes").is_none());
        assert!(
            runtime_placement_failure(EntryType::Directory, ".lekalo/consumer/model").is_none()
        );
        assert!(runtime_placement_failure(
            EntryType::Directory,
            ".lekalo/privacy/decisions/export"
        )
        .is_none());
        assert_eq!(
            runtime_placement_failure(EntryType::File, ".lekalo/cache.sqlite")
                .unwrap()
                .code,
            "structure.runtime-unexpected-entry"
        );
        assert_eq!(
            runtime_placement_failure(EntryType::File, ".lekalo/consumer")
                .unwrap()
                .code,
            "structure.runtime-unexpected-entry"
        );
        assert_eq!(
            runtime_placement_failure(EntryType::Directory, ".lekalo/consumer/other")
                .unwrap()
                .code,
            "structure.runtime-unexpected-entry"
        );
        assert_eq!(
            runtime_placement_failure(EntryType::File, ".lekalo/privacy/decisions/other")
                .unwrap()
                .code,
            "structure.runtime-unexpected-entry"
        );
    }
}
#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn path_grammar_ports_the_closed_rules_in_fixed_order() {
        let cases: [(&str, Option<&str>); 20] = [
            ("", Some("structure.path-empty")),
            ("le\u{0301}kalo", Some("structure.path-not-nfc")),
            ("/etc", Some("structure.path-absolute")),
            ("\\\\server", Some("structure.path-absolute")),
            ("C:/x", Some("structure.path-absolute")),
            ("https://x", Some("structure.path-absolute")),
            ("a:b", Some("structure.path-absolute")),
            ("~home", Some("structure.path-absolute")),
            ("a\\b", Some("structure.path-escape")),
            ("a%20b", Some("structure.path-escape")),
            ("a//b", Some("structure.path-segment")),
            ("a/..", Some("structure.path-traversal")),
            ("a/segment.", Some("structure.path-traversal")),
            ("a/con.txt", Some("structure.path-device")),
            ("a/ＣＯＮ", Some("structure.path-device")),
            ("a/~1x", Some("structure.path-short-name")),
            ("a/Audit", Some("structure.path-case")),
            ("a/-bad", Some("structure.path-segment")),
            ("a/ok_yaml.2", None),
            ("lekalo/modules/planner_v2/module.yaml", None),
        ];
        for (input, expected) in cases {
            assert_eq!(path_violation(input), expected, "input {input:?}");
        }
    }
}

#[cfg(test)]
mod grammar_tests {
    use super::*;

    #[test]
    fn selection_grammar_rejects_absolute_upward_and_encoded_selectors() {
        let cases = [
            ("", Some("structure.selection-empty")),
            ("/tmp", Some("structure.selection-absolute")),
            ("a\\b", Some("structure.selection-escape")),
            ("proj%20x", Some("structure.selection-escape")),
            ("C:\\x", Some("structure.selection-absolute")),
            ("~", Some("structure.selection-absolute")),
            ("..", Some("structure.selection-traversal")),
            ("a/./b", Some("structure.selection-traversal")),
            ("a/ ", Some("structure.selection-traversal")),
            ("CON", Some("structure.selection-device")),
            ("~1", Some("structure.selection-absolute")),
            (".", None),
            ("./fixtures", None),
            ("samples/demo", None),
        ];
        for (input, expected) in cases {
            assert_eq!(selection_violation(input), expected, "input {input:?}");
        }
    }

    #[test]
    fn dos_device_detection_covers_compatibility_aliases_and_dollar_names() {
        assert!(is_dos_device("CON"));
        assert!(is_dos_device("con.txt"));
        assert!(is_dos_device("ＣＯＮ"));
        assert!(is_dos_device("CLOCK$"));
        assert!(is_dos_device("CONIN$"));
        assert!(!is_dos_device("constants"));
        assert!(!is_dos_device("console"));
    }

    #[test]
    fn drive_and_scheme_detection_matches_the_checker() {
        assert!(starts_with_drive_or_scheme("C:\\x"));
        assert!(starts_with_drive_or_scheme("z:"));
        assert!(starts_with_drive_or_scheme("https://x"));
        assert!(starts_with_drive_or_scheme("a+b:"));
        assert!(!starts_with_drive_or_scheme("planner"));
        assert!(!starts_with_drive_or_scheme("a/b"));
    }
}
