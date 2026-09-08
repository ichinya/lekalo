//! The derived requirements report (issue #36).
//!
//! One read-only projection — `lekalo/requirements-report/v1.0.0`,
//! identity `dev.lekalo.requirements-report@1.0.0` — computed as a pure
//! function of the attachment, the compiled Model, and the provider
//! snapshots. It carries the resolved requirement catalog with its exact
//! revisions, every reference resolution status, the coverage gaps
//! (requirements no symbol links), the explicit conflicts, and the
//! changed-requirement impact. Canonical bytes are compact UTF-8 JSON
//! with byte-sorted object keys and canonically sorted collections.

use serde::Serialize;

use super::diagnostic;
use super::provider::{Origin, ProviderStatus, Snapshot};
use super::version;
use super::{canonical_value_bytes, requirement_id, sha256_hex, RequirementsAttachment};
use crate::diagnostics::DiagnosticSet;
use crate::ir::Compilation;

/// The derived report plus the gate verdict.
pub struct Resolution {
    /// The full derived report.
    pub report: Report,
    /// The gate verdict: `Pass` when every reference is fresh and no
    /// conflict exists, otherwise the aggregated `denied` set.
    pub verdict: ResolutionVerdict,
}

/// The gate verdict of one completed resolution.
pub enum ResolutionVerdict {
    /// Every reference is fresh; no conflict exists.
    Pass,
    /// At least one reference is stale or missing, or a conflict blocks
    /// resolution; the gate refuses.
    Denied(DiagnosticSet),
}

/// The derived report wire.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// The exact wire discriminator.
    pub schema_version: &'static str,
    /// The exact contract identity.
    pub identity: &'static str,
    /// The bound project id.
    pub project_id: String,
    /// The bound Model pin.
    pub model_ref: ModelRefWire,
    /// The SHA-256 over the attachment canonical bytes.
    pub attachment_digest: String,
    /// The provider catalog revision: bare 64 hex SHA-256 over the
    /// sorted (source, id, digest) rows of every resolved provider.
    pub source_revision: String,
    /// Every declared provider with its availability and counts.
    pub providers: Vec<ProviderRow>,
    /// The effective requirement catalog, sorted by (source, id).
    pub requirements: Vec<RequirementRow>,
    /// Every reference with its resolution status, sorted canonically.
    pub references: Vec<ReferenceRow>,
    /// Requirements no symbol links, sorted by (source, id).
    pub coverage_gaps: Vec<CoverageGapRow>,
    /// Every explicit conflict, sorted canonically.
    pub conflicts: Vec<ConflictRow>,
    /// The changed-requirement impact rows, sorted canonically.
    pub impact: Vec<ImpactRow>,
}

/// The Model pin wire of the report.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRefWire {
    /// The exact Model contract version.
    pub model_version: String,
    /// The SHA-256 over the canonical Model payload bytes.
    pub digest: String,
}

/// One provider availability row.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRow {
    /// The provider namespace.
    pub source: String,
    /// The provider kind.
    pub kind: String,
    /// The declared logical root.
    pub root: String,
    /// `ok` or `absent`.
    pub status: &'static str,
    /// The number of resolved capabilities (0 when absent).
    pub capability_count: usize,
    /// The number of resolved requirements (0 when absent).
    pub requirement_count: usize,
}

/// One catalog requirement row.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementRow {
    /// The provider namespace.
    pub source: String,
    /// The requirement id.
    pub id: String,
    /// The exact `sha256:` body digest.
    pub digest: String,
    /// `accepted` or `change`.
    pub origin: &'static str,
    /// The owning active change id, when the content came from one.
    pub origin_id: Option<String>,
    /// Every symbol referencing this requirement, sorted.
    pub symbols: Vec<String>,
}

/// One reference resolution row.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceRow {
    /// The linked symbol.
    pub symbol: String,
    /// The declared relation.
    pub relation: &'static str,
    /// The provider namespace.
    pub source: String,
    /// The requirement id.
    pub requirement: String,
    /// The pinned revision.
    pub revision: String,
    /// `fresh`, `stale`, `missing`, or `conflict`.
    pub status: &'static str,
    /// The current body digest when the requirement resolved.
    pub current_revision: Option<String>,
    /// The rename candidate when the pinned body moved to a new id.
    pub renamed_to: Option<String>,
}

/// One coverage gap: a requirement no symbol links.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageGapRow {
    /// The provider namespace.
    pub source: String,
    /// The requirement id.
    pub id: String,
    /// The exact `sha256:` body digest.
    pub digest: String,
}

/// One explicit conflict row.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictRow {
    /// The provider namespace.
    pub source: String,
    /// The disputed capability.
    pub capability: String,
    /// The disputed requirement title.
    pub title: String,
    /// The fixed conflict classification token.
    pub detail: String,
}

