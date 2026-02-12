use crate::format::InlineFormat;

pub trait TextBackend {
    type Error: std::fmt::Debug;

    fn text(&self) -> Result<String, Self::Error>;
    fn char_count(&self) -> Result<usize, Self::Error>;

    /// Delete `delete` chars at `pos`, then insert `text`.
    fn splice(&mut self, pos: usize, delete: usize, text: &str) -> Result<(), Self::Error>;

    /// Apply format to [start, end) half-open range.
    fn mark(&mut self, format: InlineFormat, start: usize, end: usize)
        -> Result<(), Self::Error>;

    /// Remove format from [start, end) half-open range.
    fn unmark(
        &mut self,
        format: InlineFormat,
        start: usize,
        end: usize,
    ) -> Result<(), Self::Error>;

    /// Active formats at a character position.
    fn formats_at(&self, pos: usize) -> Result<FormatSet, Self::Error>;

    /// Contiguous spans with uniform formatting (for rendering and markdown export).
    fn spans(&self) -> Result<Vec<Span>, Self::Error>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FormatSet {
    pub strong: bool,
    pub emphasis: bool,
    pub strikethrough: bool,
    pub code: bool,
}

impl FormatSet {
    pub fn has(&self, format: InlineFormat) -> bool {
        match format {
            InlineFormat::Strong => self.strong,
            InlineFormat::Emphasis => self.emphasis,
            InlineFormat::Strikethrough => self.strikethrough,
            InlineFormat::Code => self.code,
        }
    }

    pub fn set(&mut self, format: InlineFormat, value: bool) {
        match format {
            InlineFormat::Strong => self.strong = value,
            InlineFormat::Emphasis => self.emphasis = value,
            InlineFormat::Strikethrough => self.strikethrough = value,
            InlineFormat::Code => self.code = value,
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.strong && !self.emphasis && !self.strikethrough && !self.code
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub formats: FormatSet,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_set_default_is_empty() {
        let fs = FormatSet::default();
        assert!(fs.is_empty());
        for format in InlineFormat::all() {
            assert!(!fs.has(*format));
        }
    }

    #[test]
    fn test_format_set_has_and_set() {
        let mut fs = FormatSet::default();
        for format in InlineFormat::all() {
            assert!(!fs.has(*format));
            fs.set(*format, true);
            assert!(fs.has(*format));
            assert!(!fs.is_empty());
            fs.set(*format, false);
            assert!(!fs.has(*format));
        }
    }

    #[test]
    fn test_format_set_independent_fields() {
        let mut fs = FormatSet::default();
        fs.set(InlineFormat::Strong, true);
        fs.set(InlineFormat::Code, true);

        assert!(fs.has(InlineFormat::Strong));
        assert!(!fs.has(InlineFormat::Emphasis));
        assert!(!fs.has(InlineFormat::Strikethrough));
        assert!(fs.has(InlineFormat::Code));
    }
}
