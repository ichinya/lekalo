//! Comment ownership: every accepted comment must have exactly one output
//! owner before any mutation (issue #94).
//!
//! Owners are the document (header/footer) and, per mapping entry or
//! sequence item, its leading comment block and its same-line inline
//! comment. Ownership is computed from byte spans and line facts only;
//! comments move with their owner when canonical ordering moves it.

use crate::loader::frontends::{Node, Value};

use super::syntax::Trivia;

/// Why a comment could not be given an unambiguous owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LossReason {
    /// The comment shares its line with content that is not an owner
    /// start, or sits inside a structure the renderer cannot reproduce.
    AmbiguousOwner,
    /// The rendered form cannot represent the construct owning the trivia.
    UnrepresentableForm,
}

impl LossReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AmbiguousOwner => "ambiguous-owner",
            Self::UnrepresentableForm => "unrepresentable-form",
        }
    }
}

/// One trivia loss with its 1-based source line.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LossMark {
    pub line: usize,
    pub reason: LossReason,
}

/// The complete ownership assignment for one document.
#[derive(Debug, Default)]
pub(super) struct Ownership {
    /// owner start line (1-based) -> leading comment tokens, in order.
    pub leading: std::collections::BTreeMap<usize, Vec<usize>>,
    /// owner start line (1-based) -> same-line inline comment tokens.
    pub inline: std::collections::BTreeMap<usize, Vec<usize>>,
    /// Comments before the first owner line, in order.
    pub header: Vec<usize>,
    /// Comments after the last owner line, in order.
    pub footer: Vec<usize>,
    pub losses: Vec<LossMark>,
}

impl Ownership {
    /// The leading comments of one owner line (never reordered).
    pub fn leading_of(&self, line: usize) -> &[usize] {
        self.leading
            .get(&line)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// The inline comment of one owner line, if any.
    pub fn inline_of(&self, line: usize) -> Option<usize> {
        self.inline
            .get(&line)
            .and_then(|tokens| tokens.first().copied())
    }

    /// Assign every comment of the document. Any comment without an owner
    /// becomes an explicit loss, never a silent deletion.
    pub(super) fn assign(trivia: &Trivia, root: &Node) -> Ownership {
        let mut owner_lines = std::collections::BTreeSet::new();
        collect_owner_lines(root, &mut owner_lines);
        let sorted: Vec<usize> = owner_lines.into_iter().collect();

        let mut ownership = Ownership::default();
        let first_owner = sorted.first().copied();
        for (index, token) in trivia.comments.iter().enumerate() {
            // Same-line comment: the inline comment of the outermost owner
            // starting on this line.
            if sorted.binary_search(&token.line).is_ok() {
                ownership.inline.entry(token.line).or_default().push(index);
                continue;
            }
            // A comment sharing its line with content that starts no owner
            // (e.g. a trailing comment after a multi-line plain scalar)
            // has no unambiguous owner.
            if !token.standalone {
                ownership.losses.push(LossMark {
                    line: token.line,
                    reason: LossReason::AmbiguousOwner,
                });
                continue;
            }
            // Standalone comment: before all content it is document
            // header; between owners it leads the next owner and moves
            // with it; after all content it is document footer.
            let next_owner = sorted.iter().find(|line| **line > token.line).copied();
            match next_owner {
                Some(_) if first_owner.map(|first| token.line < first).unwrap_or(false) => {
                    ownership.header.push(index);
                }
                Some(line) => {
                    ownership.leading.entry(line).or_default().push(index);
                }
                None => ownership.footer.push(index),
            }
        }
        ownership
    }
}

/// Collect every owner start line: mapping entry key lines and sequence
/// item start lines at every nesting level.
fn collect_owner_lines(node: &Node, out: &mut std::collections::BTreeSet<usize>) {
    match &node.value {
        Value::Scalar(_) => {}
        Value::Seq(items) => {
            for item in items {
                out.insert(item.span.start.line);
                collect_owner_lines(item, out);
            }
        }
        Value::Map(entries) => {
            for entry in entries {
                out.insert(entry.key_span.start.line);
                collect_owner_lines(&entry.value, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::syntax;
    use crate::loader::frontends::parse_document;
    use crate::loader::source::LineIndex;

    fn assign(text: &str) -> (Trivia, Ownership) {
        let index = LineIndex::new(text);
        let parsed = parse_document(text, &index).expect("parse");
        let root = match parsed {
            crate::loader::frontends::Parsed::Root(root) => *root,
            crate::loader::frontends::Parsed::Empty => panic!("empty"),
        };
        let trivia = syntax::extract(text, &root);
        let ownership = Ownership::assign(&trivia, &root);
        (trivia, ownership)
    }

    #[test]
    fn header_leading_inline_and_footer_are_owned() {
        let text = "# top\na: 1 # on a\n# leads b\nb: 2\n# bottom\n";
        let (trivia, ownership) = assign(text);
        assert_eq!(ownership.header, vec![0]);
        assert_eq!(ownership.inline.get(&2), Some(&vec![1usize]));
        assert_eq!(ownership.leading.get(&4), Some(&vec![2]));
        assert_eq!(ownership.footer, vec![3]);
        assert_eq!(trivia.comments.len(), 4);
        assert!(ownership.losses.is_empty());
    }

    #[test]
    fn a_comment_after_content_without_owner_is_a_loss() {
        // The plain scalar continues onto line 2; line 2 starts no owner.
        let text = "a: one\n  two # trailing\n";
        let (_, ownership) = assign(text);
        assert_eq!(ownership.losses.len(), 1);
        assert_eq!(ownership.losses[0].reason, LossReason::AmbiguousOwner);
    }
}
