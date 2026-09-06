//! The two projections of one normalized capsule (issue #17).
//!
//! [`json`] emits the canonical compact envelope payload bytes and
//! [`markdown`] the agent-facing document. Both consume the exact same
//! [`Selected`] capsule — one normalized product, two deterministic
//! renderings, byte-identical across reruns, frontends, and platforms.

use super::canonical::{array, boolean, number, object, string};
use super::estimate;
use super::model::{gap_json, manifest_row_json};
use super::select::Selected;
use super::version::{IDENTITY, SCHEMA_VERSION};

/// The canonical JSON payload bytes of one capsule.
pub(crate) fn json(capsule: &Selected) -> String {
    let mut sections: Vec<(&'static str, String)> = Vec::new();
    for (key, facts) in &capsule.sections {
        sections.push((key, array(facts.iter().map(|fact| fact.json()))));
    }
    let included: Vec<&super::model::ManifestRow> = capsule
        .manifest
        .iter()
        .filter(|row| row.reason.is_none())
        .collect();
    let excluded: Vec<&super::model::ManifestRow> = capsule
        .manifest
        .iter()
        .filter(|row| row.reason.is_some())
        .collect();
    let mut fields = vec![
        (
            "budget",
            object(vec![
                ("estimated", number(capsule.estimated)),
                ("fits", boolean(capsule.fits)),
                ("limit", number(capsule.budget)),
                ("minimumRequired", number(capsule.minimum_required)),
            ]),
        ),
        ("complete", boolean(capsule.complete)),
        (
            "coverage",
            object(vec![
                ("candidates", number(capsule.manifest.len() as u64)),
                ("excluded", number(excluded.len() as u64)),
                ("included", number(included.len() as u64)),
            ]),
        ),
        (
            "estimator",
            object(vec![
                ("digest", string(&estimate::digest())),
                ("identity", string(estimate::IDENTITY)),
                ("version", string(estimate::VERSION)),
            ]),
        ),
        ("gaps", array(capsule.gaps.iter().map(gap_json))),
        ("identity", string(IDENTITY)),
        ("irDigest", string(&capsule.ir_digest)),
        (
            "manifest",
            object(vec![
                (
                    "excluded",
                    array(excluded.iter().map(|row| manifest_row_json(row))),
                ),
                (
                    "included",
                    array(included.iter().map(|row| manifest_row_json(row))),
                ),
            ]),
        ),
        ("mode", string(capsule.mode)),
        ("modelVersion", string(capsule.model_version.as_str())),
        (
            "project",
            match &capsule.project {
                Some(project) => string(project),
                None => "null".to_owned(),
            },
        ),
        (
            "roots",
            array(capsule.roots.iter().map(|root| string(root))),
        ),
        ("schemaVersion", string(SCHEMA_VERSION)),
        ("sections", object(sections)),
    ];
    if !capsule.spans.is_empty() {
        fields.push((
            "spans",
            array(capsule.spans.iter().map(|row| {
                object(vec![
                    ("id", string(&row.id)),
                    (
                        "span",
                        object(vec![
                            ("end", position(row.end)),
                            ("path", string(&row.path)),
                            ("start", position(row.start)),
                        ]),
                    ),
                ])
            })),
        ));
    }
    object(fields)
}

fn position(position: crate::loader::Position) -> String {
    object(vec![
        ("byte", number(position.byte as u64)),
        ("column", number(position.column as u64)),
        ("line", number(position.line as u64)),
    ])
}

/// The deterministic Markdown document of one capsule.
pub(crate) fn markdown(capsule: &Selected) -> String {
    let mut lines: Vec<String> = vec!["# context capsule".to_owned()];
    lines.push(format!("- identity: {IDENTITY}"));
    lines.push(format!("- contract: {SCHEMA_VERSION}"));
    lines.push(format!(
        "- model version: {}",
        capsule.model_version.as_str()
    ));
    lines.push(format!("- mode: {}", capsule.mode));
    lines.push(format!(
        "- project: {}",
        capsule.project.as_deref().unwrap_or("none")
    ));
    lines.push(format!("- roots: {}", capsule.roots.join(", ")));
    lines.push(format!("- ir digest: {}", capsule.ir_digest));
    lines.push(format!("- estimator: {}", estimate::IDENTITY));
    lines.push(format!("- estimator digest: {}", estimate::digest()));
    lines.push(format!(
        "- budget: limit {}, estimated {}, minimum required {}, fits {}",
        capsule.budget, capsule.estimated, capsule.minimum_required, capsule.fits
    ));
    let included = capsule
        .manifest
        .iter()
        .filter(|row| row.reason.is_none())
        .count();
    let excluded = capsule.manifest.len() - included;
    lines.push(format!(
        "- coverage: {} candidates, {} included, {} excluded",
        capsule.manifest.len(),
        included,
        excluded
    ));
    lines.push(format!("- complete: {}", capsule.complete));
    for (key, facts) in &capsule.sections {
        lines.push(String::new());
        lines.push(format!("## {key}"));
        for fact in facts {
            lines.extend(fact.markdown());
        }
    }
    if !capsule.spans.is_empty() {
        lines.push(String::new());
        lines.push("## spans".to_owned());
        for row in &capsule.spans {
            lines.push(format!(
                "- {}: {}:{}:{}-{}:{}",
                row.id, row.path, row.start.line, row.start.column, row.end.line, row.end.column
            ));
        }
    }
    if !capsule.gaps.is_empty() {
        lines.push(String::new());
        lines.push("## gaps".to_owned());
        for gap in &capsule.gaps {
            if gap.symbols.is_empty() {
                lines.push(format!("- {}", gap.gap));
            } else {
                lines.push(format!("- {}: {}", gap.gap, gap.symbols.join(", ")));
            }
        }
    }
    let excluded_rows: Vec<&super::model::ManifestRow> = capsule
        .manifest
        .iter()
        .filter(|row| row.reason.is_some())
        .collect();
    if !excluded_rows.is_empty() {
        lines.push(String::new());
        lines.push("## excluded".to_owned());
        for row in excluded_rows {
            lines.push(format!(
                "- {} ({}, reason: {})",
                row.id,
                row.section,
                row.reason
                    .map(super::model::ExclusionReason::as_str)
                    .unwrap_or("unknown")
            ));
        }
    }
    lines.join("\n")
}