/// One changed-requirement impact row.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactRow {
    /// The provider namespace.
    pub source: String,
    /// The requirement id the impact is about.
    pub requirement: String,
    /// `changed`, `removed`, `renamed`, or `conflict`.
    pub change: &'static str,
    /// The affected symbols, sorted.
    pub symbols: Vec<String>,
    /// The new requirement id for a rename.
    pub renamed_to: Option<String>,
}

/// Build the resolution: the derived report plus the gate verdict.
/// Pure and read-only.
pub(crate) fn build(
    attachment: &RequirementsAttachment,
    _compilation: &Compilation,
    snapshots: Vec<Snapshot>,
) -> Resolution {
    let attachment_digest = sha256_hex(canonical_value_bytes(&attachment.wire()).as_bytes());

    // Owned lookup indexes over the resolved catalogs: (source, id) to
    // entry, (source, digest) to the smallest matching id (the rename
    // candidate), and the set of disputed (source, id) pairs.
    let mut catalog: std::collections::BTreeMap<(&str, &str), &super::provider::CatalogEntry> =
        std::collections::BTreeMap::new();
    let mut digests: std::collections::BTreeMap<(&str, &str), &str> =
        std::collections::BTreeMap::new();
    let mut conflict_ids: std::collections::BTreeSet<(String, String)> =
        std::collections::BTreeSet::new();
    for snapshot in &snapshots {
        for entry in &snapshot.entries {
            catalog.insert((snapshot.source.as_str(), entry.id.as_str()), entry);
            digests
                .entry((snapshot.source.as_str(), entry.digest.as_str()))
                .or_insert(entry.id.as_str());
        }
        for row in &snapshot.conflicts {
            let slug = super::title_slug(&row.title).unwrap_or_default();
            conflict_ids.insert((
                snapshot.source.clone(),
                requirement_id(&row.capability, &slug),
            ));
        }
    }

    // Reference resolution, in the attachment's canonical order.
    let mut references: Vec<ReferenceRow> = Vec::new();
    for link in attachment.references() {
        let source = link.source.as_str();
        let requirement = link.requirement.as_str();
        let status;
        let mut current_revision = None;
        let mut renamed_to = None;
        if conflict_ids.contains(&(source.to_owned(), requirement.to_owned())) {
            status = "conflict";
        } else if let Some(entry) = catalog.get(&(source, requirement)) {
            status = if entry.digest == link.revision.as_str() {
                "fresh"
            } else {
                "stale"
            };
            current_revision = Some(entry.digest.clone());
        } else {
            status = "missing";
            renamed_to = digests
                .get(&(source, link.revision.as_str()))
                .map(|id| (*id).to_owned());
        }
        references.push(ReferenceRow {
            symbol: link.symbol.clone(),
            relation: link.relation.as_str(),
            source: link.source.clone(),
            requirement: link.requirement.clone(),
            revision: link.revision.as_str().to_owned(),
            status,
            current_revision,
            renamed_to,
        });
    }

    // Catalog rows with their referencing symbols.
    let mut requirements: Vec<RequirementRow> = Vec::new();
    for snapshot in &snapshots {
        for entry in &snapshot.entries {
            let mut symbols: Vec<String> = attachment
                .references()
                .iter()
                .filter(|link| link.source == snapshot.source && link.requirement == entry.id)
                .map(|link| link.symbol.clone())
                .collect();
            symbols.sort();
            symbols.dedup();
            requirements.push(RequirementRow {
                source: snapshot.source.clone(),
                id: entry.id.clone(),
                digest: entry.digest.clone(),
                origin: match entry.origin {
                    Origin::Accepted => "accepted",
                    Origin::Change => "change",
                },
                origin_id: entry.change.clone(),
                symbols,
            });
        }
    }
    requirements.sort_by(|left, right| {
        (&left.source[..], &left.id[..]).cmp(&(&right.source[..], &right.id[..]))
    });

    // Coverage gaps: catalog requirements no symbol links.
    let coverage_gaps: Vec<CoverageGapRow> = requirements
        .iter()
        .filter(|row| row.symbols.is_empty())
        .map(|row| CoverageGapRow {
            source: row.source.clone(),
            id: row.id.clone(),
            digest: row.digest.clone(),
        })
        .collect();

    // Impact: one row per disputed or changed requirement, aggregating
    // the affected symbols. Conflict dominates, then rename, removal,
    // and staleness; fresh requirements carry no impact row.
    let mut grouped: std::collections::BTreeMap<(&str, &str), Vec<&ReferenceRow>> =
        std::collections::BTreeMap::new();
    for row in &references {
        grouped
            .entry((row.source.as_str(), row.requirement.as_str()))
            .or_default()
            .push(row);
    }
    let mut impact: Vec<ImpactRow> = Vec::new();
    for ((source, requirement), rows) in grouped {
        let change = if rows.iter().any(|row| row.status == "conflict") {
            "conflict"
        } else if rows.iter().any(|row| row.status == "missing") {
            if rows.iter().any(|row| row.renamed_to.is_some()) {
                "renamed"
            } else {
                "removed"
            }
        } else if rows.iter().any(|row| row.status == "stale") {
            "changed"
        } else {
            continue;
        };
        let mut symbols: Vec<String> = rows.iter().map(|row| row.symbol.clone()).collect();
        symbols.sort();
        symbols.dedup();
        let renamed_to = rows.iter().find_map(|row| row.renamed_to.clone());
        impact.push(ImpactRow {
            source: source.to_owned(),
            requirement: requirement.to_owned(),
            change,
            symbols,
            renamed_to,
        });
    }
    impact.sort_by(|left, right| {
        (&left.source[..], &left.requirement[..], left.change).cmp(&(
            &right.source[..],
            &right.requirement[..],
            right.change,
        ))
    });

    // Provider rows and the catalog revision over the canonically
    // sorted (source, id, digest) rows.
    let mut providers: Vec<ProviderRow> = Vec::new();
    for snapshot in &snapshots {
        let mut capabilities: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for entry in &snapshot.entries {
            capabilities.insert(entry.id.split(".REQ-").next().unwrap_or_default());
        }
        providers.push(ProviderRow {
            source: snapshot.source.clone(),
            kind: "openspec".to_owned(),
            root: snapshot.root.clone(),
            status: match snapshot.status {
                ProviderStatus::Ok => "ok",
                ProviderStatus::Absent => "absent",
            },
            capability_count: capabilities.len(),
            requirement_count: snapshot.entries.len(),
        });
    }
    providers.sort_by(|left, right| left.source.cmp(&right.source));
    let mut revision_input = String::new();
    for row in &requirements {
        revision_input.push_str(&row.source);
        revision_input.push('\u{1f}');
        revision_input.push_str(&row.id);
        revision_input.push('\u{1f}');
        revision_input.push_str(&row.digest);
        revision_input.push('\n');
    }

    let mut conflict_rows: Vec<ConflictRow> = Vec::new();
    for snapshot in &snapshots {
        for row in &snapshot.conflicts {
            conflict_rows.push(ConflictRow {
                source: snapshot.source.clone(),
                capability: row.capability.clone(),
                title: row.title.clone(),
                detail: row.detail.to_owned(),
            });
        }
    }
    conflict_rows.sort_by(|left, right| {
        (
            &left.source[..],
            &left.capability[..],
            &left.title[..],
            &left.detail[..],
        )
            .cmp(&(
                &right.source[..],
                &right.capability[..],
                &right.title[..],
                &right.detail[..],
            ))
    });

    let report = Report {
        schema_version: version::REPORT_SCHEMA_VERSION,
        identity: version::REPORT_IDENTITY,
        project_id: attachment.project_id().as_str().to_owned(),
        model_ref: ModelRefWire {
            model_version: attachment.model_ref().model_version.clone(),
            digest: attachment.model_ref().digest.as_str().to_owned(),
        },
        attachment_digest,
        source_revision: sha256_hex(revision_input.as_bytes()),
        providers,
        requirements,
        references,
        coverage_gaps,
        conflicts: conflict_rows,
        impact,
    };
    let verdict = verdict_of(&report);
    Resolution { report, verdict }
}

