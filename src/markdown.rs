//! Markdown projection (bidirectional conversion)

use automerge::marks::{ExpandMark, Mark};
use automerge::transaction::Transactable;
use automerge::{AutoCommit, ObjId as ExId, ReadDoc};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::format::InlineFormat;

/// Convert Automerge rich text spans to markdown
///
/// Reads the text and marks from the Automerge document, then emits markdown
/// with appropriate delimiters. Marks are nested in a fixed order:
/// strong > em > strikethrough > code
///
/// Unknown marks in the document are preserved (not included in markdown output).
///
/// # Arguments
/// * `doc` - The Automerge document
/// * `text_obj` - The ExId of the text object
///
/// # Returns
/// Markdown string representation
pub fn spans_to_markdown(doc: &AutoCommit, text_obj: &ExId) -> String {
    let text = doc.text(text_obj).expect("Failed to read text");

    if text.is_empty() {
        return String::new();
    }

    let text_len = text.chars().count();
    let mut output = String::new();

    let mut pos = 0;
    while pos < text_len {
        // Get marks at current position
        let marks = doc.get_marks(text_obj, pos, None).unwrap_or_default();

        let mut has_strong = false;
        let mut has_em = false;
        let mut has_strikethrough = false;
        let mut has_code = false;

        for (mark_name, _) in marks.iter() {
            match InlineFormat::from_mark_name(mark_name) {
                Some(InlineFormat::Strong) => has_strong = true,
                Some(InlineFormat::Emphasis) => has_em = true,
                Some(InlineFormat::Strikethrough) => has_strikethrough = true,
                Some(InlineFormat::Code) => has_code = true,
                None => {}
            }
        }

        // Find the end of this span
        let span_end = (pos + 1..=text_len)
            .find(|&next_pos| {
                if next_pos >= text_len {
                    return true;
                }

                let next_marks = doc.get_marks(text_obj, next_pos, None).unwrap_or_default();

                let mut next_strong = false;
                let mut next_em = false;
                let mut next_strikethrough = false;
                let mut next_code = false;

                for (mark_name, _) in next_marks.iter() {
                    match InlineFormat::from_mark_name(mark_name) {
                        Some(InlineFormat::Strong) => next_strong = true,
                        Some(InlineFormat::Emphasis) => next_em = true,
                        Some(InlineFormat::Strikethrough) => next_strikethrough = true,
                        Some(InlineFormat::Code) => next_code = true,
                        None => {}
                    }
                }

                next_strong != has_strong
                    || next_em != has_em
                    || next_strikethrough != has_strikethrough
                    || next_code != has_code
            })
            .unwrap_or(text_len);

        // Extract span text
        let span_text: String = text
            .chars()
            .skip(pos)
            .take(span_end - pos)
            .collect();

        // Emit markdown with nested delimiters in fixed order
        if has_strong {
            output.push_str("**");
        }
        if has_em {
            output.push('*');
        }
        if has_strikethrough {
            output.push_str("~~");
        }
        if has_code {
            output.push('`');
        }

        output.push_str(&span_text);

        // Close delimiters in reverse order
        if has_code {
            output.push('`');
        }
        if has_strikethrough {
            output.push_str("~~");
        }
        if has_em {
            output.push('*');
        }
        if has_strong {
            output.push_str("**");
        }

        pos = span_end;
    }

    output
}

