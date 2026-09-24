//! The redaction engine and the leak scanner (issue #119, plan S4).
//!
//! The engine implements the closed transform set of the frozen #120
//! vocabulary that the runtime owns:
//!
//! - `redact-content`: the payload body is dropped to a deterministic
//!   digest+class stub;
//! - `redact-secrets`: token/secret patterns become
//!   `<redacted:secret>`;
//! - `redact-pii`: emails, declared person names, and phone-like runs
//!   become `<redacted:pii>`;
//! - and the always-on metadata hygiene: absolute/host paths become
//!   deterministic `<path:Hn>` role aliases, repository identity
//!   becomes `<repo:{role}:{n}>` (never the real name), and the
//!   remaining scanner classes become the closed markers
//!   `<redacted:url>` and `<redacted:tenant>`.
//!
//! The leak scanner detects secret tokens, URLs, absolute path
//! fragments, emails, phone-like runs, declared person names, tenant
//! ids, and declared repository names. It runs inside every transform
//! and as the verification pass over the final payload: a residual
//! leak fails the export, never silently ships.
//!
//! Determinism: identical inputs produce byte-identical payloads,
//! aliases, and findings. Findings are sorted, carry kinds, aliases,
//! and occurrence counts only — never the matched text, offsets, or
//! timestamps. Small-domain values (paths, repository names) are
//! pseudonymized by ordinal role aliases, never by bare hashes. The
//! value-to-alias map lives only inside the engine and is never
//! emitted: secret values never persist anywhere.

use serde::Serialize;

use super::vocab::{DataSensitivity, RepositoryRole, TransformId};
use crate::digest::sha256_hex;

/// The closed leak classes of the scanner, in priority order (the
/// lower the ordinal, the earlier a match of this class wins an
/// overlap).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum LeakKind {
    /// A secret token or credential assignment.
    SecretToken,
    /// An http(s) URL.
    Url,
    /// An absolute or host-anchored path fragment.
    PathFragment,
    /// An email address.
    Email,
    /// A phone-like digit run.
    Phone,
    /// A declared person name.
    PersonName,
    /// A tenant identifier.
    TenantId,
    /// A declared repository identity name.
    RepositoryName,
}

impl LeakKind {
    /// Every class in closed priority order.
    pub const ALL: [LeakKind; 8] = [
        LeakKind::SecretToken,
        LeakKind::Url,
        LeakKind::PathFragment,
        LeakKind::Email,
        LeakKind::Phone,
        LeakKind::PersonName,
        LeakKind::TenantId,
        LeakKind::RepositoryName,
    ];

    /// The stable scanner name (the finding key spelling).
    pub const fn as_str(self) -> &'static str {
        match self {
            LeakKind::SecretToken => "secret-token",
            LeakKind::Url => "url",
            LeakKind::PathFragment => "path-fragment",
            LeakKind::Email => "email",
            LeakKind::Phone => "phone",
            LeakKind::PersonName => "person-name",
            LeakKind::TenantId => "tenant-id",
            LeakKind::RepositoryName => "repository-name",
        }
    }

    /// The closed replacement marker (or alias family) of the class.
    pub const fn marker(self) -> &'static str {
        match self {
            LeakKind::SecretToken => "<redacted:secret>",
            LeakKind::Url => "<redacted:url>",
            LeakKind::PathFragment => "<path:H>",
            LeakKind::Email | LeakKind::Phone | LeakKind::PersonName => "<redacted:pii>",
            LeakKind::TenantId => "<redacted:tenant>",
            LeakKind::RepositoryName => "<repo>",
        }
    }
}

impl serde::Serialize for LeakKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// One scanned leak: the class, the deterministic alias the engine
/// assigned to the concrete value, and the occurrence count. The
/// concrete value never appears here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeakFinding {
    kind: LeakKind,
    alias: String,
    occurrences: usize,
}

impl LeakFinding {
    /// The leak class.
    pub const fn kind(&self) -> LeakKind {
        self.kind
    }

    /// The deterministic alias or class marker.
    pub fn alias(&self) -> &str {
        &self.alias
    }

    /// How often the value occurred.
    pub const fn occurrences(&self) -> usize {
        self.occurrences
    }
}

/// The declared subject-side inputs of one redaction run: the
/// repository identity (name plus role) and the protected terms
/// (declared person names). These are inputs only; they are never
/// copied into any output.
#[derive(Clone, Copy, Debug, Default)]
pub struct RedactionSubject<'a> {
    /// The declared repository identity: `(name, role)`.
    pub repository: Option<(&'a str, RepositoryRole)>,
    /// Declared protected terms (person names) treated as PII.
    pub protected_terms: &'a [String],
}

