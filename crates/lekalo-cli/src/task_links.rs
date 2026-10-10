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

/// The verified native qualification: the executed legs and the exact
/// receipt digest, bound to the provisioned database that still exists.
#[derive(Clone, Debug, Eq, PartialEq)]
struct NativeQualification {
    legs: std::collections::BTreeSet<String>,
    receipt_digest: String,
}

/// The executed native legs the qualification receipt must name before
/// any native-origin evidence is admitted: the durable ack-window
/// restart, the durable attempt bound, and the DB-level identity race.
const NATIVE_QUALIFICATION_LEGS: [&str; 3] = [
    "accepted_before_ack_survives_restart_as_unknown",
    "attempt_bound_survives_restart",
    "synchronized_identity_race_binds_one_task",
];

/// The exact execution boundary the battery records: real SQLite with
/// separate connections and in-process recovery. Process termination
/// and maintained-port execution are NOT covered by this boundary, and
/// the admission says so by requiring the exact recorded spelling.
const NATIVE_EXECUTION_BOUNDARY: &str = "same-process-recovery-real-sqlite";
const NATIVE_RUNNER: &str = "cargo-test-lekalo-core-native_leg";

/// Whether the environment actually stands behind native-origin
/// evidence, and with which executed legs. Qualification requires:
/// the provisioning flag, a database path whose file EXISTS, and the
/// executed battery's receipt beside it — read through the same
/// bounded no-follow discipline as every other family input, parsed
/// with the duplicate-checked decoder, closed to its exact member set,
/// and naming the same database, the recorded execution boundary, and
/// every required leg. A forged flag, a missing database, a tampered
/// or hand-written receipt, or a duplicate-member receipt is a
/// refusal (`evidence-native-unqualified`), never a silent pass.
fn native_qualification() -> Option<NativeQualification> {
    if std::env::var("LEKALO_TASKLINKS_NATIVE").as_deref() != Ok("1") {
        return None;
    }
    let database = std::env::var("LEKALO_TASKLINKS_NATIVE_DB").ok()?;
    if database.is_empty() {
        return None;
    }
    // The provisioned database must actually exist AND be a real
    // SQLite database file (the magic header every SQLite database
    // carries): a receipt naming a database nobody ever created is not
    // custody, and neither is a marker file wearing the right name.
    let database_path = std::path::Path::new(&database);
    let database_meta = std::fs::metadata(database_path).ok()?;
    if !database_meta.is_file() {
        return None;
    }
    let mut header = [0u8; 16];
    {
        use std::io::Read as _;
        std::fs::File::open(database_path)
            .ok()?
            .read_exact(&mut header)
            .ok()?;
    }
    if !header.starts_with(b"SQLite format 3\0") {
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
    const RECEIPT_KEYS: [&str; 6] = [
        "schemaVersion",
        "identity",
        "database",
        "executionBoundary",
        "runner",
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
    {
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
    if legs.len() != legs_json.len() {
        return None;
    }
    for required in NATIVE_QUALIFICATION_LEGS {
        if !legs.contains(required) {
            return None;
        }
    }
    Some(NativeQualification {
        legs,
        receipt_digest: format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes)),
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
            // The verified qualification (executed legs + receipt digest)
            // rides into the report: native target-guarantee rows claim
            // support per executed leg and pin the exact receipt, never
            // the selected profile file.
            let native = native_qualification();
            let native_evidence =
                native
                    .as_ref()
                    .map(|qualification| tl::portability::NativeEvidence {
                        legs: qualification.legs.clone(),
                        receipt_digest: qualification.receipt_digest.clone(),
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
            // The witnessed boundary is honest about its own coverage:
            // the derived joins are real, but exact changed contract
            // MEMBER paths (request/response/error fields), the outbox
            // worker internals, the native target binding, and the
            // target verification checks are not witnessed by this
            // attachment, and the payload says so explicitly instead of
            // claiming a complete reverse path.
            let contract_members = serde_json::json!({
                "request": provider_contract.request_schema_ref,
                "response": provider_contract.response_schema_ref,
                "error": provider_contract.error_schema_ref,
            });
            let coverage = serde_json::json!({
                "complete": false,
                "witnessed": ["capability", "binding", "action-policy", "command", "effect", "scenario"],
                "gaps": [
                    {"consumer": "contract-member-paths", "detail": "exact changed request/response/error field members are not witnessed"},
                    {"consumer": "outbox-worker", "detail": "worker-internal consumption is not witnessed"},
                    {"consumer": "native-binding", "detail": "target native binding is not witnessed"},
                    {"consumer": "verification-check", "detail": "target verification checks are not witnessed"}
                ]
            });
            let payload = serde_json::json!({
                "contract": provider_contract.contract_id,
                "upstreamPin": provider_contract.upstream_pin,
                "contractMembers": contract_members,
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
