//! ScreenSpec: the structured description a model sends instead of hand-written
//! HTML. The host renders it (see [`crate::spec_render`]); raw `body_html`
//! stays available as the escape hatch.
//!
//! Design rule the whole vocabulary follows: **structure and tokens in, CSS
//! out.** A node says where things sit and what they are called; appearance
//! comes only from [`Theme`] tokens and [`Fidelity`]. There is no style field
//! on purpose — free-form style is just HTML with extra steps, which is the
//! failure mode this type exists to prevent.
//!
//! The vocabulary is closed and was fitted to a corpus of real design screens
//! (see `notes/screen-spec-design.md` in the product repo): layout nodes, the
//! device/window chrome a form factor needs, high-frequency controls, and data
//! widgets — plus one `emph` marker and a fixed set of texture knobs. Anything
//! outside it goes through `body_html`.

mod de;
mod diagnostics;
mod parse;
mod types;
mod validate;

pub(crate) use self::de::parse_tracked;
pub use self::types::{
    BarItem, Fidelity, MAX_NODES, MAX_SPEC_JSON_BYTES, MockNode, ScreenSpec, SparkShape, SpecError,
    Surface, Theme, TreeItem, Variant, VizKind,
};
pub use self::validate::NODE_KINDS;
