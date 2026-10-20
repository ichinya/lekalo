//! Deterministic, fail-closed output screening (issue #33, reviews r3–r6).
//!
//! Credential stems are screened throughout assigned identifier keys,
//! including plurals and compounds; `_`, `-`, and ASCII case are folded
//! symmetrically. Compound `key` names refuse, but the bare JSON `key`
//! member and the explicit negated pseudo-key `NotAPassword` stay safe.
//! Non-empty scalar and composite credential values refuse; empty
//! strings/arrays/objects carry no material. The one count exception is
//! the quoted JSON `"tokens": <unsigned integer>` budget member, not
//! numeric passwords, other compounds, or string-valued tokens.
//!
//! PEM markers, credential headers, bare bearer schemes, drive-letter
//! and UNC paths, rooted POSIX directory spellings, home-relative paths,
//! and the conventional credential-bearing dotfile class refuse. Path
//! boundaries are fail-closed (r8): any byte that cannot continue a
//! relative path segment is a boundary, and a filesystem scheme
//! (`file:`, `ssh:`) is a filesystem reference — only hierarchical
//! `http(s)://`/`lekalo://` URLs stay exempt. Numeric-index JSON
//! pointers such as `/definitions/20` stay available; absolute API
//! routes may refuse because bytes alone cannot distinguish them from
//! host paths. Relative project paths and scheme-qualified URLs remain
//! available.
//!
//! A closed Cyrillic/Greek lookalike map covers common homoglyph keys;
//! this is NOT a general Unicode normalization or an exhaustive secret
//! detector. Conservative lexical matches can refuse benign compounds,
//! dotfile substrings, and `/home/`-style relative segments. A match
//! refuses the whole envelope with `mcp.sensitive-output`, never silently
//! redacts contract members. Time and auxiliary memory are linear in the
//! caller-bounded response, with no filesystem, process, or network access.

use crate::classification::DataKind;

// The former chunk boundary is retained only for regression probes:
// scanning the complete bounded envelope avoids splitting a long key,
// structured value, rooted path, or UTF-8 homoglyph across chunks.
#[cfg(test)]
const SCREEN_CHUNK: usize = 64 * 1024;

