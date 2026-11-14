//! Screen vocabulary types: the closed node tree, fidelity and theme tokens,
//! variants, and the whole-screen shape. Validation and parsing live in
//! siblings.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::de::{de_opt_number, de_opt_proportion, de_weights};

/// Cap on a whole `screen` spec, mirroring [`crate::envelope::MAX_BODY_HTML_BYTES`].
/// Bounds abuse only — the real budget is the token cost the schema is for.
pub const MAX_SPEC_JSON_BYTES: usize = 256 * 1024;

/// Maximum number of mock nodes in one tree. A design screen is a picture, not
/// a document; depth/width beyond this is a sign the model wants `body_html`.
pub const MAX_NODES: usize = 200;

/// How finished the mock should look. Follows the question being asked:
/// layout questions need structure, styling questions need real color, texture
/// questions need near-final rendering — without them the user judges blind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fidelity {
    /// Greyscale structure only: boxes, proportions, labels.
    Wireframe,
    /// Real color blocks from [`Theme`] color tokens.
    Color,
    /// Near-final look: color tokens plus the texture knobs.
    Texture,
}

/// Material family for `fidelity: texture`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Surface {
    Glass,
    Matte,
    Metal,
}

/// Appearance tokens. Every knob is a fixed vocabulary entry, not CSS.
///
/// Color tokens apply from `fidelity: color` up; the numeric texture knobs
/// only bite at `fidelity: texture`. They are the four axes a real refinement
/// round actually touched (frost = backdrop blur, elevation = shadow weight,
/// edge = border brightness, glow = outer bloom) — `border-radius` is absent
/// on purpose: it was the property the model changed when nobody asked.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent2: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_bright: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<Surface>,
    /// Backdrop blur budget, 1–3 (thicker glass).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frost: Option<u8>,
    /// Shadow weight, 1–3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elevation: Option<u8>,
    /// Border brightness, 1–3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge: Option<u8>,
    /// Outer bloom, 0–2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glow: Option<u8>,
}

