//! Inline formatting types and operations

use automerge::marks::ExpandMark;
use automerge::transaction::Transactable;
use automerge::{marks::Mark, AutoCommit, ObjId as ExId, ReadDoc};

/// Inline formatting options supported by the editor
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InlineFormat {
    /// Bold text (Automerge mark: "strong")
    Strong,
    /// Italic text (Automerge mark: "em")
    Emphasis,
    /// Strikethrough text (Automerge mark: "strikethrough")
    Strikethrough,
    /// Inline code (Automerge mark: "code")
    Code,
}

impl InlineFormat {
    /// Get the Automerge mark name for this format
    pub fn mark_name(&self) -> &'static str {
        match self {
            InlineFormat::Strong => "strong",
            InlineFormat::Emphasis => "em",
            InlineFormat::Strikethrough => "strikethrough",
            InlineFormat::Code => "code",
        }
    }

    /// Get the expand behavior for this mark
    /// All inline formats use ExpandMark::Both (typing at boundary continues formatting)
    pub fn expand_mark(&self) -> ExpandMark {
        ExpandMark::Both
    }

    /// Parse a mark name into an InlineFormat
    pub fn from_mark_name(name: &str) -> Option<Self> {
        match name {
            "strong" => Some(InlineFormat::Strong),
            "em" => Some(InlineFormat::Emphasis),
            "strikethrough" => Some(InlineFormat::Strikethrough),
            "code" => Some(InlineFormat::Code),
            _ => None,
        }
    }

    /// Get all supported formats
    pub fn all() -> &'static [InlineFormat] {
        &[
            InlineFormat::Strong,
            InlineFormat::Emphasis,
            InlineFormat::Strikethrough,
            InlineFormat::Code,
        ]
    }
}

/// Check if a range is fully formatted with the given mark
fn is_range_fully_formatted(
    doc: &AutoCommit,
    text_obj: &ExId,
    format: InlineFormat,
    start: usize,
    end: usize,
) -> bool {
    if start >= end {
        return false;
    }

    let mark_name = format.mark_name();

    // Check every character in the range
    for pos in start..end {
        let marks = doc.get_marks(text_obj, pos, None).unwrap_or_default();
        let has_mark = marks.iter().any(|(name, _)| name == mark_name);
        if !has_mark {
            return false;
        }
    }

    true
}