impl Report {
    /// The canonical report bytes (compact JSON, byte-sorted keys,
    /// canonical collections, no trailing LF), or the export-limit
    /// refusal.
    pub fn canonical_bytes(&self) -> Result<String, DiagnosticSet> {
        let bytes = super::canonical_value_bytes(self);
        if bytes.len() > version::MAX_EXPORT_BYTES {
            return Err(super::diagnostic::export_limit(
                "canonical-bytes",
                &format!("bytes={}", bytes.len()),
            ));
        }
        Ok(bytes)
    }

    /// Project this report into the typed #22 neutral trace manifest,
    /// re-validated by the accepted trace validator. Pure and
    /// read-only.
    /// The canonical manifest digest: `sha256:` over the canonical
    /// report bytes, or the refusal set.
    pub fn digest(&self) -> Result<String, DiagnosticSet> {
        Ok(super::sha256_hex(self.canonical_bytes()?.as_bytes()))
    }

    pub fn trace_manifest(&self) -> Result<crate::trace::TraceManifest, DiagnosticSet> {
        super::trace::validated_manifest(self)
    }
}

/// Compute the gate verdict from the finished report.
fn verdict_of(report: &Report) -> ResolutionVerdict {
    let mut stale = Vec::new();
    let mut missing = Vec::new();
    for row in &report.references {
        let subject = format!("{}:{}", row.source, row.requirement);
        match row.status {
            "stale" => stale.push(subject),
            "missing" => missing.push(subject),
            _ => {}
        }
    }
    let conflicts: Vec<String> = report
        .conflicts
        .iter()
        .map(|row| format!("{}:{}", row.source, row.title))
        .collect();
    if stale.is_empty() && missing.is_empty() && conflicts.is_empty() {
        return ResolutionVerdict::Pass;
    }
    ResolutionVerdict::Denied(
        diagnostic::gate(stale, missing, conflicts)
            .unwrap_or_else(|| crate::result::singleton_set("diagnostics.registry-invalid")),
    )
}
