//! TextBuffer implementation bridging egui and Automerge

use automerge::transaction::Transactable;
use automerge::{AutoCommit, ObjId as ExId, ReadDoc};
use egui::TextBuffer;
use std::ops::Range;

/// Text buffer that bridges egui's TextEdit with Automerge's CRDT
///
/// This struct implements egui's `TextBuffer` trait and forwards all mutations
/// to the Automerge document via `splice_text`. The Automerge document is the
/// single source of truth - this buffer maintains a cached copy for efficient
/// read access via `as_str()`.
pub struct RichTextBuffer<'a> {
    /// The Automerge document (mutable borrow)
    doc: &'a mut AutoCommit,
    /// The ExId of the text object within the document
    text_obj: ExId,
    /// Cached plain text for `as_str()` - mirrors `doc.text(text_obj)`
    plain_text: String,
}

impl<'a> RichTextBuffer<'a> {
    /// Create a new RichTextBuffer
    ///
    /// # Arguments
    /// * `doc` - Mutable reference to the Automerge document
    /// * `text_obj` - The ExId of the text object to edit
    ///
    /// # Returns
    /// A new buffer with the current text cached
    pub fn new(doc: &'a mut AutoCommit, text_obj: ExId) -> Self {
        let plain_text = doc
            .text(&text_obj)
            .expect("Failed to read text from Automerge");

        Self {
            doc,
            text_obj,
            plain_text,
        }
    }

    /// Assert that the cached plain text matches the Automerge document
    ///
    /// This is called after every mutation as a correctness check.
    #[inline]
    fn assert_cache_consistent(&self) {
        let doc_text = self
            .doc
            .text(&self.text_obj)
            .expect("Failed to read text from Automerge");

        if self.plain_text != doc_text {
            panic!(
                "RichTextBuffer cache inconsistent!\nCached: {:?}\nDoc: {:?}",
                self.plain_text, doc_text
            );
        }
    }
}