/// Apply markdown to Automerge rich text
///
/// Parses the markdown string using pulldown-cmark (CommonMark + GFM strikethrough),
/// then updates the Automerge document with the appropriate marks.
///
/// This function clears the existing text and marks, then writes the new content.
///
/// # Arguments
/// * `doc` - The Automerge document (mutable)
/// * `text_obj` - The ExId of the text object
/// * `markdown` - The markdown string to parse
pub fn apply_markdown(doc: &mut AutoCommit, text_obj: &ExId, markdown: &str) {
    // Clear existing text
    let existing_text = doc.text(text_obj).expect("Failed to read text");
    let existing_len = existing_text.chars().count();
    if existing_len > 0 {
        doc.splice_text(text_obj, 0, existing_len as isize, "")
            .expect("Failed to clear text");
    }

    if markdown.is_empty() {
        return;
    }

    // Parse markdown with GFM extensions (for strikethrough)
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);

    let parser = Parser::new_ext(markdown, options);

    // First pass: collect all text
    let mut text_builder = String::new();
    // Second pass: collect marks to apply
    let mut marks_to_apply: Vec<(String, usize, usize)> = Vec::new();

    // Track active formatting as we parse
    let mut strong_start: Option<usize> = None;
    let mut em_start: Option<usize> = None;
    let mut strikethrough_start: Option<usize> = None;

    // Current character position
    let mut char_pos = 0;

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Strong => {
                    strong_start = Some(char_pos);
                }
                Tag::Emphasis => {
                    em_start = Some(char_pos);
                }
                Tag::Strikethrough => {
                    strikethrough_start = Some(char_pos);
                }
                _ => {} // Ignore other tags for MVP (block elements)
            },
            Event::End(tag) => match tag {
                TagEnd::Strong => {
                    if let Some(start) = strong_start {
                        if char_pos > start {
                            marks_to_apply.push(("strong".to_string(), start, char_pos));
                        }
                    }
                    strong_start = None;
                }
                TagEnd::Emphasis => {
                    if let Some(start) = em_start {
                        if char_pos > start {
                            marks_to_apply.push(("em".to_string(), start, char_pos));
                        }
                    }
                    em_start = None;
                }
                TagEnd::Strikethrough => {
                    if let Some(start) = strikethrough_start {
                        if char_pos > start {
                            marks_to_apply.push(("strikethrough".to_string(), start, char_pos));
                        }
                    }
                    strikethrough_start = None;
                }
                _ => {}
            },
            Event::Text(text) => {
                let text_str = text.as_ref();
                text_builder.push_str(text_str);
                char_pos += text_str.chars().count();
            }
            Event::Code(text) => {
                // Inline code: collect text and mark
                let text_str = text.as_ref();
                let start = char_pos;
                text_builder.push_str(text_str);
                char_pos += text_str.chars().count();
                marks_to_apply.push(("code".to_string(), start, char_pos));
            }
            Event::SoftBreak | Event::HardBreak => {
                text_builder.push('\n');
                char_pos += 1;
            }
            _ => {} // Ignore other events
        }
    }

    // Now insert all text at once
    doc.splice_text(text_obj, 0, 0, &text_builder)
        .expect("Failed to insert text");

    // Then apply all marks
    for (mark_name, start, end) in marks_to_apply {
        let mark = Mark::new(mark_name, true, start, end);
        doc.mark(text_obj, mark, ExpandMark::Both)
            .expect("Failed to apply mark");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use automerge::ObjType;

    fn create_test_doc() -> (AutoCommit, ExId) {
        let mut doc = AutoCommit::new();
        let text_obj = doc
            .put_object(automerge::ROOT, "text", ObjType::Text)
            .unwrap();
        (doc, text_obj)
    }

    #[test]
    fn test_plain_text_to_markdown() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();

        let markdown = spans_to_markdown(&doc, &text_obj);
        assert_eq!(markdown, "Hello world");
    }

    #[test]
    fn test_bold_to_markdown() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();

        let mark = Mark::new("strong".to_string(), true, 0, 5);
        doc.mark(&text_obj, mark, ExpandMark::Both).unwrap();

        let markdown = spans_to_markdown(&doc, &text_obj);
        assert_eq!(markdown, "**Hello** world");
    }

    #[test]
    fn test_italic_to_markdown() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();

        let mark = Mark::new("em".to_string(), true, 6, 11);
        doc.mark(&text_obj, mark, ExpandMark::Both).unwrap();

        let markdown = spans_to_markdown(&doc, &text_obj);
        assert_eq!(markdown, "Hello *world*");
    }

    #[test]
    fn test_strikethrough_to_markdown() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();

        let mark = Mark::new("strikethrough".to_string(), true, 0, 5);
        doc.mark(&text_obj, mark, ExpandMark::Both).unwrap();

        let markdown = spans_to_markdown(&doc, &text_obj);
        assert_eq!(markdown, "~~Hello~~ world");
    }

    #[test]
    fn test_code_to_markdown() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();

        let mark = Mark::new("code".to_string(), true, 6, 11);
        doc.mark(&text_obj, mark, ExpandMark::Both).unwrap();

        let markdown = spans_to_markdown(&doc, &text_obj);
        assert_eq!(markdown, "Hello `world`");
    }

    #[test]
    fn test_combined_formatting_to_markdown() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello").unwrap();

        let strong_mark = Mark::new("strong".to_string(), true, 0, 5);
        doc.mark(&text_obj, strong_mark, ExpandMark::Both).unwrap();

        let em_mark = Mark::new("em".to_string(), true, 0, 5);
        doc.mark(&text_obj, em_mark, ExpandMark::Both).unwrap();

        let markdown = spans_to_markdown(&doc, &text_obj);
        assert_eq!(markdown, "***Hello***");
    }

    #[test]
    fn test_markdown_to_plain_text() {
        let (mut doc, text_obj) = create_test_doc();
        apply_markdown(&mut doc, &text_obj, "Hello world");

        let text = doc.text(&text_obj).unwrap();
        assert_eq!(text, "Hello world");

        let marks_at_0 = doc.get_marks(&text_obj, 0, None).unwrap();
        assert_eq!(marks_at_0.len(), 0);
    }

    #[test]
    fn test_simple_mark() {
        // Simple test to understand mark semantics
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "12345").unwrap();

        let mark = Mark::new("strong".to_string(), true, 1, 3);
        doc.mark(&text_obj, mark, ExpandMark::Both).unwrap();

        for i in 0..5 {
            let marks = doc.get_marks(&text_obj, i, None).unwrap();
            let has_strong = marks.iter().any(|(name, _)| name == "strong");
            eprintln!("Position {}: has_strong={}", i, has_strong);
        }

        // Position 1 and 2 should be marked (range 1..3 means [1, 3))
        assert!(doc.get_marks(&text_obj, 1, None).unwrap().iter().any(|(name, _)| name == "strong"));
        assert!(doc.get_marks(&text_obj, 2, None).unwrap().iter().any(|(name, _)| name == "strong"));
        assert!(!doc.get_marks(&text_obj, 0, None).unwrap().iter().any(|(name, _)| name == "strong"));
        assert!(!doc.get_marks(&text_obj, 3, None).unwrap().iter().any(|(name, _)| name == "strong"));
    }

    #[test]
    fn test_markdown_to_bold() {
        let (mut doc, text_obj) = create_test_doc();
        apply_markdown(&mut doc, &text_obj, "**Hello** world");

        let text = doc.text(&text_obj).unwrap();
        assert_eq!(text, "Hello world");

        // Check "Hello" is marked as strong
        for pos in 0..5 {
            let marks = doc.get_marks(&text_obj, pos, None).unwrap();
            assert!(marks.iter().any(|(name, _)| name == "strong"), "Position {} should have strong mark", pos);
        }

        // Check " world" is not marked
        for pos in 5..11 {
            let marks = doc.get_marks(&text_obj, pos, None).unwrap();
            assert!(!marks.iter().any(|(name, _)| name == "strong"), "Position {} should not have strong mark", pos);
        }
    }

    #[test]
    fn test_markdown_to_italic() {
        let (mut doc, text_obj) = create_test_doc();
        apply_markdown(&mut doc, &text_obj, "Hello *world*");

        let text = doc.text(&text_obj).unwrap();
        assert_eq!(text, "Hello world");

        // Check "world" is marked as em
        for pos in 6..11 {
            let marks = doc.get_marks(&text_obj, pos, None).unwrap();
            assert!(marks.iter().any(|(name, _)| name == "em"));
        }
    }

    #[test]
    fn test_markdown_to_strikethrough() {
        let (mut doc, text_obj) = create_test_doc();
        apply_markdown(&mut doc, &text_obj, "~~Hello~~ world");

        let text = doc.text(&text_obj).unwrap();
        assert_eq!(text, "Hello world");

        // Check "Hello" is marked as strikethrough
        for pos in 0..5 {
            let marks = doc.get_marks(&text_obj, pos, None).unwrap();
            assert!(marks.iter().any(|(name, _)| name == "strikethrough"));
        }
    }

    #[test]
    fn test_markdown_to_code() {
        let (mut doc, text_obj) = create_test_doc();
        apply_markdown(&mut doc, &text_obj, "Hello `world`");

        let text = doc.text(&text_obj).unwrap();
        assert_eq!(text, "Hello world");

        // Check "world" is marked as code
        for pos in 6..11 {
            let marks = doc.get_marks(&text_obj, pos, None).unwrap();
            assert!(marks.iter().any(|(name, _)| name == "code"));
        }
    }

    #[test]
    fn test_markdown_to_combined() {
        let (mut doc, text_obj) = create_test_doc();
        apply_markdown(&mut doc, &text_obj, "***Hello***");

        let text = doc.text(&text_obj).unwrap();
        assert_eq!(text, "Hello");

        // Check all characters have both strong and em
        for pos in 0..5 {
            let marks = doc.get_marks(&text_obj, pos, None).unwrap();
            assert!(marks.iter().any(|(name, _)| name == "strong"));
            assert!(marks.iter().any(|(name, _)| name == "em"));
        }
    }

    #[test]
    fn test_round_trip_plain() {
        let (mut doc, text_obj) = create_test_doc();
        let original = "Hello world";

        apply_markdown(&mut doc, &text_obj, original);
        let markdown = spans_to_markdown(&doc, &text_obj);

        assert_eq!(markdown, original);
    }

    #[test]
    fn test_round_trip_bold() {
        let (mut doc, text_obj) = create_test_doc();
        let original = "**Hello** world";

        apply_markdown(&mut doc, &text_obj, original);
        let markdown = spans_to_markdown(&doc, &text_obj);

        assert_eq!(markdown, original);
    }

    #[test]
    fn test_round_trip_combined() {
        let (mut doc, text_obj) = create_test_doc();
        let original = "***Hello***";

        apply_markdown(&mut doc, &text_obj, original);
        let markdown = spans_to_markdown(&doc, &text_obj);

        assert_eq!(markdown, original);
    }

    #[test]
    fn test_round_trip_mixed() {
        let (mut doc, text_obj) = create_test_doc();
        let original = "**bold** *italic* ~~strike~~ `code`";

        apply_markdown(&mut doc, &text_obj, original);
        let markdown = spans_to_markdown(&doc, &text_obj);

        assert_eq!(markdown, original);
    }
}
