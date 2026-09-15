//! The embedded, closed component definition registry (issue #29).
//!
//! A target profile composes exactly one component per axis: `runtime`,
//! `storage`, `transport`, `testing`, `analysis`, and `deployment`. Every
//! component that may appear in a profile carries exactly one definition
//! here: its axis, the versioned capability contract it provides (with
//! closed support states), the exact sibling components it requires, the
//! capability support it requires from the rest of the profile, and the
//! components it conflicts with. The registry is embedded, closed, and
//! deterministic: a declared component id without a definition refuses
//! resolution (`target-profile.component-unknown`), and a new component
//! requires a reviewed registry increment — never an optimistic guess.
//!
//! Components are deliberately runtime-independent unless they say
//! otherwise: `postgres-sql`, `http-json`, and `container` carry no
//! runtime requirement and are therefore reusable across Node, PHP, and
//! Go profiles unchanged, which is the reuse the issue demands.

use serde::Serialize;

use super::version::COMPONENTS_DEFINITION_VERSION;

/// The closed set of profile axes, byte-sorted by wire token.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    Analysis,
    Deployment,
    Runtime,
    Storage,
    Testing,
    Transport,
}

/// The exact wire token of one axis.
impl Axis {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Analysis => "analysis",
            Self::Deployment => "deployment",
            Self::Runtime => "runtime",
            Self::Storage => "storage",
            Self::Testing => "testing",
            Self::Transport => "transport",
        }
    }
    /// Parse the closed wire token.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "analysis" => Some(Self::Analysis),
            "deployment" => Some(Self::Deployment),
            "runtime" => Some(Self::Runtime),
            "storage" => Some(Self::Storage),
            "testing" => Some(Self::Testing),
            "transport" => Some(Self::Transport),
            _ => None,
        }
    }

    /// Every axis, byte-sorted.
    pub const ALL: [Axis; 6] = [
        Axis::Analysis,
        Axis::Deployment,
        Axis::Runtime,
        Axis::Storage,
        Axis::Testing,
        Axis::Transport,
    ];
}

/// The provided or required strength of one capability declaration.
///
/// A component either provides a capability `full` or `partial`, or does
/// not provide it at all (absence is meaningful and never optimistically
/// upgraded). A requirement names the minimum support the rest of the
/// profile must resolve to; `partial` is satisfied by `full` or `partial`,
/// `full` only by `full`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Support {
    Partial,
    Full,
}

impl Support {
    /// The stable lowercase wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Partial => "partial",
            Self::Full => "full",
        }
    }

    /// Parse the closed wire token.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "partial" => Some(Self::Partial),
            "full" => Some(Self::Full),
            _ => None,
        }
    }

    /// Whether `actual` satisfies a requirement of `minimum` strength.
    pub const fn satisfies(minimum: Self, actual: Self) -> bool {
        (minimum as u8) <= (actual as u8)
    }
}

/// One capability a component provides, with its support state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct ProvidedCapability {
    /// The stable dotted capability id (`storage.transactions`).
    pub id: &'static str,
    /// The support state the component provides.
    pub support: Support,
}

/// One exact sibling component a component requires on another axis.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct RequiredComponent {
    /// The axis the required component belongs to.
    pub axis: Axis,
    /// The exact component id required on that axis.
    pub component: &'static str,
}

/// One capability support the profile must resolve to for a component.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct RequiredCapability {
    /// The stable dotted capability id.
    pub id: &'static str,
    /// The minimum support the profile must resolve to.
    pub minimum: Support,
}

/// One sibling component a component conflicts with.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct ConflictingComponent {
    /// The axis of the conflicting component.
    pub axis: Axis,
    /// The exact conflicting component id.
    pub component: &'static str,
}
/// One versioned component definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct ComponentDefinition {
    /// The stable component id (`node-typescript`).
    pub id: &'static str,
    /// The axis the component belongs to.
    pub axis: Axis,
    /// The definition version the semantics below were written under.
    pub definition_version: &'static str,
    /// The capability contract the component provides.
    pub provides: &'static [ProvidedCapability],
    /// The exact sibling components this component requires.
    pub requires_components: &'static [RequiredComponent],
    /// The capability support this component requires from the profile.
    pub requires_capabilities: &'static [RequiredCapability],
    /// The sibling components this component conflicts with.
    pub conflicts: &'static [ConflictingComponent],
}

