//! Layout job generation from Automerge spans

use automerge::{AutoCommit, ObjId as ExId, ReadDoc};
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Stroke};

use crate::format::InlineFormat;

/// Font configuration for rendering rich text
///
/// egui does not have a bold flag - bold text requires a separate font.
/// The consumer must register bold/regular/code font variants in egui's
/// FontDefinitions before using this widget.
#[derive(Debug, Clone)]
pub struct FontConfig {
    /// Regular font (used for unmarked text)
    pub regular: FontId,
    /// Bold font (used for Strong marks)
    pub bold: FontId,
    /// Monospace font (used for Code marks)
    pub code: FontId,
    /// Background color for inline code
    pub code_background: Color32,
    /// Default text color
    pub text_color: Color32,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            regular: FontId::proportional(14.0),
            bold: FontId::proportional(14.0), // Consumer should override with actual bold font
            code: FontId::monospace(14.0),
            code_background: Color32::from_gray(240),
            text_color: Color32::from_gray(0),
        }
    }
}

/// Build an egui LayoutJob from Automerge text spans
///
/// This function reads the text and marks from the Automerge document,
/// then generates a LayoutJob with appropriate TextFormat for each span.
///
/// # Mark to TextFormat mapping:
/// - "strong" → bold font
/// - "em" → italics flag
/// - "strikethrough" → strikethrough stroke
/// - "code" → monospace font + background color
///
/// Marks compose naturally (e.g., strong + em = bold + italic).
///
/// # Arguments
/// * `doc` - The Automerge document
/// * `text_obj` - The ExId of the text object
/// * `font_config` - Font configuration
///
/// # Returns
/// A LayoutJob ready for egui rendering
pub fn build_layout_job(
    doc: &AutoCommit,
    text_obj: &ExId,
    font_config: &FontConfig,
) -> LayoutJob {
    let text = doc.text(text_obj).expect("Failed to read text");
    let mut job = LayoutJob::default();

    if text.is_empty() {
        return job;
    }

    let text_len = text.chars().count();

    // We need to process the text character by character and group consecutive
    // characters with the same formatting into spans
    let mut current_pos = 0;
    while current_pos < text_len {
        // Get marks at current position
        let marks = doc
            .get_marks(text_obj, current_pos, None)
            .unwrap_or_default();

        // Determine the formatting for this position
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
                None => {} // Unknown mark, ignore
            }
        }

        // Find the end of this span (where formatting changes)
        let span_end = (current_pos + 1..=text_len)
            .find(|&pos| {
                if pos >= text_len {
                    return true;
                }

                let next_marks = doc.get_marks(text_obj, pos, None).unwrap_or_default();

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

                // Formatting changed?
                next_strong != has_strong
                    || next_em != has_em
                    || next_strikethrough != has_strikethrough
                    || next_code != has_code
            })
            .unwrap_or(text_len);

        // Extract the text for this span (character indices -> byte indices)
        let span_text = text
            .chars()
            .skip(current_pos)
            .take(span_end - current_pos)
            .collect::<String>();

        // Build TextFormat for this span
        let mut format = TextFormat {
            font_id: if has_code {
                font_config.code.clone()
            } else if has_strong {
                font_config.bold.clone()
            } else {
                font_config.regular.clone()
            },
            color: font_config.text_color,
            italics: has_em,
            ..Default::default()
        };

        if has_strikethrough {
            format.strikethrough = Stroke::new(1.0, font_config.text_color);
        }

        if has_code {
            format.background = font_config.code_background;
        }

        // Append this span to the job
        job.append(&span_text, 0.0, format);

        current_pos = span_end;
    }

    job
}

#[cfg(test)]
mod tests {
    use super::*;
    use automerge::marks::{ExpandMark, Mark};
    use automerge::transaction::Transactable;
    use automerge::ObjType;

    fn create_test_doc() -> (AutoCommit, ExId) {
        let mut doc = AutoCommit::new();
        let text_obj = doc
            .put_object(automerge::ROOT, "text", ObjType::Text)
            .unwrap();
        (doc, text_obj)
    }

    #[test]
    fn test_empty_text() {
        let (doc, text_obj) = create_test_doc();
        let font_config = FontConfig::default();

        let job = build_layout_job(&doc, &text_obj, &font_config);

        assert_eq!(job.text, "");
        assert_eq!(job.sections.len(), 0);
    }

    #[test]
    fn test_plain_text() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();

        let font_config = FontConfig::default();
        let job = build_layout_job(&doc, &text_obj, &font_config);

