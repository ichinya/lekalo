//! Error identity: stable semantic ids, immutable machine codes, and
//! message template ids (issue #62).
//!
//! Ids reuse the accepted #6 qualified-symbol grammar: two or three
//! lowercase `a-z0-9_` segments joined by dots. Codes are exactly
//! `LEK-ERR-NNN`. Neither is ever a diagnostic id, rendered prose, HTTP
//! status, exception class, or sort index: they are the immutable identity
//! every projection must preserve. Codes are unique forever; retired codes
//! are tombstoned in the registry and never reassigned or reused.

/// One stable semantic error id in the accepted #6 qualified grammar.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ErrorId(String);

/// One immutable machine/search error code.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ErrorCode(String);

/// One stable message template reference (catalog text lives outside the
/// canonical identity).
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct MessageTemplateId(String);

impl ErrorId {
    /// Validates the qualified grammar; `None` when malformed.
    pub fn new(text: &str) -> Option<Self> {
        if text.len() > 190 || violates_grammar(text) {
            None
        } else {
            Some(Self(text.to_owned()))
        }
    }

    /// The exact spelling.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl ErrorCode {
    /// Validates the exact `LEK-ERR-NNN` spelling; `None` when malformed.
    pub fn new(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if text.len() != 11 || !text.starts_with("LEK-ERR-") {
            return None;
        }
        bytes[8..]
            .iter()
            .all(u8::is_ascii_digit)
            .then(|| Self(text.to_owned()))
    }

    /// The exact spelling.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl MessageTemplateId {
    /// Validates the dotted lowercase template grammar; `None` when
    /// malformed.
    pub fn new(text: &str) -> Option<Self> {
        let segments: Vec<&str> = text.split('.').collect();
        if text.len() > 190 || segments.len() < 3 || segments.len() > 7 {
            return None;
        }
        segments
            .iter()
            .all(|segment| {
                !segment.is_empty()
                    && segment.len() <= 63
                    && segment.bytes().enumerate().all(|(index, byte)| {
                        byte.is_ascii_lowercase()
                            || byte == b'_'
                            || (byte.is_ascii_digit() && index > 0)
                    })
                    && !segment.ends_with('_')
            })
            .then(|| Self(text.to_owned()))
    }

    /// The exact spelling.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The #6 qualified grammar: `module.name` or `module.group.name`, each
/// segment `a-z0-9_` led by a letter.
fn violates_grammar(text: &str) -> bool {
    let segments: Vec<&str> = text.split('.').collect();
    segments.len() < 2
        || segments.len() > 3
        || segments.iter().any(|segment| {
            segment.is_empty()
                || segment.len() > 63
                || !segment.as_bytes()[0].is_ascii_lowercase()
                || !segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_follow_the_qualified_grammar() {
        assert!(ErrorId::new("planner.task_not_found").is_some());
        assert!(ErrorId::new("planner.group.task_not_found").is_some());
        assert!(ErrorId::new("Planner.task").is_none());
        assert!(ErrorId::new("planner").is_none());
        assert!(ErrorId::new("planner.task.").is_none());
        assert!(ErrorId::new("planner.task.too.many").is_none());
        assert!(ErrorId::new("planner.9task").is_none());
        assert!(ErrorId::new("").is_none());
    }

    #[test]
    fn codes_are_exactly_lek_err_nnn() {
        assert!(ErrorCode::new("LEK-ERR-000").is_some());
        assert!(ErrorCode::new("LEK-ERR-999").is_some());
        assert!(ErrorCode::new("LEK-DIFF-001").is_none());
        assert!(ErrorCode::new("lek-err-001").is_none());
        assert!(ErrorCode::new("LEK-ERR-1").is_none());
        assert!(ErrorCode::new("LEK-ERR-0000").is_none());
        assert!(ErrorCode::new("").is_none());
    }

    #[test]
    fn template_ids_are_dotted_lowercase() {
        assert!(MessageTemplateId::new("planner.error.task-not_found.public").is_none());
        assert!(MessageTemplateId::new("planner.error.task_not_found.public").is_some());
        assert!(MessageTemplateId::new("planner.public").is_none());
        assert!(MessageTemplateId::new("a.b.c.d.e.f.g.h").is_none());
        assert!(MessageTemplateId::new("planner.error.trailing_.public").is_none());
    }
}
