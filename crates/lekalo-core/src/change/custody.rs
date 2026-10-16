//! Local commit custody, separate from the editable canonical audit ledger.
//!
//! A public hash chain proves internal consistency, not that a transaction ran.
//! This non-cache pin is published/restored in the SAME transaction as sources
//! and history. A ledger-only edit (even a fully recomputed chain) cannot mint
//! replay authority. Missing custody is never reconstructed from claimed file
//! outcomes or the current Model. Copying/deleting the entire runtime custody
//! and ledger is a different trust boundary, like editing the sources directly.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{receipt, ChangeError};

pub(crate) const LOGICAL_PATH: &str = ".lekalo/change-history-pin.json";
const SCHEMA_VERSION: &str = "lekalo/change-history-pin/1";
const MAX_BYTES: usize = 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Pin {
    schema_version: String,
    project: String,
    record_count: u64,
    tip_digest: String,
    history_digest: String,
}

fn invalid() -> ChangeError {
    ChangeError::HistoryInvalid {
        detail: "commit-custody",
    }
}

pub(crate) fn read(root: &Path) -> Result<Option<Vec<u8>>, ChangeError> {
    let fs = crate::project_fs::Fs::open(root).map_err(|_| invalid())?;
    // Runtime homes deliberately lie outside the canonical-path grammar;
    // both components are service constants, never ledger input.
    match fs.read_file_opt(".lekalo", "change-history-pin.json", MAX_BYTES) {
        Ok(bytes) => Ok(bytes),
        Err(crate::project_fs::FsErrorKind::NotFound) => Ok(None),
        Err(_) => Err(invalid()),
    }
}

pub(crate) fn encode(history: &receipt::History, bytes: &[u8]) -> Vec<u8> {
    serde_json::to_vec_pretty(&Pin {
        schema_version: SCHEMA_VERSION.to_owned(),
        project: history.project.clone(),
        record_count: history.records.len() as u64,
        tip_digest: history.tip_digest(),
        history_digest: receipt::bytes_digest(bytes),
    })
    .expect("custody pin serializes")
}

/// Verify both presence and all accepted-history facts, including the exact
/// ledger bytes. An absent ledger is first-use ONLY if custody is absent too.
pub(crate) fn verify(root: &Path, history_bytes: Option<&[u8]>) -> Result<(), ChangeError> {
    let pin_bytes = read(root)?;
    let (bytes, pin_bytes) = match (history_bytes, pin_bytes) {
        (None, None) => return Ok(()),
        (Some(bytes), Some(pin)) => (bytes, pin),
        _ => return Err(invalid()),
    };
    let value = super::request::unique_json(&pin_bytes).map_err(|_| invalid())?;
    let pin: Pin = serde_json::from_value(value).map_err(|_| invalid())?;
    let history = receipt::History::decode(bytes)?;
    if pin.schema_version != SCHEMA_VERSION
        || pin.project != history.project
        || pin.record_count != history.records.len() as u64
        || pin.tip_digest != history.tip_digest()
        || pin.history_digest != receipt::bytes_digest(bytes)
    {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custody_is_closed_bounded_and_never_reconstructed() {
        let root = tempfile::tempdir().unwrap();
        let bytes = include_bytes!("../../../../tests/fixtures/change/golden/change-history.json");
        let history = receipt::History::decode(bytes).unwrap();
        assert!(
            verify(root.path(), Some(bytes)).is_err(),
            "ledger cannot create custody"
        );
        std::fs::create_dir(root.path().join(".lekalo")).unwrap();
        let path = root.path().join(LOGICAL_PATH);
        let pin = encode(&history, bytes);
        std::fs::write(&path, &pin).unwrap();
        verify(root.path(), Some(bytes)).unwrap();
        assert!(
            verify(root.path(), None).is_err(),
            "missing ledger is destructive"
        );
        for bad in [
            b"".to_vec(),
            b"{}".to_vec(),
            vec![b' '; MAX_BYTES + 1],
            b"{\"schemaVersion\":\"x\",\"schemaVersion\":\"x\"}".to_vec(),
        ] {
            std::fs::write(&path, bad).unwrap();
            assert!(verify(root.path(), Some(bytes)).is_err());
        }
        let valid: serde_json::Value = serde_json::from_slice(&pin).unwrap();
        for member in [
            "schemaVersion",
            "project",
            "recordCount",
            "tipDigest",
            "historyDigest",
            "extra",
        ] {
            let mut bad = valid.clone();
            bad[member] = serde_json::json!("edited");
            std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
            assert!(verify(root.path(), Some(bytes)).is_err(), "{member}");
        }
        std::fs::write(&path, pin).unwrap();
        let mut edited_bytes = bytes.to_vec();
        edited_bytes.push(b'\n');
        assert!(
            verify(root.path(), Some(&edited_bytes)).is_err(),
            "exact committed bytes, not only a mutable chain"
        );
    }

    #[test]
    fn linked_custody_is_not_followed_even_to_identical_bytes() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let bytes = include_bytes!("../../../../tests/fixtures/change/golden/change-history.json");
        let history = receipt::History::decode(bytes).unwrap();
        let foreign = outside.path().join("pin.json");
        std::fs::write(&foreign, encode(&history, bytes)).unwrap();
        std::fs::create_dir(root.path().join(".lekalo")).unwrap();
        let path = root.path().join(LOGICAL_PATH);
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&foreign, &path).is_ok();
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_file(&foreign, &path).is_ok();
        if linked {
            assert!(verify(root.path(), Some(bytes)).is_err());
        } else {
            eprintln!("skipped: symlink privilege unavailable");
        }
    }
}
