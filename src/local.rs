use std::convert::Infallible;

use crate::backend::{FormatSet, Span, TextBackend};
use crate::format::InlineFormat;

#[derive(Debug, Clone)]
struct MarkRange {
    format: InlineFormat,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Default)]
pub struct LocalBackend {
    text: String,
    marks: Vec<MarkRange>,
}

impl LocalBackend {
    pub fn new() -> Self {
        Self::default()
    }

    fn char_len(&self) -> usize {
        self.text.chars().count()
    }

    fn byte_offset_of_char(&self, char_idx: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len())
    }

    /// Adjust all mark ranges after a splice at `pos` that deletes `delete` chars
    /// and inserts `insert_len` chars. Removes marks that collapse to zero width.
    fn adjust_marks_after_splice(&mut self, pos: usize, delete: usize, insert_len: usize) {
        let delete_end = pos + delete;

        self.marks.retain_mut(|m| {
            // Entirely before the splice — no change
            if m.end <= pos {
                return true;
            }

            // Entirely after the deleted region — shift
            if m.start >= delete_end {
                let shift = insert_len as isize - delete as isize;
                m.start = (m.start as isize + shift) as usize;
                m.end = (m.end as isize + shift) as usize;
                return true;
            }

            // Overlapping cases: clamp endpoints into the splice, then shift
            if m.start < pos {
                // Mark starts before splice
                if m.end <= delete_end {
                    // Mark ends inside deleted region — truncate to splice point + inserted text
                    m.end = pos + insert_len;
                } else {
                    // Mark spans across the entire deletion — adjust end
                    m.end = m.end - delete + insert_len;
                }
            } else {
                // Mark starts inside deleted region
                if m.end <= delete_end {
                    // Entirely consumed by deletion — replace with inserted region
                    m.start = pos;
                    m.end = pos + insert_len;
                } else {
                    // Starts inside deletion, ends after — clamp start, adjust end
                    m.start = pos + insert_len;
                    m.end = m.end - delete + insert_len;
                }
            }

            m.start < m.end
        });
    }

    fn assert_marks_consistent(&self) {
        let len = self.char_len();
        for m in &self.marks {
            if m.start >= m.end {
                panic!(
                    "mark has invalid range: start={}, end={}, format={:?}",
                    m.start, m.end, m.format
                );
            }
            if m.end > len {
                panic!(
                    "mark exceeds text length: end={}, len={}, format={:?}",
                    m.end, len, m.format
                );
            }
        }
    }
}

impl TextBackend for LocalBackend {
    type Error = Infallible;

    fn text(&self) -> Result<String, Self::Error> {
        Ok(self.text.clone())
    }

    fn char_count(&self) -> Result<usize, Self::Error> {
        Ok(self.char_len())
    }

    fn splice(&mut self, pos: usize, delete: usize, text: &str) -> Result<(), Self::Error> {
        let len = self.char_len();
        if pos > len {
            panic!("splice pos out of bounds: pos={}, len={}", pos, len);
        }
        if pos + delete > len {
            panic!(
                "splice delete extends past end: pos={}, delete={}, len={}",
                pos, delete, len
            );
        }

        let insert_char_count = text.chars().count();
        self.adjust_marks_after_splice(pos, delete, insert_char_count);

        let start_byte = self.byte_offset_of_char(pos);
        let end_byte = self.byte_offset_of_char(pos + delete);
        self.text.replace_range(start_byte..end_byte, text);

        self.assert_marks_consistent();
        Ok(())
    }

    fn mark(
        &mut self,
        format: InlineFormat,
        start: usize,
        end: usize,
    ) -> Result<(), Self::Error> {
        if start >= end {
            panic!("mark requires start < end: start={}, end={}", start, end);
        }
        let len = self.char_len();
        if end > len {
            panic!("mark end out of bounds: end={}, len={}", end, len);
        }

        // Merge with existing marks of same format that overlap or are adjacent
        let mut new_start = start;
        let mut new_end = end;

        self.marks.retain(|m| {
            if m.format != format {
                return true;
            }
            // Overlaps or adjacent?
            if m.start <= new_end && m.end >= new_start {
                new_start = new_start.min(m.start);
                new_end = new_end.max(m.end);
                false // absorbed
            } else {
                true
            }
        });

        self.marks.push(MarkRange {
            format,
            start: new_start,
            end: new_end,
        });

        self.assert_marks_consistent();
        Ok(())
    }