/// One redaction request: the payload, the requested closed
/// transforms, the classification labels for the content stub, and
/// the declared subject.
#[derive(Clone, Copy, Debug)]
pub struct RedactionRequest<'a> {
    /// The metadata payload text to redact.
    pub payload: &'a str,
    /// The requested transforms, in any order; the engine applies
    /// them in canonical vocabulary order.
    pub transforms: &'a [TransformId],
    /// The classification labels carried by the content stub.
    pub labels: &'a [DataSensitivity],
    /// The declared subject-side inputs.
    pub subject: RedactionSubject<'a>,
}

/// The redaction outcome: the redacted payload, the requested
/// transforms applied (canonical order), the pre-transform findings,
/// and the residual findings of the verification pass over the final
/// payload.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactionOutcome {
    payload: String,
    applied: Vec<TransformId>,
    findings: Vec<LeakFinding>,
    residuals: Vec<LeakFinding>,
}

impl RedactionOutcome {
    /// The redacted payload bytes.
    pub fn payload(&self) -> &str {
        &self.payload
    }

    /// The requested transforms applied, in canonical order.
    pub fn applied(&self) -> &[TransformId] {
        &self.applied
    }

    /// The pre-transform findings, sorted.
    pub fn findings(&self) -> &[LeakFinding] {
        &self.findings
    }

    /// The residual findings over the final payload, sorted. A
    /// non-empty residual list must fail the export.
    pub fn residuals(&self) -> &[LeakFinding] {
        &self.residuals
    }

    /// The exact payload digest (`sha256:`-prefixed) of the redacted
    /// bytes.
    pub fn payload_digest(&self) -> String {
        format!("sha256:{}", sha256_hex(self.payload.as_bytes()))
    }
}

/// One replacement match found by the scanners. Engine-internal: the
/// value never leaves this module.
#[derive(Clone, Debug)]
struct Match {
    start: usize,
    end: usize,
    kind: LeakKind,
    value: String,
}

/// The deterministic alias assigner: fixed markers for the
/// wide-domain classes, first-occurrence ordinals for the
/// small-domain classes (paths and repository names).
struct AliasAssigner<'a> {
    repository_role: Option<RepositoryRole>,
    assigned: Vec<(LeakKind, String)>,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> AliasAssigner<'a> {
    fn new(subject: &RedactionSubject<'a>) -> Self {
        Self {
            repository_role: subject.repository.map(|(_, role)| role),
            assigned: Vec::new(),
            _marker: std::marker::PhantomData,
        }
    }

    fn alias_for(&mut self, matched: &Match) -> String {
        match matched.kind {
            LeakKind::SecretToken
            | LeakKind::Url
            | LeakKind::Email
            | LeakKind::Phone
            | LeakKind::PersonName
            | LeakKind::TenantId => matched.kind.marker().to_owned(),
            LeakKind::PathFragment => {
                let ordinal = self.ordinal(matched);
                format!("<path:H{ordinal}>")
            }
            LeakKind::RepositoryName => {
                let ordinal = self.ordinal(matched);
                match self.repository_role {
                    Some(role) => format!("<repo:{}:{ordinal}>", role.as_str()),
                    None => format!("<repo:{ordinal}>"),
                }
            }
        }
    }

    /// The stable per-class ordinal of one distinct small-domain
    /// value, assigned in first-occurrence order.
    fn ordinal(&mut self, matched: &Match) -> usize {
        match self
            .assigned
            .iter()
            .position(|(kind, value)| *kind == matched.kind && *value == matched.value)
        {
            Some(index) => index + 1,
            None => {
                self.assigned.push((matched.kind, matched.value.clone()));
                self.assigned.len()
            }
        }
    }
}

/// Scan one payload with no declared subject inputs.
pub fn scan(payload: &str) -> Vec<LeakFinding> {
    scan_with(payload, RedactionSubject::default())
}

/// Scan one payload for every closed leak class. Findings are
/// deduplicated per (class, distinct value), sorted deterministically
/// by class then alias, and carry aliases and counts only.
pub fn scan_with(payload: &str, subject: RedactionSubject<'_>) -> Vec<LeakFinding> {
    let mut matches = collect_matches(payload, &subject);
    matches.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.end.cmp(&right.end))
    });
    let mut assigner = AliasAssigner::new(&subject);
    let mut findings: Vec<(LeakKind, String, usize)> = Vec::new();
    for matched in &matches {
        let alias = assigner.alias_for(matched);
        match findings
            .iter_mut()
            .find(|(kind, existing_alias, _)| *kind == matched.kind && *existing_alias == alias)
        {
            Some(entry) => entry.2 += 1,
            None => findings.push((matched.kind, alias, 1)),
        }
    }
    findings.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    findings
        .into_iter()
        .map(|(kind, alias, occurrences)| LeakFinding {
            kind,
            alias,
            occurrences,
        })
        .collect()
}