        assert_eq!(job.text, "Hello world");
        assert_eq!(job.sections.len(), 1);
        assert_eq!(job.sections[0].format.font_id, font_config.regular);
        assert!(!job.sections[0].format.italics);
    }

    #[test]
    fn test_bold_text() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();

        let mark = Mark::new("strong".to_string(), true, 0, 5);
        doc.mark(&text_obj, mark, ExpandMark::Both).unwrap();

        let font_config = FontConfig::default();
        let job = build_layout_job(&doc, &text_obj, &font_config);

        assert_eq!(job.text, "Hello world");
        assert_eq!(job.sections.len(), 2); // "Hello" (bold) + " world" (regular)

        // First section: "Hello" (bold)
        assert_eq!(job.sections[0].format.font_id, font_config.bold);

        // Second section: " world" (regular)
        assert_eq!(job.sections[1].format.font_id, font_config.regular);
    }

    #[test]
    fn test_italic_text() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();

        let mark = Mark::new("em".to_string(), true, 6, 11);
        doc.mark(&text_obj, mark, ExpandMark::Both).unwrap();

        let font_config = FontConfig::default();
        let job = build_layout_job(&doc, &text_obj, &font_config);

        assert_eq!(job.text, "Hello world");
        assert_eq!(job.sections.len(), 2);

        // First section: "Hello " (regular)
        assert!(!job.sections[0].format.italics);

        // Second section: "world" (italic)
        assert!(job.sections[1].format.italics);
    }

    #[test]
    fn test_strikethrough_text() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();

        let mark = Mark::new("strikethrough".to_string(), true, 0, 5);
        doc.mark(&text_obj, mark, ExpandMark::Both).unwrap();

        let font_config = FontConfig::default();
        let job = build_layout_job(&doc, &text_obj, &font_config);

        assert_eq!(job.sections.len(), 2);

        // First section: "Hello" (strikethrough)
        assert!(job.sections[0].format.strikethrough.width > 0.0);

        // Second section: " world" (no strikethrough)
        assert_eq!(job.sections[1].format.strikethrough.width, 0.0);
    }

    #[test]
    fn test_code_text() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();

        let mark = Mark::new("code".to_string(), true, 6, 11);
        doc.mark(&text_obj, mark, ExpandMark::Both).unwrap();

        let font_config = FontConfig::default();
        let job = build_layout_job(&doc, &text_obj, &font_config);

        assert_eq!(job.sections.len(), 2);

        // First section: "Hello " (regular)
        assert_eq!(job.sections[0].format.font_id, font_config.regular);
        assert_eq!(job.sections[0].format.background, Color32::TRANSPARENT);

        // Second section: "world" (code)
        assert_eq!(job.sections[1].format.font_id, font_config.code);
        assert_eq!(job.sections[1].format.background, font_config.code_background);
    }

    #[test]
    fn test_combined_bold_italic() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello").unwrap();

        let strong_mark = Mark::new("strong".to_string(), true, 0, 5);
        doc.mark(&text_obj, strong_mark, ExpandMark::Both).unwrap();

        let em_mark = Mark::new("em".to_string(), true, 0, 5);
        doc.mark(&text_obj, em_mark, ExpandMark::Both).unwrap();

        let font_config = FontConfig::default();
        let job = build_layout_job(&doc, &text_obj, &font_config);

        assert_eq!(job.sections.len(), 1);

        // Should have both bold font and italics flag
        assert_eq!(job.sections[0].format.font_id, font_config.bold);
        assert!(job.sections[0].format.italics);
    }

    #[test]
    fn test_multiple_formatted_spans() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello beautiful world")
            .unwrap();

        // "Hello" is bold
        let mark1 = Mark::new("strong".to_string(), true, 0, 5);
        doc.mark(&text_obj, mark1, ExpandMark::Both).unwrap();

        // "beautiful" is italic
        let mark2 = Mark::new("em".to_string(), true, 6, 15);
        doc.mark(&text_obj, mark2, ExpandMark::Both).unwrap();

        // "world" is code
        let mark3 = Mark::new("code".to_string(), true, 16, 21);
        doc.mark(&text_obj, mark3, ExpandMark::Both).unwrap();

        let font_config = FontConfig::default();
        let job = build_layout_job(&doc, &text_obj, &font_config);

        assert_eq!(job.text, "Hello beautiful world");
        assert_eq!(job.sections.len(), 5); // "Hello", " ", "beautiful", " ", "world"

        // "Hello" (bold)
        assert_eq!(job.sections[0].format.font_id, font_config.bold);
        assert!(!job.sections[0].format.italics);

        // " " (regular)
        assert_eq!(job.sections[1].format.font_id, font_config.regular);

        // "beautiful" (italic)
        assert!(job.sections[2].format.italics);

        // " " (regular)
        assert_eq!(job.sections[3].format.font_id, font_config.regular);

        // "world" (code)
        assert_eq!(job.sections[4].format.font_id, font_config.code);
    }
}
