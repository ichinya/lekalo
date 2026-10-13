//! The output screening pass (issue #33, F17/F15): one bounded,
//! deterministic secret-shape sweep over every outgoing tool envelope.
//!
//! The MCP surface claims sensitive-output refusal, so an envelope that
//! would disclose a secret-shaped token refuses with the registered
//! `mcp.sensitive-output` diagnostic (`LEK-MCP-003`) instead of shipping
//! the bytes. The sweep is spelling-canonical (review r3 F1, r4 F1):
//! every matcher runs over one ASCII-case-folded copy of the envelope,
//! and the assignment scan runs over one separator-canonical form of
//! that copy — `_` and `-` deleted from BOTH the key vocabulary and the
//! scanned text, with a parallel record of which canonical bytes were
//! contiguously alphanumeric in the original — so `PASSWORD`,
//! `client_secret`, `client-secret`, and `clientSecret` are ONE
//! spelling (`NotAPassword=x` stays safe: contiguous alphanumeric
//! adjacency still suppresses a key match, whatever separators the
//! original carried). Quote spellings are canonical too (r4 F1):
//! `"`, `'`, and `\"` are quote equivalents around a value, so
//! `password="x"`, `password='x'`, and an embedded `{"password":"x"}`
//! all parse as assignments. Private absolute paths are matched by
//! SHAPE, not by an enumerated segment list (r4 F1): any drive-letter
//! path (`D:/...`, `c:\\...`) and any boundary-rooted POSIX path
//! (`/srv/private/...`) refuses. The
//! pass is otherwise deliberately conservative in both directions:
//! only closed high-confidence secret shapes match (assigned
//! credential assignments, PEM private-key blocks, bearer schemes), and
//! diagnostics are never truncated into nonsense — a match refuses the
//! whole output, it never redacts contract members silently. No
//! filesystem, process, or network access.

use crate::classification::DataKind;

/// The upper bound on envelope bytes one sweep inspects; envelopes past
/// the bound are screened by chunks (the matchers are byte-local), so
/// the sweep itself is O(n) with a small closed constant.
const SCREEN_CHUNK: usize = 64 * 1024;

/// One closed secret-shape matcher: a byte-local needle (spelled in
/// lowercase — the sweep matches over the ASCII-folded envelope, r3 F1)
/// plus its closed disclosure reason.
const NEEDLES: [(&str, &str); 7] = [
    ("-----begin rsa private key-----", "private-key-block"),
    ("-----begin ec private key-----", "private-key-block"),
    ("-----begin openssh private key-----", "private-key-block"),
    ("-----begin private key-----", "private-key-block"),
    ("-----begin pgp private key block-----", "private-key-block"),
    ("authorization:", "credential-header"),
    ("set-cookie:", "credential-header"),
];

/// Absolute private-path spellings whose presence in an outgoing envelope
/// is a disclosure of the host filesystem layout (review r2 F15: a
/// description carrying `C:/Users/PrivateReview/private.txt` shipped
/// unscreened). The remaining segment needles are byte-local, closed,
/// lowercase (the sweep matches the folded envelope), and deliberately
/// match ANY path segment, not only a leading absolute one (review r3
/// F9): `app/home/page.tsx` refuses too. That availability collateral is
/// the accepted trade-off — the screen fails closed, and a relative
/// segment spelling can never be proven harmless from bytes alone. The
/// drive and user-profile spellings moved to the two SHAPE matchers
/// below (`contains_drive_letter_path`, `contains_posix_root_path`,
/// r4 F1), which the segment list could never cover.
const PRIVATE_PATH_NEEDLES: [&str; 3] = ["/home/", "/root/", "/users/"];

/// Whether one folded envelope position starts a drive-letter absolute
/// path: a single `a-z` letter, a colon, and a `/` or `\` separator,
/// where the letter is not itself preceded by an alphanumeric byte
/// (so `https://` and `127.0.0.1:8080/` never match, while `D:/x`,
/// `c:\users\x`, and the JSON-doubled `d:\\x` do). Review r4 F1:
/// private absolute paths are matched by SHAPE, not by an enumerated
/// user-root segment list — `D:/PrivateReview/finance.txt` ships under
/// any list that only knows `users|home|root`.
fn contains_drive_letter_path(folded: &[u8]) -> bool {
    for index in 0..folded.len() {
        let byte = folded[index];
        if !byte.is_ascii_lowercase() {
            continue;
        }
        if folded.get(index + 1) != Some(&b':') {
            continue;
        }
        if !matches!(folded.get(index + 2), Some(&b'/') | Some(&b'\\')) {
            continue;
        }
        let preceded_alphanumeric = index > 0 && folded[index - 1].is_ascii_alphanumeric();
        if !preceded_alphanumeric {
            return true;
        }
    }
    false
}