/// Apply the requested transforms and run the verification pass over
/// the final payload. The requested transforms are applied in
/// canonical order; the metadata-hygiene replacements (paths,
/// repository identity, tenant ids, URLs) always ride along, and
/// `redact-content` subsumes every other transform by dropping the
/// body entirely.
pub fn redact(request: &RedactionRequest<'_>) -> RedactionOutcome {
    let findings = scan_with(request.payload, request.subject);
    let applied: Vec<TransformId> = TransformId::ALL
        .iter()
        .copied()
        .filter(|transform| {
            request.transforms.contains(transform)
                && matches!(
                    transform,
                    TransformId::RedactContent
                        | TransformId::RedactSecrets
                        | TransformId::RedactPii
                )
        })
        .collect();
    let payload = if applied.contains(&TransformId::RedactContent) {
        content_stub(request)
    } else {
        let matches = collect_matches(request.payload, &request.subject);
        let mut selected = select_non_overlapping(&matches);
        selected.retain(|matched| match matched.kind {
            LeakKind::SecretToken => applied.contains(&TransformId::RedactSecrets),
            LeakKind::Email | LeakKind::Phone | LeakKind::PersonName => {
                applied.contains(&TransformId::RedactPii)
            }
            // Metadata hygiene always applies.
            LeakKind::PathFragment
            | LeakKind::Url
            | LeakKind::TenantId
            | LeakKind::RepositoryName => true,
        });
        replace_selected(request.payload, &selected, &request.subject)
    };
    let residuals = scan_with(&payload, request.subject);
    RedactionOutcome {
        payload,
        applied,
        findings,
        residuals,
    }
}

/// The deterministic digest+class stub that replaces a dropped body.
fn content_stub(request: &RedactionRequest<'_>) -> String {
    let mut labels: Vec<&str> = request.labels.iter().map(|label| label.as_str()).collect();
    labels.sort_unstable();
    labels.dedup();
    let digest = format!("sha256:{}", sha256_hex(request.payload.as_bytes()));
    serde_json::json!({
        "kind": "redacted-content",
        "class": labels,
        "contentDigest": digest,
    })
    .to_string()
}

/// Greedy non-overlapping selection: earliest start wins, then the
/// highest class priority (the lowest `LeakKind` ordinal), then the
/// earliest end.
fn select_non_overlapping(matches: &[Match]) -> Vec<Match> {
    let mut ordered: Vec<&Match> = matches.iter().collect();
    ordered.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.end.cmp(&right.end))
    });
    let mut selected: Vec<Match> = Vec::new();
    let mut cursor = 0usize;
    for matched in ordered {
        if matched.start >= cursor {
            cursor = matched.end;
            selected.push(matched.clone());
        }
    }
    selected
}

/// Replace the selected matches right-to-left with the deterministic
/// aliases, walking left-to-right first to assign ordinals.
fn replace_selected(payload: &str, selected: &[Match], subject: &RedactionSubject<'_>) -> String {
    let mut assigner = AliasAssigner::new(subject);
    let aliases: Vec<String> = selected
        .iter()
        .map(|matched| assigner.alias_for(matched))
        .collect();
    let mut output = payload.to_owned();
    for (matched, alias) in selected.iter().zip(aliases.iter()).rev() {
        output.replace_range(matched.start..matched.end, alias);
    }
    output
}

/// Collect every class match over the payload.
fn collect_matches(payload: &str, subject: &RedactionSubject<'_>) -> Vec<Match> {
    let mut matches = Vec::new();
    matches.extend(match_secret_tokens(payload));
    matches.extend(match_urls(payload));
    matches.extend(match_path_fragments(payload));
    matches.extend(match_emails(payload));
    matches.extend(match_phones(payload));
    matches.extend(match_tenants(payload));
    if let Some((name, _role)) = subject.repository {
        if !name.is_empty() {
            matches.extend(match_literal(payload, name, LeakKind::RepositoryName));
        }
    }
    for term in subject.protected_terms {
        if !term.is_empty() {
            matches.extend(match_literal(payload, term, LeakKind::PersonName));
        }
    }
    matches
}

/// Literal occurrences of one declared term.
fn match_literal(payload: &str, term: &str, kind: LeakKind) -> Vec<Match> {
    let mut matches = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = payload[cursor..].find(term) {
        let start = cursor + offset;
        let end = start + term.len();
        if payload.is_char_boundary(start) && payload.is_char_boundary(end) {
            matches.push(Match {
                start,
                end,
                kind,
                value: term.to_owned(),
            });
            cursor = end;
        } else {
            cursor = end.max(start + 1);
        }
    }
    matches
}

fn is_token_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}