/// One node of the mock tree.
///
/// Nodes carry identity (`id`) and copy (`text`) only. Which node a variant
/// emphasizes is data on the *variant* (`emph`), not a style on the node — so
/// two candidates that differ only in "where the accent lands" differ by one
/// list, not by two whole trees.
///
/// `PartialEq` only — bar magnitudes are `f64` (see [`BarItem`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MockNode {
    /// A framed panel with a title bar (browser chrome, dashboard header).
    Frame {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        body: Box<MockNode>,
    },
    /// Horizontal band; `ratio` weights the children (defaults to equal).
    /// Weights are numbers or numeric strings — `[1, 5, 0.7]` is fine.
    Cols {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(
            default,
            skip_serializing_if = "Vec::is_empty",
            deserialize_with = "de_weights"
        )]
        ratio: Vec<String>,
        children: Vec<MockNode>,
    },
    /// Vertical band; same weighting as [`MockNode::Cols`].
    Rows {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(
            default,
            skip_serializing_if = "Vec::is_empty",
            deserialize_with = "de_weights"
        )]
        ratio: Vec<String>,
        children: Vec<MockNode>,
    },
    /// `count` copies of `child`, evenly sized.
    Stack {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        count: u32,
        child: Box<MockNode>,
    },
    /// A sized region, optionally labelled. `w`/`h` are percentages of the
    /// parent (structural proportion, not style) — a number or a string.
    #[serde(rename = "box")]
    Region {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "de_opt_proportion"
        )]
        w: Option<String>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "de_opt_proportion"
        )]
        h: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// Top navigation bar: a row of labelled items.
    Nav {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        items: Vec<String>,
    },
    /// Side navigation column.
    Sidebar {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        items: Vec<String>,
    },
    Input {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    Button {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// Row of color chips showing the theme tokens in play.
    Swatches {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    /// Any single line of copy, including region annotations.
    Text {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        text: String,
    },
    /// Empty region stand-in ("内容区").
    Placeholder {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },

    // ---- data widgets: the dashboard vocabulary ----
    /// Number card: label + value + optional change badge.
    Kpi {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        value: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delta: Option<String>,
    },
    /// Oversized center number.
    Hero {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        text: String,
    },
    /// Mini trend line. Shape only — mock data is decorative.
    Sparkline {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shape: Option<SparkShape>,
    },
    /// Donut progress.
    Ring {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        pct: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Horizontal ranking bars.
    Bars {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        items: Vec<BarItem>,
    },
    /// Trend area chart placeholder.
    Area {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Center-piece visual: the kind of story the dashboard tells.
    ///
    /// `form` rather than `kind` because `kind` is already this enum's serde
    /// discriminator — a node cannot carry two of them.
    Viz {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        form: VizKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },

    // ---- device & window chrome: what a form factor wears ----
    /// Phone device shell. The status bar and home indicator are device chrome
    /// the renderer always draws; `body` is the screen itself.
    PhoneShell {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        body: Box<MockNode>,
    },
    /// Mobile screen header: back chevron, centred title, one trailing action.
    Topbar {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default)]
        back: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        action: Option<String>,
    },
    /// Bottom tab bar; `active` is the index of the selected tab.
    Tabbar {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        items: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        active: Option<u8>,
    },
    /// Action row: a menu bar's items or a tool strip's buttons.
    Toolbar {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        items: Vec<String>,
    },
    /// Modal card: title, a message or a body node, and an action row.
    Dialog {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<Box<MockNode>>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        actions: Vec<String>,
    },

    // ---- high-frequency controls ----
    /// List cell: title, note under it, right-hand meta, avatar and chevron.
    ListRow {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meta: Option<String>,
        #[serde(default)]
        chevron: bool,
        #[serde(default)]
        avatar: bool,
    },
    /// Toggle switch with an optional label.
    Switch {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(default)]
        on: bool,
    },
    /// Segmented control; `active` is the index of the selected segment.
    Segment {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        items: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        active: Option<u8>,
    },

    // ---- dense content ----
    /// Data table: `cols` headers and `rows` of cells (every row the same
    /// length). Cells may be empty; mock values are decorative like any other.
    Table {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        cols: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        rows: Vec<Vec<String>>,
    },
    /// Indented tree; `depth` nests an item.
    Tree {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        items: Vec<TreeItem>,
    },
}

impl MockNode {
    /// Depth-first walk over this node and its children.
    pub fn for_each_node(&self, visit: &mut impl FnMut(&MockNode)) {
        visit(self);
        match self {
            MockNode::Frame { body, .. }
            | MockNode::PhoneShell { body, .. }
            | MockNode::Dialog {
                body: Some(body), ..
            } => body.for_each_node(visit),
            MockNode::Cols { children, .. } | MockNode::Rows { children, .. } => {
                for child in children {
                    child.for_each_node(visit);
                }
            }
            MockNode::Stack { child, .. } => child.for_each_node(visit),
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SparkShape {
    Up,
    Down,
    Flat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VizKind {
    Map,
    Funnel,
    Graph,
    Area,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarItem {
    pub label: String,
    /// Relative magnitude — the renderer divides by the largest value, so only
    /// ordering matters. Deserialized wide on purpose, see `de_opt_number`.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_opt_number"
    )]
    pub value: Option<f64>,
}

/// One line of [`MockNode::Tree`]. `depth` is nesting level, not style.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeItem {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u8>,
}

/// One candidate. `id` aligns with an `options[].id` so the host can render the
/// card's label/note from the envelope instead of the model retyping them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variant {
    pub id: String,
    /// Full tree override — used when candidates are structurally different
    /// (layout comparisons). Otherwise the shared `mock` applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mock: Option<MockNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<Theme>,
    /// Node ids that receive the accent. The "where does the one highlight go"
    /// question becomes a difference of one list between variants.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emph: Vec<String>,
    /// Node id → replacement copy.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub content: BTreeMap<String, String>,
}

/// The whole structured screen. Replaces `body_html` when present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenSpec {
    pub fidelity: Fidelity,
    /// Shared mock tree. Each [`Variant`] may override it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mock: Option<MockNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<Variant>,
}

/// Why a spec was rejected.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SpecError {
    #[error("screen must be a JSON object")]
    NotAnObject,
    /// `screen` arrived as a JSON *string*. Strings are rejected even when
    /// their content parses — the leniency kept the model on the string path,
    /// where hand-compressed copies break (session 20261022_bb8c2dc3: five
    /// identical stringified failures at the same column). The repair is
    /// structural: write the screen to a file and pass `screen_file`, or send
    /// the object itself.
    #[error(
        "screen must be a JSON object - it arrived stringified (one JSON string, not the object). Write the screen to a file and pass its path as `screen_file`, or send `screen` as the object itself - never a stringified copy"
    )]
    Stringified,
    /// A stringified `screen` whose content is not even valid JSON — say what
    /// broke and where, on top of the stringified lesson.
    #[error(
        "screen must be a JSON object - it arrived as a string whose content is not valid JSON ({detail}) near `{near}`. Write the screen to a file and pass its path as `screen_file`, or send `screen` as the object itself - never a stringified copy"
    )]
    StringifiedBroken { detail: String, near: String },
    /// A stringified `screen` whose content is valid JSON but not an object.
    #[error(
        "screen must be a JSON object - it arrived as a string that parses as {found}, not an object. Write the screen to a file and pass its path as `screen_file`, or send `screen` as the object itself - never a stringified copy"
    )]
    StringifiedNotObject { found: String },
    #[error("invalid screen: {0}")]
    Malformed(String),
    #[error("invalid screen field `{field}`: {reason}")]
    InvalidField { field: String, reason: String },
}
