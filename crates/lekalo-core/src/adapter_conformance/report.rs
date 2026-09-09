//! The conformance report wire: canonical JSON envelope and JUnit
//! projection (issue #31).
//!
//! Both projections are deterministic: no timestamps, no durations, no
//! host paths, no raw child output. Byte-identical inputs produce
//! byte-identical reports, which is what lets CI compare runs and what
//! makes the redaction check meaningful for the report itself.

use serde::Serialize;

use super::check::{CheckId, CheckOutcome, CheckState, Verdict, CATALOG};
use super::version::{self, Profile};

/// The verified-compatibility badge: issued only for the exact protocol
/// and IR versions this run actually verified, never for a declared
/// range or wildcard.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Badge {
    /// Whether the badge was issued.
    pub issued: bool,
    /// The exact negotiated protocol version, when issued.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<&'static str>,
    /// The exact IR contract version, when issued.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ir: Option<&'static str>,
}

/// The suite section of the report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SuiteSection {
    /// The published suite identity.
    pub identity: &'static str,
    /// The product version that ran the suite.
    pub version: &'static str,
    /// The selected battery profile.
    pub profile: &'static str,
}

/// The adapter section, carried from the verified handshake evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AdapterSection {
    /// The declared adapter id token.
    pub id: String,
    /// The declared adapter version.
    pub version: String,
    /// The declared adapter digest.
    pub digest: String,
}

/// The negotiated session section.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SessionSection {
    /// The exact negotiated protocol version.
    pub protocol: &'static str,
    /// The core IR contract version the session was qualified against.
    pub ir: &'static str,
    /// The digest over the canonical capability bytes.
    pub capability_digest: String,
    /// The effective per-exchange deadline.
    pub timeout_ms: u64,
    /// The effective determinism repetition count.
    pub repeats: u8,
}

/// One report row of the closed catalog.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReportCheck {
    /// The stable check id.
    pub id: &'static str,
    /// The check's failure class.
    pub class: &'static str,
    /// The outcome state.
    pub state: &'static str,
    /// The bounded detail or skip-reason token, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<&'static str>,
}

/// The outcome counts.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Counts {
    /// Passing checks.
    pub pass: usize,
    /// Failed checks.
    pub fail: usize,
    /// Skipped checks; a skip is never a pass.
    pub skipped: usize,
}

/// The full conformance report document.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ConformanceReport {
    /// The report wire discriminator.
    pub schema_version: &'static str,
    /// The suite section.
    pub suite: SuiteSection,
    /// The verified adapter identity.
    pub adapter: AdapterSection,
    /// The negotiated session.
    pub session: SessionSection,
    /// The aggregate verdict.
    pub verdict: &'static str,
    /// The verified-compatibility badge.
    pub badge: Badge,
    /// Every catalog row in fixed order.
    pub checks: Vec<ReportCheck>,
    /// The outcome counts.
    pub counts: Counts,
}

impl ConformanceReport {
    /// Assemble the report from the session evidence and outcomes. The
    /// badge is issued only for a passing run whose catalog has no
    /// skipped core row; it names the exact verified versions.
    pub fn build(
        suite_version: &'static str,
        profile: Profile,
        adapter: AdapterSection,
        session: SessionSection,
        outcomes: &[CheckOutcome],
    ) -> Self {
        let verdict = Verdict::derive(outcomes);
        let core_skipped = outcomes
            .iter()
            .any(|outcome| outcome.state == CheckState::Skipped && is_core(outcome.id));
        let issued = verdict == Verdict::Pass && !core_skipped;
        let badge = if issued {
            Badge {
                issued: true,
                protocol: Some(session.protocol),
                ir: Some(session.ir),
            }
        } else {
            Badge {
                issued: false,
                protocol: None,
                ir: None,
            }
        };
        let checks = CATALOG
            .iter()
            .map(|id| {
                let not_run = CheckOutcome {
                    id: *id,
                    class: id.class(),
                    state: CheckState::Skipped,
                    detail: Some("not-run"),
                };
                let outcome = outcomes
                    .iter()
                    .find(|candidate| candidate.id == *id)
                    .unwrap_or(&not_run);
                ReportCheck {
                    id: id.as_str(),
                    class: outcome.class.as_str(),
                    state: outcome.state.as_str(),
                    detail: outcome.detail,
                }
            })
            .collect();
        let counts = Counts {
            pass: outcomes
                .iter()
                .filter(|outcome| outcome.state == CheckState::Pass)
                .count(),
            fail: outcomes
                .iter()
                .filter(|outcome| outcome.state == CheckState::Fail)
                .count(),
            skipped: outcomes
                .iter()
                .filter(|outcome| outcome.state == CheckState::Skipped)
                .count(),
        };
        Self {
            schema_version: version::SCHEMA_VERSION,
            suite: SuiteSection {
                identity: version::IDENTITY,
                version: suite_version,
                profile: profile.as_str(),
            },
            adapter,
            session,
            verdict: verdict.as_str(),
            badge,
            checks,
            counts,
        }
    }

