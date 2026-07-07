//! A byte-capped, FIFO ring buffer of captured log lines.
//!
//! Holds at most `capacity` bytes of line text (default 5 MiB); once exceeded,
//! the oldest lines are dropped. The cap is approximate — it counts line text
//! bytes only, not per-line struct overhead — matching `docker logs`-style
//! bounded scrollback.
#![allow(dead_code)] // Consumed by the log reader threads and UI in later sub-steps.

use std::collections::VecDeque;

/// Which stream a captured line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// One captured line of output (newline stripped).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub stream: Stream,
    pub text: String,
}

/// Default scrollback cap: 5 MiB of line text.
pub const DEFAULT_CAPACITY_BYTES: usize = 5 * 1024 * 1024;

/// A FIFO buffer of [`LogLine`]s bounded by a byte budget.
#[derive(Debug)]
pub struct LogBuffer {
    lines: VecDeque<LogLine>,
    byte_len: usize,
    capacity: usize,
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY_BYTES)
    }
}

impl LogBuffer {
    /// Create a buffer holding up to `capacity` bytes of line text.
    pub fn with_capacity(capacity: usize) -> Self {
        Self { lines: VecDeque::new(), byte_len: 0, capacity }
    }

    /// Append a line, dropping the oldest lines until the byte budget holds.
    /// The most-recently pushed line is always retained, even if it alone
    /// exceeds the capacity.
    pub fn push(&mut self, stream: Stream, text: impl Into<String>) {
        let text = text.into();
        self.byte_len += text.len();
        self.lines.push_back(LogLine { stream, text });
        while self.byte_len > self.capacity && self.lines.len() > 1 {
            if let Some(dropped) = self.lines.pop_front() {
                self.byte_len -= dropped.text.len();
            }
        }
    }

    /// Number of buffered lines.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether the buffer holds no lines.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Total bytes of retained line text.
    pub fn byte_len(&self) -> usize {
        self.byte_len
    }

    /// The line at `index` (0 = oldest), or `None` if out of range. O(1) —
    /// suited to egui's row-windowed (`show_rows`) rendering.
    pub fn get(&self, index: usize) -> Option<&LogLine> {
        self.lines.get(index)
    }

    /// Iterate lines oldest-to-newest.
    pub fn iter(&self) -> impl Iterator<Item = &LogLine> {
        self.lines.iter()
    }

    /// Drop all buffered lines.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.byte_len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_at(buf: &LogBuffer, index: usize) -> &str {
        buf.get(index).unwrap().text.as_str()
    }

    #[test]
    fn retains_order_under_cap() {
        let mut buf = LogBuffer::with_capacity(1024);
        buf.push(Stream::Stdout, "a");
        buf.push(Stream::Stderr, "b");
        buf.push(Stream::Stdout, "c");
        assert_eq!(buf.len(), 3);
        assert_eq!(text_at(&buf, 0), "a");
        assert_eq!(text_at(&buf, 2), "c");
        assert_eq!(buf.get(1).unwrap().stream, Stream::Stderr);
    }

    #[test]
    fn drops_oldest_when_over_capacity() {
        let mut buf = LogBuffer::with_capacity(10);
        for tag in ["aaaa", "bbbb", "cccc"] {
            buf.push(Stream::Stdout, tag); // 4 bytes each; 12 > 10
        }
        assert!(buf.byte_len() <= 10, "byte_len={}", buf.byte_len());
        assert_eq!(buf.len(), 2);
        assert_eq!(text_at(&buf, 0), "bbbb");
        assert_eq!(text_at(&buf, 1), "cccc");
    }

    #[test]
    fn keeps_single_oversized_line() {
        let mut buf = LogBuffer::with_capacity(4);
        buf.push(Stream::Stdout, "aaaa");
        buf.push(Stream::Stdout, "bbbbbbbbbb"); // 10 bytes, alone exceeds cap
        assert_eq!(buf.len(), 1);
        assert_eq!(text_at(&buf, 0), "bbbbbbbbbb");
        assert_eq!(buf.byte_len(), 10);
    }

    #[test]
    fn byte_len_tracks_trims() {
        let mut buf = LogBuffer::with_capacity(6);
        buf.push(Stream::Stdout, "aaa"); // 3
        buf.push(Stream::Stdout, "bbb"); // 6 total
        assert_eq!(buf.byte_len(), 6);
        buf.push(Stream::Stdout, "ccc"); // 9 -> drop "aaa" -> 6
        assert_eq!(buf.byte_len(), 6);
        assert_eq!(buf.len(), 2);
    }

    #[test]
    fn clear_resets() {
        let mut buf = LogBuffer::with_capacity(16);
        buf.push(Stream::Stdout, "x");
        buf.clear();
        assert!(buf.is_empty());
        assert_eq!(buf.byte_len(), 0);
        assert!(buf.get(0).is_none());
    }
}
