//! The SARIF 2.1.0 projection of a CI report (issue #103).
//!
//! One run per report: a Lekalo tool driver with registry-derived rules,
//! one result per normalized diagnostic, repository-relative safe paths
//! under the `%SRCROOT%` base id, and the closed Lekalo property
//! projection carrying the report/status/exit binding. Deterministic:
//! rules are sorted by rule id and deduplicated; no absolute URIs, no
//! host identity, no environment values.

use std::collections::BTreeMap;

use serde::Serialize;

use super::model::CiReport;

/// The SARIF log root.
#[derive(Serialize)]
struct SarifLog<'a> {
    #[serde(rename = "$schema")]
    schema: &'static str,
    version: &'static str,
    runs: Vec<SarifRun<'a>>,
}

/// One SARIF run.
#[derive(Serialize)]
struct SarifRun<'a> {
    tool: SarifTool<'a>,
    #[serde(rename = "columnKind")]
    column_kind: &'static str,
    #[serde(rename = "originalUriBaseIds")]
    original_uri_base_ids: BTreeMap<&'static str, SarifUriBase>,
    #[serde(rename = "automationDetails")]
    automation_details: SarifAutomation,
    results: Vec<SarifResult<'a>>,
    properties: SarifRunProperties,
}

/// The `%SRCROOT%` description (the absolute checkout URI is
/// deliberately omitted; OASIS permits this).
#[derive(Serialize)]
struct SarifUriBase {
    description: SarifMessage,
}

/// The tool driver.
#[derive(Serialize)]
struct SarifTool<'a> {
    driver: SarifDriver<'a>,
}

/// The Lekalo driver descriptor.
#[derive(Serialize)]
struct SarifDriver<'a> {
    name: &'static str,
    #[serde(rename = "semanticVersion")]
    semantic_version: &'a str,
    information_uri: &'static str,
    rules: Vec<SarifRule<'a>>,
}

/// One registry-derived rule.
#[derive(Serialize)]
struct SarifRule<'a> {
    id: &'a str,
    name: String,
    #[serde(rename = "shortDescription")]
    short_description: SarifMessage,
    #[serde(rename = "defaultConfiguration")]
    default_configuration: SarifDefaultConfiguration,
    properties: SarifRuleProperties<'a>,
}

/// The default severity configuration of one rule.
#[derive(Serialize)]
struct SarifDefaultConfiguration {
    level: &'static str,
}

/// One result.
#[derive(Serialize)]
struct SarifResult<'a> {
    #[serde(rename = "ruleId")]
    rule_id: &'a str,
    #[serde(rename = "ruleIndex")]
    rule_index: usize,
    kind: &'static str,
    level: &'static str,
    message: SarifMessage,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    locations: Vec<SarifLocation>,
    properties: SarifResultProperties,
}

/// One physical location (present only for located diagnostics).
#[derive(Serialize)]
struct SarifLocation {
    #[serde(rename = "physicalLocation")]
    physical_location: SarifPhysicalLocation,
}

/// The artifact/region pair of one location.
#[derive(Serialize)]
struct SarifPhysicalLocation {
    #[serde(rename = "artifactLocation")]
    artifact_location: SarifArtifactLocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    region: Option<SarifRegion>,
}

/// The repository-relative artifact reference.
#[derive(Serialize)]
struct SarifArtifactLocation {
    uri: String,
    #[serde(rename = "uriBaseId")]
    uri_base_id: &'static str,
}

/// The one-based inclusive start through exclusive-end region.
#[derive(Serialize)]
struct SarifRegion {
    #[serde(rename = "startLine")]
    start_line: usize,
    #[serde(rename = "startColumn")]
    start_column: usize,
    #[serde(rename = "endLine")]
    end_line: usize,
    #[serde(rename = "endColumn")]
    end_column: usize,
}

/// The closed Lekalo property projection of one rule.
#[derive(Serialize)]
struct SarifRuleProperties<'a> {
    #[serde(rename = "lekaloCategory")]
    lekalo_category: String,
    #[serde(rename = "lekaloRegistryVersion")]
    lekalo_registry_version: String,
    #[serde(rename = "lekaloMessageId")]
    lekalo_message_id: &'a str,
}

/// The closed Lekalo property projection of one result.
#[derive(Serialize)]
struct SarifResultProperties {
    #[serde(rename = "lekaloDiagnosticIndex")]
    lekalo_diagnostic_index: usize,
}

/// The closed run-level property projection.
#[derive(Serialize)]
struct SarifRunProperties {
    #[serde(rename = "lekaloReportDigest")]
    lekalo_report_digest: String,
    #[serde(rename = "lekaloStatus")]
    lekalo_status: String,
    #[serde(rename = "lekaloExitCode")]
    lekalo_exit_code: u8,
    #[serde(rename = "lekaloVerdict")]
    lekalo_verdict: String,
}

#[derive(Serialize)]
struct SarifAutomation {
    id: String,
}

#[derive(Serialize)]
struct SarifMessage {
    text: String,
}