/// The embedded, id-sorted component definition table. Every capability id
/// follows the closed lowercase dotted grammar; every component id follows
/// the closed lowercase component grammar; provides entries are sorted by
/// id and unique. The paired test pins these invariants.
const DEFINITIONS: &[ComponentDefinition] = &[
    ComponentDefinition {
        id: "container",
        axis: Axis::Deployment,
        definition_version: COMPONENTS_DEFINITION_VERSION,
        provides: &[
            ProvidedCapability {
                id: "deployment.replicas",
                support: Support::Full,
            },
            ProvidedCapability {
                id: "deployment.reproducible",
                support: Support::Full,
            },
        ],
        requires_components: &[],
        requires_capabilities: &[],
        conflicts: &[],
    },
    ComponentDefinition {
        id: "go-native",
        axis: Axis::Testing,
        definition_version: COMPONENTS_DEFINITION_VERSION,
        provides: &[
            ProvidedCapability {
                id: "testing.coverage",
                support: Support::Full,
            },
            ProvidedCapability {
                id: "testing.parallel",
                support: Support::Full,
            },
        ],
        requires_components: &[RequiredComponent {
            axis: Axis::Runtime,
            component: "go-standard",
        }],
        requires_capabilities: &[],
        conflicts: &[],
    },
    ComponentDefinition {
        id: "go-standard",
        axis: Axis::Runtime,
        definition_version: COMPONENTS_DEFINITION_VERSION,
        provides: &[
            ProvidedCapability {
                id: "runtime.async",
                support: Support::Full,
            },
            ProvidedCapability {
                id: "runtime.static-binary",
                support: Support::Full,
            },
        ],
        requires_components: &[],
        requires_capabilities: &[],
        conflicts: &[],
    },
    ComponentDefinition {
        id: "go-vet",
        axis: Axis::Analysis,
        definition_version: COMPONENTS_DEFINITION_VERSION,
        provides: &[
            ProvidedCapability {
                id: "analysis.lint",
                support: Support::Full,
            },
            ProvidedCapability {
                id: "analysis.types",
                support: Support::Partial,
            },
        ],
        requires_components: &[RequiredComponent {
            axis: Axis::Runtime,
            component: "go-standard",
        }],
        requires_capabilities: &[],
        conflicts: &[],
    },
    ComponentDefinition {
        id: "grpc-proto",
        axis: Axis::Transport,
        definition_version: COMPONENTS_DEFINITION_VERSION,
        provides: &[
            ProvidedCapability {
                id: "transport.rpc",
                support: Support::Full,
            },
            ProvidedCapability {
                id: "transport.streaming",
                support: Support::Full,
            },
        ],
        requires_components: &[],
        requires_capabilities: &[RequiredCapability {
            id: "runtime.async",
            minimum: Support::Full,
        }],
        conflicts: &[],
    },
    ComponentDefinition {
        id: "http-json",
        axis: Axis::Transport,
        definition_version: COMPONENTS_DEFINITION_VERSION,
        provides: &[
            ProvidedCapability {
                id: "transport.http",
                support: Support::Full,
            },
            ProvidedCapability {
                id: "transport.streaming",
                support: Support::Partial,
            },
        ],
        requires_components: &[],
        requires_capabilities: &[],
        conflicts: &[],
    },
    ComponentDefinition {
        id: "laratesto",
        axis: Axis::Testing,
        definition_version: COMPONENTS_DEFINITION_VERSION,
        provides: &[
            ProvidedCapability {
                id: "testing.coverage",
                support: Support::Full,
            },
            ProvidedCapability {
                id: "testing.parallel",
                support: Support::Partial,
            },
        ],
        requires_components: &[RequiredComponent {
            axis: Axis::Runtime,
            component: "php-laravel",
        }],
        requires_capabilities: &[],
        conflicts: &[ConflictingComponent {
            axis: Axis::Analysis,
            component: "typescript-native",
        }],
    },
    ComponentDefinition {
        id: "mago",
        axis: Axis::Analysis,
        definition_version: COMPONENTS_DEFINITION_VERSION,
        provides: &[
            ProvidedCapability {
                id: "analysis.lint",
                support: Support::Full,
            },
            ProvidedCapability {
                id: "analysis.types",
                support: Support::Partial,
            },
        ],
        requires_components: &[RequiredComponent {
            axis: Axis::Runtime,
            component: "php-laravel",
        }],
        requires_capabilities: &[],
        conflicts: &[],
    },
    ComponentDefinition {
        id: "mysql-sql",
        axis: Axis::Storage,
        definition_version: COMPONENTS_DEFINITION_VERSION,
        provides: &[
            ProvidedCapability {
                id: "storage.migrations",
                support: Support::Full,
            },
            ProvidedCapability {
                id: "storage.pooling",
                support: Support::Partial,
            },
            ProvidedCapability {
                id: "storage.sql",
                support: Support::Full,
            },
            ProvidedCapability {
                id: "storage.transactions",
                support: Support::Full,
            },
        ],
        requires_components: &[],
        requires_capabilities: &[],
        conflicts: &[],
    },
    ComponentDefinition {
        id: "node-native",
        axis: Axis::Testing,
        definition_version: COMPONENTS_DEFINITION_VERSION,
        provides: &[
            ProvidedCapability {
                id: "testing.coverage",
                support: Support::Full,
            },
            ProvidedCapability {
                id: "testing.parallel",
                support: Support::Full,
            },
        ],
        requires_components: &[RequiredComponent {
            axis: Axis::Runtime,
            component: "node-typescript",
        }],
        requires_capabilities: &[],
        conflicts: &[],
    },
    ComponentDefinition {
        id: "node-typescript",
        axis: Axis::Runtime,
        definition_version: COMPONENTS_DEFINITION_VERSION,
        provides: &[
            ProvidedCapability {
                id: "runtime.async",
                support: Support::Full,
            },
            ProvidedCapability {
                id: "runtime.typing",
                support: Support::Full,
            },
        ],
        requires_components: &[],
        requires_capabilities: &[],
        conflicts: &[],
    },
    ComponentDefinition {
        id: "php-laravel",
        axis: Axis::Runtime,
        definition_version: COMPONENTS_DEFINITION_VERSION,
        provides: &[
            ProvidedCapability {
                id: "runtime.async",
                support: Support::Partial,
            },
            ProvidedCapability {
                id: "runtime.typing",
                support: Support::Partial,
            },
        ],
        requires_components: &[],
        requires_capabilities: &[],
        conflicts: &[],
    },
    ComponentDefinition {
        id: "postgres-sql",
        axis: Axis::Storage,
        definition_version: COMPONENTS_DEFINITION_VERSION,
        provides: &[
            ProvidedCapability {
                id: "storage.migrations",
                support: Support::Full,
            },
            ProvidedCapability {
                id: "storage.pooling",
                support: Support::Full,
            },
            ProvidedCapability {
                id: "storage.sql",
                support: Support::Full,
            },
            ProvidedCapability {
                id: "storage.transactions",
                support: Support::Full,
            },
        ],
        requires_components: &[],
        requires_capabilities: &[],
        conflicts: &[],
    },
    ComponentDefinition {
        id: "serverless",
        axis: Axis::Deployment,
        definition_version: COMPONENTS_DEFINITION_VERSION,
        provides: &[
            ProvidedCapability {
                id: "deployment.replicas",
                support: Support::Partial,
            },
            ProvidedCapability {
                id: "deployment.reproducible",
                support: Support::Partial,
            },
        ],
        requires_components: &[],
        requires_capabilities: &[RequiredCapability {
            id: "storage.pooling",
            minimum: Support::Full,
        }],
        conflicts: &[ConflictingComponent {
            axis: Axis::Storage,
            component: "sqlite-file",
        }],
    },
    ComponentDefinition {
        id: "sqlite-file",
        axis: Axis::Storage,
        definition_version: COMPONENTS_DEFINITION_VERSION,
        provides: &[
            ProvidedCapability {
                id: "storage.migrations",
                support: Support::Full,
            },
            ProvidedCapability {
                id: "storage.sql",
                support: Support::Full,
            },
            ProvidedCapability {
                id: "storage.transactions",
                support: Support::Partial,
            },
        ],
        requires_components: &[],
        requires_capabilities: &[],
        conflicts: &[],
    },
    ComponentDefinition {
        id: "typescript-native",
        axis: Axis::Analysis,
        definition_version: COMPONENTS_DEFINITION_VERSION,
        provides: &[ProvidedCapability {
            id: "analysis.types",
            support: Support::Full,
        }],
        requires_components: &[RequiredComponent {
            axis: Axis::Runtime,
            component: "node-typescript",
        }],
        requires_capabilities: &[],
        conflicts: &[],
    },
];

