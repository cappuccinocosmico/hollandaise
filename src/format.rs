#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InlineFormat {
    Strong,
    Emphasis,
    Strikethrough,
    Code,
}

impl InlineFormat {
    pub fn mark_name(&self) -> &'static str {
        match self {
            InlineFormat::Strong => "strong",
            InlineFormat::Emphasis => "em",
            InlineFormat::Strikethrough => "strikethrough",
            InlineFormat::Code => "code",
        }
    }

    pub fn from_mark_name(name: &str) -> Option<Self> {
        match name {
            "strong" => Some(InlineFormat::Strong),
            "em" => Some(InlineFormat::Emphasis),
            "strikethrough" => Some(InlineFormat::Strikethrough),
            "code" => Some(InlineFormat::Code),
            _ => None,
        }
    }

    pub fn all() -> &'static [InlineFormat] {
        &[
            InlineFormat::Strong,
            InlineFormat::Emphasis,
            InlineFormat::Strikethrough,
            InlineFormat::Code,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mark_name_round_trip() {
        for format in InlineFormat::all() {
            let name = format.mark_name();
            let recovered = InlineFormat::from_mark_name(name)
                .expect("from_mark_name must recognize all mark_name outputs");
            assert_eq!(*format, recovered);
        }
    }

    #[test]
    fn test_from_mark_name_unknown_returns_none() {
        assert_eq!(InlineFormat::from_mark_name("unknown"), None);
        assert_eq!(InlineFormat::from_mark_name(""), None);
        assert_eq!(InlineFormat::from_mark_name("bold"), None);
    }

    #[test]
    fn test_all_returns_four_variants() {
        let all = InlineFormat::all();
        assert_eq!(all.len(), 4);
        assert!(all.contains(&InlineFormat::Strong));
        assert!(all.contains(&InlineFormat::Emphasis));
        assert!(all.contains(&InlineFormat::Strikethrough));
        assert!(all.contains(&InlineFormat::Code));
    }
}
