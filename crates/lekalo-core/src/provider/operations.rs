//! The closed operation vocabulary of the issue #34 workflow provider.
//!
//! One variant per AIFHub `/aif-*` provider operation. Detection is not
//! an operation: it is the manifest plus the installed-tool check the
//! consumer performs. Every operation carries its effect class, its
//! output schema identity, and its bounded option set, so a consumer
//! can negotiate and construct argv without ever executing
//! provider-returned command text.
#![allow(non_snake_case)] // wire field names are the published contract

use serde::Serialize;

/// The closed effect classes. `read-only` operations never write; a
/// violation is a conformance defect, not a policy decision.
/// `generated-artifacts` operations write only into the governed
/// `.lekalo/**` generated/lock custody and never into OpenSpec/HLV
/// canonical paths.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EffectClass {
    ReadOnly,
    GeneratedArtifacts,
}

impl EffectClass {
    /// The stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::GeneratedArtifacts => "generated-artifacts",
        }
    }
}

/// One advertised workflow operation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Operation {
    /// The stable operation id the consumer selects.
    id: &'static str,
    /// The effect class of the underlying command.
    effect: EffectClass,
    /// The output schema identity of the success payload: the exact
    /// `lekalo/.../v<version>` discriminator on the wire.
    outputSchema: &'static str,
    /// The recognized native command (presentation form, operands in
    /// `docs/provider-contract.md`).
    command: &'static str,
    /// Whether the command requires a validated project selection.
    requiresProject: bool,
    /// Whether the command requires an explicit target adapter
    /// program.
    requiresAdapter: bool,
}

impl Operation {
    /// The stable operation id.
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// The effect class.
    pub const fn effect(&self) -> EffectClass {
        self.effect
    }

    /// The output schema identity.
    pub const fn output_schema(&self) -> &'static str {
        self.outputSchema
    }

    /// The recognized native command.
    pub const fn command(&self) -> &'static str {
        self.command
    }

    /// Whether a validated project selection is required.
    pub const fn requires_project(&self) -> bool {
        self.requiresProject
    }

    /// Whether an explicit target adapter program is required.
    pub const fn requires_adapter(&self) -> bool {
        self.requiresAdapter
    }
}

/// The advertised operations in canonical (sorted) order. The exact
/// order is part of the closed manifest contract.
pub const OPERATIONS: [Operation; crate::provider::version::OPERATION_COUNT] = [
    Operation {
        id: "context",
        effect: EffectClass::ReadOnly,
        outputSchema: crate::context::version::SCHEMA_VERSION,
        command: "lekalo context --changed SYMBOLS --budget TOKENS",
        requiresProject: true,
        requiresAdapter: false,
    },
    Operation {
        id: "doctor",
        effect: EffectClass::ReadOnly,
        outputSchema: crate::doctor::version::SCHEMA_VERSION,
        command: "lekalo doctor [--trace PATH]...",
        requiresProject: true,
        requiresAdapter: false,
    },
    Operation {
        id: "generate",
        effect: EffectClass::GeneratedArtifacts,
        outputSchema: crate::orchestration::SCHEMA_VERSION,
        command: "lekalo generate --target TARGET [--dry-run] [--locked] -- PROGRAM [ARGS...]",
        requiresProject: true,
        requiresAdapter: true,
    },
    Operation {
        id: "impact",
        effect: EffectClass::ReadOnly,
        outputSchema: crate::impact::SCHEMA_VERSION,
        command: "lekalo impact --changed --base REF [--head REF] | --worktree",
        requiresProject: true,
        requiresAdapter: false,
    },
    Operation {
        id: "readiness",
        effect: EffectClass::ReadOnly,
        outputSchema: crate::doctor::version::SCHEMA_VERSION,
        command: "lekalo readiness --phase implement|generate|verify|release|done",
        requiresProject: true,
        requiresAdapter: false,
    },
    Operation {
        id: "status",
        effect: EffectClass::ReadOnly,
        outputSchema: crate::doctor::version::SCHEMA_VERSION,
        command: "lekalo status",
        requiresProject: true,
        requiresAdapter: false,
    },
    Operation {
        id: "trace.export",
        effect: EffectClass::ReadOnly,
        outputSchema: crate::trace::version::SCHEMA_VERSION,
        command: "lekalo trace export PATH",
        requiresProject: false,
        requiresAdapter: false,
    },
    Operation {
        id: "validate",
        effect: EffectClass::ReadOnly,
        outputSchema: crate::validator::profile::PROFILE_SCHEMA_VERSION,
        command: "lekalo validate [--module MODULE] [--strict]",
        requiresProject: true,
        requiresAdapter: false,
    },
    Operation {
        id: "verify",
        effect: EffectClass::ReadOnly,
        outputSchema: crate::orchestration::SCHEMA_VERSION,
        command: "lekalo verify [--target TARGET]... [--changed] [--locked] [--trace PATH]",
        requiresProject: true,
        requiresAdapter: false,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocabulary_is_closed_sorted_and_exactly_nine() {
        let ids: Vec<&str> = OPERATIONS.iter().map(|operation| operation.id()).collect();
        assert_eq!(
            ids,
            vec![
                "context",
                "doctor",
                "generate",
                "impact",
                "readiness",
                "status",
                "trace.export",
                "validate",
                "verify"
            ]
        );
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted, "canonical order is sorted");
        assert_eq!(OPERATIONS.len(), crate::provider::version::OPERATION_COUNT);
    }

    #[test]
    fn no_hidden_lifecycle_operation_is_advertised() {
        for operation in &OPERATIONS {
            let id = operation.id();
            assert!(
                !matches!(
                    id,
                    "init" | "install" | "update" | "sync" | "cleanup" | "migrate"
                ),
                "operation {id} must never be advertised"
            );
        }
    }

    #[test]
    fn generation_is_the_only_mutating_operation() {
        for operation in &OPERATIONS {
            if operation.id() == "generate" {
                assert_eq!(operation.effect(), EffectClass::GeneratedArtifacts);
                assert!(operation.requires_adapter());
            } else {
                assert_eq!(
                    operation.effect(),
                    EffectClass::ReadOnly,
                    "operation {} must be read-only",
                    operation.id()
                );
            }
        }
    }

    #[test]
    fn schema_identities_are_pinned_to_current_contract_families() {
        assert_eq!(
            OPERATIONS
                .iter()
                .find(|operation| operation.id() == "context")
                .expect("context")
                .output_schema(),
            "lekalo/context/v0.2.16"
        );
        assert_eq!(
            OPERATIONS
                .iter()
                .find(|operation| operation.id() == "impact")
                .expect("impact")
                .output_schema(),
            "lekalo/impact/v0.2.16"
        );
        assert_eq!(
            OPERATIONS
                .iter()
                .find(|operation| operation.id() == "validate")
                .expect("validate")
                .output_schema(),
            "lekalo/validation-profile/v0.4.0"
        );
        assert_eq!(
            OPERATIONS
                .iter()
                .find(|operation| operation.id() == "verify")
                .expect("verify")
                .output_schema(),
            "lekalo/orchestration/v0.2.16"
        );
        assert_eq!(
            OPERATIONS
                .iter()
                .find(|operation| operation.id() == "generate")
                .expect("generate")
                .output_schema(),
            "lekalo/orchestration/v0.2.16"
        );
        for operation in &OPERATIONS {
            assert!(
                operation.output_schema().starts_with("lekalo/"),
                "schema identity must use the native discriminator form"
            );
        }
    }
}