/// One closed secret-shape matcher: a byte-local needle (spelled in
/// lowercase — the sweep matches over the folded envelope, r3 F1)
/// plus its closed disclosure reason. The bare `bearer ` scheme
/// (r6 F7) needs no `authorization:` prefix — the token itself is the
/// disclosure.
const NEEDLES: [(&str, &str); 8] = [
    ("-----begin rsa private key-----", "private-key-block"),
    ("-----begin ec private key-----", "private-key-block"),
    ("-----begin openssh private key-----", "private-key-block"),
    ("-----begin private key-----", "private-key-block"),
    ("-----begin pgp private key block-----", "private-key-block"),
    ("authorization:", "credential-header"),
    ("bearer ", "credential-header"),
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
/// drive, UNC, and user-profile spellings moved to the SHAPE matchers
/// below (`contains_drive_letter_path`, `contains_unc_path`,
/// `contains_posix_root_path`, r4 F1/r6 F4), which the segment list
/// could never cover.
const PRIVATE_PATH_NEEDLES: [&str; 25] = [
    "/home/",
    "/root/",
    "/users/",
    // r6 F5: the canonical secret-bearing dotfile spellings refuse
    // anywhere they appear — `.env` (and `.env.production`-style
    // derivatives), `.ssh/`, `.aws/`, `.gnupg/`, and `.config/` trees.
    // The same fail-closed trade-off as the segment needles above: a
    // benign `.env`-shaped substring cannot be proven harmless from
    // bytes alone.
    ".env",
    ".ssh",
    ".aws",
    ".gnupg",
    ".config",
    // omp r8 F4 + devin r8 F1 (same class): the conventional
    // credential-bearing dotfile class refuses, not only an enumerated
    // prefix list. The needles below cover the conventional credential
    // stores (`.pgpass`, `.my.cnf`, `.kube/`, `.docker/`, `.gitconfig`,
    // `.boto`, `.s3cfg`, `.dockercfg`, `.terraformrc`, `.vault-token`,
    // `.azure/`, `.git/config`); the `.git` stem also covers the git
    // config spelling. The closed map stays reviewed by hand — a
    // Existing substring refusals remain conservative; the component
    // matcher below also covers the unlisted hidden-file class.
    ".netrc",
    ".npmrc",
    ".pypirc",
    ".git-credentials",
    ".gitconfig",
    ".keystore",
    ".pgpass",
    ".my.cnf",
    ".kube",
    ".docker",
    ".git",
    ".boto",
    ".s3cfg",
    ".dockercfg",
    ".terraformrc",
    ".vault-token",
    ".azure",
];

/// The closed confusable (homoglyph) fold (r6 F7): the Cyrillic and
/// Greek letters whose glyphs are indistinguishable from an ASCII
/// letter, mapped to that letter so `pаssword` (Cyrillic `а`) is the
/// SAME key spelling as `password`. The map is closed and reviewed by
/// hand — it is a spelling fold, not a Unicode normalization: every
/// entry is a two-byte UTF-8 sequence whose replacement keeps the scan
/// byte-local and deterministic.
const CONFUSABLES: [([u8; 2], u8); 43] = [
    // Cyrillic lowercase lookalikes.
    ([0xd0, 0xb0], b'a'), // а U+0430
    ([0xd0, 0xb5], b'e'), // е U+0435
    ([0xd0, 0xbe], b'o'), // о U+043E
    ([0xd1, 0x80], b'p'), // р U+0440
    ([0xd1, 0x81], b'c'), // с U+0441
    ([0xd1, 0x83], b'y'), // у U+0443
    ([0xd1, 0x85], b'x'), // х U+0445
    ([0xd1, 0x96], b'i'), // і U+0456
    ([0xd1, 0x95], b's'), // ѕ U+0455
    ([0xd1, 0x98], b'j'), // ј U+0458
    ([0xd2, 0xbb], b'h'), // һ U+04BB
    ([0xd4, 0x9b], b'q'), // ԛ U+051B
    ([0xd4, 0x9d], b'w'), // ԝ U+051D
    // Cyrillic uppercase lookalikes (fold straight to the lowercase
    // ASCII letter — the sweep is case-insensitive).
    ([0xd0, 0x90], b'a'), // А U+0410
    ([0xd0, 0x95], b'e'), // Е U+0415
    ([0xd0, 0x9e], b'o'), // О U+041E
    ([0xd0, 0xa0], b'p'), // Р U+0420
    ([0xd0, 0xa1], b'c'), // С U+0421
    ([0xd0, 0xa3], b'y'), // У U+0423
    ([0xd0, 0xa5], b'x'), // Х U+0425
    ([0xd0, 0x86], b'i'), // І U+0406
    ([0xd0, 0x85], b's'), // Ѕ U+0405
    ([0xd0, 0x88], b'j'), // Ј U+0408
    // Greek lowercase lookalikes.
    ([0xce, 0xb1], b'a'), // α U+03B1
    ([0xce, 0xb5], b'e'), // ε U+03B5
    ([0xce, 0xb9], b'i'), // ι U+03B9
    ([0xce, 0xba], b'k'), // κ U+03BA
    ([0xce, 0xbd], b'n'), // ν U+03BD
    ([0xce, 0xbf], b'o'), // ο U+03BF
    ([0xcf, 0x81], b'p'), // ρ U+03C1
    ([0xcf, 0x84], b't'), // τ U+03C4
    ([0xcf, 0x85], b'u'), // υ U+03C5
    ([0xcf, 0x87], b'x'), // χ U+03C7
    // Greek uppercase lookalikes.
    ([0xce, 0x91], b'a'), // Α U+0391
    ([0xce, 0x95], b'e'), // Ε U+0395
    ([0xce, 0x99], b'i'), // Ι U+0399
    ([0xce, 0x9a], b'k'), // Κ U+039A
    ([0xce, 0x9d], b'n'), // Ν U+039D
    ([0xce, 0x9f], b'o'), // Ο U+039F
    ([0xce, 0xa1], b'p'), // Ρ U+03A1
    ([0xce, 0xa4], b't'), // Τ U+03A4
    ([0xce, 0xa5], b'u'), // Υ U+03A5
    ([0xce, 0xa7], b'x'), // Χ U+03A7
];

/// Translate the closed lookalike set to lowercase ASCII (r6 F7).
/// Other UTF-8 bytes pass through untouched; no offset into the original
/// envelope is used after this deterministic rewrite.
fn fold_confusables(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let pair = [bytes[index], *bytes.get(index + 1).unwrap_or(&0)];
        if let Some((_, letter)) = CONFUSABLES.iter().find(|(needle, _)| *needle == pair) {
            out.push(*letter);
            index += 2;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    out
}

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

/// Whether one folded envelope position starts a boundary-rooted UNC
/// absolute path (r6 F4 / omp F2): a separator run of two or more `\`
/// or `/` bytes at the envelope start, after whitespace, or after a
/// quote (the JSON spelling doubles every backslash, so `\\server`
/// serializes as a four-backslash run — the run is consumed whole),
/// followed by a server segment, another separator run, and a share
/// segment. UNC is absolute by construction, so ANY share content is a
/// host-layout disclosure — `\\fileserver\hr\salaries.xlsx` and
/// `//server/share/file.txt` refuse. Scheme-qualified URLs and prose
/// fractions (`and//or`) sit behind non-boundary bytes and never match;
/// a boundary-rooted protocol-relative URL refuses conservatively.
fn contains_unc_path(folded: &[u8]) -> bool {
    for index in 0..folded.len() {
        let separator = |byte: Option<&u8>| matches!(byte, Some(&b'\\') | Some(&b'/'));
        if !separator(folded.get(index)) || !separator(folded.get(index + 1)) {
            continue;
        }
        if !path_boundary(folded, index) {
            continue;
        }
        let ordinary = |byte: Option<&u8>| {
            // r8 (omp F3): the span test is byte-local, so a quote byte
            // (or its serialized escape) cannot continue a real server
            // or share segment — the former span test let a following
            // quote truncate the match and ship the share name.
            byte.is_some_and(|byte| {
                !matches!(byte, b'\\' | b'/' | b'"' | b'\'' | b'>' | b'<')
                    && !byte.is_ascii_whitespace()
            })
        };
        let mut cursor = index;
        while separator(folded.get(cursor)) {
            cursor += 1;
        }
        let server_start = cursor;
        while ordinary(folded.get(cursor)) {
            cursor += 1;
        }
        if cursor == server_start {
            continue;
        }
        if !separator(folded.get(cursor)) {
            continue;
        }
        while separator(folded.get(cursor)) {
            cursor += 1;
        }
        if ordinary(folded.get(cursor)) {
            return true;
        }
    }
    false
}

/// Additional single-segment rooted-path signals. Multi-segment POSIX
/// paths already refuse by shape, without an enumerable vocabulary.
/// These also catch `/secret.conf` and `/credentials.json` directly.
const POSIX_SIGNAL_STEMS: [&str; 14] = [
    "private",
    "secret",
    "credential",
    "finance",
    "passwd",
    "shadow",
    "auth",
    "key",
    "ssh",
    "pem",
    "cert",
    "db",
    "backup",
    "password",
];

/// Whether one path segment starts with a signal stem (with the
/// optional plural `s` and a non-lowercase continuation byte).
fn segment_carries_signal(segment: &[u8]) -> bool {
    for stem in POSIX_SIGNAL_STEMS {
        let stem = stem.as_bytes();
        if segment.len() < stem.len() || &segment[..stem.len()] != stem {
            continue;
        }
        let mut rest = &segment[stem.len()..];
        if rest.first() == Some(&b's') {
            rest = &rest[1..];
        }
        match rest.first() {
            None => return true,
            Some(byte) if !byte.is_ascii_lowercase() => return true,
            _ => {}
        }
    }
    false
}

/// The narrow lexical JSON-pointer exception: a public model/schema
/// collection followed by a numeric index, with identifier-only suffix
/// segments. No filename or arbitrary absolute directory is admitted.
fn is_indexed_pointer(path: &[u8]) -> bool {
    let mut segments = path.split(|byte| *byte == b'/').skip(1);
    let Some(root) = segments.next() else {
        return false;
    };
    if !matches!(
        root,
        b"definitions" | b"modules" | b"properties" | b"items" | b"targets"
    ) {
        return false;
    }
    let Some(index) = segments.next() else {
        return false;
    };
    !index.is_empty()
        && index.iter().all(u8::is_ascii_digit)
        && segments.all(|segment| {
            !segment.is_empty()
                && segment
                    .iter()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        })
}

/// A boundary-rooted multi-segment POSIX path fails closed regardless
/// of vocabulary (r6/omp F3), except indexed JSON pointers. API routes
/// have the same lexical shape and may refuse: the accepted availability
/// collateral is preferable to leaking another unlisted host path.
fn contains_posix_root_path(folded: &[u8]) -> bool {
    let boundary = |index: usize| path_boundary(folded, index);
    for index in 0..folded.len() {
        if folded[index] != b'/' {
            continue;
        }
        let home_relative = index > 0 && folded[index - 1] == b'~' && boundary(index - 1);
        if !boundary(index) && !home_relative {
            continue;
        }
        let mut end = index + 1;
        while end < folded.len()
            && !folded[end].is_ascii_whitespace()
            && !matches!(folded[end], b'"' | b'\'' | b')' | b']' | b',' | b'}' | b'>')
        {
            end += 1;
        }
        let path = &folded[index..end];
        let tail = &path[1..];
        if !tail.is_empty()
            && (home_relative
                || (tail.contains(&b'/') && !is_indexed_pointer(path))
                || segment_carries_signal(tail))
        {
            return true;
        }
    }
    false
}

/// Fail-closed path-boundary semantics (r8, omp F3 + devin F1). The
/// former positive list of "characters that may open a path" kept
/// lagging behind real prose (markdown code spans, angle quotes, and
/// ordinary punctuation let rooted paths ship). The rule is now the
/// negation: a path candidate is boundary-rooted when the byte before
/// it CANNOT continue a relative path segment — i.e. anything that is
/// not a segment character (`alnum . - _ ~`). Only the first slash of
/// a separator run needs checking; revisiting the run would be quadratic.
/// Only exact hierarchical web/project schemes are exempt.
fn path_boundary(bytes: &[u8], index: usize) -> bool {
    if index == 0 {
        return true;
    }
    let previous = bytes[index - 1];
    if matches!(previous, b'/' | b'\\') {
        return false;
    }
    if previous == b':' {
        // Scheme-colon exemption (r8: omp F3/devin F1): a real
        // hierarchical WEB URL (`http`/`https`/the project's own
        // `lekalo` scheme, followed by `//`) is not a filesystem
        // reference. A bare `file:/etc/shadow` colon, and any other
        // scheme spelling, stays a boundary — `file`/`ssh`/`ftp` name
        // host paths, so their colon must neutralize the check.
        let prefix = &bytes[..index - 1];
        let start = prefix
            .iter()
            .rposition(|byte| !byte.is_ascii_alphanumeric() && !matches!(byte, b'+' | b'-' | b'.'))
            .map_or(0, |at| at + 1);
        if bytes.get(index + 1) == Some(&b'/')
            && bytes.get(index + 2).is_some_and(|byte| {
                !byte.is_ascii_whitespace() && !matches!(byte, b'/' | b'\\' | b'"' | b'\'')
            })
            && matches!(&prefix[start..], b"http" | b"https" | b"lekalo")
        {
            return false;
        }
        return true;
    }
    !matches!(previous, b'.' | b'-' | b'_' | b'~') && !previous.is_ascii_alphanumeric()
}

/// A hidden-file component is a private-path class, not an ever-growing
/// list of credential-product names. Dots inside normal identifiers,
/// versions, filenames, and decimal numbers are not component openers.
fn contains_dotfile_path(bytes: &[u8]) -> bool {
    bytes.iter().enumerate().any(|(index, byte)| {
        *byte == b'.'
            && bytes
                .get(index + 1)
                .is_some_and(|next| next.is_ascii_alphanumeric() || *next == b'_')
            && (path_boundary(bytes, index)
                || index > 0 && matches!(bytes[index - 1], b'/' | b'\\'))
    })
}

/// Screen one outgoing envelope. `Ok(())` ships; `Err(reason)` refuses
/// the whole output with `mcp.sensitive-output`. The screen is total:
/// every byte of the envelope (payload, diagnostics, and scope members)
/// passes the same closed matchers.
pub fn screen_envelope(envelope: &str) -> Result<(), &'static str> {
    // The transport bounds response size. Scan its complete byte span:
    // fixed overlap cannot preserve arbitrary compound keys/paths.
    screen_bytes(envelope.as_bytes(), false)?;
    // Serialized whitespace/quotes are not client-visible boundaries.
    // Screen every decoded JSON string too, including member names, so
    // real newlines/tabs and Unicode escapes cannot hide a private path.
    if let Ok(document) = serde_json::from_str::<serde_json::Value>(envelope) {
        let mut pending = vec![&document];
        while let Some(value) = pending.pop() {
            match value {
                serde_json::Value::String(text) => screen_bytes(text.as_bytes(), true)?,
                serde_json::Value::Array(items) => pending.extend(items),
                serde_json::Value::Object(members) => {
                    for (key, value) in members {
                        screen_bytes(key.as_bytes(), true)?;
                        pending.push(value);
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// `decoded` marks the per-JSON-string pass: value spans there are the
/// COMPLETE assigned value, so an empty quoted or composite prefix
/// followed by material is a nonempty assignment (r8–r10). The raw serialized
/// envelope keeps the lenient closer rule — its spans run into the
/// envelope's own quote/brace structure, and the decoded pass owns the
/// completeness judgement.
fn screen_bytes(bytes: &[u8], decoded: bool) -> Result<(), &'static str> {
    // One folded copy carries every matcher (r3 F1): the confusable
    // fold (r6 F7) is applied before the case fold, so `pаssword`
    // (Cyrillic `а`) and `authorization:` spelled with a homoglyph are
    // the same bytes as their ASCII spellings for every matcher below.
    // All matchers consume the same translated spelling; no raw-offset
    // assumptions are made after a multi-byte lookalike is folded.
    let translated = fold_confusables(bytes);
    let folded: Vec<u8> = translated
        .iter()
        .map(|byte| byte.to_ascii_lowercase())
        .collect();
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
    // r4 F1/r6 F4: private absolute paths are matched by shape too —
    // every drive-letter path, boundary-rooted UNC path, and rooted
    // POSIX directory spelling (with the indexed-pointer exception).
    if contains_drive_letter_path(&folded)
        || contains_unc_path(&folded)
        || contains_posix_root_path(&folded)
        || contains_dotfile_path(&folded)
    {
        return Err("private-absolute-path");
    }
    if contains_assigned_token(&translated, decoded) {
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

/// Credential stems in separator-canonical form (ASCII-folded, `_` and
/// `-` removed from identifier keys only). R6 matches these anywhere in
/// an assigned identifier: plurals and arbitrary compound prefixes or
/// tails no longer need enumeration. Historical explicit compounds are
/// retained, including the reviewed AWS access-key-id spelling. Any
/// non-empty assigned payload refuses; short values are not authorized
/// for disclosure merely because they miss a minimum secret length.
const ASSIGNMENT_KEYS: [&str; 21] = [
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
    "credential",
    "accesskey",
    "clientsecret",
    "dbpassword",
    "awssecretaccesskey",
    "rootpassword",
    "secretvalue",
    "awsaccesskeyid",
];

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

/// Closing punctuation is structural only if the COMPLETE decoded
/// remainder contains no material (r9/r10); inspecting its first byte
/// would exempt `[]CANARY`, `{}CANARY`, and `]CANARY` alike.
fn has_nonstructural_material(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .any(|byte| !byte.is_ascii_whitespace() && !matches!(byte, b',' | b'}' | b']'))
}

/// Whether a value span (already whitespace-trimmed) carries a
/// non-empty assigned token: a quote-equivalent-wrapped value closes
/// with the SAME opening quote spelling and escape context (`password="x"`, `password='x'`,
/// `{"password":"x"}` — the escaped spellings serialize as `\"` and
/// a lone backslash no longer breaks the scan, r4 F1); an unwrapped
/// decoded value refuses material anywhere after structural prefixes, and a composite
/// value (`[`, `{`) carries assigned material unless it is exactly
/// empty (r6 F1: `{"password":["hunter2"]}` and
/// `{"secrets":{"a":"x"}}` refuse; `[]`/`{}` mirror the empty-string
/// rule and ship). An isolated opener carries no material. Once content
/// precedes another quote boundary, fail closed even for an unterminated
/// embedded assignment; the enclosing JSON quote is not proof of emptiness.
fn carries_token(bytes: &[u8], decoded: bool) -> bool {
    if let Some(width) = quote_equivalent(bytes, 0) {
        let opener = &bytes[..width];
        let mut cursor = width;
        while cursor < bytes.len() {
            // For an ordinary quote, skip escaped characters; for the
            // serialized \" spelling, a preceding backslash pair is
            // escaped content, not a delimiter. The other quote kind
            // is always content (r7: a leading apostrophe is not empty).
            if bytes[cursor..].starts_with(opener) {
                // Quoted material always carries content.
                if cursor > width {
                    return true;
                }
                // An empty quoted prefix establishes emptiness only for
                // the COMPLETE assignment (r8/r9): punctuation after it
                // cannot hide later material (`password=""]R9CANARY`).
                // Only a wholly structural/whitespace trailer stays empty,
                // preserving complete empty JSON members such as
                // `{"password":""}` inside a decoded string. The raw
                // serialized span contains the envelope's own structure;
                // the decoded pass owns this completeness judgement.
                if !decoded {
                    return false;
                }
                return has_nonstructural_material(&bytes[cursor + opener.len()..]);
            }
            // Once material precedes another quote, fail closed too:
            // it may be the enclosing JSON string's delimiter after an
            // unterminated embedded assignment (an existing refusal).
            if cursor > width && quote_equivalent(bytes, cursor).is_some() {
                return true;
            }
            if bytes[cursor] == b'\\' && cursor + 1 < bytes.len() {
                cursor += 2;
            } else {
                cursor += 1;
            }
        }
        return false;
    }
    match bytes.first() {
        // r6 F1/r10: structured payloads refuse unless they are empty
        // AND complete. The raw pass still permits envelope structure;
        // the decoded pass rejects material after an empty prefix.
        Some(b'[') => {
            bytes.get(1) != Some(&b']') || (decoded && has_nonstructural_material(&bytes[2..]))
        }
        Some(b'{') => {
            bytes.get(1) != Some(&b'}') || (decoded && has_nonstructural_material(&bytes[2..]))
        }
        Some(_) if decoded => has_nonstructural_material(bytes),
        Some(byte) => !byte.is_ascii_whitespace() && !matches!(byte, b',' | b'}' | b']'),
        None => false,
    }
}

/// Only the quoted JSON budget member `"tokens":35` is a count.
/// Do not exempt numeric passwords, arbitrary compounds, quoted token
/// strings, or config assignments such as `tokens=12345` (r6 F2).
/// A decoded value must be complete: structural punctuation after the
/// digits cannot hide later credential material (r11). The raw pass
/// permits later envelope fields; the decoded pass owns completeness.
fn is_token_count(key: &[u8], quoted: bool, separator: u8, value: &[u8], decoded: bool) -> bool {
    if key != b"tokens" || !quoted || separator != b':' {
        return false;
    }
    let digits = value
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    let remainder = trim_space(&value[digits..]);
    digits > 0
        && matches!(remainder.first(), None | Some(b',' | b'}' | b']'))
        && (!decoded || !has_nonstructural_material(remainder))
}

fn trim_space(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    bytes
}

/// Whole-identifier admission avoids enumerating every compound or
/// spelling (r6 F2/F3): a credential stem anywhere in an assigned key
/// refuses. `mysecret`, `secret_value`, `db_passwords`, `rootpassword`,
/// and every `*key` spelling therefore use one rule. The bare `key`
/// member is not a credential, but a non-empty prefix before `key` is
/// conservative credential evidence (even benign `monkey=x` may refuse).
fn credential_key(key: &[u8], raw_key: &[u8]) -> bool {
    // Preserve only the established contiguous, explicitly negated
    // pseudo-key control, not separator-injected credential spellings.
    if key == b"notapassword" && raw_key == key {
        return false;
    }
    ASSIGNMENT_KEYS
        .iter()
        .any(|stem| contains(key, stem.as_bytes()))
        || key.windows(3).skip(1).any(|part| part == b"key")
}

fn contains_assigned_token(translated: &[u8], decoded: bool) -> bool {
    // Keep this a byte scan: unrelated non-ASCII text must not disable
    // credential matching elsewhere in the envelope.
    let text: Vec<u8> = translated.iter().map(u8::to_ascii_lowercase).collect();
    let identifier = |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-');
    let mut cursor = 0;
    while cursor < text.len() {
        if !identifier(text[cursor]) {
            cursor += 1;
            continue;
        }
        let start = cursor;
        while cursor < text.len() && identifier(text[cursor]) {
            cursor += 1;
        }
        let raw_key = &text[start..cursor];
        // Canonicalize the identifier ONLY. Deleting separators from
        // values would turn a real credential such as `"_"` into an
        // empty string and silently exempt its non-empty payload.
        let key: Vec<u8> = raw_key
            .iter()
            .filter(|byte| !matches!(byte, b'_' | b'-'))
            .copied()
            .collect();
        if !credential_key(&key, raw_key) {
            continue;
        }
        let closing_quote = quote_equivalent(&text, cursor);
        let quoted = start > 0 && text[start - 1] == b'"' && closing_quote.is_some();
        if let Some(width) = closing_quote {
            cursor += width;
        }
        while text.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        let separator = match text.get(cursor) {
            Some(byte @ (b':' | b'=')) => *byte,
            _ => continue,
        };
        cursor += 1;
        let value = trim_space(&text[cursor..]);
        if !is_token_count(raw_key, quoted, separator, value, decoded)
            && carries_token(value, decoded)
        {
            return true;
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
        // letter and boundary-rooted POSIX paths with a private signal,
        // not only the enumerated user roots the r2/r3 needles covered.
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
    fn unc_absolute_paths_refuse() {
        // r6 F4 / omp F2: a UNC path is absolute by construction — any
        // server\share spelling refuses, whatever the share carries.
        for envelope in [
            "see \\\\fileserver\\hr\\salaries.xlsx now",
            "//server/share/file.txt",
            "{\"note\":\"\\\\\\\\fileserver\\\\hr\\\\salaries.xlsx\"}",
            "log at //FILESERVER/ops/keys.pem",
        ] {
            assert_eq!(
                screen_envelope(envelope),
                Err("private-absolute-path"),
                "the UNC spelling must refuse: {envelope}"
            );
        }
        // Protocol-relative URLs sit behind a scheme colon and prose
        // fractions behind a letter — neither is boundary-rooted.
        assert_eq!(
            screen_envelope("{\"url\":\"https://example.com/a\"}"),
            Ok(())
        );
        assert_eq!(screen_envelope("{\"ratio\":\"and//or\"}"), Ok(()));
    }

    #[test]
    fn widened_posix_private_signals_refuse() {
        // r6/omp F3: the signal vocabulary covers the private-bearing
        // segment spellings beyond the r4 five — plural forms, file
        // extensions, auth/keys/cert/backup segments, and the `~/` home
        // root.
        for envelope in [
            "{\"note\":\"/etc/shadow is world-readable\"}",
            "{\"note\":\"keys at /srv/secrets/api.key\"}",
            "{\"note\":\"config /opt/credentials.json leaked\"}",
            "{\"note\":\"db at /var/lib/app/private.db\"}",
            "{\"note\":\"see /usr/local/etc/secret.conf\"}",
            "{\"note\":\"dump /mnt/backups/passwords.txt\"}",
            "{\"note\":\"tail /var/log/auth.log weekly\"}",
            "{\"note\":\"pem at /opt/company/keys/service.pem\"}",
            "{\"note\":\"notes at ~/private/notes.txt\"}",
        ] {
            assert_eq!(
                screen_envelope(envelope),
                Err("private-absolute-path"),
                "the private POSIX spelling must refuse: {envelope}"
            );
        }
        // Numeric-index JSON pointers stay available, but an arbitrary
        // rooted directory or API route has the same private-path shape.
        assert_eq!(screen_envelope("{\"ptr\":\"/definitions/20\"}"), Ok(()));
        assert_eq!(
            screen_envelope("{\"ptr\":\"/definitions/20/fields\"}"),
            Ok(())
        );
        for path in ["/api/v1/users", "/var/lib/app/data.sqlite", "~/notes.txt"] {
            assert_eq!(screen_envelope(path), Err("private-absolute-path"));
        }
    }

    #[test]
    fn dotfile_spellings_refuse() {
        // r6 F5: the canonical secret-bearing dotfile spellings refuse
        // anywhere they appear.
        for envelope in [
            "reads /.env for secrets",
            "config at .env.production",
            "found in ~/.ssh/id_rsa",
            "uses .aws/credentials profile",
            "key at .ssh/id_rsa",
            "ring at .gnupg/pubring.kbx",
            "preset in .config/generic/app.toml",
            r"found in .ssh\id_rsa",
            r"uses .aws\credentials profile",
        ] {
            assert_eq!(
                screen_envelope(envelope),
                Err("private-absolute-path"),
                "the dotfile spelling must refuse: {envelope}"
            );
        }
    }

    #[test]
    fn decoded_private_paths_and_punctuation_boundaries_refuse() {
        for path in [r"\\server\share\x", "/var/log/auth.log"] {
            for prefix in ["", "path=", "see (", "path:", "path;", "\n", "\t"] {
                let description = format!("{prefix}{path}");
                let envelope = serde_json::json!({"description": description}).to_string();
                assert_eq!(
                    screen_envelope(&envelope),
                    Err("private-absolute-path"),
                    "{envelope}"
                );
            }
        }
        for path in [
            ".netrc",
            ".npmrc",
            ".pypirc",
            ".git-credentials",
            ".keystore",
        ] {
            assert_eq!(
                screen_envelope(&serde_json::json!({"description": path}).to_string()),
                Err("private-absolute-path")
            );
        }
        for control in [
            "https://example.com/a/b",
            "lekalo://planner/summary",
            "/definitions/20",
            "/modules/2/id",
            "lekalo/modules/planner/commands.yaml",
            "and//or",
        ] {
            assert_eq!(
                screen_envelope(&serde_json::json!({"description": control}).to_string()),
                Ok(()),
                "{control}"
            );
        }
    }

    #[test]
    fn r8_boundaries_filesystem_urls_and_dotfile_peers_refuse() {
        // omp F3/devin F1: markdown code spans, angle quotes, and
        // ordinary prose punctuation cannot open a live rooted path.
        for prefix in [
            "`", "<", ">", "|", "!", ")", "]", "}", "%", "&", "#", "@", "$", "^", "*", "?",
        ] {
            for path in ["/var/log/auth.log", r"\\server\share\x"] {
                let description = format!("{prefix}{path}");
                let envelope = serde_json::json!({"description": description}).to_string();
                assert_eq!(
                    screen_envelope(&envelope),
                    Err("private-absolute-path"),
                    "{description}"
                );
            }
        }
        // Filesystem URLs are filesystem references, not benign web
        // URLs: the scheme colon does not exempt the rooted-path
        // matchers, and a bare scheme colon (no `//`) is a boundary.
        for spelling in [
            "file:///var/log/auth.log",
            "file:///opt/company/keys/service.pem",
            "file://internal-server/private-share/salaries.txt",
            "file:/etc/shadow",
            "ssh:/etc/passwd",
            "x|/etc/shadow",
            "nothttp://etc/shadow",
            "https:///etc/shadow",
        ] {
            let envelope = serde_json::json!({"description": spelling}).to_string();
            assert_eq!(
                screen_envelope(&envelope),
                Err("private-absolute-path"),
                "{spelling}"
            );
        }
        // An empty quoted prefix is emptiness only for the COMPLETE
        // assignment: material after the closer is the assigned value.
        for assignment in [
            r#"password=""R8CANARY"#,
            "password=''R8CANARY",
            r#"{"password":""R8CANARY}"#,
            "{\"password\":''R8CANARY}",
        ] {
            let envelope = serde_json::json!({"description": assignment}).to_string();
            assert_eq!(
                screen_envelope(&envelope),
                Err("assigned-credential"),
                "{assignment}"
            );
        }
        // omp F4: the conventional credential-bearing dotfile class
        // refuses, not only the enumerated five.
        for dotfile in [
            ".pgpass",
            ".my.cnf",
            ".kube/config",
            ".docker/config.json",
            ".gitconfig",
            ".boto",
            ".s3cfg",
            ".dockercfg",
            ".terraformrc",
            ".vault-token",
            ".azure/config",
            ".git/config",
            ".credentials",
            ".gitcookies",
            ".curlrc",
            "nested/.service-auth/settings",
        ] {
            let envelope = serde_json::json!({"description": dotfile}).to_string();
            assert_eq!(
                screen_envelope(&envelope),
                Err("private-absolute-path"),
                "{dotfile}"
            );
        }
        // Controls: benign hierarchical web URLs, complete empty
        // assignments, and relative-path continuations stay available.
        for control in [
            "https://x.test/a/b",
            "http://example.com/path",
            "lekalo://planner/symbol/x",
            r#"password="""#,
            "{\"password\":\"\"}",
            "x./var relative",
            "a-b/c",
            "a_b/c",
            "planner.focus_task",
            "model.v0.8.0.json",
            "dev.lekalo.inspect@0.8.0",
        ] {
            assert_eq!(
                screen_envelope(&serde_json::json!({"description": control}).to_string()),
                Ok(()),
                "{control}"
            );
        }
    }

    #[test]
    fn empty_quoted_credentials_do_not_hide_material_after_structural_tails() {
        for quote in ["\"", "'", "\\\""] {
            for tail in ["]R9CANARY", "}R9CANARY", ",R9CANARY", " } ], \tR9CANARY"] {
                let assignment = format!("password={quote}{quote}{tail}");
                let envelope = serde_json::json!({"description": assignment}).to_string();
                assert_eq!(
                    screen_envelope(&envelope),
                    Err("assigned-credential"),
                    "{assignment}"
                );
            }
        }
        // Real empty JSON values and complete empty assignments remain empty.
        for control in [
            r#"password="""#,
            "password=''",
            r#"{"password":""}"#,
            r#"[{"password":""}]"#,
        ] {
            assert_eq!(screen_envelope(control), Ok(()), "{control}");
            let envelope = serde_json::json!({"description": control}).to_string();
            assert_eq!(screen_envelope(&envelope), Ok(()), "{control}");
        }
    }

    #[test]
    fn empty_composites_and_structural_prefixes_do_not_hide_credential_material() {
        for prefix in ["[]", "{}", "[]{}", "{}[]", "]", "}", ",", " } ], \t"] {
            for key in [
                "password",
                "db_passwords",
                "client-secret",
                "awsAccessKeyId",
            ] {
                for tail in ["R10CANARY", "]R10CANARY", " } ], \tR10CANARY"] {
                    let assignment = format!("{key}={prefix}{tail}");
                    let envelope = serde_json::json!({"description": assignment}).to_string();
                    assert_eq!(
                        screen_envelope(&envelope),
                        Err("assigned-credential"),
                        "{assignment}"
                    );
                }
            }
        }
        // Complete empty composites and structural-only trailers carry no material.
        for control in [
            "password=[]",
            "password={}",
            "password=[] } ], \t",
            "password={} } ], \t",
            "password=] }, \t",
            r#"{"password":[]}"#,
            r#"[{"password":{}}]"#,
        ] {
            assert_eq!(screen_envelope(control), Ok(()), "{control}");
            let envelope = serde_json::json!({"description": control}).to_string();
            assert_eq!(screen_envelope(&envelope), Ok(()), "{control}");
        }
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
        // r6 F7: a bare bearer scheme needs no `authorization:` prefix —
        // the token itself is the disclosure.
        assert_eq!(
            screen_envelope("Bearer eyJhbGciOiJIUzI1NiJ9.sig.payload"),
            Err("credential-header")
        );
        assert_eq!(
            screen_envelope("{\"note\":\"bearer ghp_0123456789abcdef\"}"),
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
        // The explicit contiguous negated pseudo-key control stays
        // safe, whatever its ASCII casing. Other compound prefixes do
        // not grant permission to disclose an assigned credential.
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
        // Separator/case canonicalization gives snake/kebab/camel and
        // unseparated spellings one identifier-key rule (r4/r6).
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
    fn structured_composite_credential_values_refuse() {
        // r6 F1: a matched credential key with an array/object value
        // discloses every member — the same detected key spelling must
        // refuse the composite payload, not only scalar tokens.
        for envelope in [
            "{\"password\":[\"hunter2\"]}",
            "{\"password\":{\"hash\":\"hunter2\"}}",
            "secrets: {a: hunter2}",
            "{\"credential\":{\"value\":\"hunter2\"}}",
            "tokens=[ghp_0123456789abcdef]",
            "{\"clientSecret\":{\"prod\":\"synthcamel9\"}}",
            "{\"password\":\"_\"}",
            "{\"password\":\"---\"}",
            "password=___",
            "password=@synthetic",
            "password=$synthetic",
        ] {
            assert_eq!(
                screen_envelope(envelope),
                Err("assigned-credential"),
                "the structured credential value must refuse: {envelope}"
            );
        }
        // The empty composite mirrors the empty-string rule and ships.
        assert_eq!(screen_envelope("{\"password\":[]}"), Ok(()));
        assert_eq!(screen_envelope("{\"password\":{}}"), Ok(()));
    }

    #[test]
    fn plural_and_compound_credential_keys_refuse() {
        // r6 F2: plural spellings and letters-only compound tails still
        // name the credential — the stem is the credential word, the
        // tail is spelling.
        for envelope in [
            "secrets: hunter2hunter2",
            "passwords= hunter2",
            "db_passwords=list",
            "secret_value=x",
            "rootpassword=hunter2",
            "mysecret=hunter2hunter2",
            "arbitrarySecretSuffix=synthetic-r6",
            "unlisted_tokens_value=x",
            "mysecret2=123456",
            "not_a_password=synthetic",
            "passwords=123456",
            "secrets: 7",
            "tokens=123456",
            "tokens:\"35\"",
            "{\"tokens\":\"35\"}",
            "{\"tokens\":123abc}",
            "{\"passwords\":[\"hunter2\",\"hunter3\"]}",
            "aws_access_key_ids=[AKIAIOSFODNN7EXAMPLE]",
            "tokens: [ghp_0123456789abcdef]",
        ] {
            assert_eq!(
                screen_envelope(envelope),
                Err("assigned-credential"),
                "the plural/compound credential spelling must refuse: {envelope}"
            );
        }
        // The explicit NotAPassword pseudo-key and unassigned prose
        // stay safe; generic compound prefixes are not exemptions.
        assert_eq!(screen_envelope("NotAPassword=x"), Ok(()));
        assert_eq!(screen_envelope("the passwordless flow"), Ok(()));
        // A count under the plural spelling ships (r6 F2 live collateral:
        // the pinned context-budget member `"tokens":35` is a quantity,
        // not a credential payload); every other value shape refuses.
        assert_eq!(
            screen_envelope("{\"section\":\"symbol\",\"tokens\":35}"),
            Ok(())
        );
        assert_eq!(screen_envelope("{\"tokens\":0}"), Ok(()));
    }

    #[test]
    fn numeric_token_count_prefixes_do_not_hide_decoded_credential_tails() {
        for tail in [
            "]R11CANARY",
            "}R11CANARY",
            ",R11CANARY",
            " } ], \tR11CANARY",
        ] {
            for (prefix, suffix) in [("", ""), ("{", "}"), ("[{", "}]")] {
                let assignment = format!(r#"{prefix}"tokens":123{tail}{suffix}"#);
                let envelope = serde_json::json!({"description": assignment}).to_string();
                assert_eq!(
                    screen_envelope(&envelope),
                    Err("assigned-credential"),
                    "{assignment}"
                );
            }
        }
        for assignment in [
            r#"{"tokens":123,"value":"R11CANARY"}"#,
            "tokens=123]R11CANARY",
            r#""tokens":123R11CANARY"#,
            "password=R11CANARY",
        ] {
            let envelope = serde_json::json!({"description": assignment}).to_string();
            assert_eq!(screen_envelope(&envelope), Err("assigned-credential"));
        }
    }

    #[test]
    fn complete_numeric_token_counts_and_real_budget_members_ship() {
        for control in [
            r#""tokens":123,"#,
            r#""tokens":123}"#,
            r#""tokens":123]"#,
            r#"{"tokens":123}"#,
            r#"[{"tokens":123}]"#,
            "\"tokens\":123 } ], \t",
        ] {
            assert_eq!(screen_envelope(control), Ok(()), "{control}");
            let envelope = serde_json::json!({"description": control}).to_string();
            assert_eq!(screen_envelope(&envelope), Ok(()), "{control}");
        }
        // Real integer members have later envelope fields, not decoded value tails.
        for envelope in [
            r#"{"tokens":123,"section":"symbol"}"#,
            r#"{"counts":[{"tokens":123},{"tokens":0}],"status":"valid"}"#,
        ] {
            assert_eq!(screen_envelope(envelope), Ok(()), "{envelope}");
        }
    }

    #[test]
    fn the_key_tail_family_refuses_in_every_separator_spelling() {
        // r6 F3: `*_key`, `*-key`, and `*Key` are one credential-key
        // family — underscore, kebab, and camel spellings covered
        // symmetrically.
        for envelope in [
            "ssh-key: hunter2",
            "master_key=m",
            "signing_key=k",
            "license_key=l",
            "encryption-key=hunter2",
            "sshKey=hunter2",
            "masterKey=m2",
            "client-key: synthkey1",
            "{\"licenseKey\":\"synthkey2\"}",
            "sshkey=hunter2",
            "masterkey=m3",
            "unlisted_key_name=x",
            "unlistedKeyName=x",
            "unlisted-key-name=x",
            "unlistedkeyname=x",
        ] {
            assert_eq!(
                screen_envelope(envelope),
                Err("assigned-credential"),
                "the *_key family spelling must refuse: {envelope}"
            );
        }
        // The bare `key` member ships. Unseparated compounds share the
        // same rule as snake/kebab/camel forms, including conservative
        // collateral on otherwise benign `monkey`/`sortkey` assignments.
        assert_eq!(screen_envelope("{\"key\":\"v\"}"), Ok(()));
        assert_eq!(screen_envelope("{\"key\":\"monkeys\"}"), Ok(()));
        assert_eq!(screen_envelope("monkey=x"), Err("assigned-credential"));
        assert_eq!(screen_envelope("keystone=x"), Ok(()));
        assert_eq!(
            screen_envelope("{\"sortkey\":\"name\"}"),
            Err("assigned-credential")
        );
    }

    #[test]
    fn aws_access_key_id_spellings_refuse() {
        // omp F4: the AWS access-key-id pairing convention is credential
        // material in every separator spelling.
        for envelope in [
            "aws_access_key_id=AKIAIOSFODNN7EXAMPLE",
            "AWS-ACCESS-KEY-ID=AKIAIOSFODNN7EXAMPLE",
            "{\"awsAccessKeyId\":\"AKIAIOSFODNN7EXAMPLE\"}",
            "awsaccesskeyid=AKIAIOSFODNN7EXAMPLE",
        ] {
            assert_eq!(
                screen_envelope(envelope),
                Err("assigned-credential"),
                "the aws access key id spelling must refuse: {envelope}"
            );
        }
    }

    #[test]
    fn confusable_homoglyph_key_spellings_refuse() {
        // r6 F7: Cyrillic/Greek lookalikes are the same key spelling as
        // their ASCII letters — a homoglyph key can no longer smuggle a
        // value past the fold.
        let cyrillic_a = "p\u{0430}ssword=hunter2hunter2"; // pаssword
        assert_eq!(
            screen_envelope(cyrillic_a),
            Err("assigned-credential"),
            "the Cyrillic-а spelling must refuse"
        );
        let cyrillic_o = "t\u{043E}ken=ghp_0123456789"; // tоken
        assert_eq!(
            screen_envelope(cyrillic_o),
            Err("assigned-credential"),
            "the Cyrillic-о spelling must refuse"
        );
        let greek_alpha = "secret\u{03B1}value=x"; // secretαvalue — α folds to a
        assert_eq!(
            screen_envelope(greek_alpha),
            Err("assigned-credential"),
            "the Greek-α spelling must refuse"
        );
        let homoglyph_header = "\u{0430}uthorization: Bearer abc"; // аuthorization:
        assert_eq!(
            screen_envelope(homoglyph_header),
            Err("credential-header"),
            "the homoglyph header spelling must refuse"
        );
        // Ordinary non-ASCII text without a credential shape ships.
        assert_eq!(screen_envelope("{\"note\":\"Привет, мир\"}"), Ok(()));
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
            r#"{"description":"{\"password\":\"'R7CANARY\"}"}"#,
            r#"password='"R7CANARY'"#,
            r#"password="\"R7CANARY""#,
            r#"{"description":"password=\"unterminatedR7CANARY"}"#,
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
    fn long_shapes_and_utf8_survive_former_chunk_boundaries() {
        for canary in [
            format!("password{}=x", "s".repeat(256)),
            format!("/opt/{}/data.sqlite", "a".repeat(256)),
            "pаssword=hunter2".to_owned(),
        ] {
            let envelope = format!("{} {canary}", "a".repeat(SCREEN_CHUNK - 32));
            assert!(screen_envelope(&envelope).is_err(), "long shape leaked");
        }
        let envelope = format!("{}pаssword=hunter2", " ".repeat(SCREEN_CHUNK - 2));
        assert_eq!(screen_envelope(&envelope), Err("assigned-credential"));
        assert_eq!(
            screen_envelope("日本語 pаssword=hunter2"),
            Err("assigned-credential")
        );
    }

    #[test]
    fn only_the_sealed_kind_blocks_output() {
        assert!(kind_blocks_output(DataKind::Credential));
        assert!(!kind_blocks_output(DataKind::Personal));
        assert!(!kind_blocks_output(DataKind::Public));
    }
}