/// Whether one folded envelope position starts a boundary-rooted POSIX
/// absolute path: a `/` at the start of the envelope, after whitespace,
/// or after a quote, followed by one non-empty segment and a second `/`
/// — the `/srv/private/finance.txt` shape (r4 F1). Relative project
/// paths (`lekalo/modules/...`), URIs (`lekalo://p/summary`), and prose
/// fractions (`1/2`, `and/or`) never sit at such a boundary and ship.
fn contains_posix_root_path(folded: &[u8]) -> bool {
    for index in 0..folded.len() {
        if folded[index] != b'/' {
            continue;
        }
        let at_boundary = index == 0
            || folded[index - 1].is_ascii_whitespace()
            || folded[index - 1] == b'"'
            || folded[index - 1] == b'\'';
        if !at_boundary {
            continue;
        }
        // One path segment (no whitespace, no quote) and then a second
        // separator: a rooted directory spelling.
        let mut cursor = index + 1;
        let mut segment = 0;
        while cursor < folded.len() {
            let byte = folded[cursor];
            if byte == b'/' {
                if segment > 0 {
                    return true;
                }
                break;
            }
            if byte.is_ascii_whitespace() || byte == b'"' || byte == b'\'' {
                break;
            }
            segment += 1;
            cursor += 1;
        }
    }
    false
}

/// Screen one outgoing envelope. `Ok(())` ships; `Err(reason)` refuses
/// the whole output with `mcp.sensitive-output`. The screen is total:
/// every byte of the envelope (payload, diagnostics, and scope members)
/// passes the same closed matchers.
pub fn screen_envelope(envelope: &str) -> Result<(), &'static str> {
    let bytes = envelope.as_bytes();
    if bytes.len() <= SCREEN_CHUNK {
        return screen_bytes(bytes);
    }
    // Chunked sweep with a small overlap so a needle spanning a chunk
    // boundary still matches.
    let mut start = 0;
    while start < bytes.len() {
        let end = (start + SCREEN_CHUNK).min(bytes.len());
        let overlap_start = start.saturating_sub(64);
        screen_bytes(&bytes[overlap_start..end])?;
        start = end;
    }
    Ok(())
}

fn screen_bytes(bytes: &[u8]) -> Result<(), &'static str> {
    // One ASCII-folded copy carries every matcher (r3 F1): folding is a
    // byte-local, length-preserving map, so the chunked sweep and the
    // assignment scan stay byte-aligned with the original envelope.
    let folded: Vec<u8> = bytes.iter().map(|byte| byte.to_ascii_lowercase()).collect();
    for (needle, reason) in NEEDLES {
        if contains(&folded, needle.as_bytes()) {
            return Err(reason);
        }
    }
    for needle in PRIVATE_PATH_NEEDLES {
        if contains(&folded, needle.as_bytes()) {
            return Err("private-absolute-path");
        }
    }
    // r4 F1: private absolute paths are matched by shape too — every
    // drive-letter path and every boundary-rooted POSIX path refuses.
    if contains_drive_letter_path(&folded) || contains_posix_root_path(&folded) {
        return Err("private-absolute-path");
    }
    if contains_assigned_token(&folded) {
        return Err("assigned-credential");
    }
    Ok(())
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.len() > haystack.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// The assignment spellings whose non-empty value counts as an assigned
/// credential (r2 F15: a minimum length is not authorization to disclose
/// — any non-empty token refuses, and an empty assignment is not a
/// disclosure and ships),
/// spelled in their SEPARATOR-CANONICAL form (r4 F1): `_` and `-`
/// deleted, ASCII-folded — `client_secret`, `client-secret`, and
/// `clientSecret` are one spelling here, exactly as they are one
/// spelling in the canonical scan text. The compound forms are listed
/// explicitly so their whole key matches at a word boundary even where
/// a suffix key alone would be suppressed (`dbpassword`,
/// `awssecretaccesskey`), and the suffix keys still match any compound
/// the explicit list cannot foresee (`root_password`, `x-api-token`)
/// because a deleted separator leaves a non-alphanumeric boundary at
/// the suffix start.
const ASSIGNMENT_KEYS: [&str; 18] = [
    "password",
    "passwd",
    "passphrase",
    "secret",
    "secretkey",
    "token",
    "apikey",
    "apitoken",
    "accesstoken",
    "refreshtoken",
    "idtoken",
    "sessiontoken",
    "privatekey",
    "credentials",
    "accesskey",
    "clientsecret",
    "dbpassword",
    "awssecretaccesskey",
];

