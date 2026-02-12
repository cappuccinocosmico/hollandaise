use hollandaise::{Editor, InlineFormat, LocalBackend, TextBackend};

fn editor_with_text(text: &str) -> Editor<LocalBackend> {
    let mut b = LocalBackend::new();
    b.splice(0, 0, text).unwrap();
    Editor::new(b)
}

#[test]
fn test_basic_editing_workflow() {
    let mut e = Editor::new(LocalBackend::new());

    e.insert_text(0, "Hello world").unwrap();
    assert_eq!(e.text().unwrap(), "Hello world");

    e.toggle_format(InlineFormat::Strong, 0, 5).unwrap();
    assert!(e.is_range_formatted(InlineFormat::Strong, 0, 5).unwrap());

    assert_eq!(e.to_markdown().unwrap(), "**Hello** world");
}

#[test]
fn test_markdown_round_trip_workflow() {
    let mut e = Editor::new(LocalBackend::new());
    let original = "**bold** *italic* ~~strike~~ `code`";

    e.from_markdown(original).unwrap();
    assert_eq!(e.text().unwrap(), "bold italic strike code");

    assert!(e.is_range_formatted(InlineFormat::Strong, 0, 4).unwrap());
    assert!(e.is_range_formatted(InlineFormat::Emphasis, 5, 11).unwrap());

    assert_eq!(e.to_markdown().unwrap(), original);
}

#[test]
fn test_toggle_formatting_workflow() {
    let mut e = editor_with_text("Hello world");

    e.toggle_format(InlineFormat::Strong, 0, 5).unwrap();
    assert_eq!(e.to_markdown().unwrap(), "**Hello** world");

    e.toggle_format(InlineFormat::Strong, 0, 5).unwrap();
    assert_eq!(e.to_markdown().unwrap(), "Hello world");

    e.toggle_format(InlineFormat::Strong, 0, 5).unwrap();
    e.toggle_format(InlineFormat::Emphasis, 0, 5).unwrap();
    assert_eq!(e.to_markdown().unwrap(), "***Hello*** world");
}

#[test]
fn test_partial_selection_formatting() {
    let mut e = editor_with_text("Hello beautiful world");

    e.toggle_format(InlineFormat::Strong, 0, 5).unwrap();
    e.toggle_format(InlineFormat::Emphasis, 6, 15).unwrap();
    e.toggle_format(InlineFormat::Code, 16, 21).unwrap();

    let md = e.to_markdown().unwrap();
    assert_eq!(md, "**Hello** *beautiful* `world`");

    // Round-trip
    let mut e2 = Editor::new(LocalBackend::new());
    e2.from_markdown(&md).unwrap();
    assert_eq!(e2.to_markdown().unwrap(), md);
}

#[test]
fn test_editing_with_existing_formatting() {
    let mut e = Editor::new(LocalBackend::new());
    e.from_markdown("**Hello** world").unwrap();

    // Insert inside formatted region
    e.insert_text(2, "XX").unwrap();
    assert_eq!(e.text().unwrap(), "HeXXllo world");

    // XX should inherit the bold mark (LocalBackend extends marks on insert inside)
    assert!(e.formats_at(2).unwrap().has(InlineFormat::Strong));
    assert!(e.formats_at(3).unwrap().has(InlineFormat::Strong));
}

#[test]
fn test_delete_within_formatted_text() {
    let mut e = Editor::new(LocalBackend::new());
    e.from_markdown("**Hello** world").unwrap();

    e.delete_range(1, 3).unwrap();
    assert_eq!(e.text().unwrap(), "Hlo world");
    assert_eq!(e.to_markdown().unwrap(), "**Hlo** world");
}

#[test]
fn test_complex_formatting_combinations() {
    let mut e = editor_with_text("Test");

    e.toggle_format(InlineFormat::Strong, 0, 4).unwrap();
    e.toggle_format(InlineFormat::Emphasis, 0, 4).unwrap();
    e.toggle_format(InlineFormat::Strikethrough, 0, 4).unwrap();
    e.toggle_format(InlineFormat::Code, 0, 4).unwrap();

    for pos in 0..4 {
        let fs = e.formats_at(pos).unwrap();
        assert!(fs.has(InlineFormat::Strong));
        assert!(fs.has(InlineFormat::Emphasis));
        assert!(fs.has(InlineFormat::Strikethrough));
        assert!(fs.has(InlineFormat::Code));
    }

    assert_eq!(e.to_markdown().unwrap(), "***~~`Test`~~***");
}

#[test]
fn test_empty_document() {
    let e = Editor::new(LocalBackend::new());
    assert_eq!(e.text().unwrap(), "");
    assert_eq!(e.to_markdown().unwrap(), "");
    assert_eq!(e.spans().unwrap(), vec![]);
}

#[test]
fn test_markdown_import_clears_previous_content() {
    let mut e = Editor::new(LocalBackend::new());

    e.from_markdown("**Old content**").unwrap();
    assert_eq!(e.text().unwrap(), "Old content");

    e.from_markdown("*New content*").unwrap();
    assert_eq!(e.text().unwrap(), "New content");

    assert!(e.formats_at(0).unwrap().has(InlineFormat::Emphasis));
    assert!(!e.formats_at(0).unwrap().has(InlineFormat::Strong));
}

#[test]
fn test_spans_provide_rendering_data() {
    let mut e = editor_with_text("Hello beautiful world");
    e.toggle_format(InlineFormat::Strong, 0, 5).unwrap();
    e.toggle_format(InlineFormat::Emphasis, 6, 15).unwrap();

    let spans = e.spans().unwrap();

    // Consumer can iterate spans for rendering
    assert_eq!(spans.len(), 4);
    assert_eq!(spans[0].text, "Hello");
    assert!(spans[0].formats.strong);
    assert_eq!(spans[1].text, " ");
    assert!(spans[1].formats.is_empty());
    assert_eq!(spans[2].text, "beautiful");
    assert!(spans[2].formats.emphasis);
    assert_eq!(spans[3].text, " world");
    assert!(spans[3].formats.is_empty());
}

#[test]
fn test_backend_access() {
    let mut e = editor_with_text("Hello");
    assert_eq!(e.backend().char_count().unwrap(), 5);
    e.backend_mut().splice(5, 0, "!").unwrap();
    assert_eq!(e.text().unwrap(), "Hello!");
}
