//! Read-only task-links pilot commands (issue #52). No provider call,
//! no outbox write, no token import, no report persistence: the
//! commands join the closed attachment to the current compilation and
//! project typed views only. Every digest pin is recomputed here (the
//! adapter owns the reads; the core stays pure) and a divergent pin
//! fails closed before any view, validation, or projection runs.

use clap::Subcommand;
use lekalo_core::task_links as tl;
use lekalo_core::{loader::LoadSelection, DomainResult};
use std::io::Read;

/// The `task-links` subcommand surface: `validate`, `inspect`, and
/// `portability` with the exact proposed grammar.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Join the attachment to the current compilation and report the
    /// static obligations.
    Validate {
        /// The closed integration attachment path.
        #[arg(value_name = "PATH")]
        path: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// The capability admission profile.
        #[arg(long, value_name = "PROFILE", default_value = "strict")]
        profile: String,
    },
    /// Display typed relations, decisions, effects, and admitted
    /// evidence for the requested symbol; missing evidence stays
    /// unavailable. An unknown symbol is a typed not-found refusal.
    Inspect {
        /// The symbol to inspect.
        #[arg(value_name = "SYMBOL")]
        symbol: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// The closed integration attachment path.
        #[arg(long, value_name = "PATH")]
        attachment: String,
        /// Optional admitted evidence document.
        #[arg(long, value_name = "PATH")]
        evidence: Option<String>,
    },
    /// Join resolved target/component profiles and provider contract
    /// requirements into the closed portability projection.
    Portability {
        /// The closed integration attachment path.
        #[arg(value_name = "PATH")]
        path: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// The source profile document.
        #[arg(long, value_name = "PATH")]
        from_profile: String,
        /// The target profile document.
        #[arg(long, value_name = "PATH")]
        to_profile: String,
        /// The capability admission profile.
        #[arg(long, value_name = "PROFILE", default_value = "strict")]
        profile: String,
        /// Optional admitted evidence documents; rows without admitted
        /// evidence stay honestly `unknown`.
        #[arg(long, value_name = "PATH")]
        evidence: Vec<String>,
    },
    /// Derive the reverse impact of one provider contract: every
    /// capability, binding, policy, command, effect, and verification
    /// scenario that consumes it, derived from the attachment and the
    /// live compilation — never a prewritten list.
    Impact {
        /// The provider contract id.
        #[arg(value_name = "CONTRACT")]
        contract: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// The closed integration attachment path.
        #[arg(long, value_name = "PATH")]
        attachment: String,
    },
}

fn invalid(detail: &'static str) -> DomainResult {
    tl::into_result(&tl::input_failure(detail))
}

/// Open one file no-follow and read at most [`tl::MAX_BYTES`] bytes.
///
/// The no-follow discipline is component-wise: every ancestor of the
/// operand path is checked for symlinks/Windows reparse points (a
/// junctioned intermediate directory can no more redirect the read
/// than a linked leaf), and the leaf itself is opened with the
/// no-reparse/no-follow flag so the metadata check and the open cannot
/// disagree. This is the same discipline the project filesystem
/// applies to every project-owned path, applied to the command's file
/// operands.
fn read_bounded(path: &str) -> Result<Vec<u8>, DomainResult> {
    // Walk every component: a link anywhere along the path is refused
    // (`attachment-file`), matching the repo convention.
    let operand = std::path::Path::new(path);
    for ancestor in operand.ancestors() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        let metadata =
            std::fs::symlink_metadata(ancestor).map_err(|_| invalid("attachment-unreadable"))?;
        if is_link(&metadata) {
            return Err(invalid("attachment-file"));
        }
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|_| invalid("attachment-unreadable"))?;
    if is_link(&metadata) || !metadata.is_file() {
        return Err(invalid("attachment-file"));
    }
    if metadata.len() as usize > tl::MAX_BYTES {
        return Err(invalid("attachment-bound"));
    }
    // Open the leaf without following any final reparse point, so the
    // checked path and the opened file are the same object.
    let file = open_nofollow(path).map_err(|_| invalid("attachment-unreadable"))?;
    let mut bytes = vec![];
    file.take(tl::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("attachment-unreadable"))?;
    if bytes.len() > tl::MAX_BYTES {
        return Err(invalid("attachment-bound"));
    }
    Ok(bytes)
}