/// The canonical scan text of one folded envelope (r4 F1): `_` and `-`
/// deleted, with a parallel record per canonical byte of whether it was
/// preceded CONTIGUOUSLY by an alphanumeric byte of the original text.
/// A deleted separator clears the record, so the `p` of `password` in
/// `root_password` and `x-api-token` carries a word boundary, while the
/// `p` of `NotAPassword` does not — camelCase, kebab-case, and
/// snake_case keys are one spelling, and only true alphanumeric
/// adjacency suppresses a match.
struct Canonical {
    text: Vec<u8>,
    preceded_alphanumeric: Vec<bool>,
}

impl Canonical {
    fn of(folded: &[u8]) -> Self {
        let mut text = Vec::with_capacity(folded.len());
        let mut preceded_alphanumeric = Vec::with_capacity(folded.len());
        let mut contiguous = false;
        for byte in folded {
            if *byte == b'_' || *byte == b'-' {
                contiguous = false;
                continue;
            }
            text.push(*byte);
            preceded_alphanumeric.push(contiguous);
            contiguous = byte.is_ascii_alphanumeric();
        }
        Self {
            text,
            preceded_alphanumeric,
        }
    }
}

/// Whether the canonical text at `index` opens a quote-equivalent
/// spelling (r4 F1): `\"` (the JSON-escaped quote), `"`, or `'`. The
/// width of the opening spelling is returned; a lone `\` is not one.
fn quote_equivalent(text: &[u8], index: usize) -> Option<usize> {
    match text.get(index) {
        Some(b'\\') if text.get(index + 1) == Some(&b'"') => Some(2),
        Some(b'"') => Some(1),
        Some(b'\'') => Some(1),
        _ => None,
    }
}

/// Whether a value span (already whitespace-trimmed) carries a
/// non-empty assigned token: a quote-equivalent-wrapped value closes
/// with any later quote equivalent (`password="x"`, `password='x'`,
/// `{"password":"x"}` — the escaped spellings serialize as `\"` and
/// a lone backslash no longer breaks the scan, r4 F1); an unwrapped
/// value needs at least one leading token character. A value that
/// never closes before the frame ends carries no token at all —
/// structural fragments are not assignments.
fn carries_token(value: &str) -> bool {
    let bytes = value.as_bytes();
    if let Some(width) = quote_equivalent(bytes, 0) {
        let mut cursor = width;
        let mut span = 0;
        while cursor < bytes.len() {
            if quote_equivalent(bytes, cursor).is_some() {
                return span > 0;
            }
            span += 1;
            cursor += 1;
        }
        return false;
    }
    bytes
        .first()
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
}

fn contains_assigned_token(folded: &[u8]) -> bool {
    let canonical = Canonical::of(folded);
    let text = match std::str::from_utf8(&canonical.text) {
        Ok(text) => text,
        Err(_) => return false,
    };
    let boundary = &canonical.preceded_alphanumeric;
    for key in ASSIGNMENT_KEYS {
        let mut from = 0;
        while let Some(position) = text[from..].find(key) {
            let key_start = from + position;
            let mut cursor = key_start + key.len();
            from = cursor;
            // The match must start at a key boundary (r3 F1/r4 F1): only
            // CONTIGUOUS alphanumeric adjacency suppresses the match.
            // In the JSON spelling the byte before the key is the
            // opening quote; in the canonical spelling a deleted `_` or
            // `-` is a boundary exactly like that quote.
            if boundary.get(key_start).copied().unwrap_or(false) {
                continue;
            }
            // JSON spelling: the key closes with a quote equivalent
            // (`\"` for the embedded-JSON spelling, r4 F1), then
            // optional whitespace, then the separator. Config spelling:
            // the key is followed by `=` or `:`. The separator may carry
            // optional whitespace on BOTH sides in either spelling
            // (`password = <token>`, `"secret" : "<token>"`).
            if let Some(width) = quote_equivalent(text.as_bytes(), cursor) {
                cursor += width;
            }
            cursor += text[cursor..].len() - text[cursor..].trim_start().len();
            let separator_at = match text.as_bytes().get(cursor) {
                Some(&b':') | Some(&b'=') => cursor,
                _ => continue,
            };
            let value = &text[separator_at + 1..];
            if carries_token(value.trim_start()) {
                return true;
            }
        }
    }
    false
}

