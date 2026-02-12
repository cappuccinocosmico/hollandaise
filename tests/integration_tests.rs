//! Integration tests for Hollandaise
//!
//! These tests verify end-to-end behavior across multiple modules.

use automerge::transaction::Transactable;
use automerge::{AutoCommit, ObjType, ReadDoc, ROOT};
use hollandaise::{apply_markdown, spans_to_markdown, toggle_format, InlineFormat};

fn create_test_doc() -> (AutoCommit, automerge::ObjId) {
    let mut doc = AutoCommit::new();
    let text_obj = doc
        .put_object(ROOT, "text", ObjType::Text)
        .expect("Failed to create text object");
    (doc, text_obj)
}

#[test]
fn test_basic_editing_workflow() {
    let (mut doc, text_obj) = create_test_doc();

    // 1. Insert some text
    doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();
    assert_eq!(doc.text(&text_obj).unwrap(), "Hello world");

    // 2. Apply formatting
    toggle_format(&mut doc, &text_obj, InlineFormat::Strong, 0, 5);

    // 3. Verify the formatting
    for pos in 0..5 {
        let marks = doc.get_marks(&text_obj, pos, None).unwrap();
        assert!(marks.iter().any(|(name, _)| name == "strong"));
    }

    // 4. Export to markdown
    let markdown = spans_to_markdown(&doc, &text_obj);
    assert_eq!(markdown, "**Hello** world");
}

#[test]
fn test_markdown_round_trip_workflow() {
    let (mut doc, text_obj) = create_test_doc();

    // 1. Start with markdown
    let original_markdown = "**bold** *italic* ~~strike~~ `code`";
    apply_markdown(&mut doc, &text_obj, original_markdown);

    // 2. Verify text was inserted
    let text = doc.text(&text_obj).unwrap();
    assert_eq!(text, "bold italic strike code");

    // 3. Verify formatting
    let marks_at_0 = doc.get_marks(&text_obj, 0, None).unwrap();
    assert!(marks_at_0.iter().any(|(name, _)| name == "strong"));

    let marks_at_5 = doc.get_marks(&text_obj, 5, None).unwrap();
    assert!(marks_at_5.iter().any(|(name, _)| name == "em"));

    // 4. Round-trip back to markdown
    let exported_markdown = spans_to_markdown(&doc, &text_obj);
    assert_eq!(exported_markdown, original_markdown);
}

#[test]
fn test_toggle_formatting_workflow() {
    let (mut doc, text_obj) = create_test_doc();

    // 1. Insert text
    doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();

    // 2. Apply bold
    toggle_format(&mut doc, &text_obj, InlineFormat::Strong, 0, 5);
    let markdown = spans_to_markdown(&doc, &text_obj);
    assert_eq!(markdown, "**Hello** world");

    // 3. Toggle bold off
    toggle_format(&mut doc, &text_obj, InlineFormat::Strong, 0, 5);
    let markdown = spans_to_markdown(&doc, &text_obj);
    assert_eq!(markdown, "Hello world");

    // 4. Apply multiple formats
    toggle_format(&mut doc, &text_obj, InlineFormat::Strong, 0, 5);
    toggle_format(&mut doc, &text_obj, InlineFormat::Emphasis, 0, 5);
    let markdown = spans_to_markdown(&doc, &text_obj);
    assert_eq!(markdown, "***Hello*** world");
}

#[test]
fn test_partial_selection_formatting() {
    let (mut doc, text_obj) = create_test_doc();

    // 1. Insert text
    doc.splice_text(&text_obj, 0, 0, "Hello beautiful world").unwrap();

    // 2. Format different parts
    toggle_format(&mut doc, &text_obj, InlineFormat::Strong, 0, 5); // "Hello"
    toggle_format(&mut doc, &text_obj, InlineFormat::Emphasis, 6, 15); // "beautiful"
    toggle_format(&mut doc, &text_obj, InlineFormat::Code, 16, 21); // "world"

    // 3. Verify markdown
    let markdown = spans_to_markdown(&doc, &text_obj);
    assert_eq!(markdown, "**Hello** *beautiful* `world`");

    // 4. Round-trip
    let (mut doc2, text_obj2) = create_test_doc();
    apply_markdown(&mut doc2, &text_obj2, &markdown);
    let markdown2 = spans_to_markdown(&doc2, &text_obj2);
    assert_eq!(markdown2, markdown);
}