/// Open one file with the platform's no-follow flag on the final
/// component (`FILE_FLAG_OPEN_REPARSE_POINT` on Windows,
/// `O_NOFOLLOW` elsewhere).
fn open_nofollow(path: &str) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        const O_NOFOLLOW: i32 = 0o200_000;
        options.custom_flags(O_NOFOLLOW);
    }
    options.open(path)
}

/// Whether raw metadata identifies a symlink or Windows reparse point.
fn is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x0400 != 0
    }
    #[cfg(unix)]
    {
        metadata.file_type().is_symlink()
    }
}

fn compilation(
    project: &Option<String>,
) -> Result<(lekalo_core::ir::Compilation, String), DomainResult> {
    let model = lekalo_core::loader::normalize_model(&LoadSelection {
        project: project
            .clone()
            .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    })?;
    let compilation = lekalo_core::ir::compile(&model).map_err(|failure| failure.into_result())?;
    let project_id = compilation
        .project
        .project
        .as_ref()
        .map(|project| project.id.as_str().to_owned())
        .unwrap_or_default();
    Ok((compilation, project_id))
}

/// The accepted owner-contract pin digests, hashed from the exact
/// checkout contract files. The working directory is the checkout root
/// for every documented invocation (all family gates run from it).
fn accepted_owner_pins() -> Vec<tl::OwnerPin> {
    let mut pins = Vec::new();
    for (family, version) in [
        ("authorization", "v0.2.16"),
        ("transaction-concurrency", "v0.2.16"),
        ("extended-effects", "v0.2.16"),
        ("invariant-transition", "v0.2.16"),
        ("storage-projection", "v0.4.0"),
    ] {
        let file_name = format!("contracts/{family}.schema.{version}.json");
        let Ok(bytes) = std::fs::read(&file_name) else {
            continue;
        };
        let identity_version = version.trim_start_matches('v');
        pins.push(tl::OwnerPin {
            family: family.to_owned(),
            identity: format!("dev.lekalo.{family}@{identity_version}"),
            digest: tl::wire_digest(&bytes),
        });
    }
    pins.sort();
    pins
}

/// The verified native qualification: the executed legs, the exact
/// receipt digest, and the executed target binding, bound to the
/// provisioned database whose OWN durable rows the battery wrote.
#[derive(Clone, Debug, Eq, PartialEq)]
struct NativeQualification {
    legs: std::collections::BTreeSet<String>,
    receipt_digest: String,
    /// The implementation that actually executed the battery (the
    /// receipt's target binding): guarantees may only claim support for
    /// the target that ran, never for a different selected profile.
    target_implementation: String,
    /// The build profile the battery executed under.
    target_profile: String,
}

/// The executed native legs the qualification receipt must name before
/// any native-origin evidence is admitted: the durable ack-window
/// restart, the durable attempt bound, the DB-level identity race, and
/// the real process-interruption recovery.
const NATIVE_QUALIFICATION_LEGS: [&str; 4] = [
    "accepted_before_ack_survives_restart_as_unknown",
    "attempt_bound_survives_restart",
    "synchronized_identity_race_binds_one_task",
    "interrupted_transaction_recovers_durable_state",
];

/// The exact execution boundary the battery records: real SQLite with
/// separate connections, in-process recovery, and a real
/// process-kill interruption. Maintained-port execution is NOT
/// covered by this boundary, and the admission says so by requiring
/// the exact recorded spelling.
const NATIVE_EXECUTION_BOUNDARY: &str = "same-process-recovery-real-sqlite-process-kill";
const NATIVE_RUNNER: &str = "cargo-test-lekalo-core-native_leg";
/// The implementation identity of the executed battery: the
/// repository's own Rust core over the bundled SQLite engine.
const NATIVE_TARGET_IMPLEMENTATION: &str = "lekalo-core-bundled-sqlite";
/// The build profile the battery executes under.
const NATIVE_TARGET_PROFILE: &str = "cargo-test";

