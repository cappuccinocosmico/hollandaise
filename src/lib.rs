//! Hollandaise: Collaborative rich text editor widget for egui
//!
//! Provides a rich text editor widget backed by Automerge's Peritext CRDT.
//! The consumer owns the Automerge document and handles sync/networking.
//! This crate provides the editor widget and markdown projection.

mod buffer;
mod format;
mod layout;
mod markdown;

pub use format::InlineFormat;
pub use layout::FontConfig;
pub use markdown::{apply_markdown, spans_to_markdown};

use automerge::{AutoCommit, ObjId as ExId};
use egui::{Key, TextEdit, Ui};

/// Output from the rich text editor widget
#[derive(Debug, Clone)]
pub struct RichTextEditOutput {
    /// The response from the underlying TextEdit widget
    pub response: egui::Response,
    /// The current cursor range (if any)
    pub cursor_range: Option<std::ops::Range<usize>>,
}

/// Show the rich text editor widget
///
/// This function implements a three-phase borrow pattern to safely work with
/// the Automerge document:
///
/// 1. Phase 1 (immutable): Read spans from doc and collect into owned Vec
/// 2. Phase 2 (mutable): Create RichTextBuffer and show TextEdit widget
/// 3. Phase 3 (mutable): Handle formatting hotkeys
///
/// # Arguments
/// * `ui` - The egui UI context
/// * `doc` - The Automerge document (mutable for edits)
/// * `text_obj` - The ExId of the text object in the document
/// * `font_config` - Font configuration for rendering
///
/// # Returns
/// Output containing the response and cursor information
pub fn show(
    ui: &mut Ui,
    doc: &mut AutoCommit,
    text_obj: &ExId,
    font_config: &FontConfig,
) -> RichTextEditOutput {
    // PHASE 0: Check for formatting hotkeys BEFORE TextEdit consumes them
    // We need to CONSUME the events using input_mut, not just check them
    let wants_bold = ui.input_mut(|i| {
        if i.modifiers.command && i.key_pressed(Key::B) {
            eprintln!("DEBUG: Ctrl+B detected!");
            // Consume the event so TextEdit doesn't see it
            i.consume_key(egui::Modifiers::COMMAND, Key::B);
            true
        } else {
            false
        }
    });
    let wants_italic = ui.input_mut(|i| {
        if i.modifiers.command && i.key_pressed(Key::I) {
            eprintln!("DEBUG: Ctrl+I detected!");
            // Consume the event so TextEdit doesn't see it
            i.consume_key(egui::Modifiers::COMMAND, Key::I);
            true
        } else {
            false
        }
    });

    // PHASE 1: Build layout job from Automerge (immutable borrow)
    // This captures all formatting information before we take a mutable borrow
    let layout_job = layout::build_layout_job(doc, text_obj, font_config);

    // PHASE 2: Create buffer and show TextEdit (mutable borrow)
    let mut buffer = buffer::RichTextBuffer::new(doc, text_obj.clone());

    // Clone layout job for the layouter closure
    // Note: The layout job from phase 1 may be one frame behind if edits happen,
    // but this is imperceptible and self-corrects on the next frame
    let mut layouter = |ui: &Ui, _text: &dyn egui::TextBuffer, wrap_width: f32| {
        // Use the pre-built layout job from phase 1
        let mut job = layout_job.clone();
        job.wrap.max_width = wrap_width;
        ui.fonts_mut(|f| f.layout_job(job))
    };

    let text_edit = TextEdit::multiline(&mut buffer)
        .desired_width(f32::INFINITY)
        .layouter(&mut layouter);

    let response = text_edit.show(ui);

    // Get cursor range (convert CCursorRange to character indices)
    let cursor_range = response.cursor_range.map(|r| {
        let start = r.primary.index.min(r.secondary.index);
        let end = r.primary.index.max(r.secondary.index);
        start..end
    });

    // Drop buffer to release mutable borrow on doc
    drop(buffer);

    // PHASE 3: Apply formatting if hotkeys were pressed (mutable borrow)
    if let Some(ref range) = cursor_range {
        eprintln!("DEBUG: cursor_range = {:?}", range);
        if range.start < range.end {
            eprintln!("DEBUG: Non-empty selection, applying formatting...");
            // Only apply formatting if there's a non-empty selection
            if wants_bold {
                eprintln!("DEBUG: Applying bold to range {:?}", range);
                toggle_format(doc, text_obj, InlineFormat::Strong, range.start, range.end);
                ui.ctx().request_repaint(); // Force immediate repaint
            }
            if wants_italic {
                eprintln!("DEBUG: Applying italic to range {:?}", range);
                toggle_format(doc, text_obj, InlineFormat::Emphasis, range.start, range.end);
                ui.ctx().request_repaint(); // Force immediate repaint
            }
        } else {
            eprintln!("DEBUG: Empty selection, skipping formatting");
        }
    } else {
        eprintln!("DEBUG: No cursor_range");
    }

    RichTextEditOutput {
        response: response.response,
        cursor_range,
    }
}

/// Toggle inline formatting on a text range
///
/// If the entire range is already formatted, removes the format.
/// Otherwise, applies the format to the range.
///
/// # Arguments
/// * `doc` - The Automerge document
/// * `text_obj` - The ExId of the text object
/// * `format` - The format to toggle
/// * `start` - Start character index (inclusive)
/// * `end` - End character index (exclusive)
pub fn toggle_format(
    doc: &mut AutoCommit,
    text_obj: &ExId,
    format: InlineFormat,
    start: usize,
    end: usize,
) {
    format::toggle_format(doc, text_obj, format, start, end)
}