impl TextBuffer for RichTextBuffer<'_> {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        &self.plain_text
    }

    fn type_id(&self) -> std::any::TypeId {
        // RichTextBuffer is not 'static due to the lifetime parameter,
        // but we can still provide a unique TypeId for the concrete type
        std::any::TypeId::of::<RichTextBuffer<'static>>()
    }

    fn insert_text(&mut self, text: &str, char_index: usize) -> usize {
        if text.is_empty() {
            return 0;
        }

        let text_len = self.plain_text.chars().count();
        if char_index > text_len {
            panic!(
                "insert_text char_index out of bounds: index={}, len={}",
                char_index, text_len
            );
        }

        // Perform the insert in Automerge
        self.doc
            .splice_text(&self.text_obj, char_index, 0, text)
            .expect("Failed to splice text in Automerge");

        // Mirror the insert in the cached string
        let byte_index = self
            .plain_text
            .char_indices()
            .nth(char_index)
            .map(|(i, _)| i)
            .unwrap_or(self.plain_text.len());

        self.plain_text.insert_str(byte_index, text);

        // Verify consistency
        self.assert_cache_consistent();

        text.chars().count()
    }

    fn delete_char_range(&mut self, char_range: Range<usize>) {
        if char_range.start >= char_range.end {
            return; // Empty range, nothing to delete
        }

        let text_len = self.plain_text.chars().count();
        if char_range.end > text_len {
            panic!(
                "delete_char_range out of bounds: range={:?}, len={}",
                char_range, text_len
            );
        }

        let delete_len = char_range.end - char_range.start;

        // Perform the delete in Automerge
        self.doc
            .splice_text(&self.text_obj, char_range.start, delete_len as isize, "")
            .expect("Failed to splice text in Automerge");

        // Mirror the delete in the cached string
        let start_byte = self
            .plain_text
            .char_indices()
            .nth(char_range.start)
            .map(|(i, _)| i)
            .unwrap_or(self.plain_text.len());

        let end_byte = self
            .plain_text
            .char_indices()
            .nth(char_range.end)
            .map(|(i, _)| i)
            .unwrap_or(self.plain_text.len());

        self.plain_text.drain(start_byte..end_byte);

        // Verify consistency
        self.assert_cache_consistent();
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
    fn test_new_buffer_empty() {
        let (mut doc, text_obj) = create_test_doc();
        let buffer = RichTextBuffer::new(&mut doc, text_obj.clone());

        assert_eq!(buffer.as_str(), "");
        assert!(buffer.is_mutable());
    }

    #[test]
    fn test_new_buffer_with_existing_text() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();

        let buffer = RichTextBuffer::new(&mut doc, text_obj.clone());

        assert_eq!(buffer.as_str(), "Hello world");
    }

    #[test]
    fn test_insert_text_at_start() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "world").unwrap();

        let mut buffer = RichTextBuffer::new(&mut doc, text_obj.clone());
        let inserted = buffer.insert_text("Hello ", 0);

        assert_eq!(inserted, 6);
        assert_eq!(buffer.as_str(), "Hello world");
        assert_eq!(doc.text(&text_obj).unwrap(), "Hello world");
    }

    #[test]
    fn test_insert_text_at_end() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello").unwrap();

        let mut buffer = RichTextBuffer::new(&mut doc, text_obj.clone());
        let inserted = buffer.insert_text(" world", 5);

        assert_eq!(inserted, 6);
        assert_eq!(buffer.as_str(), "Hello world");
        assert_eq!(doc.text(&text_obj).unwrap(), "Hello world");
    }

    #[test]
    fn test_insert_text_in_middle() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Helloworld").unwrap();

        let mut buffer = RichTextBuffer::new(&mut doc, text_obj.clone());
        let inserted = buffer.insert_text(" ", 5);

        assert_eq!(inserted, 1);
        assert_eq!(buffer.as_str(), "Hello world");
        assert_eq!(doc.text(&text_obj).unwrap(), "Hello world");
    }

    #[test]
    fn test_insert_empty_string() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello").unwrap();

        let mut buffer = RichTextBuffer::new(&mut doc, text_obj.clone());
        let inserted = buffer.insert_text("", 5);

        assert_eq!(inserted, 0);
        assert_eq!(buffer.as_str(), "Hello");
    }

    #[test]
    fn test_insert_unicode() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello").unwrap();

        let mut buffer = RichTextBuffer::new(&mut doc, text_obj.clone());
        let inserted = buffer.insert_text(" 世界", 5);

        assert_eq!(inserted, 3); // " " + "世" + "界" = 3 characters
        assert_eq!(buffer.as_str(), "Hello 世界");
        assert_eq!(doc.text(&text_obj).unwrap(), "Hello 世界");
    }

    #[test]
    fn test_delete_char_range_entire_text() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();

        let mut buffer = RichTextBuffer::new(&mut doc, text_obj.clone());
        buffer.delete_char_range(0..11);

        assert_eq!(buffer.as_str(), "");
        assert_eq!(doc.text(&text_obj).unwrap(), "");
    }

    #[test]
    fn test_delete_char_range_from_start() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();

        let mut buffer = RichTextBuffer::new(&mut doc, text_obj.clone());
        buffer.delete_char_range(0..6);

        assert_eq!(buffer.as_str(), "world");
        assert_eq!(doc.text(&text_obj).unwrap(), "world");
    }

    #[test]
    fn test_delete_char_range_from_end() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();

        let mut buffer = RichTextBuffer::new(&mut doc, text_obj.clone());
        buffer.delete_char_range(5..11);

        assert_eq!(buffer.as_str(), "Hello");
        assert_eq!(doc.text(&text_obj).unwrap(), "Hello");
    }

    #[test]
    fn test_delete_char_range_from_middle() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello world").unwrap();

        let mut buffer = RichTextBuffer::new(&mut doc, text_obj.clone());
        buffer.delete_char_range(5..6);

        assert_eq!(buffer.as_str(), "Helloworld");
        assert_eq!(doc.text(&text_obj).unwrap(), "Helloworld");
    }

    #[test]
    fn test_delete_char_range_empty() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello").unwrap();

        let mut buffer = RichTextBuffer::new(&mut doc, text_obj.clone());
        buffer.delete_char_range(2..2);

        assert_eq!(buffer.as_str(), "Hello");
    }

    #[test]
    fn test_delete_char_range_unicode() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello 世界").unwrap();

        let mut buffer = RichTextBuffer::new(&mut doc, text_obj.clone());
        buffer.delete_char_range(6..8); // Delete "世界"

        assert_eq!(buffer.as_str(), "Hello ");
        assert_eq!(doc.text(&text_obj).unwrap(), "Hello ");
    }

    #[test]
    fn test_multiple_operations() {
        let (mut doc, text_obj) = create_test_doc();
        let mut buffer = RichTextBuffer::new(&mut doc, text_obj.clone());

        buffer.insert_text("Hello", 0);
        assert_eq!(buffer.as_str(), "Hello");

        buffer.insert_text(" world", 5);
        assert_eq!(buffer.as_str(), "Hello world");

        buffer.delete_char_range(5..6);
        assert_eq!(buffer.as_str(), "Helloworld");

        buffer.insert_text(" ", 5);
        assert_eq!(buffer.as_str(), "Hello world");

        buffer.delete_char_range(0..6);
        assert_eq!(buffer.as_str(), "world");

        // Final consistency check
        assert_eq!(doc.text(&text_obj).unwrap(), "world");
    }

    #[test]
    #[should_panic(expected = "insert_text char_index out of bounds")]
    fn test_insert_text_out_of_bounds() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello").unwrap();

        let mut buffer = RichTextBuffer::new(&mut doc, text_obj.clone());
        buffer.insert_text("x", 100);
    }

    #[test]
    #[should_panic(expected = "delete_char_range out of bounds")]
    fn test_delete_char_range_out_of_bounds() {
        let (mut doc, text_obj) = create_test_doc();
        doc.splice_text(&text_obj, 0, 0, "Hello").unwrap();

        let mut buffer = RichTextBuffer::new(&mut doc, text_obj.clone());
        buffer.delete_char_range(0..100);
    }
}