/// Check the declared native receipt against a real no-follow SQLite
/// database: leg set, boundary, runner, run and target must agree.
/// This rejects header-only files and divergent/missing rows, but the
/// receipt AND rows remain caller-writable. It is structural consistency,
/// not authenticated executed custody; portability therefore leaves
/// execution-dependent guarantees unknown even when this check passes.
fn native_qualification() -> Option<NativeQualification> {
    if std::env::var("LEKALO_TASKLINKS_NATIVE").as_deref() != Ok("1") {
        return None;
    }
    let database = std::env::var("LEKALO_TASKLINKS_NATIVE_DB").ok()?;
    if database.is_empty() {
        return None;
    }
    // The database is a file operand like every other: the same
    // component-wise no-follow discipline refuses a linked database
    // (a symlinked DB is not the provisioned custody object).
    let database_path = std::path::Path::new(&database);
    for ancestor in database_path.ancestors() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        let metadata = std::fs::symlink_metadata(ancestor).ok()?;
        if is_link(&metadata) {
            return None;
        }
    }
    let database_meta = std::fs::symlink_metadata(database_path).ok()?;
    if is_link(&database_meta) || !database_meta.is_file() {
        return None;
    }
    let receipt_path = database_path.with_file_name(format!(
        "{}.qualified.json",
        database_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
    ));
    let bytes = read_bounded(
        receipt_path
            .to_str()
            .map(str::to_owned)
            .as_deref()
            .unwrap_or_default(),
    )
    .ok()?;
    // Duplicate-checked JSON decode: a smuggled duplicate member is a
    // refusal, never a last-wins overwrite.
    let receipt: serde_json::Value = tl::json_parse(&bytes).ok()?;
    let object = receipt.as_object()?;
    const RECEIPT_KEYS: [&str; 9] = [
        "schemaVersion",
        "identity",
        "database",
        "executionBoundary",
        "runner",
        "runId",
        "targetImplementation",
        "targetProfile",
        "legs",
    ];
    if object.len() != RECEIPT_KEYS.len() {
        return None;
    }
    for key in RECEIPT_KEYS {
        if !object.contains_key(key) {
            return None;
        }
    }
    if receipt["schemaVersion"] != "lekalo/task-links-native-receipt/v0.8.0"
        || receipt["identity"] != "dev.lekalo.task-links-native-receipt@0.8.0"
        || receipt["executionBoundary"] != NATIVE_EXECUTION_BOUNDARY
        || receipt["runner"] != NATIVE_RUNNER
        || receipt["targetImplementation"] != NATIVE_TARGET_IMPLEMENTATION
        || receipt["targetProfile"] != NATIVE_TARGET_PROFILE
    {
        return None;
    }
    let run_id = receipt["runId"].as_str()?;
    if run_id.is_empty() || run_id.len() > 128 {
        return None;
    }
    // The receipt names the exact database the battery ran against.
    // Path SPELLING differs legitimately (forward vs back slashes), so
    // the comparison is canonical — but still fail-closed: a receipt
    // naming any other database (existing or not) refuses.
    let receipt_database = receipt["database"].as_str()?;
    if std::fs::canonicalize(database_path).ok()?
        != std::fs::canonicalize(std::path::Path::new(receipt_database)).ok()?
    {
        return None;
    }
    let legs_json = receipt["legs"].as_array()?;
    let mut legs = std::collections::BTreeSet::new();
    for leg in legs_json {
        let name = leg.as_str()?;
        if name.is_empty() || name.len() > 128 {
            return None;
        }
        legs.insert(name.to_owned());
    }
    if legs.len() != legs_json.len() || legs.len() != NATIVE_QUALIFICATION_LEGS.len() {
        return None;
    }
    for required in NATIVE_QUALIFICATION_LEGS {
        if !legs.contains(required) {
            return None;
        }
    }
    // Read the real DB's rows rather than trusting a magic header.
    // Agreement checks consistency only: an arbitrary local producer
    // can also write these rows and cannot thereby qualify execution.
    let connection = rusqlite::Connection::open_with_flags(
        database_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .ok()?;
    let mut statement = connection
        .prepare(
            "SELECT leg, execution_boundary, runner, run_id, target_implementation, target_profile
             FROM qualification",
        )
        .ok()?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .ok()?;
    let mut durable_legs = std::collections::BTreeSet::new();
    for row in rows {
        let (leg, boundary, runner, row_run_id, implementation, profile) = row.ok()?;
        if boundary != NATIVE_EXECUTION_BOUNDARY
            || runner != NATIVE_RUNNER
            || row_run_id != run_id
            || implementation != NATIVE_TARGET_IMPLEMENTATION
            || profile != NATIVE_TARGET_PROFILE
            || leg.is_empty()
            || leg.len() > 128
        {
            return None;
        }
        if !durable_legs.insert(leg) || durable_legs.len() > NATIVE_QUALIFICATION_LEGS.len() {
            return None;
        }
    }
    // Exact agreement, both directions: the receipt names exactly the
    // legs the database records — no phantom legs, no unrecorded
    // claims.
    if durable_legs != legs {
        return None;
    }
    Some(NativeQualification {
        legs,
        receipt_digest: format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes)),
        target_implementation: NATIVE_TARGET_IMPLEMENTATION.to_owned(),
        target_profile: NATIVE_TARGET_PROFILE.to_owned(),
    })
}

