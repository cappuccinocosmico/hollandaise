//! Demo application for the Hollandaise rich text editor
//!
//! This example shows how to use the rich text editor widget with automerge.
//! It demonstrates:
//! - Basic text editing
//! - Formatting hotkeys (Ctrl+B for bold, Ctrl+I for italic)
//! - Markdown export/import

use automerge::transaction::Transactable;
use automerge::{AutoCommit, ObjType, ReadDoc, ROOT};
use eframe::egui;
use hollandaise::{show, FontConfig};

struct DemoApp {
    doc: AutoCommit,
    text_obj: automerge::ObjId,
    font_config: FontConfig,
    markdown_view: String,
    show_markdown: bool,
}

impl Default for DemoApp {
    fn default() -> Self {
        let mut doc = AutoCommit::new();
        let text_obj = doc
            .put_object(ROOT, "text", ObjType::Text)
            .expect("Failed to create text object");

        // Initialize with some sample text
        doc.splice_text(&text_obj, 0, 0, "Welcome to Hollandaise!\n\nTry selecting text and using:\n- Ctrl+B (or Cmd+B) for bold\n- Ctrl+I (or Cmd+I) for italic")
            .expect("Failed to initialize text");

        // Create font config that works with both light and dark themes
        // Note: egui doesn't have built-in bold fonts, so we use a larger size as a visual distinction
        // In production, you'd register a real bold font in FontDefinitions
        let font_config = FontConfig {
            regular: egui::FontId::proportional(14.0),
            bold: egui::FontId::proportional(16.0), // Larger size to simulate bold
            code: egui::FontId::monospace(13.0),
            code_background: egui::Color32::from_rgba_premultiplied(128, 128, 128, 40),
            text_color: egui::Color32::WHITE, // White text for dark theme
        };

        Self {
            doc,
            text_obj,
            font_config,
            markdown_view: String::new(),
            show_markdown: false,
        }
    }
}

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Hollandaise Rich Text Editor Demo");
            ui.separator();

            // Controls
            ui.horizontal(|ui| {
                if ui.button("Export to Markdown").clicked() {
                    self.markdown_view = hollandaise::spans_to_markdown(&self.doc, &self.text_obj);
                    self.show_markdown = true;
                }

                if ui.button("Import from Markdown").clicked() && !self.markdown_view.is_empty() {
                    hollandaise::apply_markdown(&mut self.doc, &self.text_obj, &self.markdown_view);
                }

                if ui.button("Clear").clicked() {
                    let text_len = self.doc.text(&self.text_obj).expect("Failed to read text").chars().count();
                    if text_len > 0 {
                        self.doc.splice_text(&self.text_obj, 0, text_len as isize, "")
                            .expect("Failed to clear text");
                    }
                }

                ui.checkbox(&mut self.show_markdown, "Show Markdown");
            });

            ui.separator();

            // Editor
            ui.label("Editor (use Ctrl+B for bold, Ctrl+I for italic):");
            let _output = show(ui, &mut self.doc, &self.text_obj, &self.font_config);

            // Markdown view
            if self.show_markdown {
                ui.separator();
                ui.label("Markdown View:");
                ui.text_edit_multiline(&mut self.markdown_view);
            }

            // Stats
            ui.separator();
            let text = self.doc.text(&self.text_obj).expect("Failed to read text");
            ui.label(format!(
                "Characters: {}, Bytes: {}",
                text.chars().count(),
                text.len()
            ));
        });
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Hollandaise Demo",
        options,
        Box::new(|_cc| Ok(Box::new(DemoApp::default()))),
    )
}
