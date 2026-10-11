//! The opaque bounded page cursor (issue #93).
//!
//! A cursor binds the normalized query/filter digest, the recipe, the
//! security policy digest, the immutable index content digest, and the
//! last full ranking tuple. Any mismatch is `search.cursor-invalid`:
//! paging never crosses a generation, a different query, or a policy
//! change. The token carries no paths, snippets, or hidden subjects and
//! grants no read authority.

use serde::{Deserialize, Serialize};

use crate::diagnostics::normalize::build;
use crate::diagnostics::types::{token_value, DataObject};
use crate::diagnostics::DiagnosticSet;
use crate::result::Status;

use super::version::{is_sha256, MAX_CURSOR_BYTES};

/// The registered rule reused for every cursor failure.
pub(crate) const CURSOR_INVALID: &str = "search.cursor-invalid";

/// The typed rejection reason of one cursor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CursorError {
    /// Not decodable/not valid canonical JSON/too large.
    Malformed,
    /// Bound to a different query/filter digest, recipe, policy, or
    /// index generation.
    Mismatch,
}

impl CursorError {
    fn detail(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::Mismatch => "generation-mismatch",
        }
    }
}

/// The `search.cursor-invalid` failure set (no value echo).
pub fn cursor_invalid(error: CursorError) -> DiagnosticSet {
    let mut data = DataObject::new();
    data.insert("detail".to_owned(), token_value(error.detail()));
    match build(CURSOR_INVALID, None, None, data) {
        Ok(diagnostic) => DiagnosticSet::try_from_unsorted(vec![diagnostic], Status::Invalid)
            .unwrap_or_else(|_| crate::result::singleton_set("diagnostics.registry-invalid")),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

/// The binding facts every cursor carries (wire `cursorState`):
/// a closed object — unknown members are refused at every depth.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CursorBinding {
    /// The recipe the page was ranked under.
    pub recipe: String,
    /// The digest of the normalized query plus every filter member.
    pub query_digest: String,
    /// The digest of the admitted security/policy facts.
    pub policy_digest: String,
    /// The immutable content digest of the serving generation.
    pub index_digest: String,
}

/// The last ranking tuple the previous page ended on: a closed object
/// with validated tuple invariants (bounded key, bounded ranks).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LastTuple {
    /// The tier rank of the last emitted card.
    pub tier: u8,
    /// The matched-field rank.
    pub field: u8,
    /// The (reversed) exact-term coverage.
    pub coverage: usize,
    /// The edit-operation count.
    pub edits: u32,
    /// The qualified key of the last emitted card.
    pub key: String,
}

/// The opaque cursor: binding + last tuple, canonical JSON, base64
/// (URL-safe, no padding), with a trailing integrity check.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cursor {
    pub binding: CursorBinding,
    pub last: Option<LastTuple>,
}

impl Cursor {
    /// Encode to the opaque token (bounded).
    pub fn encode(&self) -> Result<String, CursorError> {
        #[derive(Serialize)]
        struct Wire<'a> {
            #[serde(rename = "binding")]
            binding: &'a CursorBinding,
            #[serde(rename = "last", skip_serializing_if = "Option::is_none")]
            last: Option<&'a LastTuple>,
        }
        let wire = Wire {
            binding: &self.binding,
            last: self.last.as_ref(),
        };
        let json = serde_json::to_vec(&wire).map_err(|_| CursorError::Malformed)?;
        if json.len() > MAX_CURSOR_BYTES / 2 {
            return Err(CursorError::Malformed);
        }
        let token = base64_encode(&json);
        let check = super::wire::sha256_digest(token.as_bytes());
        let out = format!("{}:{}", &check[..16], token);
        if out.len() > MAX_CURSOR_BYTES {
            return Err(CursorError::Malformed);
        }
        Ok(out)
    }

    /// Decode and validate one opaque token against the expected
    /// binding; any mismatch is a typed refusal (no value echo).
    pub fn decode(token: &str, expected: &CursorBinding) -> Result<Self, CursorError> {
        if token.len() > MAX_CURSOR_BYTES {
            return Err(CursorError::Malformed);
        }
        let (check, encoded) = token.split_once(':').ok_or(CursorError::Malformed)?;
        if encoded.is_empty() {
            return Err(CursorError::Malformed);
        }
        let json = base64_decode(encoded).ok_or(CursorError::Malformed)?;
        let actual = super::wire::sha256_digest(encoded.as_bytes());
        if !actual[..16].eq_ignore_ascii_case(check) {
            return Err(CursorError::Malformed);
        }
        if json.len() > MAX_CURSOR_BYTES {
            return Err(CursorError::Malformed);
        }
        // Closed-document admission: duplicate keys are rejected before
        // decode (the manifest/generation/producer convention applies to
        // cursors too), and every cursor depth denies unknown fields —
        // the wire structs carry `deny_unknown_fields`.
        let text = std::str::from_utf8(&json).map_err(|_| CursorError::Malformed)?;
        super::wire::reject_duplicate_keys(text).map_err(|_| CursorError::Malformed)?;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            binding: CursorBinding,
            #[serde(rename = "last")]
            last: Option<LastTuple>,
        }
        let wire: Wire = serde_json::from_slice(&json).map_err(|_| CursorError::Malformed)?;
        if wire.binding.recipe != expected.recipe
            || wire.binding.query_digest != expected.query_digest
            || wire.binding.policy_digest != expected.policy_digest
            || wire.binding.index_digest != expected.index_digest
            || !is_sha256(&wire.binding.index_digest)
            || !is_sha256(&wire.binding.query_digest)
        {
            return Err(CursorError::Mismatch);
        }
        if let Some(last) = &wire.last {
            // Tuple invariants: bounded ranks, bounded coverage/edits,
            // and a nonempty bounded key. A forged or drifted tuple is
            // malformed, never paged past.
            let tier_cap = crate::search::rank::Tier::Fuzzy.rank();
            let field_cap = crate::search::rank::FieldPriority::Description.rank();
            if last.tier > tier_cap
                || last.field > field_cap
                || last.coverage > u32::MAX as usize
                || last.edits > 64
                || last.key.is_empty()
                || last.key.len() > 256
            {
                return Err(CursorError::Malformed);
            }
        }
        Ok(Self {
            binding: wire.binding,
            last: wire.last,
        })
    }
}

