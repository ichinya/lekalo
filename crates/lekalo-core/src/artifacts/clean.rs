//! Safe clean planning and confirmed application (issue #21).
//!
//! Planning is pure: it scans only the declared managed root
//! (`.lekalo/generated`), subtracts manifest-claimed paths, and pins every
//! remaining file by logical path, exact content digest, and size into a
//! payload. Application revalidates the whole plan first, then deletes
//! one file at a time with an immediate re-check before each removal; any
//! refusal aborts the remaining plan with zero further deletions. Nothing
//! here ever touches manifest-recorded artifacts: only orphans inside the
//! managed root are ever removed.

use serde::Serialize;

use super::check::{observe, split, Prepared};
use super::types::{GeneratedPath, MAX_ARTIFACT_BYTES};
use super::ArtifactFailure;
use crate::loader::LoadSelection;
use crate::lockfile::Sha256Digest;
use crate::project_fs::{EntryType, Fs, FsErrorKind};

/// One file pinned by the clean plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CleanFile {
    /// The project-relative logical path inside the managed root.
    pub path: String,
    /// The exact content digest pinned at plan time.
    pub digest: String,
    /// The exact byte size pinned at plan time.
    pub size: u64,
}

/// The receipt of `generate --clean --dry-run` (and the internal plan).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CleanPlanReceipt {
    /// Always `valid`.
    pub status: &'static str,
    /// Always `generate`.
    pub operation: &'static str,
    /// Always `preview` for this receipt.
    pub mode: &'static str,
    /// The manifest digest bound into the plan; `None` when absent.
    #[serde(rename = "manifestDigest")]
    pub manifest_digest: Option<String>,
    /// The exact lock digest bound into the plan.
    #[serde(rename = "lockDigest")]
    pub lock_digest: String,
    /// The plan identity: `sha256:<64 lowercase hex>`.
    #[serde(rename = "planId")]
    pub plan_id: String,
    /// The number of planned deletions.
    pub count: usize,
    /// The sorted planned deletions.
    pub files: Vec<CleanFile>,
}

/// The receipt of a confirmed clean application.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CleanReceipt {
    /// Always `valid`.
    pub status: &'static str,
    /// Always `generate`.
    pub operation: &'static str,
    /// Always `apply` for this receipt.
    pub mode: &'static str,
    /// The applied plan identity.
    #[serde(rename = "planId")]
    pub plan_id: String,
    /// How many planned files were deleted.
    pub deleted: usize,
    /// The sorted applied deletions.
    pub files: Vec<CleanFile>,
    /// Whether anything was deleted.
    pub changed: bool,
}

pub struct CleanService;

impl CleanService {
    /// `--clean --dry-run`: compute the plan without touching a byte.
    pub fn plan(selection: &LoadSelection) -> Result<CleanPlanReceipt, ArtifactFailure> {
        let prepared = Prepared::prepare(selection)?;
        let plan = plan_content(&prepared)?;
        Ok(CleanPlanReceipt {
            status: "valid",
            operation: "generate",
            mode: "preview",
            manifest_digest: plan.manifest_digest.as_ref().map(|d| d.as_str().to_owned()),
            lock_digest: plan.lock_digest.clone(),
            plan_id: plan.plan_id.as_str().to_owned(),
            count: plan.files.len(),
            files: plan.files,
        })
    }

