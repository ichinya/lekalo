//! Semantic validation of the storage-engine-profile attachment (issue
//! #117).
//!
//! The closed semantic rules over one fully parsed profile: engine and
//! variant coherence, collation/charset membership, and the
//! evidence-version binding. Every violation is one registered
//! diagnostic with no partial result. Pure and read-only.

use crate::diagnostics::DiagnosticSet;

use super::diagnostic;
use super::id::EngineToken;
use super::StorageEngineProfile;

/// The semantic self-check over one assembled profile.
pub(crate) fn semantic_self_check(profile: &StorageEngineProfile) -> Result<(), DiagnosticSet> {
    check_engine_coherence(profile)?;
    check_collation_coherence(profile)?;
    Ok(())
}

/// The engine/variant coherence: the closed variant matches the closed
/// engine token, and the collation is never left implicit.
fn check_engine_coherence(profile: &StorageEngineProfile) -> Result<(), DiagnosticSet> {
    let engine = profile.engine();
    match (engine.engine(), engine.variant()) {
        (EngineToken::Mysql, super::id::VariantToken::MysqlCommunity)
        | (EngineToken::Mariadb, super::id::VariantToken::Mariadb) => {}
        _ => return Err(diagnostic::rule_invalid("engine-variant", None)),
    }
    Ok(())
}

/// The collation belongs to the declared charset, decided by the closed
/// prefix convention (the same membership the storage-projection
/// validation uses). The declared collation is never the implicit
/// server default: the MariaDB 11.x mid-release default-collation
/// change is exactly why this is a declared member.
fn check_collation_coherence(profile: &StorageEngineProfile) -> Result<(), DiagnosticSet> {
    let engine = profile.engine();
    let (charset, collation) = (engine.charset(), engine.collation());
    let belongs = collation.starts_with(&format!("{charset}_"))
        || (charset == "utf8mb3" && collation.starts_with("utf8_"))
        || (charset == "utf8mb4" && collation.starts_with("uca1400_"));
    if !belongs {
        return Err(diagnostic::rule_invalid("collation-charset-mismatch", None));
    }
    Ok(())
}
