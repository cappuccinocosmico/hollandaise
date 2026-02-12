use automerge::marks::ExpandMark;
use automerge::transaction::Transactable;
use automerge::{marks::Mark, AutoCommit, ObjId as ExId, ReadDoc};

use crate::backend::{FormatSet, Span, TextBackend};
use crate::format::InlineFormat;

pub struct AutomergeBackend<'a> {
    doc: &'a mut AutoCommit,
    text_obj: ExId,
}

impl<'a> AutomergeBackend<'a> {
    pub fn new(doc: &'a mut AutoCommit, text_obj: ExId) -> Self {
        Self { doc, text_obj }
    }
}

impl TextBackend for AutomergeBackend<'_> {
    type Error = automerge::AutomergeError;

    fn text(&self) -> Result<String, Self::Error> {
        self.doc.text(&self.text_obj)
    }

    fn char_count(&self) -> Result<usize, Self::Error> {
        Ok(self.doc.text(&self.text_obj)?.chars().count())
    }

    fn splice(&mut self, pos: usize, delete: usize, text: &str) -> Result<(), Self::Error> {
        self.doc
            .splice_text(&self.text_obj, pos, delete as isize, text)
    }

    fn mark(
        &mut self,
        format: InlineFormat,
        start: usize,
        end: usize,
    ) -> Result<(), Self::Error> {
        let mark = Mark::new(format.mark_name().to_string(), true, start, end);
        self.doc.mark(&self.text_obj, mark, ExpandMark::Both)?;
        Ok(())
    }

    fn unmark(
        &mut self,
        format: InlineFormat,
        start: usize,
        end: usize,
    ) -> Result<(), Self::Error> {
        self.doc
            .unmark(&self.text_obj, format.mark_name(), start, end, ExpandMark::Both)
    }

    fn formats_at(&self, pos: usize) -> Result<FormatSet, Self::Error> {
        let marks = self.doc.get_marks(&self.text_obj, pos, None)?;
        let mut fs = FormatSet::default();
        for (name, _) in marks.iter() {
            if let Some(fmt) = InlineFormat::from_mark_name(name) {
                fs.set(fmt, true);
            }
        }
        Ok(fs)
    }

    fn spans(&self) -> Result<Vec<Span>, Self::Error> {
        let text = self.doc.text(&self.text_obj)?;
        let len = text.chars().count();

        if len == 0 {
            return Ok(Vec::new());
        }

        let mut result = Vec::new();
        let mut pos = 0;

        while pos < len {
            let current_formats = self.formats_at(pos)?;

            let span_end = (pos + 1..=len)
                .find(|&p| {
                    if p >= len {
                        return true;
                    }
                    self.formats_at(p).unwrap() != current_formats
                })
                .unwrap_or(len);

            let span_text: String = text.chars().skip(pos).take(span_end - pos).collect();

            result.push(Span {
                text: span_text,
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
    use automerge::ObjType;

    fn create_test_backend() -> (AutoCommit, ExId) {
        let mut doc = AutoCommit::new();
        let text_obj = doc
            .put_object(automerge::ROOT, "text", ObjType::Text)
            .unwrap();
        (doc, text_obj)
    }

    #[test]
    fn test_text_and_char_count() {
        let (mut doc, text_obj) = create_test_backend();
        let mut b = AutomergeBackend::new(&mut doc, text_obj);
        b.splice(0, 0, "Hello").unwrap();
        assert_eq!(b.text().unwrap(), "Hello");
        assert_eq!(b.char_count().unwrap(), 5);
    }

    #[test]
    fn test_splice_insert_delete_replace() {
        let (mut doc, text_obj) = create_test_backend();
        let mut b = AutomergeBackend::new(&mut doc, text_obj);

        b.splice(0, 0, "Hello world").unwrap();
        assert_eq!(b.text().unwrap(), "Hello world");

        b.splice(5, 1, "").unwrap();
        assert_eq!(b.text().unwrap(), "Helloworld");

        b.splice(5, 0, " ").unwrap();
        assert_eq!(b.text().unwrap(), "Hello world");

        b.splice(0, 5, "Goodbye").unwrap();
        assert_eq!(b.text().unwrap(), "Goodbye world");
    }

    #[test]
    fn test_mark_and_formats_at() {
        let (mut doc, text_obj) = create_test_backend();
        let mut b = AutomergeBackend::new(&mut doc, text_obj);

        b.splice(0, 0, "Hello world").unwrap();
        b.mark(InlineFormat::Strong, 0, 5).unwrap();

        for pos in 0..5 {
            assert!(b.formats_at(pos).unwrap().has(InlineFormat::Strong));
        }
        for pos in 5..11 {
            assert!(!b.formats_at(pos).unwrap().has(InlineFormat::Strong));
        }
    }

    #[test]
    fn test_unmark() {
        let (mut doc, text_obj) = create_test_backend();
        let mut b = AutomergeBackend::new(&mut doc, text_obj);

        b.splice(0, 0, "Hello world").unwrap();
        b.mark(InlineFormat::Strong, 0, 5).unwrap();
        b.unmark(InlineFormat::Strong, 0, 5).unwrap();

        for pos in 0..5 {
            assert!(!b.formats_at(pos).unwrap().has(InlineFormat::Strong));
        }
    }

    #[test]
    fn test_spans() {
        let (mut doc, text_obj) = create_test_backend();
        let mut b = AutomergeBackend::new(&mut doc, text_obj);

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
    fn test_multiple_formats() {
        let (mut doc, text_obj) = create_test_backend();
        let mut b = AutomergeBackend::new(&mut doc, text_obj);

        b.splice(0, 0, "Hello").unwrap();
        b.mark(InlineFormat::Strong, 0, 5).unwrap();
        b.mark(InlineFormat::Emphasis, 0, 5).unwrap();

        let fs = b.formats_at(0).unwrap();
        assert!(fs.has(InlineFormat::Strong));
        assert!(fs.has(InlineFormat::Emphasis));

        let spans = b.spans().unwrap();
        assert_eq!(spans.len(), 1);
        assert!(spans[0].formats.has(InlineFormat::Strong));
        assert!(spans[0].formats.has(InlineFormat::Emphasis));
    }

    #[test]
    fn test_editor_integration() {
        use crate::editor::Editor;

        let (mut doc, text_obj) = create_test_backend();
        let b = AutomergeBackend::new(&mut doc, text_obj);
        let mut e = Editor::new(b);

        e.insert_text(0, "Hello world").unwrap();
        e.toggle_format(InlineFormat::Strong, 0, 5).unwrap();
        assert_eq!(e.to_markdown().unwrap(), "**Hello** world");

        // Toggle off
        e.toggle_format(InlineFormat::Strong, 0, 5).unwrap();
        assert_eq!(e.to_markdown().unwrap(), "Hello world");
    }

    #[test]
    fn test_markdown_round_trip() {
        use crate::editor::Editor;

        let (mut doc, text_obj) = create_test_backend();
        let b = AutomergeBackend::new(&mut doc, text_obj);
        let mut e = Editor::new(b);

        let original = "**bold** *italic* ~~strike~~ `code`";
        e.from_markdown(original).unwrap();
        assert_eq!(e.to_markdown().unwrap(), original);
    }
}
