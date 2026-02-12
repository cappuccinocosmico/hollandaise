mod backend;
mod editor;
mod format;
mod local;
mod markdown;

#[cfg(feature = "automerge")]
mod automerge;

pub use backend::{FormatSet, Span, TextBackend};
pub use editor::Editor;
pub use format::InlineFormat;
pub use local::LocalBackend;

#[cfg(feature = "automerge")]
pub use crate::automerge::AutomergeBackend;