/// Map the diagnostic severity onto the SARIF level.
fn level_of(severity: &str) -> &'static str {
    match severity {
        "error" => "error",
        "warning" => "warning",
        _ => "note",
    }
}

/// Render the deterministic SARIF 2.1.0 document (compact JSON, no
/// trailing newline inside the bytes; the writer adds exactly one LF).
pub fn render(report: &CiReport) -> String {
    // Rules: sorted unique by rule id, carrying the registry facts.
    // The typed accessors keep this projection closed: read the wire
    // item fields through one serialization of the source diagnostic.
    let items: Vec<serde_json::Value> = report
        .diagnostics
        .iter()
        .map(|diagnostic| serde_json::to_value(diagnostic).expect("diagnostic serializes"))
        .collect();
    let mut rules: Vec<(&str, String, String, String, String, String)> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for item in &items {
        let id = item["id"].as_str().unwrap_or_default();
        if id.is_empty() || !seen.insert(id.to_owned()) {
            continue;
        }
        let code = item["code"].as_str().unwrap_or_default();
        rules.push((
            leak_rule_id(code),
            format!("lekalo/{}", id.replace('.', "/")),
            item["message"].as_str().unwrap_or_default().to_owned(),
            item["category"].as_str().unwrap_or_default().to_owned(),
            item["registry_version"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            item["message_id"].as_str().unwrap_or_default().to_owned(),
        ));
    }
    rules.sort_by(|left, right| left.0.cmp(right.0));
    let rule_ids: Vec<&str> = rules.iter().map(|(id, ..)| *id).collect();

    let mut results = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let id = item["id"].as_str().unwrap_or_default();
        let code = item["code"].as_str().unwrap_or_default();
        let rule_index = rule_ids.iter().position(|candidate| *candidate == code);
        let Some(rule_index) = rule_index else {
            continue;
        };
        let locations = item["source"]
            .as_object()
            .and_then(|source| {
                source["path"].as_str().map(|path| {
                    let range = source["range"].as_object();
                    SarifLocation {
                        physical_location: SarifPhysicalLocation {
                            artifact_location: SarifArtifactLocation {
                                uri: path.to_owned(),
                                uri_base_id: "%SRCROOT%",
                            },
                            region: range.map(|range| SarifRegion {
                                start_line: range["start"]["line"].as_u64().unwrap_or(1) as usize,
                                start_column: range["start"]["column"].as_u64().unwrap_or(1)
                                    as usize,
                                end_line: range["end"]["line"].as_u64().unwrap_or(1) as usize,
                                end_column: range["end"]["column"].as_u64().unwrap_or(1) as usize,
                            }),
                        },
                    }
                })
            })
            .into_iter()
            .collect::<Vec<_>>();
        let code = item["code"].as_str().unwrap_or(id);
        results.push(SarifResult {
            rule_id: leak_rule_id(code),
            rule_index,
            kind: "review",
            level: level_of(item["severity"].as_str().unwrap_or("note")),
            message: SarifMessage {
                text: item["message"].as_str().unwrap_or_default().to_owned(),
            },
            locations,
            properties: SarifResultProperties {
                lekalo_diagnostic_index: index,
            },
        });
    }

    let log = SarifLog {
        schema: "https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/schemas/sarif-schema-2.1.0.json",
        version: "2.1.0",
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifDriver {
                    name: "Lekalo",
                    semantic_version: super::version::REPORT_VERSION,
                    information_uri: "https://github.com/ichinya/lekalo",
                    rules: rules
                        .iter()
                        .map(|(id, name, message, category, registry, message_id)| SarifRule {
                            id,
                            name: name.clone(),
                            short_description: SarifMessage {
                                text: message.clone(),
                            },
                            default_configuration: SarifDefaultConfiguration {
                                level: "note",
                            },
                            properties: SarifRuleProperties {
                                lekalo_category: category.clone(),
                                lekalo_registry_version: registry.clone(),
                                lekalo_message_id: leak_rule_id(message_id),
                            },
                        })
                        .collect(),
                },
            },
            column_kind: "unicodeCodePoints",
            original_uri_base_ids: [(
                "%SRCROOT%",
                SarifUriBase {
                    description: SarifMessage {
                        text: "Repository root".to_owned(),
                    },
                },
            )]
            .into_iter()
            .collect(),
            automation_details: SarifAutomation {
                id: format!(
                    "lekalo/{}/{}/",
                    report.invocation.command.as_str(),
                    report.evaluation.verdict.as_str()
                ),
            },
            results,
            properties: SarifRunProperties {
                lekalo_report_digest: report.digest_spelling(),
                lekalo_status: report.evaluation.status.to_owned(),
                lekalo_exit_code: report.evaluation.exit_code,
                lekalo_verdict: report.evaluation.verdict.as_str().to_owned(),
            },
        }],
    };
    serde_json::to_string(&log).expect("sarif serializes")
}

/// The rule ids arrive from validated core diagnostics whose `id` field
/// is registry-grammar-checked; the lifetime note keeps the borrowed
/// rule id alive for the serialized document built in this call.
fn leak_rule_id(id: &str) -> &str {
    id
}