    fn unmark(
        &mut self,
        format: InlineFormat,
        start: usize,
        end: usize,
    ) -> Result<(), Self::Error> {
        if start >= end {
            panic!(
                "unmark requires start < end: start={}, end={}",
                start, end
            );
        }
        let len = self.char_len();
        if end > len {
            panic!("unmark end out of bounds: end={}, len={}", end, len);
        }

        let mut new_marks = Vec::new();

        self.marks.retain_mut(|m| {
            if m.format != format {
                return true;
            }
            // No overlap — keep as is
            if m.end <= start || m.start >= end {
                return true;
            }
            // Fully contained — remove
            if m.start >= start && m.end <= end {
                return false;
            }
            // Partial overlap — may split into 0, 1, or 2 pieces
            if m.start < start && m.end > end {
                // Split: keep [m.start, start), add [end, m.end)
                new_marks.push(MarkRange {
                    format: m.format,
                    start: end,
                    end: m.end,
                });
                m.end = start;
                return true;
            }
            if m.start < start {
                // Trim end
                m.end = start;
            } else {
                // Trim start
                m.start = end;
            }
            m.start < m.end
        });

        self.marks.extend(new_marks);
        self.assert_marks_consistent();
        Ok(())
    }

    fn formats_at(&self, pos: usize) -> Result<FormatSet, Self::Error> {
        let len = self.char_len();
        if pos >= len {
            panic!("formats_at pos out of bounds: pos={}, len={}", pos, len);
        }

        let mut fs = FormatSet::default();
        for m in &self.marks {
            if m.start <= pos && pos < m.end {
                fs.set(m.format, true);
            }
        }
        Ok(fs)
    }

    fn spans(&self) -> Result<Vec<Span>, Self::Error> {
        let len = self.char_len();
        if len == 0 {
            return Ok(Vec::new());
        }

        let mut result = Vec::new();
        let mut pos = 0;

        while pos < len {
            let current_formats = self.formats_at(pos)?;

            // Find end of this span: where formats change
            let span_end = (pos + 1..=len)
                .find(|&p| {
                    if p >= len {
                        return true;
                    }
                    self.formats_at(p).unwrap() != current_formats
                })
                .unwrap_or(len);

            let start_byte = self.byte_offset_of_char(pos);
            let end_byte = self.byte_offset_of_char(span_end);

            result.push(Span {
                text: self.text[start_byte..end_byte].to_string(),
                formats: current_formats,
            });

            pos = span_end;
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_backend() {
        let b = LocalBackend::new();
        assert_eq!(b.text().unwrap(), "");
        assert_eq!(b.char_count().unwrap(), 0);
        assert_eq!(b.spans().unwrap(), vec![]);
    }

    #[test]
    fn test_splice_insert() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello").unwrap();
        assert_eq!(b.text().unwrap(), "Hello");
        assert_eq!(b.char_count().unwrap(), 5);
    }

    #[test]
    fn test_splice_insert_middle() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Helloworld").unwrap();
        b.splice(5, 0, " ").unwrap();
        assert_eq!(b.text().unwrap(), "Hello world");
    }

