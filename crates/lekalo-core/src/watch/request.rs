//! The typed `lekalo watch` request (issue #95): the selected stages,
//! the module/target selectors, the timing policy, and the backend
//! selection. Pure data with total bound validation; the CLI maps its
//! arguments onto this and the core refuses every out-of-bound
//! combination as a usage error before any subscription starts.

use crate::result::DomainResult;

/// The default coalescing quiet window in milliseconds.
pub const DEFAULT_DEBOUNCE_MS: u64 = 150;
/// The default maximum wait before a coalesced batch must run.
pub const DEFAULT_MAX_WAIT_MS: u64 = 1_000;
/// The default poll backend interval in milliseconds.
pub const DEFAULT_POLL_INTERVAL_MS: u64 = 1_000;
/// The default independent reconciliation interval in milliseconds.
pub const DEFAULT_RECONCILE_MS: u64 = 5_000;
/// The default read-only adapter request deadline in milliseconds.
pub const DEFAULT_ADAPTER_TIMEOUT_MS: u64 = 30_000;

/// The hard bound family of the watch session (research contract):
/// every bound is explicit, tested, and fails observably.
pub mod limits {
    /// Minimum accepted debounce window.
    pub const MIN_DEBOUNCE_MS: u64 = 1;
    /// Maximum accepted debounce window.
    pub const MAX_DEBOUNCE_MS: u64 = 10_000;
    /// Minimum accepted maximum wait.
    pub const MIN_MAX_WAIT_MS: u64 = 1;
    /// Maximum accepted maximum wait.
    pub const MAX_MAX_WAIT_MS: u64 = 60_000;
    /// Minimum accepted poll interval.
    pub const MIN_POLL_INTERVAL_MS: u64 = 100;
    /// Maximum accepted poll interval.
    pub const MAX_POLL_INTERVAL_MS: u64 = 60_000;
    /// Minimum accepted reconciliation interval.
    pub const MIN_RECONCILE_MS: u64 = 100;
    /// Maximum accepted reconciliation interval.
    pub const MAX_RECONCILE_MS: u64 = 60_000;
    /// Minimum accepted adapter deadline.
    pub const MIN_ADAPTER_TIMEOUT_MS: u64 = 100;
    /// Maximum accepted adapter deadline.
    pub const MAX_ADAPTER_TIMEOUT_MS: u64 = 120_000;
    /// Maximum distinct pending paths before the batch collapses to an
    /// explicit full-rescan marker.
    pub const MAX_PENDING_PATHS: usize = 4_096;
    /// Maximum queued serialized output bytes before the producer must
    /// backpressure or terminate.
    pub const MAX_OUTPUT_QUEUE_BYTES: usize = 16 * 1024 * 1024;
    /// Maximum watched directories.
    pub const MAX_WATCHED_DIRS: usize = 4_096;
    /// Maximum captured input bytes across one snapshot.
    pub const MAX_CAPTURED_BYTES: u64 = 128 * 1024 * 1024;
    /// Maximum retained accepted-result state bytes.
    pub const MAX_RESULT_STATE_BYTES: usize = 64 * 1024 * 1024;
    /// Maximum bounded capture attempts before an unstable snapshot is
    /// reported instead of committed.
    pub const MAX_CAPTURE_ATTEMPTS: usize = 3;
    /// Closed result pins admit at most eight target adapters.
    pub const MAX_TARGETS: usize = 8;
    /// Retained module partitions plus one mandatory global work row.
    pub const MAX_TARGET_PARTITIONS: usize = 4096;
}

/// One independently selected watch stage. With no stage flag the
/// default is core validation only; selecting any stage runs exactly
/// that selection plus its mandatory preparation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WatchStages {
    /// The semantic validation stage (default when nothing is selected).
    pub validate: bool,
    /// The affected-symbol impact stage.
    pub impact: bool,
    /// The generated-artifact drift stage (read-only manifest check).
    pub generate_check: bool,
}

impl WatchStages {
    /// The default bare-watch selection: validation only.
    pub const fn default_selection() -> Self {
        Self {
            validate: true,
            impact: false,
            generate_check: false,
        }
    }

    /// Whether any stage was explicitly selected.
    pub const fn any(&self) -> bool {
        self.validate || self.impact || self.generate_check
    }

    /// Whether nothing at all is selected (maps to the default).
    pub const fn none(&self) -> bool {
        !self.any()
    }

    /// The wire spellings of the selected stages in fixed order.
    pub fn selected(&self) -> Vec<&'static str> {
        let mut stages = Vec::new();
        if self.validate {
            stages.push("validate");
        }
        if self.impact {
            stages.push("impact");
        }
        if self.generate_check {
            stages.push("generate-check");
        }
        stages
    }
}

