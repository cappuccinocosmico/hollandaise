# Overview:

This whole entire thing is designed to fill a need in an existing project I have. Namely its designed to be a:

"Collaborative rich text editor"

for the gui framework egui (https://docs.rs/egui/latest/egui/). 

As for the CRDT system, I think that using automerge (https://automerge.org/docs/hello/) is the best option, not only because its second in popularity in YJS and is written in rust, but also because it supports this insanely cool authentication and management solution in keyhive 
- vision doc: https://www.inkandswitch.com/keyhive/notebook/
- project gh: https://github.com/inkandswitch/keyhive


Another optional thing would be that the editor is to some extent markdown native. So the text essentially has 2 equal and equivalent states, one as Rich Text, and the other as markdown. So any human working on the document can see the entire thing as rich text and edit it exactly like they would in any other rich text editor.

And at the same time any LLM can come up to the collaborative session and see a document in markdown, make modification to the markdown document, and those show up as rich text modifications on the frontend UI editor that the humans are using.

There are seemingly multiple ways of doing this. Which is essentially dependant on if you want your fundamental source of truth for the data to be a simple array/rope of markdown utf8 encoded bytes or a rich text tree that encodes the styling therein. And for the system that you do support you just use native text editing, but the other system you solve by having a bidirectional transform, and then transforming any edits. The milkdown editor takes the first approach of using markdown as the native format: (https://milkdown.dev/docs/guide/getting-started). But the other approach has merits, (its what I used on my last project), what would your thoughts be on all this? Is there anything you would want to look up before continuing the architecture discussion.

---

# v2: Backend-Agnostic Rich Text Editor Core

## Context

The v1 implementation is tightly coupled to both egui (for rendering) and Automerge (for storage). Two problems:
1. **egui fights us** — TextEdit consumes hotkeys, no real bold font support, complex borrow gymnastics
2. **Not portable** — only works in egui apps, forces Automerge dependency

The core API mirrors CRDT-style operations (splice, mark, unmark) so that an Automerge adapter is a trivially thin passthrough, but the core itself has zero framework dependencies. Consumers handle cursor management and rendering.

## Architecture

Three layers, cleanly separated:

```
┌─────────────────────────────────────────────────────┐
│  Consumer (GUI app, CLI tool, LLM agent)            │
│  - Owns cursor/selection state                      │
│  - Handles rendering (egui, wgpu, terminal, etc.)   │
│  - Wires up input events                            │
├─────────────────────────────────────────────────────┤
│  hollandaise (this crate)                           │
│  - Editor<B: TextBackend>                           │
│  - toggle_format, markdown, spans for rendering     │
│  - Zero framework dependencies                      │
├─────────────────────────────────────────────────────┤
│  TextBackend implementations                        │
│  - LocalBackend (in-crate, for testing/standalone)  │
│  - AutomergeBackend (feature-gated)                 │
│  - YjsBackend, etc. (future, consumer-provided)     │
└─────────────────────────────────────────────────────┘
```

## Module Structure

```
src/
  lib.rs          -- Public re-exports only
  backend.rs      -- TextBackend trait, Span, FormatSet
  format.rs       -- InlineFormat enum (salvaged from v1)
  editor.rs       -- Editor<B> with toggle_format, markdown delegation
  local.rs        -- LocalBackend: String + Vec<MarkRange>
  markdown.rs     -- Generic over TextBackend (rewritten from v1)
  automerge.rs    -- AutomergeBackend (feature = "automerge")
```

## Core Types

### `TextBackend` trait (`backend.rs`)

Mirrors CRDT operations so Automerge adapter is trivially thin:

```rust
pub trait TextBackend {
    type Error: std::fmt::Debug;

    fn text(&self) -> Result<String, Self::Error>;
    fn char_count(&self) -> Result<usize, Self::Error>;
    fn splice(&mut self, pos: usize, delete: usize, text: &str) -> Result<(), Self::Error>;
    fn mark(&mut self, format: InlineFormat, start: usize, end: usize) -> Result<(), Self::Error>;
    fn unmark(&mut self, format: InlineFormat, start: usize, end: usize) -> Result<(), Self::Error>;
    fn formats_at(&self, pos: usize) -> Result<FormatSet, Self::Error>;
    fn spans(&self) -> Result<Vec<Span>, Self::Error>;
}
```

### `FormatSet` and `Span` (`backend.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FormatSet {
    pub strong: bool,
    pub emphasis: bool,
    pub strikethrough: bool,
    pub code: bool,
}

pub struct Span {
    pub text: String,
    pub formats: FormatSet,
}
```

### `Editor<B>` (`editor.rs`)

Consumer-facing API. Wraps a `TextBackend` and provides high-level operations:

```rust
pub struct Editor<B: TextBackend> {
    backend: B,
}
```

Methods: `insert_text`, `delete_range`, `toggle_format`, `is_range_formatted`, `to_markdown`, `from_markdown`, `spans`, `text`.

Cursor/selection is NOT here — consumer manages that and passes explicit ranges.

### `AutomergeBackend` (`automerge.rs`, feature-gated)

Trivially thin — every method is 1-5 lines forwarding to Automerge's API.

### `LocalBackend` (`local.rs`)

For testing and non-collaborative use. `String` + `Vec<MarkRange>`. `type Error = std::convert::Infallible`.

## Dependencies

```toml
[dependencies]
pulldown-cmark = "0.12"
automerge = { version = "0.7", optional = true }

[features]
default = []
automerge = ["dep:automerge"]
```

No egui. No rendering framework. `pulldown-cmark` is the only hard dependency.

## Rendering (Consumer's Responsibility)

The core provides `editor.spans() -> Vec<Span>`. Consumers render however they want (egui LayoutJob, terminal ANSI codes, HTML tags, etc.).