    #[test]
    fn test_splice_delete() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello world").unwrap();
        b.splice(5, 1, "").unwrap();
        assert_eq!(b.text().unwrap(), "Helloworld");
    }

    #[test]
    fn test_splice_replace() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello world").unwrap();
        b.splice(0, 5, "Goodbye").unwrap();
        assert_eq!(b.text().unwrap(), "Goodbye world");
    }

    #[test]
    fn test_splice_unicode() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello 世界").unwrap();
        assert_eq!(b.char_count().unwrap(), 8);
        b.splice(6, 2, "world").unwrap();
        assert_eq!(b.text().unwrap(), "Hello world");
    }

    #[test]
    #[should_panic(expected = "splice pos out of bounds")]
    fn test_splice_pos_out_of_bounds() {
        let mut b = LocalBackend::new();
        b.splice(1, 0, "x").unwrap();
    }

    #[test]
    #[should_panic(expected = "splice delete extends past end")]
    fn test_splice_delete_past_end() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hi").unwrap();
        b.splice(0, 5, "").unwrap();
    }

    #[test]
    fn test_mark_and_formats_at() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello world").unwrap();
        b.mark(InlineFormat::Strong, 0, 5).unwrap();

        for pos in 0..5 {
            let fs = b.formats_at(pos).unwrap();
            assert!(fs.has(InlineFormat::Strong), "pos {} should be strong", pos);
        }
        for pos in 5..11 {
            let fs = b.formats_at(pos).unwrap();
            assert!(
                !fs.has(InlineFormat::Strong),
                "pos {} should not be strong",
                pos
            );
        }
    }

    #[test]
    fn test_mark_merges_adjacent() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello world").unwrap();
        b.mark(InlineFormat::Strong, 0, 3).unwrap();
        b.mark(InlineFormat::Strong, 3, 5).unwrap();

        // Should have merged into one mark covering [0,5)
        for pos in 0..5 {
            assert!(b.formats_at(pos).unwrap().has(InlineFormat::Strong));
        }
        assert!(!b.formats_at(5).unwrap().has(InlineFormat::Strong));
    }

    #[test]
    fn test_mark_merges_overlapping() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello world").unwrap();
        b.mark(InlineFormat::Strong, 0, 5).unwrap();
        b.mark(InlineFormat::Strong, 3, 8).unwrap();

        for pos in 0..8 {
            assert!(b.formats_at(pos).unwrap().has(InlineFormat::Strong));
        }
        assert!(!b.formats_at(8).unwrap().has(InlineFormat::Strong));
    }

    #[test]
    fn test_unmark_full() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello world").unwrap();
        b.mark(InlineFormat::Strong, 0, 5).unwrap();
        b.unmark(InlineFormat::Strong, 0, 5).unwrap();

        for pos in 0..5 {
            assert!(!b.formats_at(pos).unwrap().has(InlineFormat::Strong));
        }
    }

    #[test]
    fn test_unmark_partial_start() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello world").unwrap();
        b.mark(InlineFormat::Strong, 0, 5).unwrap();
        b.unmark(InlineFormat::Strong, 0, 3).unwrap();

        assert!(!b.formats_at(0).unwrap().has(InlineFormat::Strong));
        assert!(!b.formats_at(2).unwrap().has(InlineFormat::Strong));
        assert!(b.formats_at(3).unwrap().has(InlineFormat::Strong));
        assert!(b.formats_at(4).unwrap().has(InlineFormat::Strong));
    }

    #[test]
    fn test_unmark_partial_end() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello world").unwrap();
        b.mark(InlineFormat::Strong, 0, 5).unwrap();
        b.unmark(InlineFormat::Strong, 3, 5).unwrap();

        assert!(b.formats_at(0).unwrap().has(InlineFormat::Strong));
        assert!(b.formats_at(2).unwrap().has(InlineFormat::Strong));
        assert!(!b.formats_at(3).unwrap().has(InlineFormat::Strong));
        assert!(!b.formats_at(4).unwrap().has(InlineFormat::Strong));
    }

    #[test]
    fn test_unmark_split() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello world").unwrap();
        b.mark(InlineFormat::Strong, 0, 11).unwrap();
        b.unmark(InlineFormat::Strong, 3, 8).unwrap();

        assert!(b.formats_at(0).unwrap().has(InlineFormat::Strong));
        assert!(b.formats_at(2).unwrap().has(InlineFormat::Strong));
        assert!(!b.formats_at(3).unwrap().has(InlineFormat::Strong));
        assert!(!b.formats_at(7).unwrap().has(InlineFormat::Strong));
        assert!(b.formats_at(8).unwrap().has(InlineFormat::Strong));
        assert!(b.formats_at(10).unwrap().has(InlineFormat::Strong));
    }

    #[test]
    fn test_splice_adjusts_marks_shift() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello world").unwrap();
        b.mark(InlineFormat::Strong, 6, 11).unwrap(); // "world"

        // Insert "XX" at position 0 — mark should shift right by 2
        b.splice(0, 0, "XX").unwrap();
        assert_eq!(b.text().unwrap(), "XXHello world");
        assert!(!b.formats_at(7).unwrap().has(InlineFormat::Strong));
        assert!(b.formats_at(8).unwrap().has(InlineFormat::Strong));
        assert!(b.formats_at(12).unwrap().has(InlineFormat::Strong));
    }

    #[test]
    fn test_splice_adjusts_marks_delete_before() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello world").unwrap();
        b.mark(InlineFormat::Strong, 6, 11).unwrap(); // "world"

        // Delete "Hello " (6 chars) — mark shifts left
        b.splice(0, 6, "").unwrap();
        assert_eq!(b.text().unwrap(), "world");
        assert!(b.formats_at(0).unwrap().has(InlineFormat::Strong));
        assert!(b.formats_at(4).unwrap().has(InlineFormat::Strong));
    }

    #[test]
    fn test_splice_delete_consuming_mark() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello world").unwrap();
        b.mark(InlineFormat::Strong, 2, 4).unwrap(); // "ll"

        // Delete "ello" (positions 1..5) — mark entirely consumed
        b.splice(1, 4, "").unwrap();
        assert_eq!(b.text().unwrap(), "H world");
        // No strong marks remain
        for pos in 0..b.char_count().unwrap() {
            assert!(!b.formats_at(pos).unwrap().has(InlineFormat::Strong));
        }
    }

    #[test]
    fn test_splice_delete_overlapping_mark_start() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "ABCDEFGH").unwrap();
        b.mark(InlineFormat::Strong, 2, 6).unwrap(); // "CDEF"

        // Delete "BCD" (positions 1..4)
        b.splice(1, 3, "").unwrap();
        assert_eq!(b.text().unwrap(), "AEFGH");
        // Mark should cover "EF" — positions 1,2
        assert!(!b.formats_at(0).unwrap().has(InlineFormat::Strong));
        assert!(b.formats_at(1).unwrap().has(InlineFormat::Strong));
        assert!(b.formats_at(2).unwrap().has(InlineFormat::Strong));
        assert!(!b.formats_at(3).unwrap().has(InlineFormat::Strong));
    }

    #[test]
    fn test_splice_insert_inside_mark_extends() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello world").unwrap();
        b.mark(InlineFormat::Strong, 0, 5).unwrap(); // "Hello"

        // Insert "XX" at position 2 (inside mark)
        b.splice(2, 0, "XX").unwrap();
        assert_eq!(b.text().unwrap(), "HeXXllo world");
        // Mark should now cover [0, 7) = "HeXXllo"
        for pos in 0..7 {
            assert!(
                b.formats_at(pos).unwrap().has(InlineFormat::Strong),
                "pos {} should be strong",
                pos
            );
        }
        assert!(!b.formats_at(7).unwrap().has(InlineFormat::Strong));
    }

    #[test]
    fn test_splice_replace_inside_mark() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello world").unwrap();
        b.mark(InlineFormat::Strong, 0, 5).unwrap(); // "Hello"

        // Replace "ell" with "a" (pos=1, delete=3, insert="a")
        b.splice(1, 3, "a").unwrap();
        assert_eq!(b.text().unwrap(), "Hao world");
        // Mark [0,5) → delete 3 inside, insert 1 → [0, 3)
        assert!(b.formats_at(0).unwrap().has(InlineFormat::Strong));
        assert!(b.formats_at(1).unwrap().has(InlineFormat::Strong));
        assert!(b.formats_at(2).unwrap().has(InlineFormat::Strong));
        assert!(!b.formats_at(3).unwrap().has(InlineFormat::Strong));
    }

    #[test]
    fn test_spans_plain_text() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello world").unwrap();

        let spans = b.spans().unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "Hello world");
        assert!(spans[0].formats.is_empty());
    }

    #[test]
    fn test_spans_single_format() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello world").unwrap();
        b.mark(InlineFormat::Strong, 0, 5).unwrap();

        let spans = b.spans().unwrap();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].text, "Hello");
        assert!(spans[0].formats.has(InlineFormat::Strong));
        assert_eq!(spans[1].text, " world");
        assert!(spans[1].formats.is_empty());
    }

    #[test]
    fn test_spans_multiple_formats() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello beautiful world").unwrap();
        b.mark(InlineFormat::Strong, 0, 5).unwrap();
        b.mark(InlineFormat::Emphasis, 6, 15).unwrap();
        b.mark(InlineFormat::Code, 16, 21).unwrap();

        let spans = b.spans().unwrap();
        assert_eq!(spans.len(), 5);
        assert_eq!(spans[0].text, "Hello");
        assert!(spans[0].formats.has(InlineFormat::Strong));
        assert_eq!(spans[1].text, " ");
        assert!(spans[1].formats.is_empty());
        assert_eq!(spans[2].text, "beautiful");
        assert!(spans[2].formats.has(InlineFormat::Emphasis));
        assert_eq!(spans[3].text, " ");
        assert!(spans[3].formats.is_empty());
        assert_eq!(spans[4].text, "world");
        assert!(spans[4].formats.has(InlineFormat::Code));
    }

    #[test]
    fn test_spans_overlapping_formats() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello").unwrap();
        b.mark(InlineFormat::Strong, 0, 5).unwrap();
        b.mark(InlineFormat::Emphasis, 0, 5).unwrap();

        let spans = b.spans().unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "Hello");
        assert!(spans[0].formats.has(InlineFormat::Strong));
        assert!(spans[0].formats.has(InlineFormat::Emphasis));
    }

    #[test]
    fn test_multiple_formats_different_ranges() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "ABCDE").unwrap();
        b.mark(InlineFormat::Strong, 0, 3).unwrap(); // ABC
        b.mark(InlineFormat::Emphasis, 2, 5).unwrap(); // CDE

        let spans = b.spans().unwrap();
        assert_eq!(spans.len(), 3);
        // AB: strong only
        assert_eq!(spans[0].text, "AB");
        assert!(spans[0].formats.has(InlineFormat::Strong));
        assert!(!spans[0].formats.has(InlineFormat::Emphasis));
        // C: strong + emphasis
        assert_eq!(spans[1].text, "C");
        assert!(spans[1].formats.has(InlineFormat::Strong));
        assert!(spans[1].formats.has(InlineFormat::Emphasis));
        // DE: emphasis only
        assert_eq!(spans[2].text, "DE");
        assert!(!spans[2].formats.has(InlineFormat::Strong));
        assert!(spans[2].formats.has(InlineFormat::Emphasis));
    }

    #[test]
    #[should_panic(expected = "mark requires start < end")]
    fn test_mark_empty_range() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello").unwrap();
        b.mark(InlineFormat::Strong, 2, 2).unwrap();
    }

    #[test]
    #[should_panic(expected = "mark end out of bounds")]
    fn test_mark_out_of_bounds() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello").unwrap();
        b.mark(InlineFormat::Strong, 0, 100).unwrap();
    }

    #[test]
    #[should_panic(expected = "unmark requires start < end")]
    fn test_unmark_empty_range() {
        let mut b = LocalBackend::new();
        b.splice(0, 0, "Hello").unwrap();
        b.unmark(InlineFormat::Strong, 2, 2).unwrap();
    }

    #[test]
    #[should_panic(expected = "formats_at pos out of bounds")]
    fn test_formats_at_out_of_bounds() {
        let b = LocalBackend::new();
        b.formats_at(0).unwrap();
    }
}