/// The filesystem subscription backend.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WatchBackend {
    /// The recommended native backend with an explicit visible fallback
    /// to polling when native cannot run.
    #[default]
    Auto,
    /// Native only; a native refusal is terminal.
    Native,
    /// Content-hash polling only.
    Poll,
}

impl WatchBackend {
    /// Parse the closed CLI vocabulary.
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "auto" => Some(Self::Auto),
            "native" => Some(Self::Native),
            "poll" => Some(Self::Poll),
            _ => None,
        }
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Native => "native",
            Self::Poll => "poll",
        }
    }
}

/// One watch request: everything the pure scheduler and the host need.
#[derive(Clone, Debug)]
pub struct WatchRequest {
    /// Project root selector, relative to the invocation directory.
    pub project: Option<String>,
    /// The selected stages.
    pub stages: WatchStages,
    /// The semantic module selector (`--module planner`); the watch
    /// scope is this module plus its dependency/public obligation
    /// closure. Not a filesystem path.
    pub module: Option<String>,
    /// Explicitly selected locked targets for targeted validation.
    pub targets: Vec<String>,
    /// Bypass all in-memory fragment reuse for a clean oracle run.
    pub no_cache: bool,
    /// Strengthen the lock preflight; watch never updates the lock.
    pub locked: bool,
    /// Select the strict validation profile.
    pub strict: bool,
    /// The coalescing quiet window.
    pub debounce_ms: u64,
    /// The maximum wait before a busy batch must run.
    pub max_wait_ms: u64,
    /// The subscription backend.
    pub backend: WatchBackend,
    /// The poll backend interval.
    pub poll_interval_ms: u64,
    /// The independent reconciliation interval.
    pub reconcile_ms: u64,
    /// The read-only adapter request deadline.
    pub adapter_timeout_ms: u64,
}

impl Default for WatchRequest {
    fn default() -> Self {
        Self {
            project: None,
            stages: WatchStages::default_selection(),
            module: None,
            targets: Vec::new(),
            no_cache: false,
            locked: false,
            strict: false,
            debounce_ms: DEFAULT_DEBOUNCE_MS,
            max_wait_ms: DEFAULT_MAX_WAIT_MS,
            backend: WatchBackend::Auto,
            poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
            reconcile_ms: DEFAULT_RECONCILE_MS,
            adapter_timeout_ms: DEFAULT_ADAPTER_TIMEOUT_MS,
        }
    }
}

impl WatchRequest {
    /// Validate every bound. Any violation is a usage error before any
    /// subscription, backend, or run starts.
    pub fn validate(&self) -> Result<(), DomainResult> {
        use limits::*;
        if self
            .module
            .as_ref()
            .is_some_and(|id| !crate::ir::grammar::is_segment(id))
            || self.targets.len() > MAX_TARGETS
            || self
                .targets
                .iter()
                .any(|id| !crate::ir::grammar::is_target_name(id))
            || self
                .targets
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.targets.len()
            || self.stages.none()
            || (!self.targets.is_empty() && !self.stages.validate)
        {
            return Err(DomainResult::usage_error());
        }
        if !(MIN_DEBOUNCE_MS..=MAX_DEBOUNCE_MS).contains(&self.debounce_ms) {
            return Err(DomainResult::usage_error());
        }
        if !(MIN_MAX_WAIT_MS..=MAX_MAX_WAIT_MS).contains(&self.max_wait_ms) {
            return Err(DomainResult::usage_error());
        }
        if self.max_wait_ms < self.debounce_ms {
            return Err(DomainResult::usage_error());
        }
        if !(MIN_POLL_INTERVAL_MS..=MAX_POLL_INTERVAL_MS).contains(&self.poll_interval_ms) {
            return Err(DomainResult::usage_error());
        }
        if !(MIN_RECONCILE_MS..=MAX_RECONCILE_MS).contains(&self.reconcile_ms) {
            return Err(DomainResult::usage_error());
        }
        if !(MIN_ADAPTER_TIMEOUT_MS..=MAX_ADAPTER_TIMEOUT_MS).contains(&self.adapter_timeout_ms) {
            return Err(DomainResult::usage_error());
        }
        Ok(())
    }
}

#[cfg(test)]
mod target_limit_tests {
    use super::*;
    #[test]
    fn target_count_matches_the_closed_stream_bound() {
        let mut request = WatchRequest {
            targets: (0..limits::MAX_TARGETS)
                .map(|i| format!("target-{i}"))
                .collect(),
            ..WatchRequest::default()
        };
        assert!(request.validate().is_ok());
        request.targets.push("target-overflow".into());
        assert!(request.validate().is_err());
    }
}
