//! Search helpers ported from `SessionDB`.

use regex::{Captures, Regex};
use std::sync::OnceLock;

fn quoted_phrase_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#""[^"]*""#).expect("valid quoted phrase regex"))
}

fn fts_special_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"[+{}()"^]"#).expect("valid FTS special regex"))
}

fn repeated_star_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\*+").expect("valid repeated star regex"))
}

fn leading_star_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(^|\s)\*").expect("valid leading star regex"))
}

fn leading_bool_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^(AND|OR|NOT)\b\s*").expect("valid leading boolean regex"))
}

fn trailing_bool_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\s+(AND|OR|NOT)\s*$").expect("valid trailing boolean regex"))
}

fn compound_term_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\w+(?:[._-]\w+)+)\b").expect("valid compound term regex"))
}

/// Sanitize user input for SQLite FTS5 MATCH queries.
///
/// This mirrors `SessionDB._sanitize_fts5_query`:
/// - Preserve balanced quoted phrases.
/// - Strip unmatched FTS5 metacharacters.
/// - Keep valid prefix stars while removing leading stars.
/// - Remove dangling boolean operators.
/// - Quote dotted, hyphenated, and underscored terms so FTS5 keeps phrase
///   semantics.
pub fn sanitize_fts5_query(query: &str) -> String {
    let mut quoted_parts: Vec<String> = Vec::new();
    let mut sanitized = quoted_phrase_re()
        .replace_all(query, |captures: &Captures<'_>| {
            let placeholder = format!("\u{0}Q{}\u{0}", quoted_parts.len());
            quoted_parts.push(captures[0].to_string());
            placeholder
        })
        .into_owned();

    sanitized = fts_special_re().replace_all(&sanitized, " ").into_owned();
    sanitized = repeated_star_re().replace_all(&sanitized, "*").into_owned();
    sanitized = leading_star_re().replace_all(&sanitized, "$1").into_owned();

    let trimmed = sanitized.trim().to_string();
    sanitized = leading_bool_re().replace(&trimmed, "").into_owned();
    let trimmed = sanitized.trim().to_string();
    sanitized = trailing_bool_re().replace(&trimmed, "").into_owned();

    sanitized = compound_term_re()
        .replace_all(&sanitized, r#""$1""#)
        .into_owned();

    for (idx, quoted) in quoted_parts.iter().enumerate() {
        let placeholder = format!("\u{0}Q{}\u{0}", idx);
        sanitized = sanitized.replace(&placeholder, quoted);
    }

    sanitized.trim().to_string()
}

/// Return true when `cp` is in one of the CJK ranges used by Hermes search.
pub fn is_cjk_codepoint(cp: u32) -> bool {
    (0x4E00..=0x9FFF).contains(&cp)
        || (0x3400..=0x4DBF).contains(&cp)
        || (0x20000..=0x2A6DF).contains(&cp)
        || (0x3000..=0x303F).contains(&cp)
        || (0x3040..=0x309F).contains(&cp)
        || (0x30A0..=0x30FF).contains(&cp)
        || (0xAC00..=0xD7AF).contains(&cp)
}

/// Check whether `text` contains Chinese, Japanese, or Korean codepoints.
pub fn contains_cjk(text: &str) -> bool {
    text.chars().any(|ch| is_cjk_codepoint(ch as u32))
}

/// Count Chinese, Japanese, or Korean codepoints in `text`.
pub fn count_cjk(text: &str) -> usize {
    text.chars()
        .filter(|ch| is_cjk_codepoint(*ch as u32))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_basic_queries() {
        assert_eq!(sanitize_fts5_query("hello world"), "hello world");
        assert!(!sanitize_fts5_query("C++").contains('+'));
        assert!(!sanitize_fts5_query("\"unterminated").contains('"'));
        assert!(!sanitize_fts5_query("(problem").contains('('));
        assert!(!sanitize_fts5_query("{test}").contains('{'));
        assert_eq!(sanitize_fts5_query("hello AND"), "hello");
        assert_eq!(sanitize_fts5_query("OR world"), "world");
        assert_eq!(sanitize_fts5_query("***"), "");
        assert_eq!(sanitize_fts5_query("deploy*"), "deploy*");
    }

    #[test]
    fn sanitize_preserves_balanced_quotes() {
        assert_eq!(sanitize_fts5_query("\"exact phrase\""), "\"exact phrase\"");
        let result = sanitize_fts5_query("\"hello world\" OR \"foo bar\"");
        assert!(result.contains("\"hello world\""));
        assert!(result.contains("\"foo bar\""));
        assert_eq!(
            sanitize_fts5_query("\"my chat-send thing\""),
            "\"my chat-send thing\""
        );
    }

    #[test]
    fn sanitize_quotes_compound_terms() {
        assert_eq!(sanitize_fts5_query("chat-send"), "\"chat-send\"");
        assert_eq!(
            sanitize_fts5_query("docker-compose-up"),
            "\"docker-compose-up\""
        );
        assert_eq!(sanitize_fts5_query("P2.2"), "\"P2.2\"");
        assert_eq!(
            sanitize_fts5_query("simulate.p2.test.ts"),
            "\"simulate.p2.test.ts\""
        );
        assert_eq!(sanitize_fts5_query("sp_new"), "\"sp_new\"");
        assert_eq!(
            sanitize_fts5_query("docker-compose_up"),
            "\"docker-compose_up\""
        );
        assert_eq!(
            sanitize_fts5_query("my.app_config.ts"),
            "\"my.app_config.ts\""
        );
        assert_eq!(sanitize_fts5_query("\"chat-send\""), "\"chat-send\"");
    }

    #[test]
    fn cjk_detection_matches_python_ranges() {
        assert!(contains_cjk("记忆断裂"));
        assert!(contains_cjk("こんにちは"));
        assert!(contains_cjk("カタカナ"));
        assert!(contains_cjk("안녕하세요"));
        assert!(contains_cjk("日本語mixedwithenglish"));
        assert!(!contains_cjk("hello world"));
        assert!(!contains_cjk(""));
        assert_eq!(count_cjk("A记B忆"), 2);
    }
}