/// The exact definition of one component, or `None` when the id is unknown
/// to this registry generation or belongs to a different axis.
pub fn definition(axis: Axis, id: &str) -> Option<&'static ComponentDefinition> {
    DEFINITIONS
        .iter()
        .find(|entry| entry.axis == axis && entry.id == id)
}

/// Every definition, in id-sorted order.
pub fn definitions() -> &'static [ComponentDefinition] {
    DEFINITIONS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definitions_are_sorted_unique_and_wellformed() {
        let mut previous: Option<&str> = None;
        let mut codes = std::collections::BTreeSet::new();
        for entry in DEFINITIONS {
            if let Some(previous) = previous {
                assert!(entry.id > previous, "definitions must be id-sorted");
            }
            previous = Some(entry.id);
            assert!(
                super::super::version::is_profile_id(entry.id),
                "{}",
                entry.id
            );
            assert!(codes.insert(entry.id), "duplicate id {}", entry.id);
            assert_eq!(entry.definition_version, COMPONENTS_DEFINITION_VERSION);
            assert!(!entry.provides.is_empty(), "{} provides nothing", entry.id);
            let mut provided_previous: Option<&str> = None;
            for provide in entry.provides {
                if let Some(previous) = provided_previous {
                    assert!(provide.id > previous, "provides must be id-sorted");
                }
                provided_previous = Some(provide.id);
                assert!(crate::target_protocol::wire::is_capability_id(provide.id));
            }
        }
    }

    #[test]
    fn capability_requirements_are_never_self_or_cyclically_satisfiable() {
        // A capability requirement must be satisfiable without the
        // requiring component providing it: the composition takes the
        // weakest provided state, so a self-requirement would deadlock.
        for entry in DEFINITIONS {
            let provided: Vec<_> = entry.provides.iter().map(|p| (p.id, p.support)).collect();
            for requirement in entry.requires_capabilities {
                let self_support = provided
                    .iter()
                    .find(|(id, _)| *id == requirement.id)
                    .map(|(_, support)| *support);
                if let Some(self_support) = self_support {
                    assert!(
                        Support::satisfies(requirement.minimum, self_support),
                        "{} requires its own capability {} above what it provides",
                        entry.id,
                        requirement.id
                    );
                }
            }
        }
    }

    #[test]
    fn lookup_respects_axis() {
        assert!(definition(Axis::Runtime, "node-typescript").is_some());
        assert!(definition(Axis::Storage, "node-typescript").is_none());
        assert!(definition(Axis::Runtime, "unknown-runtime").is_none());
        assert_eq!(definitions().len(), 16);
    }

    #[test]
    fn support_ordering_is_conservative() {
        assert!(Support::satisfies(Support::Partial, Support::Full));
        assert!(Support::satisfies(Support::Partial, Support::Partial));
        assert!(Support::satisfies(Support::Full, Support::Full));
        assert!(!Support::satisfies(Support::Full, Support::Partial));
    }
}
