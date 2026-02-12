# Hollandaise

A collaborative rich text editor widget for [egui](https://github.com/emilk/egui), backed by [Automerge's Peritext](https://www.inkandswitch.com/peritext/) rich text CRDT.

## Features

- **Rich text editing** with inline formatting:
  - Bold (`Ctrl+B` / `Cmd+B`)
  - Italic (`Ctrl+I` / `Cmd+I`)
  - Strikethrough
  - Inline code
- **Markdown projection**: Bidirectional conversion between rich text and markdown
- **CRDT-backed**: Uses Automerge for conflict-free collaborative editing
- **Bring your own sync**: You own the Automerge document and handle networking

## MVP Scope

This is the MVP implementation supporting **inline formatting only**. Block elements (headings, lists, code blocks, etc.) are planned for future releases.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
hollandaise = "0.1.0"
automerge = "0.7"
egui = "0.33"
```

## Usage

### Basic Setup

```rust
use automerge::{AutoCommit, ObjType, ROOT};
use hollandaise::{show, FontConfig};

// Create an Automerge document with a text object
let mut doc = AutoCommit::new();
let text_obj = doc.put_object(ROOT, "text", ObjType::Text).unwrap();

// Show the editor in your egui UI
let font_config = FontConfig::default();
let output = show(ui, &mut doc, &text_obj, &font_config);
```

### Markdown Projection

```rust
use hollandaise::{spans_to_markdown, apply_markdown};

// Export to markdown
let markdown = spans_to_markdown(&doc, &text_obj);

// Import from markdown
apply_markdown(&mut doc, &text_obj, "**bold** *italic*");
```

### Custom Formatting

```rust
use hollandaise::{toggle_format, InlineFormat};

// Toggle formatting programmatically
toggle_format(&mut doc, &text_obj, InlineFormat::Strong, 0, 5);
```

## Architecture

Hollandaise uses a three-phase borrow pattern to safely work with the Automerge document:

1. **Phase 1 (immutable)**: Read spans from Automerge and build layout information
2. **Phase 2 (mutable)**: Create `RichTextBuffer` and show `TextEdit` widget
3. **Phase 3 (mutable)**: Handle formatting hotkeys

This design allows the layouter to access pre-computed formatting while the buffer manages text mutations.

## Demo

Run the demo application:

```bash
cargo run --example demo
```

The demo shows:
- Real-time rich text editing
- Formatting hotkeys (Ctrl+B, Ctrl+I)
- Markdown export/import

## Font Configuration

egui doesn't have a built-in bold flag - bold text requires a separate font. Configure fonts like this:

```rust
use egui::FontId;
use hollandaise::FontConfig;

let font_config = FontConfig {
    regular: FontId::proportional(14.0),
    bold: FontId::new(14.0, egui::FontFamily::Name("Bold".into())),
    code: FontId::monospace(14.0),
    code_background: egui::Color32::from_gray(240),
    text_color: egui::Color32::BLACK,
};
```

Make sure to register your bold font in egui's `FontDefinitions` before using the widget.

## Testing

```bash
# Run all tests
cargo test

# Run unit tests only
cargo test --lib

# Run integration tests
cargo test --test integration_tests
```

Test coverage:
- **49 unit tests** covering individual modules
- **10 integration tests** covering end-to-end workflows

## License

MIT OR Apache-2.0

## Contributing

This is an early MVP. Contributions are welcome! Areas for improvement:

- Block elements (headings, lists, code blocks, blockquotes)
- Additional inline formats (underline, highlight, colors)
- Undo/redo support
- Better font weight handling in egui
- Performance optimizations for large documents

## Acknowledgments

- Built on [Automerge](https://automerge.org/) and [Peritext](https://www.inkandswitch.com/peritext/)
- Powered by [egui](https://github.com/emilk/egui)
- Markdown parsing with [pulldown-cmark](https://github.com/raphlinus/pulldown-cmark)