/// The closed secret-token vocabulary: PEM private-key headers, AWS
/// access keys, GitHub/Slack/OpenAI-style tokens, Bearer
/// authorizations, and credential assignments (`key: "value"`).
fn match_secret_tokens(payload: &str) -> Vec<Match> {
    let mut matches = Vec::new();
    for marker in [
        "-----BEGIN PRIVATE KEY-----",
        "-----BEGIN RSA PRIVATE KEY-----",
        "-----BEGIN EC PRIVATE KEY-----",
        "-----BEGIN OPENSSH PRIVATE KEY-----",
        "-----BEGIN ENCRYPTED PRIVATE KEY-----",
    ] {
        matches.extend(match_literal(payload, marker, LeakKind::SecretToken));
    }

    let bytes = payload.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let rest = &payload[index..];
        let rest_bytes = &bytes[index..];
        let token_end = if rest.starts_with("AKIA")
            && rest.len() >= 20
            && rest_bytes[4..20]
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() && !byte.is_ascii_lowercase())
        {
            Some(20)
        } else if rest.len() >= 40
            && matches!(
                rest.get(..4),
                Some("ghp_" | "gho_" | "ghu_" | "ghs_" | "ghr_")
            )
        {
            token_run(rest_bytes, 4, 40)
        } else if rest.starts_with("github_pat_") && rest.len() >= 34 {
            token_run(rest_bytes, 11, 34)
        } else if rest.len() >= 15
            && matches!(
                rest.get(..4),
                Some("xoxb" | "xoxa" | "xoxp" | "xoxr" | "xoxs")
            )
        {
            token_run(rest_bytes, 5, 15)
        } else if rest.starts_with("sk-") && rest.len() >= 23 {
            token_run(rest_bytes, 3, 23)
        } else {
            None
        };
        if let Some(end) = token_end {
            matches.push(Match {
                start: index,
                end: index + end,
                kind: LeakKind::SecretToken,
                value: rest[..end].to_owned(),
            });
            index += end;
            continue;
        }
        // Bearer authorizations.
        if rest.len() >= 24 && rest[..7].eq_ignore_ascii_case("Bearer ") {
            let end = rest_bytes[7..]
                .iter()
                .position(|byte| !(is_token_char(*byte) || matches!(byte, b'.' | b'=')))
                .map(|position| 7 + position)
                .unwrap_or(rest.len());
            if end >= 23 {
                matches.push(Match {
                    start: index,
                    end: index + end,
                    kind: LeakKind::SecretToken,
                    value: rest[..end].to_owned(),
                });
                index += end;
                continue;
            }
        }
        if let Some(assign) = match_credential_assignment(payload, index) {
            index = assign.end;
            matches.push(assign);
            continue;
        }
        index += 1;
    }
    matches
}

/// The end of a token run starting at `offset`, or `None` when the
/// run is shorter than `minimum`.
fn token_run(rest_bytes: &[u8], offset: usize, minimum: usize) -> Option<usize> {
    let end = rest_bytes[offset..]
        .iter()
        .position(|byte| !is_token_char(*byte))
        .map(|position| offset + position)
        .unwrap_or(rest_bytes.len());
    (end >= minimum).then_some(end)
}

