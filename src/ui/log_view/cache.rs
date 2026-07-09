//! Filter + match caching. Matching the whole buffer is O(lines), so the grep
//! filter and the find-match scan are recomputed only when the query, a toggle,
//! or the line count changes — not every frame.

use super::{Displayed, FindMatch, LogView};
use crate::process::log_buffer::LogBuffer;
use crate::search::Matcher;

/// The inputs the cached match set depends on. When this is unchanged frame to
/// frame, the (whole-buffer) filter and match scan are skipped.
struct CacheKey {
    find: String,
    case: bool,
    word: bool,
    regex: bool,
    grep: String,
    grep_case: bool,
    grep_word: bool,
    grep_regex: bool,
    len: usize,
}

/// Everything derived from a [`CacheKey`]: the compiled find matcher (kept for
/// per-row highlight ranges), any regex error, the grep row set, and the flat
/// list of find matches.
#[derive(Default)]
pub(super) struct Cache {
    key: Option<CacheKey>,
    pub(super) find: Option<Matcher>,
    pub(super) find_error: Option<String>,
    pub(super) grep_error: Option<String>,
    pub(super) filter: Option<Vec<usize>>, // None = no grep (all lines shown)
    pub(super) matches: Vec<FindMatch>,
}

/// Rebuild the filter + match cache when the query/toggles/line-count change;
/// otherwise reuse last frame's. A content change (anything but new lines
/// arriving) restarts find navigation at the first match.
pub(super) fn ensure_cache(state: &mut LogView, logs: &LogBuffer) {
    let len = logs.len();
    if cache_is_current(&state.cache.key, state, len) {
        return;
    }
    let content_changed = state.cache.key.as_ref().is_none_or(|k| {
        k.find != state.find.trim()
            || k.case != state.find_case
            || k.word != state.find_word
            || k.regex != state.find_regex
            || k.grep != state.grep.trim()
            || k.grep_case != state.grep_case
            || k.grep_word != state.grep_word
            || k.grep_regex != state.grep_regex
    });
    if content_changed {
        state.active = 0;
    }

    let (find, find_error) = compile(
        state.find.trim(),
        state.find_case,
        state.find_word,
        state.find_regex,
    );
    // grep shares the same toggles as find; it just filters instead of stepping.
    let (grep, grep_error) = compile(
        state.grep.trim(),
        state.grep_case,
        state.grep_word,
        state.grep_regex,
    );

    let filter = grep.as_ref().map(|m| {
        (0..len)
            .filter(|&i| {
                logs.get(i)
                    .is_some_and(|l| m.is_match(&crate::ansi::strip(&l.text)))
            })
            .collect::<Vec<_>>()
    });
    let displayed = match filter.as_deref() {
        Some(rows) => Displayed::Filtered(rows),
        None => Displayed::All(len),
    };
    let matches = collect_matches(logs, &displayed, find.as_ref());

    state.cache = Cache {
        key: Some(CacheKey {
            find: state.find.trim().to_string(),
            case: state.find_case,
            word: state.find_word,
            regex: state.find_regex,
            grep: state.grep.trim().to_string(),
            grep_case: state.grep_case,
            grep_word: state.grep_word,
            grep_regex: state.grep_regex,
            len,
        }),
        find,
        find_error,
        grep_error,
        filter,
        matches,
    };
}

/// Whether the cache key still matches the live state (no allocation).
fn cache_is_current(key: &Option<CacheKey>, state: &LogView, len: usize) -> bool {
    matches!(key, Some(k)
        if k.find == state.find.trim()
        && k.case == state.find_case
        && k.word == state.find_word
        && k.regex == state.find_regex
        && k.grep == state.grep.trim()
        && k.grep_case == state.grep_case
        && k.grep_word == state.grep_word
        && k.grep_regex == state.grep_regex
        && k.len == len)
}

/// Compile `query` under the toggles, split into the matcher (if the query is
/// non-empty and valid) and an error message (if `regex` is on and the pattern
/// is bad). Shared by find and grep so both handle a bad pattern the same way.
fn compile(query: &str, case: bool, word: bool, regex: bool) -> (Option<Matcher>, Option<String>) {
    match Matcher::new(query, case, word, regex) {
        Ok(matcher) => (matcher, None),
        Err(err) => (None, Some(err)),
    }
}

/// Scan the displayed lines for every find match (empty when find is inactive).
fn collect_matches(
    logs: &LogBuffer,
    displayed: &Displayed<'_>,
    find: Option<&Matcher>,
) -> Vec<FindMatch> {
    let mut matches = Vec::new();
    let Some(matcher) = find else { return matches };
    for row in 0..displayed.len() {
        let Some(index) = displayed.line_index(row) else {
            continue;
        };
        let Some(line) = logs.get(index) else {
            continue;
        };
        let stripped = crate::ansi::strip(&line.text);
        for range in matcher.find_ranges(&stripped) {
            matches.push(FindMatch { row, range });
        }
    }
    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::log_buffer::Stream;

    fn buf(lines: &[&str]) -> LogBuffer {
        let mut b = LogBuffer::with_capacity(10_000);
        for line in lines {
            b.push(Stream::Stdout, *line);
        }
        b
    }

    fn matcher(query: &str) -> Matcher {
        Matcher::new(query, false, false, false).unwrap().unwrap()
    }

    #[test]
    fn collect_matches_empty_when_find_inactive() {
        let logs = buf(&["error here", "all good"]);
        let d = Displayed::All(logs.len());
        assert!(collect_matches(&logs, &d, None).is_empty());
    }

    #[test]
    fn collect_matches_records_row_and_ranges() {
        let logs = buf(&["nope", "hit and hit", "done"]);
        let d = Displayed::All(logs.len());
        let found = collect_matches(&logs, &d, Some(&matcher("hit")));
        assert_eq!(found.len(), 2);
        assert_eq!((found[0].row, found[0].range.clone()), (1, 0..3));
        assert_eq!((found[1].row, found[1].range.clone()), (1, 8..11));
    }

    #[test]
    fn collect_matches_uses_display_rows_under_grep() {
        let logs = buf(&["skip", "keep hit", "skip", "keep hit"]);
        // grep left buffer rows 1 and 3, which become display rows 0 and 1.
        let rows = vec![1usize, 3];
        let d = Displayed::Filtered(&rows);
        let found = collect_matches(&logs, &d, Some(&matcher("hit")));
        assert_eq!(found.iter().map(|m| m.row).collect::<Vec<_>>(), vec![0, 1]);
    }
}