fn check_profile(profile: &str) -> Result<(), DomainResult> {
    if !tl::PROFILES.contains(&profile) {
        return Err(invalid("profile"));
    }
    Ok(())
}

/// The declared target names of the compilation: the profile ids a
/// portability run may resolve at all.
fn declared_targets(compilation: &lekalo_core::ir::Compilation) -> Vec<String> {
    compilation
        .project
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            lekalo_core::ir::Definition::TargetBinding(binding) => {
                Some(binding.target.as_str().to_owned())
            }
            _ => None,
        })
        .collect()
}

/// The scenario coverage table of the compilation: scenario id -> the
/// symbols it covers. Evidence binds to facts through this table.
fn scenario_covers(
    compilation: &lekalo_core::ir::Compilation,
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<String>> {
    let mut covers = std::collections::BTreeMap::new();
    for definition in &compilation.project.definitions {
        if let lekalo_core::ir::Definition::Scenario(scenario) = definition {
            covers.insert(
                scenario.id.as_str().to_owned(),
                scenario
                    .covers
                    .iter()
                    .map(|symbol| symbol.as_str().to_owned())
                    .collect(),
            );
        }
    }
    covers
}

fn profile_ref(
    path: &str,
    compilation: &lekalo_core::ir::Compilation,
) -> Result<tl::portability::ProfileRef, DomainResult> {
    let bytes = read_bounded(path)?;
    let digest = format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes));
    // The profile document is a closed single-member object: unknown
    // members, duplicate members, and shape drift fail closed before
    // any row is built.
    let value: serde_json::Value = tl::json_parse(&bytes).map_err(|_| invalid("profile-shape"))?;
    let object = value.as_object().ok_or_else(|| invalid("profile-shape"))?;
    if object.len() != 1 || !object.contains_key("profileId") {
        return Err(invalid("profile-shape"));
    }
    let profile_id = object
        .get("profileId")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid("profile-shape"))?;
    if profile_id.is_empty() || profile_id.len() > 256 {
        return Err(invalid("profile-shape"));
    }
    // A profile is only resolved when the compilation actually binds
    // the named target: an imaginary target can never carry guarantee
    // rows.
    if !declared_targets(compilation).contains(&profile_id.to_owned()) {
        return Err(invalid("profile-unresolved"));
    }
    Ok(tl::portability::ProfileRef {
        profile_id: profile_id.to_owned(),
        digest,
    })
}

