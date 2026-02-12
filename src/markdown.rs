use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::backend::TextBackend;
use crate::format::InlineFormat;

/// Convert rich text spans to markdown.
///
/// Uses `backend.spans()` — each backend implements span scanning once.
pub fn spans_to_markdown<B: TextBackend>(backend: &B) -> Result<String, B::Error> {
    let spans = backend.spans()?;

    if spans.is_empty() {
        return Ok(String::new());
    }

    let mut output = String::new();

    for span in &spans {
        let fs = &span.formats;

        // Open delimiters in fixed order: strong > em > strikethrough > code
        if fs.strong {
            output.push_str("**");
        }
        if fs.emphasis {
            output.push('*');
        }
        if fs.strikethrough {
            output.push_str("~~");
        }
        if fs.code {
            output.push('`');
        }

        output.push_str(&span.text);

        // Close in reverse order
        if fs.code {
            output.push('`');
        }
        if fs.strikethrough {
            output.push_str("~~");
        }
        if fs.emphasis {
            output.push('*');
        }
        if fs.strong {
            output.push_str("**");
        }
    }

    Ok(output)
}

/// Parse markdown and apply it to a backend, clearing existing content.
///
/// Uses splice for text insertion and mark for formatting.
pub fn apply_markdown<B: TextBackend>(backend: &mut B, markdown: &str) -> Result<(), B::Error> {
    // Clear existing content
    let existing_len = backend.char_count()?;
    if existing_len > 0 {
        backend.splice(0, existing_len, "")?;
    }

    if markdown.is_empty() {
        return Ok(());
    }

    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);

    let parser = Parser::new_ext(markdown, options);

    let mut text_builder = String::new();
    let mut marks_to_apply: Vec<(InlineFormat, usize, usize)> = Vec::new();

    let mut strong_start: Option<usize> = None;
    let mut em_start: Option<usize> = None;
    let mut strikethrough_start: Option<usize> = None;

    let mut char_pos: usize = 0;

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Strong => strong_start = Some(char_pos),
                Tag::Emphasis => em_start = Some(char_pos),
                Tag::Strikethrough => strikethrough_start = Some(char_pos),
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Strong => {
                    if let Some(start) = strong_start.take() {
                        if char_pos > start {
                            marks_to_apply.push((InlineFormat::Strong, start, char_pos));
                        }
                    }
                }
                TagEnd::Emphasis => {
                    if let Some(start) = em_start.take() {
                        if char_pos > start {
                            marks_to_apply.push((InlineFormat::Emphasis, start, char_pos));
                        }
                    }
                }
                TagEnd::Strikethrough => {
                    if let Some(start) = strikethrough_start.take() {
                        if char_pos > start {
                            marks_to_apply.push((InlineFormat::Strikethrough, start, char_pos));
                        }
                    }
                }
                _ => {}
            },
            Event::Text(text) => {
                let text_str = text.as_ref();
                text_builder.push_str(text_str);
                char_pos += text_str.chars().count();
            }
            Event::Code(text) => {
                let text_str = text.as_ref();
                let start = char_pos;
                text_builder.push_str(text_str);
                char_pos += text_str.chars().count();
                marks_to_apply.push((InlineFormat::Code, start, char_pos));
            }
            Event::SoftBreak | Event::HardBreak => {
                text_builder.push('\n');
                char_pos += 1;
            }
            _ => {}
        }
    }

    // Insert all text at once
    backend.splice(0, 0, &text_builder)?;

    // Apply marks
    for (format, start, end) in marks_to_apply {
        backend.mark(format, start, end)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::LocalBackend;

    fn backend_with_text(text: &str) -> LocalBackend {
        let mut b = LocalBackend::new();
        b.splice(0, 0, text).unwrap();
        b
    }

    #[test]
    fn test_plain_text_to_markdown() {
        let b = backend_with_text("Hello world");
        assert_eq!(spans_to_markdown(&b).unwrap(), "Hello world");
    }

    #[test]
    fn test_bold_to_markdown() {
        let mut b = backend_with_text("Hello world");
        b.mark(InlineFormat::Strong, 0, 5).unwrap();
        assert_eq!(spans_to_markdown(&b).unwrap(), "**Hello** world");
    }

    #[test]
    fn test_italic_to_markdown() {
        let mut b = backend_with_text("Hello world");
        b.mark(InlineFormat::Emphasis, 6, 11).unwrap();
        assert_eq!(spans_to_markdown(&b).unwrap(), "Hello *world*");
    }

    #[test]
    fn test_strikethrough_to_markdown() {
        let mut b = backend_with_text("Hello world");
        b.mark(InlineFormat::Strikethrough, 0, 5).unwrap();
        assert_eq!(spans_to_markdown(&b).unwrap(), "~~Hello~~ world");
    }

    #[test]
    fn test_code_to_markdown() {
        let mut b = backend_with_text("Hello world");
        b.mark(InlineFormat::Code, 6, 11).unwrap();
        assert_eq!(spans_to_markdown(&b).unwrap(), "Hello `world`");
    }

    #[test]
    fn test_combined_bold_italic_to_markdown() {
        let mut b = backend_with_text("Hello");
        b.mark(InlineFormat::Strong, 0, 5).unwrap();
        b.mark(InlineFormat::Emphasis, 0, 5).unwrap();
        assert_eq!(spans_to_markdown(&b).unwrap(), "***Hello***");
    }

    #[test]
    fn test_all_four_formats_to_markdown() {
        let mut b = backend_with_text("Test");
        b.mark(InlineFormat::Strong, 0, 4).unwrap();
        b.mark(InlineFormat::Emphasis, 0, 4).unwrap();
        b.mark(InlineFormat::Strikethrough, 0, 4).unwrap();
        b.mark(InlineFormat::Code, 0, 4).unwrap();
        assert_eq!(spans_to_markdown(&b).unwrap(), "***~~`Test`~~***");
    }

    #[test]
    fn test_empty_to_markdown() {
        let b = LocalBackend::new();
        assert_eq!(spans_to_markdown(&b).unwrap(), "");
    }

    #[test]
    fn test_apply_plain_text() {
        let mut b = LocalBackend::new();
        apply_markdown(&mut b, "Hello world").unwrap();
        assert_eq!(b.text().unwrap(), "Hello world");
        assert!(b.formats_at(0).unwrap().is_empty());
    }

    #[test]
    fn test_apply_bold() {
        let mut b = LocalBackend::new();
        apply_markdown(&mut b, "**Hello** world").unwrap();
        assert_eq!(b.text().unwrap(), "Hello world");

        for pos in 0..5 {
            assert!(b.formats_at(pos).unwrap().has(InlineFormat::Strong));
        }
        for pos in 5..11 {
            assert!(!b.formats_at(pos).unwrap().has(InlineFormat::Strong));
        }
    }

    #[test]
    fn test_apply_italic() {
        let mut b = LocalBackend::new();
        apply_markdown(&mut b, "Hello *world*").unwrap();
        assert_eq!(b.text().unwrap(), "Hello world");

        for pos in 6..11 {
            assert!(b.formats_at(pos).unwrap().has(InlineFormat::Emphasis));
        }
    }

    #[test]
    fn test_apply_strikethrough() {
        let mut b = LocalBackend::new();
        apply_markdown(&mut b, "~~Hello~~ world").unwrap();
        assert_eq!(b.text().unwrap(), "Hello world");

        for pos in 0..5 {
            assert!(b.formats_at(pos).unwrap().has(InlineFormat::Strikethrough));
        }
    }

    #[test]
    fn test_apply_code() {
        let mut b = LocalBackend::new();
        apply_markdown(&mut b, "Hello `world`").unwrap();
        assert_eq!(b.text().unwrap(), "Hello world");

        for pos in 6..11 {
            assert!(b.formats_at(pos).unwrap().has(InlineFormat::Code));
        }
    }

    #[test]
    fn test_apply_combined() {
        let mut b = LocalBackend::new();
        apply_markdown(&mut b, "***Hello***").unwrap();
        assert_eq!(b.text().unwrap(), "Hello");

        for pos in 0..5 {
            assert!(b.formats_at(pos).unwrap().has(InlineFormat::Strong));
            assert!(b.formats_at(pos).unwrap().has(InlineFormat::Emphasis));
        }
    }

    #[test]
    fn test_apply_clears_previous_content() {
        let mut b = LocalBackend::new();
        apply_markdown(&mut b, "**Old**").unwrap();
        apply_markdown(&mut b, "*New*").unwrap();

        assert_eq!(b.text().unwrap(), "New");
        assert!(b.formats_at(0).unwrap().has(InlineFormat::Emphasis));
        assert!(!b.formats_at(0).unwrap().has(InlineFormat::Strong));
    }

    #[test]
    fn test_round_trip_plain() {
        let mut b = LocalBackend::new();
        let original = "Hello world";
        apply_markdown(&mut b, original).unwrap();
        assert_eq!(spans_to_markdown(&b).unwrap(), original);
    }

    #[test]
    fn test_round_trip_bold() {
        let mut b = LocalBackend::new();
        let original = "**Hello** world";
        apply_markdown(&mut b, original).unwrap();
        assert_eq!(spans_to_markdown(&b).unwrap(), original);
    }

    #[test]
    fn test_round_trip_combined() {
        let mut b = LocalBackend::new();
        let original = "***Hello***";
        apply_markdown(&mut b, original).unwrap();
        assert_eq!(spans_to_markdown(&b).unwrap(), original);
    }

    #[test]
    fn test_round_trip_mixed() {
        let mut b = LocalBackend::new();
        let original = "**bold** *italic* ~~strike~~ `code`";
        apply_markdown(&mut b, original).unwrap();
        assert_eq!(spans_to_markdown(&b).unwrap(), original);
    }

    #[test]
    fn test_apply_empty() {
        let mut b = LocalBackend::new();
        apply_markdown(&mut b, "").unwrap();
        assert_eq!(b.text().unwrap(), "");
    }
}
