//! Governed immutable input capture. All reads go through the accepted
//! no-follow project filesystem; two complete content inventories must
//! agree before publication. Directory identity, deletion and empty
//! directories are part of the snapshot, not guesses from notifications.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::request::limits;
use crate::project_fs::{EntryType, Fs, FsErrorKind};

pub const GENERATED_ROOT: &str = ".lekalo/generated";
pub const MAX_GENERATED_FILES: usize = 4096;
pub const MAX_GENERATED_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileRecord {
    pub sha256: String,
    pub len: u64,
    pub bytes: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureError {
    Unstable { attempts: usize },
    Refused { code: &'static str, path: String },
    Limit { limit: &'static str, count: u64 },
    Io { path: String },
}
impl CaptureError {
    pub fn domain_result(&self) -> crate::DomainResult {
        use crate::result::Status;
        match self {
            Self::Refused { code, path } => {
                let diagnostic = crate::loader::error::Diagnostic::new(*code);
                let diagnostic = if path.is_empty() {
                    diagnostic
                } else {
                    diagnostic.with_path(path.clone())
                };
                crate::loader::diagnostic::failure(
                    if code.starts_with("structure.") {
                        Status::Denied
                    } else {
                        Status::Invalid
                    },
                    vec![diagnostic],
                )
            }
            Self::Limit { limit, count } => crate::DomainResult::denied(
                crate::diagnostics::DiagnosticSet::try_from_unsorted(
                    vec![super::diagnostics::resource_limit(limit, *count)],
                    Status::Denied,
                )
                .expect("watch limit registered"),
            ),
            Self::Unstable { attempts } => crate::DomainResult::unavailable(
                crate::diagnostics::DiagnosticSet::try_from_unsorted(
                    vec![super::diagnostics::snapshot_unstable(*attempts as u64)],
                    Status::Unavailable,
                )
                .expect("watch unstable registered"),
            ),
            Self::Io { path } => crate::loader::diagnostic::failure(
                Status::Invalid,
                vec![crate::loader::error::Diagnostic::new("loader.io")
                    .with_path(path.clone())
                    .with_data(serde_json::json!({"detail":"read-failed"}))],
            ),
        }
    }
}

/// Classified roots are explicit: Model/metadata plus only selected
/// target read roots. Root-level build names do not exempt a governed
/// document (e.g. `lekalo/modules/target/module.yaml`).
#[derive(Clone, Debug, Default)]
pub struct CapturePolicy {
    pub generate_check: bool,
    pub target_roots: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InputSnapshot {
    pub files: BTreeMap<String, FileRecord>,
    pub generated: BTreeMap<String, FileRecord>,
    pub directories: BTreeSet<String>,
}
impl InputSnapshot {
    pub fn source_revision(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut digest = Sha256::new();
        for path in &self.directories {
            if !path.starts_with(GENERATED_ROOT) {
                digest.update(b"dir\0");
                digest.update(path.as_bytes());
                digest.update(b"\n");
            }
        }
        for (path, record) in &self.files {
            digest.update(b"file\0");
            digest.update(path.as_bytes());
            digest.update(b"\0");
            digest.update(record.sha256.as_bytes());
            digest.update(b"\n");
        }
        format!("sha256:{:x}", digest.finalize())
    }
    pub fn generated_digest(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut digest = Sha256::new();
        for (path, record) in &self.generated {
            digest.update(path.as_bytes());
            digest.update(b"\0");
            digest.update(record.sha256.as_bytes());
            digest.update(b"\n");
        }
        format!("sha256:{:x}", digest.finalize())
    }
    pub fn total_bytes(&self) -> u64 {
        self.files
            .values()
            .chain(self.generated.values())
            .map(|r| r.len)
            .sum()
    }
    pub fn without_bytes(&self) -> Self {
        let records = |map: &BTreeMap<String, FileRecord>| {
            map.iter()
                .map(|(path, record)| {
                    (
                        path.clone(),
                        FileRecord {
                            sha256: record.sha256.clone(),
                            len: record.len,
                            bytes: None,
                        },
                    )
                })
                .collect()
        };
        Self {
            files: records(&self.files),
            generated: records(&self.generated),
            directories: self.directories.clone(),
        }
    }
    pub fn same_inventory(&self, other: &Self) -> bool {
        let same_records = |left: &BTreeMap<String, FileRecord>,
                            right: &BTreeMap<String, FileRecord>| {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|((path, record), (other_path, other_record))| {
                        path == other_path
                            && record.sha256 == other_record.sha256
                            && record.len == other_record.len
                    })
        };
        self.directories == other.directories
            && same_records(&self.files, &other.files)
            && same_records(&self.generated, &other.generated)
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("sha256:{}", crate::digest::sha256_hex(bytes))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventClass {
    Semantic,
    Generated,
    Ignored,
}
pub fn classify_event_path(path: &str) -> EventClass {
    if path == GENERATED_ROOT || path.starts_with(".lekalo/generated/") {
        EventClass::Generated
    } else if path == "lekalo" || path.starts_with("lekalo/") || path == "lekalo.lock" {
        EventClass::Semantic
    } else {
        EventClass::Ignored
    }
}

fn failure(path: &str, error: FsErrorKind) -> CaptureError {
    match error {
        FsErrorKind::Limit { max } => CaptureError::Limit {
            limit: "capture-document-bytes",
            count: (max as u64) + 1,
        },
        _ => CaptureError::Io {
            path: path.to_owned(),
        },
    }
}

fn logical_violation(path: &str) -> Option<&'static str> {
    // Runtime homes have their own accepted leading-dot root. The
    // portable logical-path grammar still governs every child segment.
    let portable = path.strip_prefix(".lekalo/").unwrap_or(path);
    crate::project_fs::path_violation(portable)
}

fn inventory(
    root: &Path,
    policy: &CapturePolicy,
    with_bytes: bool,
) -> Result<InputSnapshot, CaptureError> {
    let fs = Fs::open(root).map_err(|error| failure("", error))?;
    if let Some(code) = crate::versioning::migration_recovery_code(&fs)
        .or_else(|| crate::format::transaction::recovery_code(&fs))
    {
        return Err(CaptureError::Refused {
            code,
            path: String::new(),
        });
    }
    let mut snapshot = InputSnapshot::default();
    let mut budget = 0usize;
    for path in ["lekalo", "lekalo.lock"]
        .into_iter()
        .chain(policy.target_roots.iter().map(String::as_str))
        .chain(policy.generate_check.then_some(GENERATED_ROOT))
    {
        if let Some(code) = logical_violation(path) {
            return Err(CaptureError::Refused {
                code,
                path: path.to_owned(),
            });
        }
        let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
        match fs.entry_type(dir, name) {
            Ok(kind) => walk(&fs, path, kind, 0, with_bytes, &mut budget, &mut snapshot)?,
            Err(FsErrorKind::NotFound) => {}
            Err(error) => return Err(failure(path, error)),
        }
    }
    Ok(snapshot)
}

fn walk(
    fs: &Fs,
    path: &str,
    kind: EntryType,
    depth: usize,
    with_bytes: bool,
    budget: &mut usize,
    snapshot: &mut InputSnapshot,
) -> Result<(), CaptureError> {
    *budget += 1;
    if *budget > crate::project_fs::MAX_WALK_ENTRIES {
        return Err(CaptureError::Limit {
            limit: "walk-entries",
            count: *budget as u64,
        });
    }
    if depth > crate::project_fs::MAX_WALK_DEPTH {
        return Err(CaptureError::Limit {
            limit: "walk-depth",
            count: depth as u64,
        });
    }
    match kind {
        EntryType::Symlink => Err(CaptureError::Refused {
            code: "structure.path-link",
            path: path.to_owned(),
        }),
        EntryType::Special => Err(CaptureError::Refused {
            code: "structure.path-special",
            path: path.to_owned(),
        }),
        EntryType::Directory => {
            if snapshot.directories.len() >= limits::MAX_WATCHED_DIRS {
                return Err(CaptureError::Limit {
                    limit: "watched-directories",
                    count: snapshot.directories.len() as u64 + 1,
                });
            }
            snapshot.directories.insert(path.to_owned());
            let entries = fs
                .entries_bounded(
                    path,
                    crate::project_fs::MAX_WALK_ENTRIES.saturating_sub(*budget),
                )
                .map_err(|error| failure(path, error))?;
            for (name, kind) in entries {
                let child = format!("{path}/{name}");
                if let Some(code) = logical_violation(&child) {
                    return Err(CaptureError::Refused { code, path: child });
                }
                walk(fs, &child, kind, depth + 1, with_bytes, budget, snapshot)?;
            }
            Ok(())
        }
        EntryType::File => {
            let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
            let bytes = fs
                .read_file_opt(dir, name, crate::loader::source::MAX_DOCUMENT_BYTES)
                .map_err(|error| failure(path, error))?
                .ok_or_else(|| CaptureError::Io {
                    path: path.to_owned(),
                })?;
            if snapshot.total_bytes() + bytes.len() as u64 > limits::MAX_CAPTURED_BYTES {
                return Err(CaptureError::Limit {
                    limit: "capture-total-bytes",
                    count: snapshot.total_bytes() + bytes.len() as u64,
                });
            }
            if snapshot.files.len() + snapshot.generated.len() >= limits::MAX_PENDING_PATHS {
                return Err(CaptureError::Limit {
                    limit: "captured-files",
                    count: (snapshot.files.len() + snapshot.generated.len() + 1) as u64,
                });
            }
            let generated = path.starts_with(".lekalo/generated/");
            if generated && snapshot.generated.len() >= MAX_GENERATED_FILES {
                return Err(CaptureError::Limit {
                    limit: "generated-files",
                    count: snapshot.generated.len() as u64 + 1,
                });
            }
            let record = FileRecord {
                sha256: hash(&bytes),
                len: bytes.len() as u64,
                bytes: with_bytes.then_some(bytes),
            };
            let dest = if generated {
                &mut snapshot.generated
            } else {
                &mut snapshot.files
            };
            dest.insert(path.to_owned(), record);
            Ok(())
        }
    }
}

pub fn capture_with_policy(
    root: &Path,
    policy: &CapturePolicy,
) -> Result<InputSnapshot, CaptureError> {
    for _ in 0..limits::MAX_CAPTURE_ATTEMPTS {
        let captured = inventory(root, policy, true)?;
        let after = inventory(root, policy, false)?;
        if captured.same_inventory(&after) {
            return Ok(captured);
        }
    }
    Err(CaptureError::Unstable {
        attempts: limits::MAX_CAPTURE_ATTEMPTS,
    })
}
pub fn capture(root: &Path, generate_check: bool) -> Result<InputSnapshot, CaptureError> {
    capture_with_policy(
        root,
        &CapturePolicy {
            generate_check,
            ..CapturePolicy::default()
        },
    )
}
pub fn poll_inventory(root: &Path) -> Result<InputSnapshot, CaptureError> {
    inventory(root, &CapturePolicy::default(), false)
}
pub fn poll_with_policy(
    root: &Path,
    policy: &CapturePolicy,
) -> Result<InputSnapshot, CaptureError> {
    inventory(root, policy, false)
}

/// Scratch output is never a source of authority: logical paths are
/// validated again before writes, and callers must supply a fresh
/// private temporary directory, never the governed project root.
pub fn materialize(snapshot: &InputSnapshot, dest: &Path) -> Result<(), CaptureError> {
    let metadata = std::fs::symlink_metadata(dest).map_err(|_| CaptureError::Io {
        path: String::new(),
    })?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || std::fs::read_dir(dest)
            .map_err(|_| CaptureError::Io {
                path: String::new(),
            })?
            .next()
            .is_some()
    {
        return Err(CaptureError::Refused {
            code: "structure.path-link",
            path: String::new(),
        });
    }
    for dir in &snapshot.directories {
        if let Some(code) = logical_violation(dir) {
            return Err(CaptureError::Refused {
                code,
                path: dir.clone(),
            });
        }
        std::fs::create_dir_all(dest.join(dir))
            .map_err(|_| CaptureError::Io { path: dir.clone() })?;
    }
    for (path, record) in snapshot.files.iter().chain(snapshot.generated.iter()) {
        if let Some(code) = logical_violation(path) {
            return Err(CaptureError::Refused {
                code,
                path: path.clone(),
            });
        }
        let Some(bytes) = &record.bytes else {
            return Err(CaptureError::Io { path: path.clone() });
        };
        if hash(bytes) != record.sha256 {
            return Err(CaptureError::Io { path: path.clone() });
        }
        let target = dest.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|_| CaptureError::Io { path: path.clone() })?;
        }
        std::fs::write(target, bytes).map_err(|_| CaptureError::Io { path: path.clone() })?;
    }
    Ok(())
}