/// One credential assignment starting exactly at `index`:
/// `key` spaces (`:`|`=`|`=>`) spaces optional-quote value quote.
fn match_credential_assignment(payload: &str, index: usize) -> Option<Match> {
    const KEYS: [&str; 11] = [
        "api-key",
        "apikey",
        "api_key",
        "secret",
        "secret-key",
        "token",
        "access-token",
        "password",
        "passwd",
        "pwd",
        "private-key",
    ];
    let rest = &payload[index..];
    let mut key_len = None;
    for key in KEYS {
        if rest.len() >= key.len() && rest[..key.len()].eq_ignore_ascii_case(key) {
            let boundary = rest.as_bytes().get(key.len());
            let delimited = boundary.map_or(true, |byte| {
                !(byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            });
            if delimited {
                key_len = Some(key.len());
                break;
            }
        }
    }
    let key_len = key_len?;
    let after_key = &rest[key_len..];
    let separator_offset = after_key.find([':', '='])?;
    let separator_len = if after_key[separator_offset..].starts_with("=>") {
        2
    } else {
        1
    };
    let between = &after_key[..separator_offset];
    if !between.bytes().all(|byte| matches!(byte, b' ' | b'\t')) || between.len() > 16 {
        return None;
    }
    let value_part = &after_key[separator_offset + separator_len..];
    let spaces = value_part.len() - value_part.trim_start_matches([' ', '\t']).len();
    if spaces > 16 {
        return None;
    }
    let value_part = &value_part[spaces..];
    let quote_len = usize::from(value_part.starts_with('"') || value_part.starts_with('\''));
    let value_body = &value_part[quote_len..];
    let value_end = value_body
        .bytes()
        .position(|byte| !(is_token_char(byte) || matches!(byte, b'.' | b'/' | b'+' | b'=')))
        .unwrap_or(value_body.len());
    if value_end < 8 {
        return None;
    }
    let total_end = index
        + key_len
        + separator_offset
        + separator_len
        + spaces
        + quote_len
        + value_end
        + quote_len;
    Some(Match {
        start: index,
        end: total_end,
        kind: LeakKind::SecretToken,
        value: payload[index..total_end].to_owned(),
    })
}

/// http(s) URLs up to the first delimiter.
fn match_urls(payload: &str) -> Vec<Match> {
    let mut matches = Vec::new();
    let lower = payload.to_lowercase();
    let mut cursor = 0usize;
    while cursor < lower.len() {
        let http = lower[cursor..].find("http://");
        let https = lower[cursor..].find("https://");
        let start = match (http, https) {
            (Some(a), Some(b)) => cursor + a.min(b),
            (Some(a), None) => cursor + a,
            (None, Some(b)) => cursor + b,
            (None, None) => break,
        };
        let scheme_end = start
            + usize::from(lower[start..].starts_with("https://")) * 8
            + usize::from(!lower[start..].starts_with("https://")) * 7;
        let mut end = scheme_end;
        for (offset, character) in payload[scheme_end..].char_indices() {
            if character.is_whitespace()
                || matches!(
                    character,
                    '"' | '\'' | '<' | '>' | ')' | ']' | '}' | '`' | ','
                )
            {
                break;
            }
            end = scheme_end + offset + character.len_utf8();
        }
        if end > scheme_end {
            matches.push(Match {
                start,
                end,
                kind: LeakKind::Url,
                value: payload[start..end].to_owned(),
            });
            cursor = end;
        } else {
            cursor = scheme_end;
        }
    }
    matches
}

fn is_path_char(character: char) -> bool {
    !character.is_whitespace()
        && !matches!(
            character,
            '"' | '\'' | '<' | '>' | ')' | '(' | ']' | '[' | '}' | '{' | '`' | ',' | ';' | '|'
        )
}

/// Absolute and host-anchored path fragments: Unix-rooted (at least
/// one non-empty segment), Windows drive paths, UNC shares, and
/// `~/`-prefixed home paths.
fn match_path_fragments(payload: &str) -> Vec<Match> {
    let mut matches = Vec::new();
    let bytes = payload.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let rest = &payload[index..];
        let anchored = rest.starts_with("~/")
            || (rest.len() >= 3
                && rest.as_bytes()[0].is_ascii_alphabetic()
                && rest.as_bytes()[1] == b':'
                && matches!(rest.as_bytes()[2], b'\\' | b'/'))
            || (rest.starts_with("\\\\") && rest.len() >= 3 && rest.as_bytes()[2] != b'\\')
            || rest.starts_with('/');
        if anchored {
            let mut end = index;
            for (offset, character) in rest.char_indices() {
                if !is_path_char(character) {
                    break;
                }
                end = index + offset + character.len_utf8();
            }
            let candidate = &payload[index..end];
            let has_body = candidate.chars().skip(1).any(|character| {
                character.is_alphanumeric() || matches!(character, '_' | '~' | '.')
            });
            let has_separator = candidate.contains('/') || candidate.contains('\\');
            if has_body && has_separator {
                matches.push(Match {
                    start: index,
                    end,
                    kind: LeakKind::PathFragment,
                    value: candidate.to_owned(),
                });
                index = end;
                continue;
            }
        }
        index += 1;
    }
    matches
}

/// Email addresses: local part, one `@`, and a dotted domain whose
/// last label is alphabetic.
fn match_emails(payload: &str) -> Vec<Match> {
    let mut matches = Vec::new();
    for (index, character) in payload.char_indices() {
        if character != '@' {
            continue;
        }
        let local_start = payload[..index]
            .char_indices()
            .rev()
            .take_while(|(_, taken)| {
                taken.is_ascii_alphanumeric() || matches!(taken, '.' | '_' | '%' | '+' | '-')
            })
            .last()
            .map(|(offset, _)| offset);
        let Some(local_start) = local_start else {
            continue;
        };
        let local = &payload[local_start..index];
        let starts_alnum = local
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric());
        if !starts_alnum {
            continue;
        }
        let after = &payload[index + 1..];
        let domain_end = after
            .char_indices()
            .find(|(_, taken)| !(taken.is_ascii_alphanumeric() || matches!(taken, '.' | '-')))
            .map(|(offset, _)| offset)
            .unwrap_or(after.len());
        let domain = &after[..domain_end];
        let labels: Vec<&str> = domain.split('.').collect();
        let labels_ok = labels.len() >= 2
            && labels.iter().all(|label| {
                !label.is_empty()
                    && label
                        .chars()
                        .all(|taken| taken.is_ascii_alphanumeric() || taken == '-')
            })
            && labels.last().is_some_and(|label| {
                label.len() >= 2 && label.chars().all(|taken| taken.is_ascii_alphabetic())
            });
        if !labels_ok {
            continue;
        }
        matches.push(Match {
            start: local_start,
            end: index + 1 + domain_end,
            kind: LeakKind::Email,
            value: payload[local_start..index + 1 + domain_end].to_owned(),
        });
    }
    matches
}