#[test]
fn test_editing_with_existing_formatting() {
    let (mut doc, text_obj) = create_test_doc();

    // 1. Start with formatted text
    apply_markdown(&mut doc, &text_obj, "**Hello** world");

    // 2. Insert text in the middle of formatted region
    doc.splice_text(&text_obj, 2, 0, "XX").unwrap();

    // 3. The inserted text should inherit the bold mark (due to ExpandMark::Both)
    let text = doc.text(&text_obj).unwrap();
    assert_eq!(text, "HeXXllo world");

    // Check that XX is bold (positions 2 and 3)
    let marks_at_2 = doc.get_marks(&text_obj, 2, None).unwrap();
    assert!(marks_at_2.iter().any(|(name, _)| name == "strong"));

    let marks_at_3 = doc.get_marks(&text_obj, 3, None).unwrap();
    assert!(marks_at_3.iter().any(|(name, _)| name == "strong"));
}

#[test]
fn test_delete_within_formatted_text() {
    let (mut doc, text_obj) = create_test_doc();

    // 1. Start with formatted text
    apply_markdown(&mut doc, &text_obj, "**Hello** world");

    // 2. Delete part of the formatted text
    doc.splice_text(&text_obj, 1, 2, "").unwrap(); // Delete "el"

    // 3. Verify text
    let text = doc.text(&text_obj).unwrap();
    assert_eq!(text, "Hlo world");

    // 4. Remaining "Hlo" should still be bold
    let markdown = spans_to_markdown(&doc, &text_obj);
    assert_eq!(markdown, "**Hlo** world");
}

#[test]
fn test_complex_formatting_combinations() {
    let (mut doc, text_obj) = create_test_doc();

    // Test all four formats on the same text
    doc.splice_text(&text_obj, 0, 0, "Test").unwrap();

    toggle_format(&mut doc, &text_obj, InlineFormat::Strong, 0, 4);
    toggle_format(&mut doc, &text_obj, InlineFormat::Emphasis, 0, 4);
    toggle_format(&mut doc, &text_obj, InlineFormat::Strikethrough, 0, 4);
    toggle_format(&mut doc, &text_obj, InlineFormat::Code, 0, 4);

    // All four marks should be present
    for pos in 0..4 {
        let marks = doc.get_marks(&text_obj, pos, None).unwrap();
        assert!(marks.iter().any(|(name, _)| name == "strong"));
        assert!(marks.iter().any(|(name, _)| name == "em"));
        assert!(marks.iter().any(|(name, _)| name == "strikethrough"));
        assert!(marks.iter().any(|(name, _)| name == "code"));
    }

    // Markdown should show nested formatting
    let markdown = spans_to_markdown(&doc, &text_obj);
    assert_eq!(markdown, "***~~`Test`~~***");
}

#[test]
fn test_empty_document() {
    let (doc, text_obj) = create_test_doc();

    // Empty document
    assert_eq!(doc.text(&text_obj).unwrap(), "");

    // Export to markdown should be empty
    let markdown = spans_to_markdown(&doc, &text_obj);
    assert_eq!(markdown, "");
}

#[test]
fn test_markdown_with_special_characters() {
    let (mut doc, text_obj) = create_test_doc();

    // Test text with characters that might need escaping
    let text = "Test with * and _ and ~ chars";
    doc.splice_text(&text_obj, 0, 0, text).unwrap();

    // Apply formatting to part of it
    toggle_format(&mut doc, &text_obj, InlineFormat::Strong, 0, 4);

    let markdown = spans_to_markdown(&doc, &text_obj);
    assert_eq!(markdown, "**Test** with * and _ and ~ chars");

    // Round-trip should preserve the text
    let (mut doc2, text_obj2) = create_test_doc();
    apply_markdown(&mut doc2, &text_obj2, &markdown);
    assert_eq!(doc2.text(&text_obj2).unwrap(), text);
}

#[test]
fn test_markdown_import_clears_previous_content() {
    let (mut doc, text_obj) = create_test_doc();

    // 1. Add some initial content
    apply_markdown(&mut doc, &text_obj, "**Old content**");
    assert_eq!(doc.text(&text_obj).unwrap(), "Old content");

    // 2. Import new markdown (should clear old content)
    apply_markdown(&mut doc, &text_obj, "*New content*");
    assert_eq!(doc.text(&text_obj).unwrap(), "New content");

    // 3. Verify only new formatting exists
    let marks_at_0 = doc.get_marks(&text_obj, 0, None).unwrap();
    assert!(marks_at_0.iter().any(|(name, _)| name == "em"));
    assert!(!marks_at_0.iter().any(|(name, _)| name == "strong"));
}
