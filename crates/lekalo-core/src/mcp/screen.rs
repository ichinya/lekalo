//! The output screening pass (issue #33, F17/F15): one bounded,
//! deterministic secret-shape sweep over every outgoing tool envelope.
//!
//! The MCP surface claims sensitive-output refusal, so an envelope that
//! would disclose a secret-shaped token refuses with the registered
//! `mcp.sensitive-output` diagnostic (`LEK-MCP-003`) instead of shipping
//! the bytes. The pass is deliberately conservative in both directions:
//! only closed high-confidence secret spellings match (assigned
//! credential assignments, PEM private-key blocks, bearer schemes), and
//! diagnostics are never truncated into nonsense — a match refuses the
//! whole output, it never redacts contract members silently. No
//! filesystem, process, or network access.

use crate::classification::DataKind;

/// The upper bound on envelope bytes one sweep inspects; envelopes past
/// the bound are screened by chunks (the matchers are byte-local), so
/// the sweep itself is O(n) with a small closed constant.
const SCREEN_CHUNK: usize = 64 * 1024;

/// One closed secret-shape matcher: a byte-local needle plus its closed
/// disclosure reason.
const NEEDLES: [(&str, &str); 8] = [
    ("-----BEGIN RSA PRIVATE KEY-----", "private-key-block"),
    ("-----BEGIN EC PRIVATE KEY-----", "private-key-block"),
    ("-----BEGIN OPENSSH PRIVATE KEY-----", "private-key-block"),
    ("-----BEGIN PRIVATE KEY-----", "private-key-block"),
    ("-----BEGIN PGP PRIVATE KEY BLOCK-----", "private-key-block"),
    ("Authorization:", "credential-header"),
    ("authorization:", "credential-header"),
    ("Set-Cookie:", "credential-header"),
];

/// The minimum length of an assigned-token value in an assignment
/// spelling (`password=<value>`, `token: <value>`, `secret = <value>`).
/// Review r2 F15: a minimum length is not authorization to disclose a
/// credential — any non-empty assigned value refuses, so short live
/// credentials (`password=short`) and spaced config spellings
/// (`password = 0123...`) are refused exactly like long ones. An empty
/// assignment (`password=` with no value) is not a disclosure and ships.
const ASSIGNED_TOKEN_MIN: usize = 1;

/// Absolute private-path spellings whose presence in an outgoing envelope
/// is a disclosure of the host filesystem layout (review r2 F15: a
/// description carrying `C:/Users/PrivateReview/private.txt` shipped
/// unscreened). The needles are byte-local, closed, and cover the Windows
/// drive and user-profile spellings plus the POSIX home roots; the JSON
/// transport escapes a backslash as `\\`, which the raw envelope scan
/// still matches through the `:\\` and `\\\\` needles.
const PRIVATE_PATH_NEEDLES: [&str; 8] = [
    ":\\Users\\",
    ":/Users/",
    ":\\users\\",
    ":/users/",
    "/home/",
    "/root/",
    // The JSON-escaped backslash spelling (`C:\\Users\\alice` on the
    // wire) carries doubled separators; the raw envelope scan matches it
    // through these doubled-separator needles.
    ":\\\\Users\\\\",
    ":\\\\users\\\\",
];

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
    for (needle, reason) in NEEDLES {
        if contains(bytes, needle.as_bytes()) {
            return Err(reason);
        }
    }
    for needle in PRIVATE_PATH_NEEDLES {
        if contains(bytes, needle.as_bytes()) {
            return Err("private-absolute-path");
        }
    }
    if contains_assigned_token(bytes) {
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

/// The assignment spellings whose non-empty value of at least
/// [`ASSIGNED_TOKEN_MIN`] characters counts as an assigned credential.
const ASSIGNMENT_KEYS: [&str; 9] = [
    "password",
    "passwd",
    "secret",
    "token",
    "api_key",
    "apikey",
    "access_token",
    "refresh_token",
    "private_key",
];

fn contains_assigned_token(bytes: &[u8]) -> bool {
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => return false,
    };
    for key in ASSIGNMENT_KEYS {
        let mut from = 0;
        while let Some(position) = text[from..].find(key) {
            let key_start = from + position;
            let mut cursor = key_start + key.len();
            from = cursor;
            // The match must start at a key boundary: the byte before is
            // not an identifier character (so `NotAPasswordValue` never
            // matches) and, in the JSON spelling, the key is quoted.
            if key_start > 0 {
                let before = text.as_bytes()[key_start - 1] as char;
                let quoted_key = before == '"';
                if !quoted_key && (before.is_ascii_alphanumeric() || before == '_') {
                    continue;
                }
            }
            // JSON spelling: the key closes with a quote, then optional
            // whitespace, then the colon. Config spelling: the key is
            // followed by `=` or `:`. Review r2 F15: the separator may carry
            // optional whitespace on BOTH sides in either spelling
            // (`password = <token>`), so whitespace is skipped between the
            // key (or its closing quote) and the separator.
            if text.as_bytes().get(cursor) == Some(&b'"') {
                cursor += 1;
            }
            cursor += text[cursor..].len() - text[cursor..].trim_start().len();
            let separator_at = match text.as_bytes().get(cursor) {
                Some(&b':') | Some(&b'=') => cursor,
                _ => continue,
            };
            let mut value = &text[separator_at + 1..];
            value = &value[value.len() - value.trim_start().len()..];
            // A JSON-quoted value: the closing quote ends it, and a quote
            // that closes immediately (or never closes before the frame
            // ends) carries no token at all — structural fragments like
            // the `"}` tail of `"password="}` are not assignments.
            let quoted = value.starts_with('"');
            let token = if quoted {
                let mut span = String::new();
                let mut closed = false;
                for character in value.chars().skip(1) {
                    if character == '"' {
                        closed = true;
                        break;
                    }
                    if character == '\\' {
                        break;
                    }
                    span.push(character);
                }
                if !closed || span.is_empty() {
                    continue;
                }
                span
            } else {
                value
                    .chars()
                    .take_while(|character| {
                        character.is_ascii_alphanumeric()
                            || matches!(*character, '_' | '-' | '+' | '/' | '=')
                    })
                    .collect()
            };
            if token.chars().count() >= ASSIGNED_TOKEN_MIN {
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
        // The JSON-escaped backslash spelling still matches the raw scan.
        assert_eq!(
            screen_envelope("{\"note\":\"C:\\\\Users\\\\PrivateReview\\\\f.txt\"}"),
            Err("private-absolute-path")
        );
        assert_eq!(
            screen_envelope("{\"note\":\"/home/alice/notes.txt\"}"),
            Err("private-absolute-path")
        );
        // Relative project paths and prose never match.
        assert_eq!(
            screen_envelope("{\"path\":\"lekalo/modules/planner/commands.yaml\"}"),
            Ok(())
        );
        assert_eq!(
            screen_envelope("{\"note\":\"the Users table holds profiles\"}"),
            Ok(())
        );
    }

    #[test]
    fn header_spellings_refuse() {
        assert_eq!(
            screen_envelope("{\"header\":\"Authorization: Bearer xyz\"}"),
            Err("credential-header")
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
