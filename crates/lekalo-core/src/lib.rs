//! Target-neutral request and result contracts for Lekalo.

pub mod ir;
pub mod loader;
pub mod lockfile;
pub mod project_fs;
pub mod result;
pub mod versioning;

pub use result::{Capability, DomainResult, ReasonCode, Status, CAPABILITY_UNAVAILABLE, CLI_USAGE};

/// A syntactically valid request accepted by the foundation CLI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Request {
    Validate,
    Inspect { symbol: String },
    Impact { symbol: String },
    Context { symbol: String, budget: u64 },
}

impl Request {
    /// Return the capability this request will eventually invoke.
    pub const fn capability(&self) -> Capability {
        match self {
            Self::Validate => Capability::Validate,
            Self::Inspect { .. } => Capability::Inspect,
            Self::Impact { .. } => Capability::Impact,
            Self::Context { .. } => Capability::Context,
        }
    }

    /// Dispatch a request through the current foundation implementation.
    ///
    /// Issue #3 intentionally recognizes each command without implementing
    /// filesystem, model, graph, target, or provider behavior.
    pub fn dispatch(&self) -> DomainResult {
        DomainResult::unsupported(self.capability())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_request_dispatches_to_its_named_unavailable_capability() {
        let cases = [
            (Request::Validate, Capability::Validate),
            (
                Request::Inspect {
                    symbol: "planner.task".to_owned(),
                },
                Capability::Inspect,
            ),
            (
                Request::Impact {
                    symbol: "planner.task".to_owned(),
                },
                Capability::Impact,
            ),
            (
                Request::Context {
                    symbol: "planner.task".to_owned(),
                    budget: 512,
                },
                Capability::Context,
            ),
        ];

        for (request, capability) in cases {
            let result = request.dispatch();
            assert_eq!(request.capability(), capability);
            assert_eq!(result.status(), Status::Unsupported);
            assert_eq!(result.exit_code(), 4);
            assert_eq!(result.capability(), Some(capability));
            assert_eq!(
                result.reason_codes(),
                &[ReasonCode::capability_unavailable()]
            );
        }
    }
}
