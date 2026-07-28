//! PhotoRust image engine.
//!
//! The Rust half of the "C++ shell, Rust core" split described in CLAUDE.md:
//! everything that touches pixels lives here, and the QWidgets UI lives in
//! `shell/`. The two meet at exactly one place, [`bridge`].
//!
//! # Module map
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`buffer`] | RGBA8 pixel buffers and rectangles |
//! | [`blend`] | The 27 Photoshop blend modes |
//! | [`layer`] | Layer model and the ordered stack |
//! | [`compositor`] | Walks the stack and produces the final image |
//! | [`selection`] | Coverage-mask selections |
//! | [`brush`] | Dab-based stroke rendering |
//! | [`filters`] | Adjustments and convolutions |
//! | [`history`] | Bounded linear undo |
//! | [`document`] | One open image; ties the above together |
//! | [`psd`] | `.psd` parsing and writing |
//! | [`bridge`] | The CXX-Qt QObject exposed to C++ |
//!
//! # Conventions
//!
//! * Colour is **straight (non-premultiplied) alpha** everywhere except the
//!   moment a buffer is handed to Qt. See [`buffer::Pixmap::premultiply`].
//! * Layer stacks are stored **bottom-first** (index 0 is the Background).
//!   The Layers panel shows them top-first, and that flip happens only in
//!   [`bridge`].

pub mod blend;
pub mod bridge;
pub mod brush;
pub mod buffer;
pub mod compositor;
pub mod document;
pub mod filters;
pub mod history;
pub mod layer;
pub mod psd;
pub mod selection;

pub use blend::BlendMode;
pub use brush::Brush;
pub use buffer::{Pixmap, Rect, Rgba8};
pub use document::Document;
pub use layer::{Layer, LayerId, LayerStack};
pub use selection::{Selection, SelectionOp};
