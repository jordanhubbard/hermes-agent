//! Session title normalization ported from `SessionDB.sanitize_title`.

use regex::Regex;
use std::fmt;
use std::sync::OnceLock;

pub const MAX_TITLE_LENGTH: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TitleError {
    TooLong { len: usize, max: usize },
}

impl fmt::Display for TitleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TitleError::TooLong { len, max } => {
                write!(f, "Title too long ({len} chars, max {max})")
            }
        }
    }
}

impl std::error::Error for TitleError {}

fn ascii_control_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]").expect("valid ASCII control regex")
    })
}

fn unicode_control_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"[\u{200b}-\u{200f}\u{2028}-\u{202e}\u{2060}-\u{2069}\u{feff}\u{fffc}\u{fff9}-\u{fffb}]")
            .expect("valid unicode control regex")
    })
}

fn whitespace_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s+").expect("valid whitespace regex"))
}

/// Validate and normalize a session title.
///
/// Returns `Ok(None)` for empty or whitespace-only input. Otherwise this:
/// removes control characters, collapses whitespace, trims the result, and
/// enforces `MAX_TITLE_LENGTH`.
pub fn sanitize_title(title: Option<&str>) -> Result<Option<String>, TitleError> {
    let Some(title) = title else {
        return Ok(None);
    };
    if title.is_empty() {
        return Ok(None);
    }

    let cleaned = ascii_control_re().replace_all(title, "");
    let cleaned = unicode_control_re().replace_all(&cleaned, "");
    let cleaned = whitespace_re().replace_all(&cleaned, " ");
    let cleaned = cleaned.trim();

    if cleaned.is_empty() {
        return Ok(None);
    }

    let len = cleaned.chars().count();
    if len > MAX_TITLE_LENGTH {
        return Err(TitleError::TooLong {
            len,
            max: MAX_TITLE_LENGTH,
        });
    }

    Ok(Some(cleaned.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_titles_normalize_to_none() {
        assert_eq!(sanitize_title(None).unwrap(), None);
        assert_eq!(sanitize_title(Some("")).unwrap(), None);
        assert_eq!(sanitize_title(Some(" \t\n ")).unwrap(), None);
    }

    #[test]
    fn whitespace_is_collapsed_and_trimmed() {
        assert_eq!(
            sanitize_title(Some("  hello\t\nworld  ")).unwrap(),
            Some("hello world".to_string())
        );
    }

    #[test]
    fn control_characters_are_removed() {
        assert_eq!(
            sanitize_title(Some("he\u{0000}llo\u{200b}\u{202e} world")).unwrap(),
            Some("hello world".to_string())
        );
    }

    #[test]
    fn overlong_title_is_rejected() {
        let title = "a".repeat(MAX_TITLE_LENGTH + 1);
        assert_eq!(
            sanitize_title(Some(&title)),
            Err(TitleError::TooLong {
                len: MAX_TITLE_LENGTH + 1,
                max: MAX_TITLE_LENGTH
            })
        );
    }
}