    /// `--clean --confirm sha256:<planId>`: apply exactly that plan or
    /// delete nothing.
    pub fn apply(
        selection: &LoadSelection,
        plan_id: &str,
    ) -> Result<CleanReceipt, ArtifactFailure> {
        let prepared = Prepared::prepare(selection)?;
        let plan = plan_content(&prepared)?;
        if plan.plan_id.as_str() != plan_id {
            // The previewed state is gone: never delete from a stale plan.
            return Err(ArtifactFailure::PlanChanged);
        }
        // Revalidate everything before the first deletion.
        for file in &plan.files {
            revalidate(&prepared, file)?;
        }
        let mut deleted_files = Vec::with_capacity(plan.files.len());
        for file in &plan.files {
            // Immediate re-check before each removal bounds the swap
            // window; a refusal aborts the remaining plan.
            revalidate(&prepared, file)?;
            let (dir, name) = split(&file.path);
            let physical = if dir.is_empty() {
                prepared.root().join(name)
            } else {
                prepared
                    .root()
                    .join(dir.replace('/', std::path::MAIN_SEPARATOR_STR))
                    .join(name)
            };
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;

                let metadata = std::fs::symlink_metadata(&physical)
                    .map_err(|_| ArtifactFailure::Io("clean-revalidate"))?;
                if metadata.nlink() != 1 {
                    return Err(ArtifactFailure::Structure {
                        code: "structure.path-special",
                        denied: true,
                    });
                }
            }
            std::fs::remove_file(&physical).map_err(|_| ArtifactFailure::Io("clean-delete"))?;
            deleted_files.push(file.clone());
        }
        Ok(CleanReceipt {
            status: "valid",
            operation: "generate",
            mode: "apply",
            plan_id: plan.plan_id.as_str().to_owned(),
            deleted: deleted_files.len(),
            files: deleted_files,
            changed: !plan.files.is_empty(),
        })
    }
}

/// The deterministic plan content shared by preview and apply.
struct PlanContent {
    manifest_digest: Option<Sha256Digest>,
    lock_digest: String,
    plan_id: Sha256Digest,
    files: Vec<CleanFile>,
}

fn plan_content(prepared: &Prepared) -> Result<PlanContent, ArtifactFailure> {
    let manifest_digest = prepared.manifest().map(|m| m.manifest_digest().clone());
    let claimed: Vec<&str> = prepared
        .manifest()
        .map(|manifest| {
            manifest
                .artifacts()
                .iter()
                .map(|entry| entry.key().path().as_str())
                .collect()
        })
        .unwrap_or_default();
    let orphan_paths = scan_orphans(prepared.fs(), &claimed)?;
    let mut files = Vec::with_capacity(orphan_paths.len());
    for path in orphan_paths {
        let observed =
            observe(prepared.fs(), path.as_str())?.ok_or(ArtifactFailure::Io("orphan-vanished"))?;
        files.push(CleanFile {
            path: path.as_str().to_owned(),
            digest: observed.digest.as_str().to_owned(),
            size: observed.size,
        });
    }
    files.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    let lock_digest = prepared.lock().digest().as_str().to_owned();
    let plan_id = plan_digest(manifest_digest.as_ref(), &lock_digest, &files);
    Ok(PlanContent {
        manifest_digest,
        lock_digest,
        plan_id,
        files,
    })
}

/// The plan identity: SHA-256 over the canonical plan payload (compact
/// JSON, byte-sorted keys, manifest digest or null, lock digest, and the
/// sorted files with their pinned digests and sizes). Never over absolute
/// paths or timestamps.
fn plan_digest(
    manifest_digest: Option<&Sha256Digest>,
    lock_digest: &str,
    files: &[CleanFile],
) -> Sha256Digest {
    use crate::loader::canonical::Canonical;
    let manifest_value = match manifest_digest {
        Some(digest) => Canonical::Str(digest.as_str().to_owned()),
        None => Canonical::Null,
    };
    let files_value = Canonical::Seq(
        files
            .iter()
            .map(|file| {
                Canonical::Map(vec![
                    ("digest".to_owned(), Canonical::Str(file.digest.clone())),
                    ("path".to_owned(), Canonical::Str(file.path.clone())),
                    ("size".to_owned(), Canonical::Int(file.size as i128)),
                ])
            })
            .collect(),
    );
    let mut pairs = vec![
        ("files".to_owned(), files_value),
        ("lock".to_owned(), Canonical::Str(lock_digest.to_owned())),
        ("manifest".to_owned(), manifest_value),
    ];
    pairs.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let payload = Canonical::Map(pairs).to_json();
    Sha256Digest::from_hex(&crate::versioning::plan::sha256_hex(payload.as_bytes()))
}