/// Whether one classification kind is a disclosure blocker for output
/// screening (`credential` is sealed; the MCP surface never carries
/// lower kinds as secret material, so only the sealed rank refuses).
pub fn kind_blocks_output(kind: DataKind) -> bool {
    kind == DataKind::Credential
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_envelopes_ship() {
        assert_eq!(screen_envelope("{\"status\":\"valid\"}"), Ok(()));
        assert_eq!(
            screen_envelope("{\"note\":\"the password field is required\"}"),
            Ok(()),
            "field names and prose never match"
        );
        assert_eq!(
            screen_envelope("{\"value\":\"short\"}"),
            Ok(()),
            "unassigned short values never match"
        );
    }

    #[test]
    fn private_key_blocks_refuse() {
        assert_eq!(
            screen_envelope("{\"pem\":\"-----BEGIN RSA PRIVATE KEY-----\"}"),
            Err("private-key-block")
        );
        assert_eq!(
            screen_envelope("-----BEGIN OPENSSH PRIVATE KEY-----"),
            Err("private-key-block")
        );
        // The folded sweep also catches mixed and lowercased markers.
        assert_eq!(
            screen_envelope("-----begin private key-----"),
            Err("private-key-block")
        );
    }

    #[test]
    fn assigned_tokens_refuse_but_names_do_not() {
        assert_eq!(
            screen_envelope("{\"password\":\"hunter2hunter2\"}"),
            Err("assigned-credential")
        );
        assert_eq!(
            screen_envelope("token: ghp_0123456789abcdef0123456789"),
            Err("assigned-credential")
        );
        // The field name without an assigned value ships (an empty
        // assignment is not a disclosure).
        assert_eq!(screen_envelope("{\"schema\":\"password=\"}"), Ok(()));
        // Review r2 F15: a minimum length is not authorization to
        // disclose — a short live credential refuses too.
        assert_eq!(
            screen_envelope("{\"description\":\"set password=short\"}"),
            Err("assigned-credential")
        );
        // An identifier merely containing the word does not match.
        assert_eq!(screen_envelope("{\"field\":\"NotAPasswordValue\"}"), Ok(()));
    }

    #[test]
    fn spaced_assignment_spellings_refuse() {
        // The config-style spaced spelling the r2 probe smuggled through.
        assert_eq!(
            screen_envelope("{\"description\":\"password = 0123456789abcdef0123456789\"}"),
            Err("assigned-credential")
        );
        assert_eq!(
            screen_envelope("{\"secret\" : \"hunter2hunter2\"}"),
            Err("assigned-credential")
        );
        assert_eq!(
            screen_envelope("api_key = ghp_0123456789"),
            Err("assigned-credential")
        );
    }

    #[test]
    fn private_absolute_paths_refuse() {
        assert_eq!(
            screen_envelope("{\"note\":\"see C:/Users/PrivateReview/private.txt\"}"),
            Err("private-absolute-path")
        );
        // The JSON-escaped backslash spelling still matches the shape
        // scan.
        assert_eq!(
            screen_envelope("{\"note\":\"C:\\\\Users\\\\PrivateReview\\\\f.txt\"}"),
            Err("private-absolute-path")
        );
        assert_eq!(
            screen_envelope("{\"note\":\"/home/alice/notes.txt\"}"),
            Err("private-absolute-path")
        );
        // r3 F1: the shape matchers fold case — every casing of the
        // Windows profile spellings is one spelling.
        assert_eq!(
            screen_envelope("{\"note\":\"C:/USERS/PrivateReview/private.txt\"}"),
            Err("private-absolute-path")
        );
        // r4 F1: private absolute paths refuse by SHAPE — any drive
        // letter and any boundary-rooted POSIX path, not only the
        // enumerated user roots the r2/r3 needles covered.
        assert_eq!(
            screen_envelope("{\"note\":\"D:/PrivateReview/finance.txt\"}"),
            Err("private-absolute-path")
        );
        assert_eq!(
            screen_envelope("{\"note\":\"/srv/private/finance.txt\"}"),
            Err("private-absolute-path")
        );
        assert_eq!(
            screen_envelope("{\"note\":\"/etc/passwd is readable\"}"),
            Err("private-absolute-path")
        );
        assert_eq!(
            screen_envelope("{\"env\":\"e:\\\\secret\\\\k.json\"}"),
            Err("private-absolute-path")
        );
        // Relative project paths, URIs, and prose never match: the
        // boundary rules keep them out of the rooted-path shapes.
        assert_eq!(
            screen_envelope("{\"path\":\"lekalo/modules/planner/commands.yaml\"}"),
            Ok(())
        );
        assert_eq!(
            screen_envelope("{\"uri\":\"lekalo://planner/symbol/planner.focus_task\"}"),
            Ok(())
        );
        assert_eq!(
            screen_envelope("{\"note\":\"the Users table holds profiles\"}"),
            Ok(())
        );
        assert_eq!(
            screen_envelope("{\"url\":\"https://example.com/a/b\"}"),
            Ok(())
        );
        assert_eq!(screen_envelope("{\"host\":\"127.0.0.1:8080/x\"}"), Ok(()));
        assert_eq!(screen_envelope("{\"ratio\":\"and/or either\"}"), Ok(()));
    }

    #[test]
    fn header_spellings_refuse() {
        assert_eq!(
            screen_envelope("{\"header\":\"Authorization: Bearer xyz\"}"),
            Err("credential-header")
        );
        // r3 F1: the header needles fold case — every casing of the
        // credential headers is one spelling.
        assert_eq!(
            screen_envelope("{\"header\":\"AUTHORIZATION: Bearer abc\"}"),
            Err("credential-header")
        );
        assert_eq!(
            screen_envelope("Authorization: Bearer abc"),
            Err("credential-header")
        );
        assert_eq!(
            screen_envelope("{\"cookie\":\"Set-Cookie: session=abc\"}"),
            Err("credential-header")
        );
    }

    #[test]
    fn underscore_joined_and_case_variant_credential_keys_refuse() {
        // r3 F1: the dominant real-world credential spellings —
        // `_`-joined compound keys and any non-lowercase casing — are
        // one spelling each under the folded, separator-canonical scan.
        for envelope in [
            "db_password=hunter2hunter2",
            "client_secret=topsecretvalue",
            "aws_secret_access_key=AKIAIOSFODNN7EXAMPLE",
            "api_token=tok_live_12345",
            "PASSWORD=hunter2hunter2",
            "Password: hunter2hunter2",
            "SECRET = hunter2hunter2",
            "DATABASE_PASSWORD = hunter2hunter2",
            "{\"AWS_SECRET_ACCESS_KEY\":\"AKIAIOSFODNN7EXAMPLE\"}",
            "{\"CLIENT_SECRET\":\"topsecretvalue\"}",
            "{\"db_password\":\"hunter2hunter2\"}",
            "x-api-token: tok_live_12345",
            "root_password=hunter2hunter2",
        ] {
            assert_eq!(
                screen_envelope(envelope),
                Err("assigned-credential"),
                "the credential spelling must refuse: {envelope}"
            );
        }
        // The safe adjacency class stays safe: contiguous alphanumeric
        // adjacency still suppresses the key match, whatever the casing.
        assert_eq!(screen_envelope("NotAPassword=x"), Ok(()));
        assert_eq!(screen_envelope("NOTAPASSWORD=x"), Ok(()));
        assert_eq!(screen_envelope("{\"field\":\"NOTAPASSWORDVALUE\"}"), Ok(()));
        assert_eq!(
            screen_envelope("{\"field\":\"the Password field\"}"),
            Ok(())
        );
    }

    #[test]
    fn camelcase_and_kebab_credential_keys_refuse() {
        // r4 F1: the separator-canonical scan folds `_`, `-`, and case
        // on BOTH sides, so the camelCase and kebab-case spellings —
        // the dominant forms of the JSON/JavaScript ecosystem — are the
        // same key as their snake_case spelling, and the alphanumeric
        // characters INSIDE a camelCase key no longer suppress the
        // suffix match (only contiguous adjacency does, and a deleted
        // separator is a boundary).
        for envelope in [
            "clientSecret=synthcamelsecret1",
            "accessToken=eyJhbGciOi.example",
            "dbPassword=hunter2hunter2",
            "api-key=synthapikey4",
            "aws-secret-access-key=AKIAIOSFODNN7EXAMPLE",
            "api-Key=synthapikey5",
            "ClientSecret=synthcamel2",
            "{\"clientSecret\":\"synthcamel3\"}",
            "x-api-key: synthapikey6",
            "refresh-token=synthrefresh7",
        ] {
            assert_eq!(
                screen_envelope(envelope),
                Err("assigned-credential"),
                "the credential spelling must refuse: {envelope}"
            );
        }
        // Prose that merely contains the words is not an assignment.
        assert_eq!(screen_envelope("the client secret is rotated"), Ok(()));
        assert_eq!(screen_envelope("client-secret-name field"), Ok(()));
    }

    #[test]
    fn quoted_and_embedded_json_assignment_spellings_refuse() {
        // r4 F1: quote equivalents — `"`, `'`, and the JSON-escaped
        // `\"` — wrap values in every combination, and an embedded JSON
        // object serializes with escaped quotes that no longer hide the
        // assignment. Each row is the exact on-the-wire spelling the
        // round-4 probes leaked.
        for envelope in [
            "password=\"synthetic-secret-q6\"",
            "password='synthetic-secret-q7'",
            "{\"description\":\"password=\\\"synthetic-secret-q6\\\"\"}",
            "{\"description\":\"password='synthetic-secret-q7'\"}",
            "{\"note\":\"{\\\"password\\\":\\\"synthetic-secret-json8\\\"}\"}",
            "secret : 'topsecretvalue'",
        ] {
            assert_eq!(
                screen_envelope(envelope),
                Err("assigned-credential"),
                "the quoted assignment must refuse: {envelope}"
            );
        }
        // The empty assignment and structural fragments still ship.
        assert_eq!(screen_envelope("{\"schema\":\"password=\"}"), Ok(()));
        assert_eq!(screen_envelope("password=\"\""), Ok(()));
    }

    #[test]
    fn the_posix_home_needle_over_matches_by_design() {
        // r3 F9, recorded: the bare `/home/` (and `/root/`, `/users/`)
        // needle
        // matches ANY path segment, so benign relative content like
        // `app/home/page.tsx` refuses as `private-absolute-path`. The
        // over-match is the accepted fail-closed trade-off — a relative
        // `home` segment cannot be proven harmless from bytes alone —
        // and this test pins it as deliberate behavior, not an accident.
        assert_eq!(
            screen_envelope("{\"note\":\"files live under app/home/page.tsx\"}"),
            Err("private-absolute-path")
        );
        assert_eq!(
            screen_envelope("{\"note\":\"see /root/setup.log\"}"),
            Err("private-absolute-path")
        );
        // The macOS absolute profile spelling carries no drive colon, so
        // the colon-less users-root needle is what catches it (the r3
        // live probe's `/Users/alice/private.pem` leak class).
        assert_eq!(
            screen_envelope("{\"note\":\"/Users/alice/private.pem\"}"),
            Err("private-absolute-path")
        );
        assert_eq!(
            screen_envelope("{\"note\":\"app/users/page.tsx\"}"),
            Err("private-absolute-path")
        );
        // Ordinary relative project paths without those segments ship.
        assert_eq!(
            screen_envelope("{\"path\":\"lekalo/modules/planner/commands.yaml\"}"),
            Ok(())
        );
    }

    #[test]
    fn large_envelopes_are_screened_across_chunk_boundaries() {
        let filler = "a".repeat(SCREEN_CHUNK + 32);
        let mut envelope = String::with_capacity(filler.len() + 40);
        envelope.push_str("{\"pad\":\"");
        envelope.push_str(&filler);
        envelope.push_str("\",\"secret\":\"");
        envelope.push_str("0123456789abcdef0123456789");
        envelope.push_str("\"}");
        assert_eq!(screen_envelope(&envelope), Err("assigned-credential"));
        // A clean large envelope still ships.
        let mut clean = String::from("{\"pad\":\"");
        clean.push_str(&filler);
        clean.push_str("\"}");
        assert_eq!(screen_envelope(&clean), Ok(()));
    }

    #[test]
    fn only_the_sealed_kind_blocks_output() {
        assert!(kind_blocks_output(DataKind::Credential));
        assert!(!kind_blocks_output(DataKind::Personal));
        assert!(!kind_blocks_output(DataKind::Public));
    }
}
