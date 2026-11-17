/// Whether `text` is a legal requirement id:
/// `<capability>.REQ-<slug>` with kebab-case capability and slug parts
/// (`planner.REQ-focus-task`).
pub fn is_requirement_id(text: &str) -> bool {
    let Some((capability, slug)) = text.split_once(".REQ-") else {
        return false;
    };
    let ok_part = |bytes: &[u8]| {
        !bytes.is_empty()
            && bytes
                .iter()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
    };
    text.len() <= 128
        && capability.len() <= 63
        && slug.len() <= 64
        && capability
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && slug
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && ok_part(capability.as_bytes())
        && ok_part(slug.as_bytes())
}

/// The stable slug of one requirement title: lowercased ASCII, every
/// non-alphanumeric run collapsed to one `-`, trimmed; fails closed when
/// the title carries no sluggable characters or exceeds the slug bound.
pub fn title_slug(title: &str) -> Option<String> {
    if title.chars().count() > crate::limits::MAX_TITLE_CHARS {
        return None;
    }
    let mut slug = String::new();
    let mut pending_dash = false;
    for character in title.chars() {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(character);
        } else if character.is_ascii_uppercase() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(character.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() || slug.len() > 64 {
        return None;
    }
    Some(slug)
}

/// Whether `text` is a legal change-directory id.
pub fn is_change_id(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some((&first, rest)) = bytes.split_first() else {
        return false;
    };
    (first.is_ascii_lowercase() || first.is_ascii_digit())
        && rest.len() <= 127
        && rest.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

/// The canonical requirement id of one capability/title pair.
pub fn requirement_id(capability: &str, slug: &str) -> String {
    format!("{capability}.REQ-{slug}")
}