/// Phone-like runs: 7..=15 digits in two or more groups separated by
/// the closed separator set, with no letters in the run.
fn match_phones(payload: &str) -> Vec<Match> {
    let mut matches = Vec::new();
    let bytes = payload.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() && bytes[index] != b'+' {
            index += 1;
            continue;
        }
        let run_start = index;
        let mut digits = 0usize;
        let mut groups = 1usize;
        let mut separators = 0usize;
        let mut end = index;
        let mut previous_was_digit = false;
        while end < bytes.len() {
            let byte = bytes[end];
            if byte.is_ascii_digit() {
                if !previous_was_digit && digits > 0 {
                    groups += 1;
                }
                digits += 1;
                previous_was_digit = true;
            } else if matches!(byte, b' ' | b'-' | b'.' | b'(' | b')' | b'/') {
                if previous_was_digit {
                    separators += 1;
                }
                previous_was_digit = false;
            } else if byte == b'+' && end == run_start {
                // A leading plus is part of the run.
            } else {
                break;
            }
            end += 1;
        }
        if (7..=15).contains(&digits) && groups >= 2 && separators >= 1 {
            matches.push(Match {
                start: run_start,
                end,
                kind: LeakKind::Phone,
                value: payload[run_start..end].to_owned(),
            });
            index = end;
        } else {
            index = end.max(run_start + 1);
        }
    }
    matches
}

