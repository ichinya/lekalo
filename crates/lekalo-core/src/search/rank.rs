//! The frozen `semantic-search/1` normalization and ranking recipe
//! (issue #93).
//!
//! Deterministic lexical retrieval over admitted metadata only:
//!
//! - Normalization is Unicode NFC, ASCII-only case folding, and term
//!   splitting on whitespace plus the ID separators `.`, `_`, `-`, `/`,
//!   `#`. Original bytes and case are preserved for exact identity.
//! - Ranking is a fixed tuple: exact full ID, exact qualified
//!   native/external ID, exact normalized name, all exact lexical terms,
//!   prefix tokens, optional bounded fuzzy tokens. Within a tier:
//!   matched-field priority (id/name/description), greater exact term
//!   coverage, fewer edit operations, then the qualified key in UTF-8
//!   byte order.
//! - Fuzzy is opt-in, labeled `lexical-fuzzy`, bounded at
//!   [`crate::search::version::MAX_FUZZY_EDITS`] edit operations and
//!   [`crate::search::version::MAX_FUZZY_CANDIDATES`] candidate keys.
//! - No embeddings, network model, vector store, or locale-dependent
//!   collation exists anywhere in this module.

use crate::search::version::{MAX_FUZZY_CANDIDATES, MAX_FUZZY_EDITS};

/// The closed matched-field priority (lower sorts first).
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum FieldPriority {
    /// The byte-exact semantic ID matched.
    Id,
    /// The normalized name matched.
    Name,
    /// Approved description metadata matched.
    Description,
}

impl FieldPriority {
    /// The closed rank (smaller is better).
    pub const fn rank(self) -> u8 {
        match self {
            Self::Id => 0,
            Self::Name => 1,
            Self::Description => 2,
        }
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::Name => "name",
            Self::Description => "description",
        }
    }
}

/// The closed exactness tier (lower sorts first). Every exact full ID
/// outranks every lexical or fuzzy result.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Tier {
    /// The query bytes equal the full qualified semantic ID.
    ExactId,
    /// The query bytes equal the qualified native/external identity.
    ExactQualified,
    /// The query equals the normalized name.
    ExactName,
    /// Every query term matched lexically (no edits).
    AllTerms,
    /// Some query terms matched as prefixes.
    Prefix,
    /// Some query terms matched within the bounded edit distance.
    Fuzzy,
}

impl Tier {
    /// The closed rank (smaller is better).
    pub const fn rank(self) -> u8 {
        match self {
            Self::ExactId => 0,
            Self::ExactQualified => 1,
            Self::ExactName => 2,
            Self::AllTerms => 3,
            Self::Prefix => 4,
            Self::Fuzzy => 5,
        }
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExactId => "exact-id",
            Self::ExactQualified => "exact-qualified",
            Self::ExactName => "exact-name",
            Self::AllTerms => "all-terms",
            Self::Prefix => "prefix",
            Self::Fuzzy => "fuzzy",
        }
    }
}

/// Unicode NFC, then ASCII-only case folding. Non-ASCII case folding is
/// outside recipe 1 and is never applied.
pub fn normalize_text(text: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let normalized: String = text.nfc().collect();
    normalized
        .chars()
        .map(|character| {
            if character.is_ascii() {
                character.to_ascii_lowercase()
            } else {
                character
            }
        })
        .collect()
}

