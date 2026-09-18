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

const GATE_KINDS: [&str; 4] = ["build", "typecheck", "lint", "test"];
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
const MANAGERS: [&str; 5] = [
    "pnpm-workspace",
    "npm-standalone",
    "npm-workspaces",
    "yarn",
    "bun",
];

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
    for excluded in &plan.excluded {
        if !package_ids.contains(&excluded.package_id)
            || !EXCLUDED_REASONS.contains(&excluded.reason.as_str())
        {
            return shape("excluded-entry");
        }
    }
    // Commands: direct argv only, bounded, referencing real packages.
    let mut command_ids = std::collections::BTreeSet::new();
    for command in &plan.commands {
        if !is_command_id(&command.id) || command_ids.contains(&command.id) {
            return shape("command-id");
        }
        command_ids.insert(command.id.clone());
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
    // Trust: the plan never claims more than the accepted generations.
    if plan.trust.mode != "private"
        && plan.trust.mode != "untrusted"
        && plan.trust.mode != "public-fixture"
    {
        return shape("trust-mode");
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
}