    /// The exact JSON envelope bytes: `status`, the report, then the
    /// derived failure diagnostics when the verdict is not a pass.
    pub fn envelope_json(
        &self,
        status: crate::result::Status,
        diagnostics: &[crate::diagnostics::Diagnostic],
    ) -> String {
        #[derive(Serialize)]
        struct Envelope<'a> {
            status: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            report: Option<&'a ConformanceReport>,
            #[serde(skip_serializing_if = "Option::is_none")]
            diagnostics: Option<&'a [crate::diagnostics::Diagnostic]>,
            #[serde(rename = "reasonCodes", skip_serializing_if = "Option::is_none")]
            reason_codes: Option<Vec<&'a str>>,
        }
        let envelope = Envelope {
            status: status.as_str(),
            report: Some(self),
            diagnostics: if diagnostics.is_empty() {
                None
            } else {
                Some(diagnostics)
            },
            reason_codes: if diagnostics.is_empty() {
                None
            } else {
                Some(diagnostics.iter().map(|d| d.id()).collect())
            },
        };
        serde_json::to_string_pretty(&envelope).expect("report envelope serializes")
    }

    /// The deterministic JUnit XML document. One suite carries one
    /// testcase per catalog row; failures carry their class as the
    /// failure type, skips carry their reason.
    pub fn junit(&self) -> String {
        let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        out.push_str(&format!(
            "<testsuites name=\"lekalo adapter conformance\" tests=\"{}\" failures=\"{}\" skipped=\"{}\">\n",
            self.checks.len(),
            self.counts.fail,
            self.counts.skipped
        ));
        out.push_str(&format!(
            "<testsuite name=\"adapter-conformance\" tests=\"{}\" failures=\"{}\" skipped=\"{}\" profile=\"{}\" adapter=\"{}\" protocol=\"{}\" ir=\"{}\">\n",
            self.checks.len(),
            self.counts.fail,
            self.counts.skipped,
            self.suite.profile,
            xml_attr(&self.adapter.id),
            self.session.protocol,
            self.session.ir,
        ));
        for row in &self.checks {
            let id = CheckId::from_report_id(row.id);
            out.push_str(&format!(
                "  <testcase name=\"{}\" classname=\"{}\">\n",
                xml_attr(row.id),
                id.classname(),
            ));
            match row.state {
                "pass" => {}
                "fail" => {
                    out.push_str(&format!(
                        "    <failure type=\"{}\" message=\"{}\"/>\n",
                        xml_attr(row.class),
                        xml_attr(row.detail.unwrap_or("failed")),
                    ));
                }
                _ => {
                    out.push_str(&format!(
                        "    <skipped message=\"{}\"/>\n",
                        xml_attr(row.detail.unwrap_or("skipped")),
                    ));
                }
            }
            out.push_str("  </testcase>\n");
        }
        out.push_str("</testsuite>\n</testsuites>\n");
        out
    }

    /// The human summary lines: the verdict, the badge, and one line per
    /// non-passing row.
    pub fn human(&self) -> String {
        let mut lines = vec![format!(
            "adapter {} protocol {} ir {} profile {}",
            self.adapter.id, self.session.protocol, self.session.ir, self.suite.profile,
        )];
        for row in &self.checks {
            match row.state {
                "pass" => lines.push(format!("pass  {}", row.id)),
                "fail" => lines.push(format!(
                    "fail  {} ({}) {}",
                    row.id,
                    row.class,
                    row.detail.unwrap_or("failed"),
                )),
                _ => lines.push(format!(
                    "skip  {} {}",
                    row.id,
                    row.detail.unwrap_or("skipped"),
                )),
            }
        }
        let badge = if self.badge.issued {
            format!(
                "badge verified protocol={} ir={}",
                self.badge.protocol.unwrap_or(""),
                self.badge.ir.unwrap_or(""),
            )
        } else {
            "badge withheld".to_owned()
        };
        lines.push(badge);
        lines.push(format!(
            "verdict {} (pass {}, fail {}, skipped {})",
            self.verdict, self.counts.pass, self.counts.fail, self.counts.skipped,
        ));
        lines.join("\n")
    }
}

impl CheckId {
    /// Resolve a report row id back to the catalog entry.
    pub fn from_report_id(text: &str) -> Self {
        CATALOG
            .iter()
            .copied()
            .find(|id| id.as_str() == text)
            .unwrap_or(CheckId::DescribeHandshake)
    }
}

/// Core checks can never be skipped on a badge-eligible adapter: the
/// handshake, capability, confinement, determinism, and redaction
/// sections always apply.
fn is_core(id: CheckId) -> bool {
    !matches!(
        id,
        CheckId::CapabilityIrDeclaration
            | CheckId::CapabilitySurface
            | CheckId::InputInvalidIr
            | CheckId::DiagnosticsStructured
            | CheckId::GenerateDryRunPlan
            | CheckId::GenerateApplyPlan
            | CheckId::ApplyRetryDiscipline
            | CheckId::PlanCleanCycle
            | CheckId::ScenarioNormalization
            | CheckId::ArtifactManifestEvidence
            | CheckId::ProcessCancellation
    )
}

/// Escape one attribute value for the JUnit XML.
fn xml_attr(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out
}
