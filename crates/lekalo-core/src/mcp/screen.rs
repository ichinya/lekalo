//! The output screening pass (issue #33, F17/F15): one bounded,
//! deterministic secret-shape sweep over every outgoing tool envelope.
//!
//! The MCP surface claims sensitive-output refusal, so an envelope that
//! would disclose a secret-shaped token refuses with the registered
//! `mcp.sensitive-output` diagnostic (`LEK-MCP-003`) instead of shipping
//! the bytes. The sweep is spelling-canonical (review r3 F1): every
//! matcher runs over one ASCII-case-folded copy of the envelope, and the
//! assignment scan admits `_` and `-` as pre-key separators, so
//! `PASSWORD`, `Password:`, `db_password`, `client_secret`, and
//! `aws_secret_access_key` are one spelling each — only alphanumeric
//! adjacency suppresses a key match (`NotAPassword=x` stays safe). The
//! pass is otherwise deliberately conservative in both directions:
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
/// unscreened). The needles are byte-local, closed, lowercase (the sweep
/// matches the folded envelope, so `C:/USERS/` and `c:\Users\` are one
/// spelling), and cover the Windows drive and user-profile spellings
/// plus the POSIX home roots; the JSON transport escapes a backslash as
/// `\\`, which the raw envelope scan still matches through the
/// single- and doubled-separator needles. The POSIX needles `/home/`
/// and `/root/` deliberately match ANY `home`/`root` path segment, not
/// only a leading absolute one (review r3 F9): `app/home/page.tsx`
/// refuses too. That availability collateral is the accepted trade-off —
/// the screen fails closed, and a relative segment spelling can never
/// be proven harmless from bytes alone.
const PRIVATE_PATH_NEEDLES: [&str; 5] = [
    ":\\users\\",
    ":/users/",
    "/home/",
    "/root/",
    // The JSON-escaped backslash spelling (`C:\\Users\\alice` on the
    // wire) carries doubled separators; the raw envelope scan matches it
    // through this doubled-separator needle.
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

/// The assignment spellings whose non-empty value of at least
/// [`ASSIGNED_TOKEN_MIN`] characters counts as an assigned credential.
/// Spelled in lowercase: the scan runs over the folded envelope (r3 F1).
/// The compound forms are listed explicitly where the separator does
/// not directly follow the suffix key (`aws_secret_access_key`'s
/// `secret` is followed by `_access_key`), and matched anyway through
/// suffix keys where it does (`db_password`, `client_secret`,
/// `api_token`) because `_` and `-` are admitted pre-key separators —
/// only alphanumeric adjacency suppresses a match.
const ASSIGNMENT_KEYS: [&str; 20] = [
    "password",
    "passwd",
    "passphrase",
    "secret",
    "secret_key",
    "token",
    "api_key",
    "apikey",
    "api_token",
    "access_token",
    "refresh_token",
    "id_token",
    "session_token",
    "private_key",
    "privatekey",
    "credentials",
    "access_key",
    "client_secret",
    "db_password",
    "aws_secret_access_key",
];

fn contains_assigned_token(folded: &[u8]) -> bool {
    let text = match std::str::from_utf8(folded) {
        Ok(text) => text,
        Err(_) => return false,
    };
    for key in ASSIGNMENT_KEYS {
        let mut from = 0;
        while let Some(position) = text[from..].find(key) {
            let key_start = from + position;
            let mut cursor = key_start + key.len();
            from = cursor;
            // The match must start at a key boundary (r3 F1): only
            // alphanumeric adjacency suppresses the match — `_` and `-`
            // are admitted pre-key separators, so `db_password`,
            // `client_secret`, and `x-password` all match their key,
            // while `NotAPasswordValue` never matches (`A` is
            // alphanumeric). In the JSON spelling the byte before the
            // key is the opening quote, which is not alphanumeric and
            // therefore admits the match.
            if key_start > 0 {
                let before = text.as_bytes()[key_start - 1];
                if before.is_ascii_alphanumeric() {
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
        // The JSON-escaped backslash spelling still matches the raw scan.
        assert_eq!(
            screen_envelope("{\"note\":\"C:\\\\Users\\\\PrivateReview\\\\f.txt\"}"),
            Err("private-absolute-path")
        );
        assert_eq!(
            screen_envelope("{\"note\":\"/home/alice/notes.txt\"}"),
            Err("private-absolute-path")
        );
        // r3 F1: the private-path needles fold case, so the uppercase
        // Windows profile spellings are one spelling with the lowercase
        // ones.
        assert_eq!(
            screen_envelope("{\"note\":\"C:/USERS/PrivateReview/private.txt\"}"),
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
        ] {
            assert_eq!(
                screen_envelope(envelope),
                Err("assigned-credential"),
                "the credential spelling must refuse: {envelope}"
            );
        }
        // The safe adjacency class stays safe: alphanumeric adjacency
        // still suppresses the key match, whatever the casing.
        assert_eq!(screen_envelope("NotAPassword=x"), Ok(()));
        assert_eq!(screen_envelope("NOTAPASSWORD=x"), Ok(()));
        assert_eq!(screen_envelope("{\"field\":\"NOTAPASSWORDVALUE\"}"), Ok(()));
        assert_eq!(
            screen_envelope("{\"field\":\"the Password field\"}"),
            Ok(())
        );
    }

    #[test]
    fn the_posix_home_needle_over_matches_by_design() {
        // r3 F9, recorded: the bare `/home/` (and `/root/`) needle
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
