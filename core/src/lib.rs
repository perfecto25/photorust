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
//! | [`stamp`] | Clone Stamp sampling |
//! | [`filters`] | Adjustments and convolutions |
//! | [`gradient`] | Colour ramps and the five gradient shapes |
//! | [`bucket`] | Flood filling for the Paint Bucket |
//! | [`focus`] | The Blur and Sharpen tools |
//! | [`smudge`] | The Smudge tool |
//! | [`tone`] | The Dodge, Burn and Sponge tools |
//! | [`path`] | Vector paths for the Pen tool and Paths panel |
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

pub mod annotation;
pub mod blend;
pub mod bridge;
pub mod brush;
pub mod bucket;
pub mod buffer;
pub mod compositor;
pub mod document;
pub mod erase;
pub mod filters;
pub mod focus;
pub mod gpu;
pub mod gradient;
pub mod healing;
pub mod history;
pub mod layer;
pub mod magnetic;
pub mod metadata;
pub mod mixer;
pub mod path;
pub mod pattern;
pub mod perspective;
pub mod psd;
pub mod replace;
pub mod sample;
pub mod selection;
pub mod shape;
pub mod slice;
pub mod smudge;
pub mod stamp;
pub mod tone;
pub mod wand;

pub use blend::BlendMode;
pub use brush::Brush;
pub use buffer::{Pixmap, Rect, Rgba8};
pub use document::Document;
pub use layer::{Layer, LayerId, LayerStack};
pub use selection::{Selection, SelectionOp};
