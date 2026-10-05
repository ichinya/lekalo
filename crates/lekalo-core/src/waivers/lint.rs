//! Adapter from the shipped lint facts to the one governance matcher.
use super::{
    policy::{LintState, ProfileState},
    *,
};
use crate::{
    ai_lint::{self as lint, input, Config, Evidence, Report},
    ir::Compilation,
    project_fs::Fs,
    DomainResult,
};

pub fn facts(
    report: &Report,
    config: &Config,
    evidence: &[Evidence],
    compilation: &Compilation,
    fs: &Fs,
) -> Result<Input, DomainResult> {
    let profile = LintState {
        config,
        id: &report.profile,
    };
    let lock_ref = match fs.read_file_opt("", "lekalo.lock", input::MAX_BYTES) {
        Ok(Some(bytes)) => State::Known(input::digest(bytes.strip_suffix(b"\n").unwrap_or(&bytes))),
        Ok(None) | Err(crate::project_fs::FsErrorKind::NotFound) => State::Unknown,
        Err(_) => return Err(failure("waivers-lock-read")),
    };
    let project_id = compilation
        .project
        .project
        .as_ref()
        .ok_or_else(|| failure("waivers-project-missing"))?
        .id
        .as_str()
        .into();
    let facts = report
        .findings
        .iter()
        .map(|f| {
            let native = evidence.iter().find(|e| e.target == f.target);
            let symbol = f
                .semantic_symbol
                .known()
                .filter(|s| s.contains('.'))
                .cloned()
                .map_or(State::Unknown, State::Known);
            let module = symbol
                .known()
                .and_then(|s| s.split('.').next())
                .filter(|id| {
                    compilation
                        .project
                        .modules
                        .iter()
                        .any(|m| m.id.as_str() == *id)
                })
                .map_or(State::Unknown, |s| State::Known(s.into()));
            let path = native
                .and_then(|e| {
                    let paths: std::collections::BTreeSet<_> = e
                        .locations
                        .iter()
                        .filter(|l| f.locations.contains(&l.id))
                        .filter_map(|l| {
                            e.sources.iter().find(|s| s.id == l.source).map(|s| &s.path)
                        })
                        .collect();
                    if paths.len() == 1 {
                        paths.first().map(|p| (*p).clone())
                    } else {
                        None
                    }
                })
                .map_or(State::Unknown, State::Known);
            let revision = native.map_or_else(
                || State::Known(input::hash(&(&report.model_ref, &report.ir_ref))),
                |e| match &e.pins.revision {
                    State::Known(r) => {
                        State::Known(input::hash(&(r, &e.sources, &e.input_manifest_digest)))
                    }
                    State::Unknown => State::Unknown,
                    State::Withheld => State::Withheld,
                    State::Unsupported => State::Unsupported,
                },
            );
            Fact {
                id: f.id.clone(),
                selector: Selector {
                    kind: SelectorKind::Rule,
                    id: f.rule_id.clone(),
                },
                subject: f.subject.clone(),
                target: f.target.clone(),
                symbol,
                module,
                path,
                condition_digest: f.condition_digest.clone(),
                fingerprint: Fingerprint {
                    model: State::Known(report.model_ref.clone()),
                    ir: State::Known(report.ir_ref.clone()),
                    adapter: native.map_or(State::Unsupported, |e| {
                        State::Known(input::hash(&e.producer))
                    }),
                    revision,
                    capabilities: native
                        .map_or(State::Unsupported, |e| e.pins.capabilities.clone()),
                },
                source_outcome: "warning".into(),
                source_severity: State::Known(f.severity.clone()),
                source_confidence: State::Known(
                    serde_json::to_value(f.confidence)
                        .expect("confidence")
                        .as_str()
                        .expect("string")
                        .into(),
                ),
                evidence_refs: report.evidence_refs.clone(),
            }
        })
        .collect();
    let i = Input {
        schema_version: format!("lekalo/waiver-input/v{VERSION}"),
        identity: format!("dev.lekalo.waiver-input@{VERSION}"),
        project_id,
        profile_ref: profile.reference(),
        lock_ref,
        complete: false,
        facts,
    };
    validate_input(&i)?;
    Ok(i)
}

pub fn apply(
    report: &mut Report,
    store: &Store,
    facts: &Input,
    config: &Config,
    as_of: &str,
    baseline: Option<&Report>,
) -> Result<Audit, DomainResult> {
    let audit = super::audit(
        store,
        facts,
        &LintState {
            config,
            id: &report.profile,
        },
        as_of,
        604800,
        None,
    )?;
    report.waiver_ref = State::Known(input::hash(store));
    for finding in &mut report.findings {
        let disposition = audit
            .findings
            .iter()
            .find(|f| f.fact.id == finding.id)
            .expect("same facts");
        if let Some(id) = disposition.waiver.known() {
            finding.disposition = "waived".into();
            finding.waiver = State::Known(id.clone());
        }
    }
    report.summary.waived = report
        .findings
        .iter()
        .filter(|f| f.disposition == "waived")
        .count() as u64;
    report.summary.active = report.summary.raw - report.summary.waived;
    report.summary.diagnostic_projection_truncated = report.summary.active > 252;
    for metric in &mut report.metrics {
        metric.waived = report
            .findings
            .iter()
            .filter(|f| {
                f.rule_id == metric.rule
                    && f.confidence == metric.confidence
                    && f.disposition == "waived"
            })
            .count() as u64;
        metric.active = metric.raw - metric.waived;
    }
    report.comparison = State::Unknown;
    if let Some(base) = baseline {
        report.comparison = State::Known(lint::compare::compare(base, report));
    }
    lint::compare::validate_report(report)?;
    Ok(audit)
}
