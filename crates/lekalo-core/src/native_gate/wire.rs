//! Strict decoding and independent validation of the native gate plan
//! (issue #48): the shape mirrors the schema document, the bounds are
//! enforced here, and the recorded digest is recomputed over the pinned
//! domain. Nothing consumes a plan that failed this validation.

use super::{plan_digest, NativePlan, PlanRejection, PLAN_SCHEMA_VERSION};

/// Maximum serialized plan bytes the host accepts (contract ceiling).
pub const MAX_PLAN_BYTES: usize = 8 * 1024 * 1024;
/// Maximum packages and edges (contract ceilings).
pub const MAX_PACKAGES: usize = 1024;
pub const MAX_EDGES: usize = 8192;
/// Maximum commands, argv entries, and argv bytes.
pub const MAX_COMMANDS: usize = 128;
pub const MAX_ARGV: usize = 64;
pub const MAX_ARGV_ELEMENT_BYTES: usize = 1024;
pub const MAX_ARGV_TOTAL_BYTES: usize = 16 * 1024;

fn is_sha256(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// Whether one string is the closed package id spelling (root=name).
pub fn package_id_is_valid(value: &str) -> bool {
    is_package_id(value)
}

fn is_package_id(value: &str) -> bool {
    let Some((root, name)) = value.split_once('=') else {
        return false;
    };
    let root_ok = !root.is_empty()
        && root.len() <= 64
        && root.split('/').all(|segment| {
            !segment.is_empty()
                && segment
                    .starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.')
                && segment.bytes().all(|b| {
                    b.is_ascii_lowercase()
                        || b.is_ascii_digit()
                        || b == b'.'
                        || b == b'-'
                        || b == b'_'
                })
        });
    root_ok && !name.is_empty() && name.len() <= 192
}

fn is_command_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && value.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-' || b == b'_'
        })
}

fn is_logical_path(value: &str) -> bool {
    crate::target_protocol::scopes::is_logical_path(value)
}

/// Whether one string is a grammatical env name: uppercase with digits
/// and underscores, exactly the shape the JSON schema requires.
fn is_env_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.starts_with(|c: char| c.is_ascii_uppercase())
        && value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

const GATE_KINDS: [&str; 4] = ["build", "typecheck", "lint", "test"];
/// The closed v0.4.0 gate-kind metadata set (issue #61): the gate
/// member stays the four-value execution class, the gate_kind member
/// distinguishes the Composer/Laravel semantics without inventing
/// values in the old enum.
pub const GATE_KIND_METADATA: [&str; 14] = [
    "composer-script",
    "mago-format",
    "mago-lint",
    "mago-analyze",
    "mago-guard",
    "laratesto",
    "pest",
    "phpunit",
    "artisan-check",
    "boot-smoke",
    "migration-static",
    "migration-execute",
    "discovery-smoke",
    "legacy-suite",
];
const REASON_KINDS: [&str; 6] = [
    "changed-package",
    "dependent-closure",
    "graph-bound-test",
    "build-prerequisite",
    "release-rule",
    "explicit-binding",
];
const EXCLUDED_REASONS: [&str; 5] = [
    "no-reason",
    "no-confirmation",
    "no-edge",
    "owner-unknown",
    "out-of-scope",
];
const MANAGERS: [&str; 6] = [
    "pnpm-workspace",
    "npm-standalone",
    "npm-workspaces",
    "yarn",
    "bun",
    "composer-project",
];

/// Whether one string is a closed gate-kind metadata value.
pub fn gate_kind_is_valid(value: &str) -> bool {
    GATE_KIND_METADATA.contains(&value)
}

