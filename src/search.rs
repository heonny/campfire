//! Text matching for the log view's two search modes:
//! - **find** — highlight every match in place and step between them,
//! - **grep** — filter the log down to matching lines.
//!
//! Both share one [`Matcher`], which folds the case / whole-word / regex
//! toggles into a single compiled `regex_lite` pattern. grep is just find with
//! the toggles off (a plain case-insensitive substring), so there is one code
//! path for matching and one place to reason about it.

use regex_lite::RegexBuilder;
use std::ops::Range;

/// A compiled search pattern. Use [`Matcher::is_match`] to filter lines (grep)
/// and [`Matcher::find_ranges`] to locate highlight spans (find).
pub struct Matcher {
    re: regex_lite::Regex,
}

impl Matcher {
    /// Compile `query` under the find toggles.
    ///
    /// - `Ok(None)` — `query` is empty; there is nothing to match, and callers
    ///   treat this as "search inactive" (no filtering, no highlight).
    /// - `Ok(Some(_))` — a usable matcher.
    /// - `Err(msg)` — `regex` is on and the pattern is invalid. Surfaced to the
    ///   user (bad-pattern indicator) rather than silently matching nothing.
    ///
    /// A non-regex query is escaped so metacharacters match literally; a regex
    /// query passes through. `whole_word` wraps the pattern in `\b(?:…)\b`.
    /// Matching is case-insensitive unless `case_sensitive`.
    pub fn new(
        query: &str,
        case_sensitive: bool,
        whole_word: bool,
        regex: bool,
    ) -> Result<Option<Matcher>, String> {
        if query.is_empty() {
            return Ok(None);
        }
        let base = if regex {
            query.to_string()
        } else {
            regex_lite::escape(query)
        };
        let pattern = if whole_word {
            format!(r"\b(?:{base})\b")
        } else {
            base
        };
        let re = RegexBuilder::new(&pattern)
            .case_insensitive(!case_sensitive)
            .build()
            .map_err(|err| err.to_string())?;
        Ok(Some(Matcher { re }))
    }

    /// Does `text` contain at least one match? (grep's line predicate.)
    pub fn is_match(&self, text: &str) -> bool {
        self.re.is_match(text)
    }

    /// Every non-overlapping match as a byte range in `text`, left to right.
    /// Zero-width matches (e.g. `a*` against a non-`a` run) are dropped so
    /// highlighting and navigation only deal with visible spans.
    pub fn find_ranges(&self, text: &str) -> Vec<Range<usize>> {
        self.re
            .find_iter(text)
            .filter(|m| m.start() != m.end())
            .map(|m| m.start()..m.end())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a matcher, asserting it compiled and is non-empty.
    fn m(query: &str, case: bool, word: bool, regex: bool) -> Matcher {
        Matcher::new(query, case, word, regex).unwrap().unwrap()
    }

    #[test]
    fn empty_query_is_none() {
        assert!(Matcher::new("", false, false, false).unwrap().is_none());
    }

    #[test]
    fn literal_is_case_insensitive_by_default() {
        let matcher = m("world", false, false, false);
        assert!(matcher.is_match("hello WORLD"));
        assert_eq!(matcher.find_ranges("hello WORLD"), vec![6..11]);
    }

    #[test]
    fn case_sensitive_respects_case() {
        let matcher = m("WORLD", true, false, false);
        assert!(!matcher.is_match("hello world"));
        assert!(matcher.is_match("hello WORLD"));
    }

    #[test]
    fn literal_escapes_regex_metacharacters() {
        // "a.b" as a literal matches only "a.b", not "axb".
        let matcher = m("a.b", false, false, false);
        assert!(matcher.is_match("a.b"));
        assert!(!matcher.is_match("axb"));
    }

    #[test]
    fn regex_mode_interprets_metacharacters() {
        assert!(m("a.b", false, false, true).is_match("axb"));
        assert_eq!(
            m(r"\d+", false, false, true).find_ranges("abc123def"),
            vec![3..6]
        );
    }

    #[test]
    fn whole_word_requires_boundaries() {
        let matcher = m("cat", false, true, false);
        assert!(matcher.is_match("a cat sat"));
        assert!(!matcher.is_match("category"));
    }

    #[test]
    fn invalid_regex_is_error() {
        assert!(Matcher::new("(", false, false, true).is_err());
    }

    #[test]
    fn find_ranges_returns_all_matches() {
        assert_eq!(
            m("ab", false, false, false).find_ranges("ab_ab_ab"),
            vec![0..2, 3..5, 6..8]
        );
    }

    #[test]
    fn find_ranges_skips_empty_matches() {
        // `a*` matches empty at non-`a` positions; only the "aaa" span counts.
        assert_eq!(m("a*", false, false, true).find_ranges("baaa"), vec![1..4]);
    }

    #[test]
    fn matches_hangul() {
        // "빌드 " is 7 bytes, so "완료" spans 7..13.
        let matcher = m("완료", false, false, false);
        assert!(matcher.is_match("빌드 완료됨"));
        assert_eq!(matcher.find_ranges("빌드 완료됨"), vec![7..13]);
    }
}