/// Scan the declared managed root for files no manifest entry claims.
///
/// The scan is explicit and bounded: it walks `.lekalo/generated` only,
/// never classifies arbitrary project files, refuses link or special
/// entries fail-closed, and orders output by path bytes. Unclaimed files
/// with non-portable names are refused rather than reported: the managed
/// root accepts only what the path policy accepts.
pub(crate) fn scan_orphans(
    fs: &Fs,
    claimed: &[&str],
) -> Result<Vec<GeneratedPath>, ArtifactFailure> {
    let mut orphans = Vec::new();
    match fs.entry_type(".lekalo", "generated") {
        Err(FsErrorKind::NotFound) => return Ok(orphans),
        Err(FsErrorKind::Limit { .. }) | Err(FsErrorKind::Io) => {
            return Err(ArtifactFailure::Io("scan"))
        }
        Ok(EntryType::Symlink) => {
            return Err(ArtifactFailure::Structure {
                code: "structure.path-link",
                denied: true,
            })
        }
        Ok(EntryType::File) | Ok(EntryType::Special) => {
            return Err(ArtifactFailure::Structure {
                code: "structure.path-special",
                denied: true,
            })
        }
        Ok(EntryType::Directory) => {}
    }
    walk(
        fs,
        super::types::GENERATED_ROOT,
        claimed,
        &mut orphans,
        0,
        &mut 0,
    )?;
    orphans.sort();
    Ok(orphans)
}

fn walk(
    fs: &Fs,
    dir: &str,
    claimed: &[&str],
    orphans: &mut Vec<GeneratedPath>,
    depth: usize,
    visited: &mut usize,
) -> Result<(), ArtifactFailure> {
    if depth > crate::project_fs::MAX_WALK_DEPTH {
        return Err(ArtifactFailure::Io("scan-limit"));
    }
    let entries = match fs.entries(dir) {
        Ok(entries) => entries,
        Err(FsErrorKind::NotFound) => return Ok(()),
        Err(_) => return Err(ArtifactFailure::Io("scan")),
    };
    for (name, kind) in entries {
        *visited += 1;
        if *visited > crate::project_fs::MAX_WALK_ENTRIES {
            return Err(ArtifactFailure::Io("scan-limit"));
        }
        let relative = format!("{dir}/{name}");
        match kind {
            EntryType::Directory => {
                // The manifest's own bookkeeping directory is never
                // generated output; its contents can never be orphans.
                if dir == super::types::GENERATED_ROOT && name == "manifests" {
                    continue;
                }
                walk(fs, &relative, claimed, orphans, depth + 1, visited)?;
            }
            EntryType::Symlink => {
                return Err(ArtifactFailure::Structure {
                    code: "structure.path-link",
                    denied: true,
                });
            }
            EntryType::Special => {
                return Err(ArtifactFailure::Structure {
                    code: "structure.path-special",
                    denied: true,
                });
            }
            EntryType::File => {
                if claimed.contains(&relative.as_str()) {
                    continue;
                }
                let path = GeneratedPath::parse(&relative).ok_or(ArtifactFailure::Structure {
                    code: "structure.path-special",
                    denied: true,
                })?;
                orphans.push(path);
            }
        }
    }
    Ok(())
}

/// One immediate pre-deletion revalidation: the entry must still be the
/// exact same regular single-linked file with the exact pinned bytes.
fn revalidate(prepared: &Prepared, file: &CleanFile) -> Result<(), ArtifactFailure> {
    let path = GeneratedPath::parse(&file.path).ok_or(ArtifactFailure::PlanChanged)?;
    let (dir, name) = split(path.as_str());
    match prepared.fs().entry_type(dir, name) {
        Ok(EntryType::File) => {}
        Ok(EntryType::Symlink) => {
            return Err(ArtifactFailure::Structure {
                code: "structure.path-link",
                denied: true,
            })
        }
        _ => return Err(ArtifactFailure::PlanChanged),
    }
    let bytes = prepared
        .fs()
        .read_file_opt(dir, name, MAX_ARTIFACT_BYTES)
        .map_err(|_| ArtifactFailure::PlanChanged)?
        .ok_or(ArtifactFailure::PlanChanged)?;
    let digest = Sha256Digest::from_hex(&crate::versioning::plan::sha256_hex(&bytes));
    if digest.as_str() != file.digest || bytes.len() as u64 != file.size {
        return Err(ArtifactFailure::PlanChanged);
    }
    Ok(())
}