/// Toggle inline formatting on a text range
///
/// If the entire range is already formatted, removes the format.
/// Otherwise, applies the format to the range.
///
/// # Panics
/// Panics if start >= end (empty ranges are not supported)
///
/// # Arguments
/// * `doc` - The Automerge document
/// * `text_obj` - The ExId of the text object
/// * `format` - The format to toggle
/// * `start` - Start character index (inclusive)
/// * `end` - End character index (exclusive)
pub fn toggle_format(
    doc: &mut AutoCommit,
    text_obj: &ExId,
    format: InlineFormat,
    start: usize,
    end: usize,
) {
    if start >= end {
        panic!(
            "toggle_format requires non-empty range: start={}, end={}",
            start, end
        );
    }

    let text_len = doc.text(text_obj).unwrap_or_default().len();
    if end > text_len {
        panic!(
            "toggle_format end index out of bounds: end={}, text_len={}",
            end, text_len
        );
    }

    let mark_name = format.mark_name();
    let expand = format.expand_mark();

    if is_range_fully_formatted(doc, text_obj, format, start, end) {
        // Remove the mark
        doc.unmark(text_obj, mark_name, start, end, expand)
            .expect("Failed to unmark text");
    } else {
        // Apply the mark - Mark::new takes (name, value, start, end)
        // For boolean marks, we use true as the value
        let mark = Mark::new(mark_name.to_string(), true, start, end);
        doc.mark(text_obj, mark, expand)
            .expect("Failed to mark text");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use automerge::transaction::Transactable;

    #[test]
    fn test_mark_name_mapping() {
        assert_eq!(InlineFormat::Strong.mark_name(), "strong");
        assert_eq!(InlineFormat::Emphasis.mark_name(), "em");
        assert_eq!(InlineFormat::Strikethrough.mark_name(), "strikethrough");
        assert_eq!(InlineFormat::Code.mark_name(), "code");
    }

    #[test]
    fn test_from_mark_name() {
        assert_eq!(
            InlineFormat::from_mark_name("strong"),
            Some(InlineFormat::Strong)
        );
        assert_eq!(
            InlineFormat::from_mark_name("em"),
            Some(InlineFormat::Emphasis)
        );
        assert_eq!(
            InlineFormat::from_mark_name("strikethrough"),
            Some(InlineFormat::Strikethrough)
        );
        assert_eq!(
            InlineFormat::from_mark_name("code"),
            Some(InlineFormat::Code)
        );
        assert_eq!(InlineFormat::from_mark_name("unknown"), None);
    }

    #[test]
    fn test_expand_mark() {
        for format in InlineFormat::all() {
            assert_eq!(format.expand_mark(), ExpandMark::Both);
        }
    }

    #[test]
    fn test_toggle_format_apply() {
        let mut doc = AutoCommit::new();
        let text_obj = doc.put_object(automerge::ROOT, "text", automerge::ObjType::Text).unwrap();
        doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();

        // Apply bold to "Hello"
        toggle_format(&mut doc, &text_obj, InlineFormat::Strong, 0, 5);

        // Verify marks
        for pos in 0..5 {
            let marks = doc.get_marks(&text_obj, pos, None).unwrap();
            assert!(marks.iter().any(|(name, _)| name == "strong"));
        }
        for pos in 5..11 {
            let marks = doc.get_marks(&text_obj, pos, None).unwrap();
            assert!(!marks.iter().any(|(name, _)| name == "strong"));
        }
    }

    #[test]
    fn test_toggle_format_remove() {
        let mut doc = AutoCommit::new();
        let text_obj = doc.put_object(automerge::ROOT, "text", automerge::ObjType::Text).unwrap();
        doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();

        // Apply bold
        let mark = Mark::new("strong".to_string(), true, 0, 5);
        doc.mark(&text_obj, mark, ExpandMark::Both).unwrap();

        // Remove bold
        toggle_format(&mut doc, &text_obj, InlineFormat::Strong, 0, 5);

        // Verify no marks
        for pos in 0..5 {
            let marks = doc.get_marks(&text_obj, pos, None).unwrap();
            assert!(!marks.iter().any(|(name, _)| name == "strong"));
        }
    }

    #[test]
    fn test_toggle_format_partial() {
        let mut doc = AutoCommit::new();
        let text_obj = doc.put_object(automerge::ROOT, "text", automerge::ObjType::Text).unwrap();
        doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();

        // Apply bold to "Hello"
        let mark = Mark::new("strong".to_string(), true, 0, 5);
        doc.mark(&text_obj, mark, ExpandMark::Both).unwrap();

        // Toggle bold on "Hello world" (partially formatted)
        // Should apply to entire range
        toggle_format(&mut doc, &text_obj, InlineFormat::Strong, 0, 11);

        // Verify all marked
        for pos in 0..11 {
            let marks = doc.get_marks(&text_obj, pos, None).unwrap();
            assert!(marks.iter().any(|(name, _)| name == "strong"));
        }
    }

    #[test]
    #[should_panic(expected = "toggle_format requires non-empty range")]
    fn test_toggle_format_empty_range() {
        let mut doc = AutoCommit::new();
        let text_obj = doc.put_object(automerge::ROOT, "text", automerge::ObjType::Text).unwrap();
        doc.splice_text(&text_obj, 0, 0, "Hello").unwrap();

        toggle_format(&mut doc, &text_obj, InlineFormat::Strong, 2, 2);
    }

    #[test]
    #[should_panic(expected = "toggle_format end index out of bounds")]
    fn test_toggle_format_out_of_bounds() {
        let mut doc = AutoCommit::new();
        let text_obj = doc.put_object(automerge::ROOT, "text", automerge::ObjType::Text).unwrap();
        doc.splice_text(&text_obj, 0, 0, "Hello").unwrap();

        toggle_format(&mut doc, &text_obj, InlineFormat::Strong, 0, 100);
    }
}