/// Tenant identifiers: `tenant[-_:]id` markers, `tenantid=`/`tid=`
/// assignments, and GUID-shaped hexadecimal groups.
fn match_tenants(payload: &str) -> Vec<Match> {
    let mut matches = Vec::new();
    let lower = payload.to_lowercase();
    for marker in [
        "tenant-",
        "tenant_",
        "tenant:",
        "tenantid=",
        "tenant-id=",
        "tid=",
    ] {
        let mut cursor = 0usize;
        while cursor < lower.len() {
            let Some(position) = lower[cursor..].find(marker) else {
                break;
            };
            let start = cursor + position;
            let id_start = start + marker.len();
            let rest = &payload[id_start..];
            let end = rest
                .char_indices()
                .find(|(_, taken)| {
                    !(taken.is_ascii_alphanumeric() || matches!(taken, '-' | '_' | '.'))
                })
                .map(|(offset, _)| offset)
                .unwrap_or(rest.len());
            if end >= 4 {
                matches.push(Match {
                    start,
                    end: id_start + end,
                    kind: LeakKind::TenantId,
                    value: payload[start..id_start + end].to_owned(),
                });
                cursor = id_start + end;
            } else {
                cursor = id_start;
            }
        }
    }
    // GUID shapes: 8-4-4-4-12 hexadecimal groups.
    let bytes = payload.as_bytes();
    let mut index = 0usize;
    while index + 36 <= bytes.len() {
        let window = &bytes[index..index + 36];
        let is_guid = window
            .iter()
            .enumerate()
            .all(|(offset, byte)| match offset {
                8 | 13 | 18 | 23 => *byte == b'-',
                _ => byte.is_ascii_hexdigit(),
            });
        if is_guid {
            matches.push(Match {
                start: index,
                end: index + 36,
                kind: LeakKind::TenantId,
                value: payload[index..index + 36].to_owned(),
            });
            index += 36;
        } else {
            index += 1;
        }
    }
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject(
        repository: Option<(&'static str, RepositoryRole)>,
        terms: &[&str],
    ) -> RedactionSubject<'static> {
        let owned: Vec<String> = terms.iter().map(|term| (*term).to_owned()).collect();
        RedactionSubject {
            repository,
            protected_terms: Box::leak(owned.into_boxed_slice()),
        }
    }

    fn request<'a>(
        payload: &'a str,
        transforms: &'a [TransformId],
        subject: RedactionSubject<'a>,
    ) -> RedactionRequest<'a> {
        RedactionRequest {
            payload,
            transforms,
            labels: &[DataSensitivity::Internal],
            subject,
        }
    }

    fn kinds(findings: &[LeakFinding]) -> Vec<LeakKind> {
        findings.iter().map(|finding| finding.kind()).collect()
    }

    /// The scanner detects every closed class exactly once per
    /// distinct value class, with no duplicates or misses.
    #[test]
    fn scanner_detects_every_closed_class() {
        let payload = concat!(
            "deploy with AKIAABCDEFGHIJKLMNOP and ghp_",
            "abcdefghijklmnopqrstuvwxyz0123456789abcd\n",
            "contact jane.doe@example.com or +1 (555) 123-4567\n",
            "pull from https://git.example.com/acme/widgets.git\n",
            "logs in /var/lib/acme and C:\\Users\\dev\\secrets.txt\n",
            "tenant-acme123 maps to 6f9619ff-8b86-d011-b42d-00c04fc964ff\n",
            "password: \"hunter2hunter2\"\n",
        );
        let findings = scan_with(
            payload,
            subject(Some(("widgets", RepositoryRole::ConsumerRepository)), &[]),
        );
        let found = kinds(&findings);
        assert!(found.contains(&LeakKind::SecretToken), "{found:?}");
        assert!(found.contains(&LeakKind::Email), "{found:?}");
        assert!(found.contains(&LeakKind::Phone), "{found:?}");
        assert!(found.contains(&LeakKind::Url), "{found:?}");
        assert!(found.contains(&LeakKind::PathFragment), "{found:?}");
        assert!(found.contains(&LeakKind::TenantId), "{found:?}");
        assert!(found.contains(&LeakKind::RepositoryName), "{found:?}");
        // Findings are sorted by class then alias.
        let mut sorted = findings.clone();
        sorted.sort_by(|left, right| {
            left.kind()
                .cmp(&right.kind())
                .then_with(|| left.alias().cmp(right.alias()))
        });
        assert_eq!(findings, sorted);
        // No finding text carries any payload substring: aliases and
        // markers only.
        for finding in &findings {
            assert!(finding.alias().starts_with('<'), "{finding:?}");
            assert!(finding.alias().ends_with('>'), "{finding:?}");
        }
    }

    /// Secret redaction replaces every token and credential
    /// assignment; the residual scan over the payload is empty.
    #[test]
    fn secret_transform_replaces_and_verifies() {
        let payload = "key AKIAABCDEFGHIJKLMNOP\ntoken: ghp_abcdefghijklmnopqrstuvwxyz0123456789abcd\npassword=\"hunter2hunter2\"\n-----BEGIN PRIVATE KEY-----";
        let transforms = [TransformId::RedactSecrets];
        let outcome = redact(&request(payload, &transforms, RedactionSubject::default()));
        assert_eq!(outcome.applied(), [TransformId::RedactSecrets]);
        assert!(!outcome.payload().contains("AKIA"));
        assert!(!outcome.payload().contains("ghp_"));
        assert!(!outcome.payload().contains("hunter2"));
        assert!(!outcome.payload().contains("PRIVATE KEY"));
        assert!(outcome.payload().contains("<redacted:secret>"));
        assert_eq!(outcome.residuals(), []);
        // Pre-transform findings carry counts and markers only.
        let secrets = outcome
            .findings()
            .iter()
            .find(|finding| finding.kind() == LeakKind::SecretToken)
            .expect("secret finding");
        assert_eq!(secrets.alias(), "<redacted:secret>");
        assert!(secrets.occurrences() >= 4);
    }

    /// PII redaction replaces emails, phones, and declared person
    /// names with the exact closed marker.
    #[test]
    fn pii_transform_replaces_emails_phones_and_names() {
        let payload = "jane.doe@example.com called +1 (555) 123-4567 about Jane Doe";
        let transforms = [TransformId::RedactPii];
        let outcome = redact(&request(payload, &transforms, subject(None, &["Jane Doe"])));
        assert!(!outcome.payload().contains("@"));
        assert!(!outcome.payload().contains("555"));
        assert!(!outcome.payload().contains("Jane Doe"));
        assert!(outcome.payload().contains("<redacted:pii>"));
        assert_eq!(outcome.residuals(), []);
        assert_eq!(outcome.applied(), [TransformId::RedactPii]);
    }

    /// Path pseudonymization assigns deterministic first-occurrence
    /// ordinals, and repository identity becomes a role alias.
    #[test]
    fn small_domains_are_pseudonymized_by_role_alias() {
        let payload = "/var/lib/acme/cache then C:\\Users\\dev then /var/lib/acme again";
        let outcome = redact(&request(payload, &[], RedactionSubject::default()));
        assert!(outcome.payload().contains("<path:H1>"));
        assert!(outcome.payload().contains("<path:H2>"));
        assert!(!outcome.payload().contains("/var"));
        assert!(!outcome.payload().contains("C:\\"));
        // The same distinct value always maps to the same alias, and
        // each distinct path gets its own first-occurrence ordinal.
        assert_eq!(outcome.payload().matches("<path:H1>").count(), 1);
        assert_eq!(outcome.payload().matches("<path:H2>").count(), 1);
        assert_eq!(outcome.payload().matches("<path:H3>").count(), 1);

        let repo_payload = "mirror of lekalo onto lekalo mirror";
        let repo_outcome = redact(&request(
            repo_payload,
            &[],
            subject(Some(("lekalo", RepositoryRole::LekaloRepository)), &[]),
        ));
        assert!(!repo_outcome.payload().contains("lekalo mirror"));
        assert!(!repo_outcome.payload().contains("of lekalo"));
        assert!(repo_outcome
            .payload()
            .contains("<repo:lekalo-repository:1>"));
        // The finding alias carries the role alias, never the name.
        let repo_finding = repo_outcome
            .findings()
            .iter()
            .find(|finding| finding.kind() == LeakKind::RepositoryName)
            .expect("repository finding");
        assert_eq!(repo_finding.alias(), "<repo:lekalo-repository:1>");
    }

    /// `redact-content` drops the body to the exact deterministic
    /// digest+class stub; the stub carries no payload fragment and no
    /// residual leak.
    #[test]
    fn content_transform_drops_the_body_to_a_stub() {
        let payload = "totally secret body with token ghp_abcdefghijklmnopqrstuvwxyz0123456789abcd";
        let transforms = [TransformId::RedactContent, TransformId::RedactPii];
        let outcome = redact(&request(payload, &transforms, subject(None, &["Jane Doe"])));
        assert_eq!(
            outcome.applied(),
            [TransformId::RedactContent, TransformId::RedactPii]
        );
        assert!(!outcome.payload().contains("secret"));
        assert!(!outcome.payload().contains("ghp_"));
        let stub = serde_json::from_str::<serde_json::Value>(outcome.payload()).expect("stub JSON");
        assert_eq!(stub["kind"], "redacted-content");
        assert_eq!(stub["class"], serde_json::json!(["internal"]));
        let digest = stub["contentDigest"].as_str().unwrap();
        assert_eq!(
            digest,
            format!("sha256:{}", crate::digest::sha256_hex(payload.as_bytes()))
        );
        assert_eq!(outcome.residuals(), []);
        // The outcome digest is the digest of the redacted bytes.
        assert_eq!(
            outcome.payload_digest(),
            format!(
                "sha256:{}",
                crate::digest::sha256_hex(outcome.payload().as_bytes())
            )
        );
    }

    /// Identical inputs produce byte-identical outcomes; findings are
    /// stable and the serialized outcome never contains a matched
    /// value.
    #[test]
    fn outcomes_are_deterministic_and_leak_free() {
        let payload = "mail bob@example.com host /Users/bob/secret token ghp_abcdefghijklmnopqrstuvwxyz0123456789abcd";
        let transforms = [TransformId::RedactSecrets, TransformId::RedactPii];
        let run_one = redact(&request(
            payload,
            &transforms,
            subject(Some(("acme", RepositoryRole::ConsumerRepository)), &["Bob"]),
        ));
        let run_two = redact(&request(
            payload,
            &transforms,
            subject(Some(("acme", RepositoryRole::ConsumerRepository)), &["Bob"]),
        ));
        assert_eq!(run_one.payload(), run_two.payload());
        assert_eq!(run_one.findings(), run_two.findings());
        assert_eq!(run_one.residuals(), run_two.residuals());
        let serialized = serde_json::to_string(&run_one).expect("outcome serializes");
        for leaked in ["bob@example.com", "/Users/bob", "ghp_abc"] {
            assert!(
                !serialized.contains(leaked),
                "{leaked} leaked into the report"
            );
        }
    }

    /// The transform selection is exact: requesting secrets only
    /// leaves PII as a residual leak — which the verification pass
    /// reports so the export refuses.
    #[test]
    fn residual_leaks_never_silently_pass() {
        let payload = "bob@example.com with AKIAABCDEFGHIJKLMNOP";
        let transforms = [TransformId::RedactSecrets];
        let outcome = redact(&request(payload, &transforms, RedactionSubject::default()));
        assert!(outcome.payload().contains("bob@example.com"));
        assert!(!outcome.payload().contains("AKIA"));
        assert!(outcome
            .residuals()
            .iter()
            .any(|finding| finding.kind() == LeakKind::Email));
        // And the URL and tenant markers ride along as hygiene.
        let messy = "see https://private.example/x and tenant-acme now";
        let hygiene = redact(&request(messy, &[], RedactionSubject::default()));
        assert!(hygiene.payload().contains("<redacted:url>"));
        assert!(hygiene.payload().contains("<redacted:tenant>"));
        assert_eq!(hygiene.residuals(), []);
        assert!(hygiene.applied().is_empty());
    }
}
