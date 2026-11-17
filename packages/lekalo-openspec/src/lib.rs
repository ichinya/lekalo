//! Bounded OpenSpec interpretation, independent of the Lekalo host and filesystem.
//! Hosts supply a confined, no-follow read capability. No process or network I/O.
pub mod ids;
mod native_lexical;
mod provider;
pub use provider::*;

pub mod limits {
    pub const MAX_CAPABILITIES: usize = 256;
    pub const MAX_REQUIREMENTS: usize = 10_000;
    pub const MAX_CHANGES: usize = 256;
    pub const MAX_TITLE_CHARS: usize = 128;
    pub const MAX_SPEC_BYTES: usize = 1024 * 1024;
}

pub struct ProviderDecl {
    pub source: String,
    pub root: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryType {
    Directory,
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    NotFound,
    Limit,
    Io,
}

/// All paths are logical relative paths. Implementations must reject traversal,
/// symlinks and escapes, bound directory reads, and enforce max_bytes before
/// allocating file contents. The provider never gets an ambient OS path.
pub trait Source {
    fn entries(&self, path: &str) -> Result<Vec<(String, EntryType)>, ReadError>;
    fn read_file_opt(
        &self,
        dir: &str,
        name: &str,
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, ReadError>;
}
fn sha256_digest(bytes: &[u8]) -> String {
    use sha2::Digest;
    format!(
        "sha256:{}",
        sha2::Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}
