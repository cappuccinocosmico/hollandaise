use crate::backend::{FormatSet, Span, TextBackend};
use crate::format::InlineFormat;
use crate::markdown;

pub struct Editor<B: TextBackend> {
    backend: B,
}

impl<B: TextBackend> Editor<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    pub fn text(&self) -> Result<String, B::Error> {
        self.backend.text()
    }

    pub fn spans(&self) -> Result<Vec<Span>, B::Error> {
        self.backend.spans()
    }

    pub fn insert_text(&mut self, pos: usize, text: &str) -> Result<(), B::Error> {
        if text.is_empty() {
            return Ok(());
        }
        self.backend.splice(pos, 0, text)
    }

    pub fn delete_range(&mut self, start: usize, end: usize) -> Result<(), B::Error> {
        if start >= end {
            panic!(
                "delete_range requires start < end: start={}, end={}",
                start, end
            );
        }
        self.backend.splice(start, end - start, "")
    }

    /// Returns true if every character in [start, end) has the given format.
    pub fn is_range_formatted(
        &self,
        format: InlineFormat,
        start: usize,
        end: usize,
    ) -> Result<bool, B::Error> {
        if start >= end {
            panic!(
                "is_range_formatted requires start < end: start={}, end={}",
                start, end
            );
        }
        for pos in start..end {
            let fs = self.backend.formats_at(pos)?;
            if !fs.has(format) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Toggle format: unmark if the entire range already has it, otherwise mark.
    pub fn toggle_format(
        &mut self,
        format: InlineFormat,
        start: usize,
        end: usize,
    ) -> Result<(), B::Error> {
        if start >= end {
            panic!(
                "toggle_format requires start < end: start={}, end={}",
                start, end
            );
        }
        let fully_formatted = self.is_range_formatted(format, start, end)?;
        if fully_formatted {
            self.backend.unmark(format, start, end)
        } else {
            self.backend.mark(format, start, end)
        }
    }

    pub fn formats_at(&self, pos: usize) -> Result<FormatSet, B::Error> {
        self.backend.formats_at(pos)
    }

    pub fn to_markdown(&self) -> Result<String, B::Error> {
        markdown::spans_to_markdown(&self.backend)
    }

    pub fn from_markdown(&mut self, markdown: &str) -> Result<(), B::Error> {
        markdown::apply_markdown(&mut self.backend, markdown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::LocalBackend;

    fn editor_with_text(text: &str) -> Editor<LocalBackend> {
        let mut b = LocalBackend::new();
        b.splice(0, 0, text).unwrap();
        Editor::new(b)
    }

    #[test]
    fn test_insert_and_read() {
        let mut e = Editor::new(LocalBackend::new());
        e.insert_text(0, "Hello").unwrap();
        assert_eq!(e.text().unwrap(), "Hello");

        e.insert_text(5, " world").unwrap();
        assert_eq!(e.text().unwrap(), "Hello world");
    }

    #[test]
    fn test_delete_range() {
        let mut e = editor_with_text("Hello world");
        e.delete_range(5, 11).unwrap();
        assert_eq!(e.text().unwrap(), "Hello");
    }

    #[test]
    #[should_panic(expected = "delete_range requires start < end")]
    fn test_delete_range_empty() {
        let mut e = editor_with_text("Hello");
        e.delete_range(2, 2).unwrap();
    }

    #[test]
    fn test_toggle_format_apply() {
        let mut e = editor_with_text("Hello world");
        e.toggle_format(InlineFormat::Strong, 0, 5).unwrap();

        assert!(e.is_range_formatted(InlineFormat::Strong, 0, 5).unwrap());
        assert!(!e.is_range_formatted(InlineFormat::Strong, 5, 11).unwrap());
    }

    #[test]
    fn test_toggle_format_remove() {
        let mut e = editor_with_text("Hello world");
        e.toggle_format(InlineFormat::Strong, 0, 5).unwrap();
        e.toggle_format(InlineFormat::Strong, 0, 5).unwrap();

        assert!(!e.is_range_formatted(InlineFormat::Strong, 0, 5).unwrap());
    }

    #[test]
    fn test_toggle_format_partial_applies() {
        let mut e = editor_with_text("Hello world");
        e.toggle_format(InlineFormat::Strong, 0, 5).unwrap();
        // Range [0,11) is partially formatted — toggle should apply to all
        e.toggle_format(InlineFormat::Strong, 0, 11).unwrap();

        assert!(e.is_range_formatted(InlineFormat::Strong, 0, 11).unwrap());
    }

    #[test]
    #[should_panic(expected = "toggle_format requires start < end")]
    fn test_toggle_format_empty_range() {
        let mut e = editor_with_text("Hello");
        e.toggle_format(InlineFormat::Strong, 2, 2).unwrap();
    }

    #[test]
    fn test_to_markdown() {
        let mut e = editor_with_text("Hello world");
        e.toggle_format(InlineFormat::Strong, 0, 5).unwrap();
        assert_eq!(e.to_markdown().unwrap(), "**Hello** world");
    }

    #[test]
    fn test_from_markdown() {
        let mut e = Editor::new(LocalBackend::new());
        e.from_markdown("**Hello** world").unwrap();
        assert_eq!(e.text().unwrap(), "Hello world");
        assert!(e.is_range_formatted(InlineFormat::Strong, 0, 5).unwrap());
    }

    #[test]
    fn test_from_markdown_clears_existing() {
        let mut e = editor_with_text("old text");
        e.from_markdown("**new**").unwrap();
        assert_eq!(e.text().unwrap(), "new");
        assert!(e.is_range_formatted(InlineFormat::Strong, 0, 3).unwrap());
    }

    #[test]
    fn test_spans() {
        let mut e = editor_with_text("Hello world");
        e.toggle_format(InlineFormat::Strong, 0, 5).unwrap();

        let spans = e.spans().unwrap();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].text, "Hello");
        assert!(spans[0].formats.has(InlineFormat::Strong));
        assert_eq!(spans[1].text, " world");
        assert!(spans[1].formats.is_empty());
    }

    #[test]
    fn test_insert_empty_is_noop() {
        let mut e = editor_with_text("Hello");
        e.insert_text(0, "").unwrap();
        assert_eq!(e.text().unwrap(), "Hello");
    }

    #[test]
    fn test_backend_access() {
        let e = editor_with_text("Hello");
        assert_eq!(e.backend().char_count().unwrap(), 5);
    }

    #[test]
    fn test_full_workflow() {
        let mut e = Editor::new(LocalBackend::new());

        // Type some text
        e.insert_text(0, "Hello beautiful world").unwrap();

        // Format different parts
        e.toggle_format(InlineFormat::Strong, 0, 5).unwrap();
        e.toggle_format(InlineFormat::Emphasis, 6, 15).unwrap();
        e.toggle_format(InlineFormat::Code, 16, 21).unwrap();

        // Export to markdown
        let md = e.to_markdown().unwrap();
        assert_eq!(md, "**Hello** *beautiful* `world`");

        // Round-trip
        let mut e2 = Editor::new(LocalBackend::new());
        e2.from_markdown(&md).unwrap();
        assert_eq!(e2.to_markdown().unwrap(), md);
    }
}