/// URL-safe base64 without padding.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(triple >> 18) as usize & 63] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(triple >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[triple as usize & 63] as char);
        }
    }
    out
}

/// URL-safe base64 decode without padding.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    const INVALID: u8 = u8::MAX;
    fn value_of(byte: u8) -> u8 {
        match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => INVALID,
        }
    }
    if text.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let bytes = text.as_bytes();
    for chunk in bytes.chunks(4) {
        let mut values = [0u8; 4];
        for (index, byte) in chunk.iter().enumerate() {
            values[index] = value_of(*byte);
            if values[index] == INVALID {
                return None;
            }
        }
        let triple = ((values[0] as u32) << 18)
            | ((values[1] as u32) << 12)
            | ((values[2] as u32) << 6)
            | values[3] as u32;
        out.push((triple >> 16) as u8);
        if chunk.len() > 2 {
            out.push((triple >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(triple as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding() -> CursorBinding {
        CursorBinding {
            recipe: "semantic-search/1".to_owned(),
            query_digest: format!("sha256:{}", "a".repeat(64)),
            policy_digest: format!("sha256:{}", "b".repeat(64)),
            index_digest: format!("sha256:{}", "c".repeat(64)),
        }
    }

    #[test]
    fn round_trips_and_rejects_cross_generation() {
        let expected = binding();
        let cursor = Cursor {
            binding: expected.clone(),
            last: Some(LastTuple {
                tier: 3,
                field: 1,
                coverage: 2,
                edits: 0,
                key: "def:planner.focus_task".to_owned(),
            }),
        };
        let token = cursor.encode().expect("encodes");
        let decoded = Cursor::decode(&token, &expected).expect("decodes");
        assert_eq!(decoded, cursor);
        // A different generation mismatches.
        let mut other = binding();
        other.index_digest = format!("sha256:{}", "d".repeat(64));
        assert_eq!(
            Cursor::decode(&token, &other).unwrap_err(),
            CursorError::Mismatch
        );
        // Garbage is malformed, never guessed.
        assert_eq!(
            Cursor::decode("zzzz", &expected).unwrap_err(),
            CursorError::Malformed
        );
    }

    #[test]
    fn duplicate_and_unknown_cursor_members_are_refused() {
        let expected = binding();
        let cursor = Cursor {
            binding: expected.clone(),
            last: None,
        };
        let token = cursor.encode().expect("encodes");
        // Rebuild the same payload with an unknown member at the top
        // depth, then recompute the public checksum so only closed-field
        // admission and duplicate-key rejection can refuse it.
        let (check, encoded) = token.split_once(':').expect("token");
        let decoded = base64_decode(encoded).expect("decodes");
        let text = String::from_utf8(decoded).expect("utf8");
        for hostile in [
            // Unknown top-level member.
            text.replace("}", ",\"extra\":\"unadmitted\"}"),
            // Duplicate keys at the binding depth.
            text.replace("\"index_digest\"", "\"extra\":1,\"index_digest\""),
            // Duplicate top-level member (last-wins JSON parse would
            // otherwise silently accept it).
            format!(
                "{{{},\"last\":null,\"last\":null}}",
                &text[1..text.len() - 1]
            ),
        ] {
            let reencoded = base64_encode(hostile.as_bytes());
            let digest = super::super::wire::sha256_digest(reencoded.as_bytes());
            let forged = format!("{}:{}", &digest[..16], reencoded);
            assert_eq!(
                Cursor::decode(&forged, &expected).unwrap_err(),
                CursorError::Malformed,
                "hostile cursor body must be refused: {hostile}"
            );
        }
        // Tuple invariants: an out-of-range rank or an unbounded key is
        // malformed, never paged past.
        let with_last = Cursor {
            binding: expected.clone(),
            last: Some(LastTuple {
                tier: 9,
                field: 1,
                coverage: 2,
                edits: 0,
                key: "def:planner.focus_task".to_owned(),
            }),
        }
        .encode()
        .expect("encodes");
        assert_eq!(
            Cursor::decode(&with_last, &expected).unwrap_err(),
            CursorError::Malformed
        );
        let _ = check;
    }
}