/// Split a normalized string into terms on whitespace and the frozen ID
/// separators (`.` `_` `-` `/` `#`). Empty terms never escape.
pub fn tokenize(normalized: &str) -> Vec<String> {
    normalized
        .split(|character: char| {
            character.is_whitespace() || matches!(character, '.' | '_' | '-' | '/' | '#')
        })
        .filter(|term| !term.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The bounded classic Levenshtein edit distance with an early exit at
/// the accepted cap. Edit operations are insertions, deletions, and
/// substitutions of single Unicode scalar values.
pub fn bounded_edit_distance(left: &[char], right: &[char], cap: u32) -> Option<u32> {
    let cap = cap as usize;
    if left.len().abs_diff(right.len()) > cap {
        return None;
    }
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    for (index, left_char) in left.iter().enumerate() {
        let mut current = vec![index + 1; right.len() + 1];
        for (position, right_char) in right.iter().enumerate() {
            let substitution = previous[position] + usize::from(left_char != right_char);
            current[position + 1] = (previous[position + 1] + 1)
                .min(current[position] + 1)
                .min(substitution);
        }
        if current.iter().min().copied().unwrap_or(usize::MAX) > cap {
            return None;
        }
        previous = current;
    }
    let distance = previous[right.len()];
    if distance > cap {
        None
    } else {
        Some(distance as u32)
    }
}

/// One candidate's ranking facts: the tier it reached, the best matched
/// field, exact term coverage, and total edit operations (fuzzy only).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RankFacts {
    pub tier: Tier,
    pub field: FieldPriority,
    pub exact_terms: usize,
    pub total_terms: usize,
    pub edits: u32,
}

impl RankFacts {
    /// The fixed tuple sort key of the recipe.
    pub(crate) fn sort_key(&self) -> (u8, u8, std::cmp::Reverse<usize>, u32) {
        (
            self.tier.rank(),
            self.field.rank(),
            std::cmp::Reverse(self.exact_terms),
            self.edits,
        )
    }
}

/// The exactness facts of one card against the normalized query.
///
/// `qualified_ids` carries additional exact byte identities (native
/// symbol spellings, external requirement IDs) checked between the full
/// semantic ID and the normalized name.
pub fn match_facts(
    card_id: &str,
    card_name: Option<&str>,
    card_description: Option<&str>,
    qualified_ids: &[String],
    raw_query: &str,
    terms: &[String],
    fuzzy: bool,
) -> Option<RankFacts> {
    let raw_trim = raw_query.trim();
    if raw_trim.is_empty() {
        return None;
    }
    if raw_trim == card_id {
        return Some(RankFacts {
            tier: Tier::ExactId,
            field: FieldPriority::Id,
            exact_terms: terms.len(),
            total_terms: terms.len(),
            edits: 0,
        });
    }
    if qualified_ids.iter().any(|qualified| qualified == raw_trim) {
        return Some(RankFacts {
            tier: Tier::ExactQualified,
            field: FieldPriority::Id,
            exact_terms: terms.len(),
            total_terms: terms.len(),
            edits: 0,
        });
    }
    if let Some(name) = card_name {
        if normalize_text(name) == normalize_text(raw_trim) {
            return Some(RankFacts {
                tier: Tier::ExactName,
                field: FieldPriority::Name,
                exact_terms: terms.len(),
                total_terms: terms.len(),
                edits: 0,
            });
        }
    }
    // Lexical scoring over the three matched fields. The ID field
    // tokenizes its raw bytes (case preserved) so byte-exact term
    // presence beats folded matches on the same field rank.
    let id_terms = tokenize(card_id);
    let name_terms = card_name.map(|name| tokenize(&normalize_text(name)));
    let description_terms = card_description.map(|text| tokenize(&normalize_text(text)));
    let mut best: Option<RankFacts> = None;
    let consider = |candidate: RankFacts, best: &mut Option<RankFacts>| {
        if candidate.exact_terms == 0 && candidate.edits == 0 {
            return;
        }
        let better = match &best {
            None => true,
            Some(current) => candidate.sort_key() < current.sort_key(),
        };
        if better {
            *best = Some(candidate);
        }
    };
    for (field, field_terms) in [
        (FieldPriority::Id, Some(&id_terms)),
        (FieldPriority::Name, name_terms.as_ref()),
        (FieldPriority::Description, description_terms.as_ref()),
    ] {
        let Some(field_terms) = field_terms else {
            continue;
        };
        let exact: usize = terms
            .iter()
            .filter(|term| field_terms.iter().any(|candidate| candidate == *term))
            .count();
        if exact > 0 {
            consider(
                RankFacts {
                    tier: if exact == terms.len() {
                        Tier::AllTerms
                    } else {
                        Tier::Prefix
                    },
                    field,
                    exact_terms: exact,
                    total_terms: terms.len(),
                    edits: 0,
                },
                &mut best,
            );
        }
        // Prefix matches count below exact term matches on the same
        // field and only when no exact match exists for the term.
        let prefix: usize = terms
            .iter()
            .filter(|term| {
                !field_terms.iter().any(|candidate| candidate == *term)
                    && field_terms
                        .iter()
                        .any(|candidate| candidate.starts_with(term.as_str()))
            })
            .count();
        if prefix > 0 {
            consider(
                RankFacts {
                    tier: Tier::Prefix,
                    field,
                    exact_terms: exact + prefix,
                    total_terms: terms.len(),
                    edits: 0,
                },
                &mut best,
            );
        }
        if fuzzy {
            let mut fuzzy_terms = 0usize;
            let mut edits = 0u32;
            for term in terms {
                if field_terms.iter().any(|candidate| candidate == term) {
                    continue;
                }
                let mut matched: Option<u32> = None;
                for candidate in field_terms {
                    if candidate.len().abs_diff(term.len()) > MAX_FUZZY_EDITS as usize {
                        continue;
                    }
                    let candidate_chars: Vec<char> = candidate.chars().collect();
                    let term_chars: Vec<char> = term.chars().collect();
                    if let Some(distance) =
                        bounded_edit_distance(&term_chars, &candidate_chars, MAX_FUZZY_EDITS)
                    {
                        matched = Some(matched.map_or(distance, |current| current.min(distance)));
                    }
                }
                if let Some(distance) = matched {
                    fuzzy_terms += 1;
                    edits += distance;
                }
            }
            if fuzzy_terms > 0 {
                consider(
                    RankFacts {
                        tier: Tier::Fuzzy,
                        field,
                        exact_terms: exact + fuzzy_terms,
                        total_terms: terms.len(),
                        edits,
                    },
                    &mut best,
                );
            }
        }
    }
    best
}

/// The bounded fuzzy work guard: whether one more candidate may be
/// expanded before scoring.
pub struct FuzzyBudget {
    remaining: usize,
}

impl FuzzyBudget {
    /// A fresh budget of [`MAX_FUZZY_CANDIDATES`] candidates.
    pub fn new() -> Self {
        Self {
            remaining: MAX_FUZZY_CANDIDATES,
        }
    }

    /// Consume one candidate slot; `false` means the cap was hit.
    pub fn take(&mut self) -> bool {
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        true
    }

    /// Whether the cap was hit.
    pub fn exhausted(&self) -> bool {
        self.remaining == 0
    }
}

impl Default for FuzzyBudget {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_is_nfc_ascii_fold_and_splits_separators() {
        assert_eq!(normalize_text("Focus TASK"), "focus task");
        let composed = format!("fo{}", '\u{00F4}'); // precomposed ô
        let decomposed = "fo\u{006F}\u{0302}";
        assert_eq!(normalize_text(&composed), normalize_text(decomposed));
        assert_eq!(
            tokenize(&normalize_text("planner.focus_task-x/y#z w")),
            vec!["planner", "focus", "task", "x", "y", "z", "w"]
        );
    }

    #[test]
    fn exact_full_id_outranks_every_lexical_tier() {
        let facts = match_facts(
            "planner.focus_task",
            Some("Focus one task"),
            Some("Focus one task by id"),
            &[],
            "planner.focus_task",
            &["planner".to_owned(), "focus".to_owned(), "task".to_owned()],
            true,
        )
        .expect("exact match");
        assert_eq!(facts.tier, Tier::ExactId);
    }

    #[test]
    fn fuzzy_is_opt_in_and_bounded() {
        let exact_only = match_facts(
            "planner.focus_task",
            Some("Focus one task"),
            None,
            &[],
            "fokus",
            &["fokus".to_owned()],
            false,
        );
        assert!(exact_only.is_none(), "non-fuzzy mode never edits");
        let fuzzy = match_facts(
            "planner.focus_task",
            Some("Focus one task"),
            None,
            &[],
            "fokus",
            &["fokus".to_owned()],
            true,
        )
        .expect("fuzzy match");
        assert_eq!(fuzzy.tier, Tier::Fuzzy);
        assert_eq!(fuzzy.edits, 1);
    }

    #[test]
    fn edit_distance_is_bounded() {
        let a: Vec<char> = "kitten".chars().collect();
        let b: Vec<char> = "sitting".chars().collect();
        assert_eq!(bounded_edit_distance(&a, &b, 2), None);
        assert_eq!(bounded_edit_distance(&a, &b, 3), Some(3));
        let c: Vec<char> = "focus".chars().collect();
        let d: Vec<char> = "fokus".chars().collect();
        assert_eq!(bounded_edit_distance(&c, &d, 2), Some(1));
    }

    #[test]
    fn qualified_ids_beat_the_name_tier() {
        let facts = match_facts(
            "planner.focus_task",
            Some("focus task"),
            None,
            &["apps/api/src/planner/focus-task.ts#focusTask".to_owned()],
            "apps/api/src/planner/focus-task.ts#focusTask",
            &["apps".to_owned(), "api".to_owned()],
            false,
        )
        .expect("qualified match");
        assert_eq!(facts.tier, Tier::ExactQualified);
    }
}
