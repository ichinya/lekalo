//! The versioned capability definition registry (issue #28).
//!
//! Every named capability an adapter may declare on the 0.3.1 describe
//! response carries exactly one definition here: a stable dotted id, the
//! semantic meaning of its support states, and the definition version the
//! semantics were written under. The registry is embedded, closed, and
//! deterministic: a declared id without a definition is a response
//! refusal (the client cannot interpret what it cannot define), and a new
//! id requires a reviewed minor registry increment — never an optimistic
//! on-the-fly guess.

use serde::Serialize;

/// The identity of the embedded capability definition registry
/// (`dev.lekalo.target-capabilities@0.4.0`). The 0.4.0 generation
/// adds `generate.transport-http` and `verify.transport-http`
/// (issue #70).
pub const REGISTRY_IDENTITY: &str = "dev.lekalo.target-capabilities@0.4.0";

/// One versioned capability definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CapabilityDefinition {
    /// The stable dotted capability id (`scan.symbols`).
    pub id: &'static str,
    /// The definition version the semantics below were written under.
    pub definition_version: &'static str,
    /// The operation domain the capability belongs to.
    pub domain: &'static str,
    /// The human-readable semantic contract of the support states.
    pub semantics: &'static str,
}

/// The closed, id-sorted definition table. Ids follow the same closed
/// lowercase dotted grammar as the lock's `CapabilityId`; the paired test
/// pins the grammar and the sort order.
const DEFINITIONS: &[CapabilityDefinition] = &[
    CapabilityDefinition {
        id: "generate.openapi",
        definition_version: "0.3.1",
        domain: "generate",
        semantics: "Emits an OpenAPI document from the compiled project IR. `full` covers every declared operation and type; `partial` covers a declared subset; `unsupported` never emits; `unknown` is a declared state the core does not treat as available.",
    },
    CapabilityDefinition {
        id: "generate.transport-http",
        definition_version: "0.4.0",
        domain: "generate",
        semantics: "Generates the HTTP route layer from the transport-http evidence. `full` covers every declared endpoint, parameter, error projection, and security scheme; `partial` covers a declared subset or reports declared streaming/upload/download capabilities it does not implement as unsupported; `unsupported` never emits; `unknown` is a declared state the core does not treat as available.",
    },
    CapabilityDefinition {
        id: "generate.ui",
        definition_version: "0.3.1",
        domain: "generate",
        semantics: "Emits a user-interface projection from the compiled project IR. `full` covers every declared screen and component; `partial` covers a declared subset; `unsupported` never emits; `unknown` is a declared state the core does not treat as available.",
    },
    CapabilityDefinition {
        id: "generate.zod",
        definition_version: "0.3.1",
        domain: "generate",
        semantics: "Emits Zod schemas from the compiled project IR. `full` covers every declared type and invariant; `partial` covers a declared subset; `unsupported` never emits; `unknown` is a declared state the core does not treat as available.",
    },
    CapabilityDefinition {
        id: "scan.symbols",
        definition_version: "0.3.1",
        domain: "scan",
        semantics: "Enumerates project symbols through the `scan` operation. `full` covers every declared module and entity; `partial` covers a declared subset; `unsupported` never scans; `unknown` is a declared state the core does not treat as available.",
    },
    CapabilityDefinition {
        id: "verify.scenarios",
        definition_version: "0.3.1",
        domain: "verify",
        semantics: "Verifies scenario coverage through the `verify` operation. `full` covers every declared scenario; `partial` covers a declared subset; `unsupported` never verifies; `unknown` is a declared state the core does not treat as available.",
    },
    CapabilityDefinition {
        id: "verify.transport-http",
        definition_version: "0.4.0",
        domain: "verify",
        semantics: "Verifies black-box endpoint scenarios through the `verify` operation against the transport-http evidence. `full` executes every declared scenario coverage reference; `partial` covers a declared subset; `unsupported` never verifies; `unknown` is a declared state the core does not treat as available.",
    },
    CapabilityDefinition {
        id: "plan.native-gates",
        definition_version: "0.3.2",
        domain: "plan",
        semantics: "Plans native build/test gates over a detected Node workspace through the read-only `plan-native` operation. `full` produces the immutable plan over every confirmed gate; `partial` covers a declared subset; `unsupported` never plans; `unknown` is a declared state the core does not treat as available.",
    },
];

/// The exact definition of one capability id, or `None` when the id is
/// unknown to this registry generation.
pub fn definition(id: &str) -> Option<&'static CapabilityDefinition> {
    DEFINITIONS.iter().find(|entry| entry.id == id)
}

/// Every definition, in id-sorted order.
pub fn definitions() -> &'static [CapabilityDefinition] {
    DEFINITIONS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_identity_and_definitions_are_pinned() {
        assert_eq!(REGISTRY_IDENTITY, "dev.lekalo.target-capabilities@0.4.0");
        let ids: Vec<&str> = definitions().iter().map(|entry| entry.id).collect();
        assert_eq!(
            ids,
            vec![
                "generate.openapi",
                "generate.transport-http",
                "generate.ui",
                "generate.zod",
                "scan.symbols",
                "verify.scenarios",
                "verify.transport-http",
                "plan.native-gates",
            ]
        );
        for entry in definitions() {
            assert!(
                entry.definition_version == "0.3.1"
                    || entry.definition_version == "0.3.2"
                    || entry.definition_version == "0.4.0",
                "definition versions stay on the accepted generations"
            );
            assert!(
                crate::target_protocol::wire::is_capability_id(entry.id),
                "every defined id satisfies the wire grammar"
            );
        }
    }

    #[test]
    fn lookup_is_exact_and_total_over_the_table() {
        assert_eq!(definition("scan.symbols").unwrap().domain, "scan");
        assert_eq!(definition("generate.ui").unwrap().domain, "generate");
        assert!(definition("Scan.symbols").is_none(), "ids are exact");
        assert!(definition("scan").is_none(), "no prefix matching");
        assert!(definition("generate.rust-docs").is_none());
        assert!(definition("").is_none());
    }
}