/// Parse and fully verify one attachment against the live compilation:
/// closed parse, recomputed IR digest, recomputed attachment revision,
/// and verified owner-contract pins. Any divergence fails closed
/// before the attachment is used.
fn verified_attachment(
    path: &str,
    compilation: &lekalo_core::ir::Compilation,
) -> Result<tl::Attachment, DomainResult> {
    let bytes = read_bounded(path)?;
    let attachment = tl::parse(&bytes).map_err(|f| tl::into_result(&f))?;
    tl::verify_pins(&attachment, compilation, &accepted_owner_pins(), &bytes)
        .map_err(|f| tl::into_result(&f))?;
    Ok(attachment)
}

pub fn run(command: Command) -> DomainResult {
    execute(command).unwrap_or_else(|result| result)
}

fn execute(command: Command) -> Result<DomainResult, DomainResult> {
    match command {
        Command::Validate {
            path,
            project,
            profile,
        } => {
            check_profile(&profile)?;
            let (compilation, _) = compilation(&project)?;
            let attachment = verified_attachment(&path, &compilation)?;
            let report = tl::validate(&attachment, &compilation, &profile)
                .map_err(|failure| tl::into_result(&failure))?;
            if tl::invalidates(&report) {
                Ok(tl::render_invalid(&report, &report.findings))
            } else if report.status == "gaps-admitted" {
                Ok(tl::render_with_gaps(&report, &report.findings))
            } else {
                Ok(tl::render(&report))
            }
        }
        Command::Inspect {
            symbol,
            project,
            attachment: attachment_path,
            evidence,
        } => {
            let (compilation, project_id) = compilation(&project)?;
            let attachment = verified_attachment(&attachment_path, &compilation)?;
            if attachment.project_id() != project_id {
                return Err(invalid("project-mismatch"));
            }
            // The contribution switches are read, not decorative: with
            // the inspect contribution disabled, the attachment
            // declares no inspect surface and the command refuses.
            if !attachment.contributions().inspect {
                return Err(invalid("contribution-disabled"));
            }
            // Inspect joins the semantic validation: a malformed
            // attachment never renders as an authoritative view.
            let validation = tl::validate(&attachment, &compilation, "strict")
                .map_err(|failure| tl::into_result(&failure))?;
            if tl::invalidates(&validation) {
                return Ok(tl::render_invalid(&validation, &validation.findings));
            }
            let admitted = match &evidence {
                Some(path) => Some(
                    tl::admit_evidence(
                        &read_bounded(path)?,
                        &attachment,
                        &compilation,
                        native_qualification().is_some(),
                    )
                    .map_err(|f| tl::into_result(&f))?,
                ),
                None => None,
            };
            let view = tl::view(&attachment, admitted);
            // The operand is the contract: resolve the symbol against the
            // view's rows before anything renders. An unknown symbol is
            // one typed not-found refusal, never the whole view.
            if !tl::view_contains_symbol(&view, &symbol) {
                return Err(invalid("symbol-not-found"));
            }
            Ok(tl::render(&view))
        }
        Command::Portability {
            path,
            project,
            from_profile,
            to_profile,
            profile,
            evidence,
        } => {
            check_profile(&profile)?;
            let (compilation, project_id) = compilation(&project)?;
            let attachment = verified_attachment(&path, &compilation)?;
            if attachment.project_id() != project_id {
                return Err(invalid("project-mismatch"));
            }
            // The impact contribution feeds the portability/impact
            // surface: with it disabled, the attachment declares no
            // projection and the command refuses.
            if !attachment.contributions().impact {
                return Err(invalid("contribution-disabled"));
            }
            let from = profile_ref(&from_profile, &compilation)?;
            let to = profile_ref(&to_profile, &compilation)?;
            // Portability never bypasses semantic validation: the shared
            // admission runs first and an invalid attachment is a
            // refusal, not a report with gaps.
            let validation = tl::validate(&attachment, &compilation, "strict")
                .map_err(|failure| tl::into_result(&failure))?;
            if tl::invalidates(&validation) {
                return Ok(tl::render_invalid(&validation, &validation.findings));
            }
            // Admitted evidence joins the report bound to the facts it
            // witnessed; every --evidence path goes through the same
            // admission as inspect.
            let mut admitted = Vec::new();
            for evidence_path in &evidence {
                let summary = tl::admit_evidence(
                    &read_bounded(evidence_path)?,
                    &attachment,
                    &compilation,
                    native_qualification().is_some(),
                )
                .map_err(|f| tl::into_result(&f))?;
                admitted.push(tl::portability::AdmittedEvidence {
                    evidence_id: summary.evidence_id,
                    origin: summary.origin,
                    scenario_id: summary.scenario_id,
                    executed: summary.executed,
                });
            }
            // Preserve the structurally checked run/target declaration.
            // It is not an owner-attested receipt: the report cannot
            // promote guarantees from this caller-writable provenance.
            let native = native_qualification();
            let native_evidence =
                native
                    .as_ref()
                    .map(|qualification| tl::portability::NativeEvidence {
                        legs: qualification.legs.clone(),
                        receipt_digest: qualification.receipt_digest.clone(),
                        target_implementation: qualification.target_implementation.clone(),
                        target_profile: qualification.target_profile.clone(),
                    });
            let report = tl::portability::report(
                &attachment,
                from,
                to,
                &profile,
                &admitted,
                &scenario_covers(&compilation),
                native_evidence.as_ref(),
            );
            let gaps = tl::portability::findings(&report);
            if profile == "strict" && !gaps.is_empty() {
                Ok(tl::render_invalid(&report, &gaps))
            } else if !gaps.is_empty() {
                Ok(tl::render_with_gaps(&report, &gaps))
            } else {
                Ok(tl::render(&report))
            }
        }
        Command::Impact {
            contract,
            project,
            attachment: attachment_path,
        } => {
            let (compilation, project_id) = compilation(&project)?;
            let attachment = verified_attachment(&attachment_path, &compilation)?;
            if attachment.project_id() != project_id {
                return Err(invalid("project-mismatch"));
            }
            // The contribution switches are read, not decorative: with
            // the impact contribution disabled, the attachment declares
            // no impact surface and the command refuses.
            if !attachment.contributions().impact {
                return Err(invalid("contribution-disabled"));
            }
            // Impact never bypasses semantic admission: the shared
            // strict validation runs before any projection, so a
            // malformed attachment never renders as an authoritative
            // consumer list.
            let validation = tl::validate(&attachment, &compilation, "strict")
                .map_err(|failure| tl::into_result(&failure))?;
            if tl::invalidates(&validation) {
                return Ok(tl::render_invalid(&validation, &validation.findings));
            }
            let Some(provider_contract) = attachment.provider_contract(&contract) else {
                return Err(invalid("contract-unresolved"));
            };
            // The reverse impact is derived, never prewritten: walk the
            // attachment's own joins (capabilities -> bindings ->
            // action policies -> commands -> effects) and the
            // compilation's declared scenarios covering those commands.
            let capabilities: Vec<&str> = attachment
                .capability_requirements()
                .iter()
                .filter(|capability| capability.contract_id == provider_contract.contract_id)
                .map(|capability| capability.capability_id.as_str())
                .collect();
            let mut consumers: Vec<(&'static str, String)> = Vec::new();
            consumers.push(("provider-contract", contract.clone()));
            for capability in attachment.capability_requirements() {
                if capabilities.contains(&capability.capability_id.as_str()) {
                    consumers.push((
                        "capability",
                        format!(
                            "{}@{}",
                            capability.capability_id, provider_contract.contract_id
                        ),
                    ));
                }
            }
            for binding in attachment.operation_bindings() {
                if capabilities.contains(&binding.capability_ref.as_str()) {
                    consumers.push(("binding", binding.operation.clone()));
                    if let Some(policy) = attachment
                        .action_policies()
                        .iter()
                        .find(|policy| policy.policy_id == binding.action_policy_ref)
                    {
                        consumers.push(("action-policy", policy.policy_id.clone()));
                        consumers.push(("command", policy.action.clone()));
                    }
                    consumers.push(("effect", binding.effect_ref.clone()));
                }
            }
            // Verification scenarios: every declared scenario that
            // covers a consuming command witnesses this contract.
            let commands: std::collections::BTreeSet<String> = consumers
                .iter()
                .filter(|(role, _)| *role == "command")
                .map(|(_, symbol)| symbol.clone())
                .collect();
            for definition in &compilation.project.definitions {
                if let lekalo_core::ir::Definition::Scenario(scenario) = definition {
                    if scenario
                        .covers
                        .iter()
                        .any(|symbol| commands.contains(symbol.as_str()))
                    {
                        consumers.push(("scenario", scenario.id.as_str().to_owned()));
                    }
                }
            }
            consumers
                .sort_by(|left, right| (left.0, left.1.as_str()).cmp(&(right.0, right.1.as_str())));
            consumers.dedup();
            // The exact member paths: every request/response/error field
            // of the contract's referenced schemas, each with its DERIVED
            // reverse consumer chain — request members are consumed by
            // every operation bound to the contract (they build the
            // request) and, when any consumer commits a durable outbox
            // intent, by the delivery-surface worker; response/error
            // members are consumed by the delivery surface that records
            // receipts and reconciles uncertainty; both routes re-verify
            // on every declared target binding and are witnessed by the
            // covering verification scenarios. Derived from the live
            // compilation and the attachment's own joins, never a
            // prewritten list.
            let delivery_surface = ["tasklinks.dispatch_outbox", "tasklinks.ack_delivery"];
            let outbox_entity = "tasklinks.outbox_item";
            let commands_of_consumers: std::collections::BTreeSet<String> = consumers
                .iter()
                .filter(|(role, _)| *role == "command")
                .map(|(_, symbol)| symbol.clone())
                .collect();
            let command_writes_outbox = |command: &str| {
                compilation
                    .project
                    .definitions
                    .iter()
                    .any(|definition| match definition {
                        lekalo_core::ir::Definition::Command(candidate)
                            if candidate.id.as_str() == command =>
                        {
                            candidate.effects.iter().any(|effect| {
                                compilation.project.definitions.iter().any(|definition| {
                                    match definition {
                                        lekalo_core::ir::Definition::Effect(candidate)
                                            if candidate.id.as_str() == effect.as_str() =>
                                        {
                                            candidate.entity.as_str() == outbox_entity
                                        }
                                        _ => false,
                                    }
                                })
                            })
                        }
                        _ => false,
                    })
            };
            let worker_witnesses_outbox = commands_of_consumers
                .iter()
                .any(|command| command_writes_outbox(command));
            // These pilot worker roles must resolve to actual commands
            // writing the same durable outbox, not just known names.
            let workers: Vec<&str> = delivery_surface
                .iter()
                .copied()
                .filter(|command| worker_witnesses_outbox && command_writes_outbox(command))
                .collect();
            let targets: Vec<(&str, &str)> = compilation
                .project
                .definitions
                .iter()
                .filter_map(|definition| match definition {
                    lekalo_core::ir::Definition::TargetBinding(binding)
                        if workers.iter().any(|worker| {
                            worker.split('.').next() == binding.id.as_str().split('.').next()
                        }) =>
                    {
                        Some((binding.id.as_str(), binding.target.as_str()))
                    }
                    _ => None,
                })
                .collect();
            let scenarios_covering = |wanted: &dyn Fn(&str) -> bool| -> Vec<String> {
                compilation
                    .project
                    .definitions
                    .iter()
                    .filter_map(|definition| match definition {
                        lekalo_core::ir::Definition::Scenario(scenario)
                            if scenario.covers.iter().any(|symbol| wanted(symbol.as_str())) =>
                        {
                            Some(scenario.id.as_str().to_owned())
                        }
                        _ => None,
                    })
                    .collect()
            };
            let verification_scenarios: Vec<String> = {
                let mut merged =
                    scenarios_covering(&|symbol| commands_of_consumers.contains(symbol));
                merged.extend(scenarios_covering(&|symbol| workers.contains(&symbol)));
                merged.sort();
                merged.dedup();
                merged
            };
            // Provider bindings consume the whole request/response/error
            // contract. These are conservative schema-consumer paths,
            // not a claim that individual implementation field reads ran.
            let path_consumers: Vec<serde_json::Value> = consumers.iter()
                .map(|(role, symbol)| serde_json::json!({"role": role, "symbol": symbol}))
                .chain(workers.iter().map(|command|
                    serde_json::json!({"role": "worker", "symbol": command})))
                .chain(verification_scenarios.iter().map(|scenario|
                    serde_json::json!({"role": "verification", "symbol": scenario})))
                .chain(targets.iter().map(|(binding, target)|
                    serde_json::json!({"role": "target-binding", "symbol": binding, "target": target})))
                .collect();
            let mut member_paths = Vec::new();
            let mut member_fields_resolved = true;
            for (kind, schema_ref) in [
                ("request", provider_contract.request_schema_ref.as_str()),
                ("response", provider_contract.response_schema_ref.as_str()),
                ("error", provider_contract.error_schema_ref.as_str()),
            ] {
                let fields: Option<Vec<String>> = compilation.project.definitions.iter().find_map(
                    |definition| match definition {
                        lekalo_core::ir::Definition::ValueObject(value_object)
                            if value_object.id.as_str() == schema_ref =>
                        {
                            Some(
                                value_object
                                    .fields
                                    .iter()
                                    .map(|field| field.name.as_str().to_owned())
                                    .collect(),
                            )
                        }
                        _ => None,
                    },
                );
                match fields {
                    None => {
                        member_fields_resolved = false;
                        member_paths.push(serde_json::json!({
                            "kind": kind,
                            "schema": schema_ref,
                            "resolved": false,
                        }));
                    }
                    Some(fields) => {
                        member_fields_resolved &= !fields.is_empty();
                        for field in fields {
                            member_paths.push(serde_json::json!({
                                "kind": kind,
                                "schema": schema_ref,
                                "field": field,
                                "resolved": true,
                                "consumers": path_consumers,
                            }));
                        }
                    }
                }
            }
            // Completeness is bounded to the declared dependency paths.
            // Every required leg must actually resolve; a schema alone
            // cannot manufacture a worker, binding or verification check.
            let mut gaps = Vec::new();
            for (missing, consumer, detail) in [
                (
                    !member_fields_resolved,
                    "contract-member-paths",
                    "member fields are unavailable",
                ),
                (
                    commands_of_consumers.is_empty(),
                    "operation-policy",
                    "no consuming operation/policy join",
                ),
                (
                    workers.len() != delivery_surface.len(),
                    "outbox-worker",
                    "delivery workers do not resolve to the consumer outbox",
                ),
                (
                    targets.is_empty(),
                    "native-binding",
                    "no target binding in the worker module",
                ),
                (
                    commands_of_consumers
                        .iter()
                        .map(String::as_str)
                        .chain(workers.iter().copied())
                        .any(|command| scenarios_covering(&|symbol| symbol == command).is_empty()),
                    "verification-check",
                    "a consumer or worker has no covering verification scenario",
                ),
            ] {
                if missing {
                    gaps.push(serde_json::json!({"consumer": consumer, "detail": detail}));
                }
            }
            let witnessed: std::collections::BTreeSet<&str> = path_consumers
                .iter()
                .filter_map(|consumer| consumer["role"].as_str())
                .collect();
            let contract_members = serde_json::json!({
                "request": provider_contract.request_schema_ref,
                "response": provider_contract.response_schema_ref,
                "error": provider_contract.error_schema_ref,
            });
            let coverage = serde_json::json!({
                "complete": gaps.is_empty(),
                "boundary": "declared-schema-consumers",
                "executionVerified": false,
                "witnessed": witnessed,
                "gaps": gaps,
            });
            let payload = serde_json::json!({
                "contract": provider_contract.contract_id,
                "upstreamPin": provider_contract.upstream_pin,
                "contractMembers": contract_members,
                "memberPaths": member_paths,
                "consumers": consumers
                    .into_iter()
                    .map(|(role, symbol)| serde_json::json!({"role": role, "symbol": symbol}))
                    .collect::<Vec<_>>(),
                "coverage": coverage,
            });
            Ok(tl::render(&payload))
        }
    }
}
