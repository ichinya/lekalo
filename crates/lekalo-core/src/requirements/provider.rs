//! Host capability adapter for the pinned external OpenSpec package.
use super::{ProviderDecl, ProviderKind};
use crate::project_fs::{EntryType, Fs, FsErrorKind};
pub(crate) use lekalo_openspec::{CatalogEntry, Loaded, Origin, ProviderStatus, Snapshot};
use lekalo_openspec::{ReadError, Source};

struct ConfinedSource<'a>(&'a Fs);
fn map_error(error: FsErrorKind) -> ReadError {
    match error {
        FsErrorKind::NotFound => ReadError::NotFound,
        FsErrorKind::Limit { .. } => ReadError::Limit,
        FsErrorKind::Io => ReadError::Io,
    }
}
impl Source for ConfinedSource<'_> {
    fn entries(&self, path: &str) -> Result<Vec<(String, lekalo_openspec::EntryType)>, ReadError> {
        self.0
            .entries(path)
            .map(|entries| {
                entries
                    .into_iter()
                    .map(|(name, kind)| {
                        (
                            name,
                            if kind == EntryType::Directory {
                                lekalo_openspec::EntryType::Directory
                            } else {
                                lekalo_openspec::EntryType::Other
                            },
                        )
                    })
                    .collect()
            })
            .map_err(map_error)
    }
    fn read_file_opt(
        &self,
        dir: &str,
        name: &str,
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, ReadError> {
        self.0
            .read_file_opt(dir, name, max_bytes)
            .map_err(map_error)
    }
}
fn declaration(decl: &ProviderDecl) -> lekalo_openspec::ProviderDecl {
    lekalo_openspec::ProviderDecl {
        source: decl.source.clone(),
        root: decl.root.clone(),
    }
}
pub(crate) fn absent(decl: &ProviderDecl) -> Snapshot {
    Snapshot::absent(&declaration(decl))
}
pub(crate) fn load_snapshot(fs: &Fs, decl: &ProviderDecl) -> Loaded {
    match decl.kind {
        ProviderKind::Openspec => {
            lekalo_openspec::load_snapshot(&ConfinedSource(fs), &declaration(decl))
        }
    }
}