/// Decode one native gate plan from canonical JSON bytes and validate it
/// independently: shape, bounds, enum closure, package/edge consistency,
/// and the recomputed digest. The bytes must be the canonical form.
pub fn decode_plan(bytes: &[u8]) -> Result<NativePlan, PlanRejection> {
    if bytes.len() > MAX_PLAN_BYTES {
        return Err(PlanRejection::Shape("plan-oversize"));
    }
    let plan: NativePlan = serde_json::from_slice(bytes).map_err(|_| PlanRejection::Malformed)?;
    validate_plan(&plan)?;
    Ok(plan)
}

/// Validate one decoded plan: every check the schema cannot express
/// (digest recomputation, cross-references, package/edge consistency).
pub fn validate_plan(plan: &NativePlan) -> Result<(), PlanRejection> {
    let shape = |detail: &'static str| Err(PlanRejection::Shape(detail));
    if plan.schema_version != PLAN_SCHEMA_VERSION || plan.kind != "native-plan" {
        return shape("schema-version");
    }
    if !is_sha256(&plan.plan_digest) {
        return shape("digest-spelling");
    }
    if plan.workspace.manager != "pnpm-workspace"
        && !MANAGERS.contains(&plan.workspace.manager.as_str())
    {
        return shape("manager");
    }
    if plan.workspace.packages.len() > MAX_PACKAGES || plan.workspace.edges.len() > MAX_EDGES {
        return shape("collection-bound");
    }
    if plan.commands.len() > MAX_COMMANDS {
        return shape("command-bound");
    }
    // Package ids are unique; package roots stay canonical.
    let mut package_ids = std::collections::BTreeSet::new();
    for package in &plan.workspace.packages {
        if !is_package_id(&package.id) || package_ids.contains(&package.id) {
            return shape("package-id");
        }
        if package.root != "." && !is_logical_path(&package.root) {
            return shape("package-root");
        }
        if !is_sha256(&package.manifest_digest) {
            return shape("package-manifest-digest");
        }
        package_ids.insert(package.id.clone());
    }
    for edge in &plan.workspace.edges {
        if !package_ids.contains(&edge.from) || !package_ids.contains(&edge.to) {
            return shape("edge-endpoint");
        }
        if edge.from == edge.to {
            return shape("edge-self-loop");
        }
    }
    // Affected entries reference real packages with closed reason kinds.
    for affected in &plan.affected {
        if !package_ids.contains(&affected.package_id) || affected.reasons.is_empty() {
            return shape("affected-entry");
        }
        for reason in &affected.reasons {
            if !REASON_KINDS.contains(&reason.kind.as_str()) {
                return shape("reason-kind");
            }
            if let Some(path) = &reason.edge_path {
                if path.len() > 32 || path.iter().any(|hop| !package_ids.contains(hop)) {
                    return shape("reason-edge-path");
                }
            }
        }
    }
    // The selection document (issue #61): mode must agree with the
    // top-level selection_mode, mandatory gate ids must name executed
    // commands, and exclusions/uncertainties reference real packages.
    if plan.selection.mode != plan.selection_mode {
        return shape("selection-mode-disagree");
    }
    if plan.selection.modules.len() > 256 || plan.selection.tests.len() > 256 {
        return shape("selection-bound");
    }
    if plan.selection.mandatory_gate_ids.len() > 128 {
        return shape("selection-bound");
    }
    for module in &plan.selection.modules {
        if !is_command_id(module) {
            return shape("selection-module");
        }
    }
    for suite in &plan.selection.tests {
        if !is_command_id(suite) {
            return shape("selection-suite");
        }
    }
    let executed_gate_ids: std::collections::BTreeSet<&str> = plan
        .commands
        .iter()
        .map(|command| command.gate_id.as_str())
        .collect();
    for gate_id in &plan.selection.mandatory_gate_ids {
        if !is_command_id(gate_id) || !executed_gate_ids.contains(gate_id.as_str()) {
            return shape("selection-mandatory-gate");
        }
    }
    for excluded in &plan.selection.excluded {
        if !package_ids.contains(&excluded.package_id)
            || !EXCLUDED_REASONS.contains(&excluded.reason.as_str())
        {
            return shape("selection-excluded");
        }
    }
    for uncertainty in &plan.selection.uncertainties {
        if let Some(package_id) = &uncertainty.package_id {
            if !package_ids.contains(package_id) {
                return shape("selection-uncertainty");
            }
        }
    }
    if let Some(rule_ref) = &plan.selection.fallback_rule_ref {
        if !is_sha256(rule_ref) {
            return shape("selection-fallback-ref");
        }
    }
    for excluded in &plan.excluded {
        if !package_ids.contains(&excluded.package_id)
            || !EXCLUDED_REASONS.contains(&excluded.reason.as_str())
        {
            return shape("excluded-entry");
        }
    }
    // Commands: direct argv only, bounded, referencing real packages.
    let mut command_ids = std::collections::BTreeSet::new();
    let mut gate_ids = std::collections::BTreeSet::new();
    for command in &plan.commands {
        if !is_command_id(&command.id) || command_ids.contains(&command.id) {
            return shape("command-id");
        }
        command_ids.insert(command.id.clone());
        if !is_command_id(&command.gate_id) || !gate_ids.insert(command.gate_id.clone()) {
            return shape("command-gate-id");
        }
        if !GATE_KIND_METADATA.contains(&command.gate_kind.as_str()) {
            return shape("command-gate-kind");
        }
        if !is_sha256(&command.selection_ref) {
            return shape("command-selection-ref");
        }
        // The selection reference must equal the digest the host
        // computes over the plan's own selection document: the
        // selection artifacts are pinned inside the approved plan.
        if command.selection_ref != super::selection_digest(&plan.selection) {
            return shape("command-selection-ref");
        }
        if command.covers_suite_ids.len() > 256 {
            return shape("command-suite-bound");
        }
        for suite in &command.covers_suite_ids {
            if !is_command_id(suite) {
                return shape("command-suite-id");
            }
        }
        if !package_ids.contains(&command.package_id) {
            return shape("command-package");
        }
        if !GATE_KINDS.contains(&command.gate.as_str()) {
            return shape("command-gate");
        }
        if command.cwd != "." && !is_logical_path(&command.cwd) {
            return shape("command-cwd");
        }
        if command.argv.is_empty() || command.argv.len() > MAX_ARGV {
            return shape("command-argv-count");
        }
        let mut argv_bytes = 0usize;
        for element in &command.argv {
            if element.is_empty() || element.len() > MAX_ARGV_ELEMENT_BYTES {
                return shape("command-argv-element");
            }
            // Shell metacharacters never survive into an approved argv.
            if element.bytes().any(|b| {
                matches!(
                    b,
                    b'|' | b'&'
                        | b';'
                        | b'<'
                        | b'>'
                        | b'$'
                        | b'`'
                        | b'\\'
                        | b'"'
                        | b'\''
                        | b'\n'
                        | b'\r'
                )
            }) {
                return shape("command-argv-shell");
            }
            argv_bytes += element.len();
        }
        if argv_bytes > MAX_ARGV_TOTAL_BYTES {
            return shape("command-argv-total");
        }
        if !is_sha256(&command.script_digest)
            || !is_sha256(&command.confirmation_ref)
            || !is_sha256(&command.read_manifest_ref)
        {
            return shape("command-digests");
        }
        if command.limits.timeout_ms_per_command > plan.limits.timeout_ms_per_run {
            return shape("command-limits");
        }
    }
    // Depends_on endpoints must resolve to declared command ids: a
    // dangling endpoint would order a command against nothing.
    for command in &plan.commands {
        for dependency in &command.depends_on {
            if !command_ids.contains(dependency) {
                return shape("command-depends-on");
            }
        }
    }
    // Trust: the plan never claims more than the accepted generations.
    if plan.trust.mode != "private"
        && plan.trust.mode != "untrusted"
        && plan.trust.mode != "public-fixture"
    {
        return shape("trust-mode");
    }
    // Env recipe cross-checks (the schema spells the grammars; the
    // approvals are cross-member): allowed names and binding names use
    // the closed env grammar, binding kinds come from the closed set,
    // and every command's env grants must ride the approved names.
    for name in &plan.env.allowed_names {
        if !is_env_name(name) {
            return shape("env-allowed-name");
        }
    }
    for binding in &plan.env.bindings {
        if !is_env_name(&binding.name) {
            return shape("env-binding-name");
        }
        if ![
            "literal",
            "execution-temp",
            "execution-home",
            "platform-system-root",
        ]
        .contains(&binding.kind.as_str())
        {
            return shape("env-binding-kind");
        }
    }
    for command in &plan.commands {
        for name in &command.env {
            if !is_env_name(name) || !plan.env.allowed_names.contains(name) {
                return shape("command-env-unapproved");
            }
        }
    }
    if !is_sha256(&plan.input_manifest_digest)
        || !is_sha256(&plan.tool_catalog_digest)
        || !is_sha256(&plan.capability_snapshot_digest)
    {
        return shape("custody-digests");
    }
    // Digest recomputation: the recorded digest must equal the value the
    // host computes over the canonical plan without the digest member.
    if plan_digest(plan) != plan.plan_digest {
        return Err(PlanRejection::DigestMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn golden_plan() -> NativePlan {
        let bytes = include_bytes!(
            "../../../../tests/fixtures/node-native-gates/protocol/plan.golden.json"
        );
        serde_json::from_slice(bytes).expect("golden plan decodes")
    }

    #[test]
    fn the_golden_plan_validates_and_its_digest_recomputes() {
        let plan = golden_plan();
        assert!(validate_plan(&plan).is_ok());
        assert_eq!(plan_digest(&plan), plan.plan_digest);
    }

    #[test]
    fn unknown_members_are_refused() {
        let mut value = serde_json::to_value(golden_plan()).unwrap();
        value["invented"] = serde_json::json!(1);
        let bytes = serde_json::to_vec(&value).unwrap();
        assert_eq!(decode_plan(&bytes).unwrap_err(), PlanRejection::Malformed);
    }

    #[test]
    fn digest_drift_is_refused_before_anything_consumes_the_plan() {
        let mut plan = golden_plan();
        plan.input_manifest_digest = format!("sha256:{}", "9".repeat(64));
        assert_eq!(
            validate_plan(&plan).unwrap_err(),
            PlanRejection::DigestMismatch
        );
    }

    #[test]
    fn shell_metacharacters_never_survive_into_a_command_argv() {
        let mut plan = golden_plan();
        plan.commands[0].argv.push("a|b".into());
        // The digest is recomputed after the tamper so the shell check is
        // what fires, not the digest mismatch.
        plan.plan_digest = plan_digest(&plan);
        assert_eq!(
            validate_plan(&plan).unwrap_err(),
            PlanRejection::Shape("command-argv-shell")
        );
    }

    #[test]
    fn absolute_cwd_and_unknown_gates_are_shape_refusals() {
        let mut plan = golden_plan();
        plan.commands[0].cwd = "C:/Users/User/evil".into();
        plan.plan_digest = plan_digest(&plan);
        assert_eq!(
            validate_plan(&plan).unwrap_err(),
            PlanRejection::Shape("command-cwd")
        );

        let mut plan = golden_plan();
        plan.commands[0].gate = "deploy".into();
        plan.plan_digest = plan_digest(&plan);
        assert_eq!(
            validate_plan(&plan).unwrap_err(),
            PlanRejection::Shape("command-gate")
        );
    }

    #[test]
    fn self_edges_and_unknown_endpoints_are_workspace_refusals() {
        let mut plan = golden_plan();
        plan.workspace.edges.push(crate::native_gate::NativeEdge {
            from: plan.workspace.packages[0].id.clone(),
            to: plan.workspace.packages[0].id.clone(),
            kind: crate::native_gate::NativeEdgeKind::Dependency,
            scope: None,
            specifier: None,
            provenance: "manifest-evidence".into(),
        });
        plan.plan_digest = plan_digest(&plan);
        assert_eq!(
            validate_plan(&plan).unwrap_err(),
            PlanRejection::Shape("edge-self-loop")
        );
    }

    #[test]
    fn the_composer_manager_is_accepted_only_in_the_successor() {
        // Issue #61: composer-project is the proposed manager of the
        // v0.4.0 successor — never a silent v0.3.2 enum broadening.
        let mut plan = golden_plan();
        plan.workspace.manager = "composer-project".into();
        plan.plan_digest = plan_digest(&plan);
        assert!(validate_plan(&plan).is_ok());
    }

    #[test]
    fn unknown_gate_kind_metadata_is_a_shape_refusal() {
        let mut plan = golden_plan();
        plan.commands[0].gate_kind = "deploy-special".into();
        plan.plan_digest = plan_digest(&plan);
        assert_eq!(
            validate_plan(&plan).unwrap_err(),
            PlanRejection::Shape("command-gate-kind")
        );
    }

    #[test]
    fn a_selection_ref_not_pinned_to_the_selection_document_is_refused() {
        let mut plan = golden_plan();
        plan.commands[0].selection_ref = format!("sha256:{}", "0".repeat(64));
        plan.plan_digest = plan_digest(&plan);
        assert_eq!(
            validate_plan(&plan).unwrap_err(),
            PlanRejection::Shape("command-selection-ref")
        );
    }

    #[test]
    fn a_selection_mode_disagreement_is_a_shape_refusal() {
        let mut plan = golden_plan();
        plan.selection.mode = "release-full".into();
        plan.selection.fallback_rule_ref = Some(format!("sha256:{}", "3".repeat(64)));
        plan.selection_mode = "targeted".into();
        plan.plan_digest = plan_digest(&plan);
        assert_eq!(
            validate_plan(&plan).unwrap_err(),
            PlanRejection::Shape("selection-mode-disagree")
        );
    }

    #[test]
    fn a_dangling_depends_on_endpoint_is_refused() {
        let mut plan = golden_plan();
        plan.commands[0].depends_on = vec!["never-planned-command".to_owned()];
        plan.plan_digest = plan_digest(&plan);
        assert_eq!(
            validate_plan(&plan).unwrap_err(),
            PlanRejection::Shape("command-depends-on")
        );
    }

    #[test]
    fn a_command_env_grant_outside_the_approved_names_is_refused() {
        let mut plan = golden_plan();
        plan.env.allowed_names = vec!["APPROVED_VAR".to_owned()];
        plan.commands[0].env = vec!["UNAPPROVED_VAR".to_owned()];
        plan.plan_digest = plan_digest(&plan);
        assert_eq!(
            validate_plan(&plan).unwrap_err(),
            PlanRejection::Shape("command-env-unapproved")
        );
        // The approved grant passes, and the env-name grammar is
        // enforced on the allowed-names list itself.
        plan.commands[0].env = vec!["APPROVED_VAR".to_owned()];
        plan.plan_digest = plan_digest(&plan);
        assert!(validate_plan(&plan).is_ok());
        plan.env.allowed_names = vec!["lowercase_var".to_owned()];
        plan.commands[0].env = vec![];
        plan.plan_digest = plan_digest(&plan);
        assert_eq!(
            validate_plan(&plan).unwrap_err(),
            PlanRejection::Shape("env-allowed-name")
        );
    }

    #[test]
    fn a_mandatory_gate_id_without_an_executed_command_is_refused() {
        let mut plan = golden_plan();
        plan.selection
            .mandatory_gate_ids
            .push("never-planned-gate".into());
        plan.plan_digest = plan_digest(&plan);
        assert_eq!(
            validate_plan(&plan).unwrap_err(),
            PlanRejection::Shape("selection-mandatory-gate")
        );
    }
}
