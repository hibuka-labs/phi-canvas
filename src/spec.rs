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

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

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

/// A structural proportion arriving as either a JSON number or a string.
///
/// `w`/`h` are percentages of the parent — `18`, `"18"`, and `"18%"` all mean
/// the same share. Models reach for the bare number first (observed in two
/// consecutive live runs); rejecting it costs a whole push round to repeat
/// what they already meant.
fn de_opt_proportion<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct ProportionVisitor;

    impl<'de> serde::de::Visitor<'de> for ProportionVisitor {
        type Value = Option<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a proportion (number or string, like 18 or \"18%\")")
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_some<D2>(self, deserializer: D2) -> Result<Self::Value, D2::Error>
        where
            D2: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(ProportionVisitor)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(value.to_string()))
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(value.to_string()))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(value.to_string()))
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(value.to_string()))
        }
    }

    deserializer.deserialize_any(ProportionVisitor)
}

/// Canonical track weight: `3`, `"3"`, and `3.0` all print as `3`, while a
/// finer split keeps its decimals — `minmax(0, 3fr)` / `minmax(0, 0.7fr)`.
fn weight_text(value: f64) -> String {
    let text = format!("{value}");
    if let Some(stem) = text.strip_suffix(".0") {
        stem.to_string()
    } else {
        text
    }
}

/// `ratio` weights arriving as numbers or numeric strings.
///
/// Weights are proportions, not integers — `ratio: [1, 5, 0.7]` is the model
/// reaching for a finer split than whole numbers allow (observed live).
/// Rejecting the decimal cost a whole push round to re-type it as `[3, 14, 1]`.
fn de_weights<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct WeightsVisitor;

    impl<'de> serde::de::Visitor<'de> for WeightsVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a list of weights (numbers or numeric strings, like [1, 0.7])")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut out = Vec::new();
            while let Some(weight) = seq.next_element_seed(WeightSeed)? {
                out.push(weight);
            }
            Ok(out)
        }
    }

    struct WeightSeed;
    impl<'de> serde::de::DeserializeSeed<'de> for WeightSeed {
        type Value = String;

        fn deserialize<D2>(self, deserializer: D2) -> Result<Self::Value, D2::Error>
        where
            D2: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(WeightVisitor)
        }
    }

    struct WeightVisitor;
    impl<'de> serde::de::Visitor<'de> for WeightVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a weight (a number like 1 or 0.7)")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(weight_text(value as f64))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(weight_text(value as f64))
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(weight_text(value))
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            match value.trim().parse::<f64>() {
                Ok(number) => Ok(weight_text(number)),
                Err(_) => Err(E::invalid_value(serde::de::Unexpected::Str(value), &self)),
            }
        }
    }

    deserializer.deserialize_seq(WeightsVisitor)
}

/// One node of the mock tree.
///
/// Nodes carry identity (`id`) and copy (`text`) only. Which node a variant
/// emphasizes is data on the *variant* (`emph`), not a style on the node — so
/// two candidates that differ only in "where the accent lands" differ by one
/// list, not by two whole trees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarItem {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<u32>,
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// A deserialize failure with the field path serde gave up at.
pub(crate) struct FieldError {
    pub(crate) path: String,
    pub(crate) message: String,
}

impl std::fmt::Display for FieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.path.is_empty() || self.path == "." {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

/// Deserialize `value` into `T`, keeping the failing field path. A schema
/// fumble must name the field: a pathless "expected u32" makes the model guess
/// and re-push the whole screen (observed: two retries per wrong field).
pub(crate) fn parse_tracked<T: serde::de::DeserializeOwned>(
    value: &serde_json::Value,
) -> Result<T, FieldError> {
    use serde::de::IntoDeserializer;
    let de = value.clone().into_deserializer();
    match serde_path_to_error::deserialize(de) {
        Ok(parsed) => Ok(parsed),
        Err(err) => {
            let path = err.path().to_string();
            let inner = err.into_inner();
            let path = if path == "." { String::new() } else { path };
            Err(FieldError {
                path,
                message: inner.to_string(),
            })
        }
    }
}

/// A type error at a node boundary — `mock` or `variants[N].mock` — is all
/// serde can report: an internally-tagged enum buffers its content, so the
/// tracked path stops at the node and the field is lost. Name it.
fn refine_or_malformed(root: &serde_json::Value, err: &FieldError) -> SpecError {
    let node = match err.path.as_str() {
        "mock" => root.get("mock"),
        other => other
            .strip_prefix("variants[")
            .and_then(|rest| rest.split_once("]."))
            .filter(|(_, tail)| *tail == "mock")
            .and_then(|(index, _)| index.parse::<usize>().ok())
            .and_then(|index| root.get("variants")?.get(index)?.get("mock")),
    };
    node.and_then(|node| refine_node_type_error(node, &format!("screen.{}", err.path)))
        .unwrap_or_else(|| SpecError::Malformed(err.to_string()))
}

/// Probe a node's fields one at a time to find the one carrying a bad value.
/// Nested nodes recurse (`body`, `child`, `children[i]`) so the rejection ends
/// at the field itself: `screen.mock.children[1].w: invalid type: …`.
fn refine_node_type_error(node: &serde_json::Value, path: &str) -> Option<SpecError> {
    let obj = node.as_object()?;
    let kind = obj.get("kind").and_then(serde_json::Value::as_str)?;
    for (key, field) in obj {
        if key == "kind" {
            continue;
        }
        let Some(reason) = probe_field(kind, key, field) else {
            continue;
        };
        if is_node(field) {
            return refine_node_type_error(field, &format!("{path}.{key}"))
                .or_else(|| Some(invalid(&format!("{path}.{key}"), &reason)));
        }
        if let Some(items) = field.as_array() {
            for (index, item) in items.iter().enumerate() {
                let item_path = format!("{path}.{key}[{index}]");
                if probe_field(kind, key, &serde_json::json!([item])).is_none() {
                    continue;
                }
                return Some(if is_node(item) {
                    refine_node_type_error(item, &item_path)
                        .or_else(|| {
                            node_in_string_slot(item, &reason)
                                .map(|teach| invalid(&item_path, &teach))
                        })
                        .unwrap_or_else(|| invalid(&item_path, &reason))
                } else {
                    invalid(&item_path, &reason)
                });
            }
            return Some(invalid(&format!("{path}.{key}"), &reason));
        }
        return Some(invalid(&format!("{path}.{key}"), &reason));
    }
    None
}

/// The type error `value` produces as the only non-`kind` field of a `kind`
/// node — `None` when the field type is fine. A probe that only complains
/// about what the probe left out (a missing sibling field) says nothing about
/// this field.
fn probe_field(kind: &str, key: &str, value: &serde_json::Value) -> Option<String> {
    let mut probe = serde_json::Map::new();
    probe.insert("kind".to_string(), serde_json::json!(kind));
    probe.insert(key.to_string(), value.clone());
    let err = parse_tracked::<MockNode>(&serde_json::Value::Object(probe)).err()?;
    if err.message.contains("missing field") {
        return None;
    }
    if let Some(teach) = node_in_string_slot(value, &err.message) {
        return Some(teach);
    }
    Some(err.message)
}

fn is_node(value: &serde_json::Value) -> bool {
    value
        .as_object()
        .is_some_and(|obj| obj.get("kind").is_some_and(serde_json::Value::is_string))
}

/// A node sent where a field wants a string — serde's "invalid type: map,
/// expected a string" is true but teaches nothing. The live fumble is a
/// control node inside `items` (twice, on the push path): say what the field
/// takes and where the node belongs instead.
fn node_in_string_slot(node: &serde_json::Value, reason: &str) -> Option<String> {
    if !reason.contains("expected a string") {
        return None;
    }
    let kind = node
        .as_object()?
        .get("kind")
        .and_then(serde_json::Value::as_str)?;
    Some(format!(
        "expected a string, got a `{kind}` node — this field takes label text \
         (`items` entries are label strings); a `{kind}` node belongs beside it \
         in the layout tree (`cols`/`rows`/`body`), never inside this field"
    ))
}

/// The JSON kind name for a diagnostic ("a JSON array").
fn json_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Object(_) => "a JSON object",
        serde_json::Value::Array(_) => "a JSON array",
        serde_json::Value::String(_) => "a JSON string",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Null => "null",
    }
}

/// serde_json reports a 1-based line and a byte column within that line.
fn error_offset(text: &str, err: &serde_json::Error) -> usize {
    let mut offset = 0usize;
    for _ in 1..err.line() {
        match text[offset..].find('\n') {
            Some(at) => offset += at + 1,
            None => break,
        }
    }
    offset + err.column().saturating_sub(1)
}

/// A short window around the byte offset serde gave up at, so the model can
/// see its own fumble instead of diffing 2 KB of stringified JSON. Clamped to
/// char boundaries — the content carries CJK copy.
fn excerpt_around(text: &str, offset: usize) -> String {
    const WINDOW: usize = 36;
    let mut start = offset.saturating_sub(WINDOW);
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (offset + WINDOW).min(text.len());
    while end > start && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[start..end].to_string()
}

impl ScreenSpec {
    /// Parse and validate a `screen` argument.
    pub fn parse(value: &serde_json::Value) -> Result<Self, SpecError> {
        // A `screen` sometimes arrives double-encoded — the whole object as
        // one JSON string. Every string is rejected: unwrapping the valid ones
        // is what kept the model stringifying (session 20261022_bb8c2dc3, five
        // broken copies in a row). A broken one is diagnosed with the spot it
        // broke so the next attempt is not blind; all three answers name
        // `screen_file` as the way to send a big screen whole.
        let value = match value.as_str() {
            Some(text) => match serde_json::from_str::<serde_json::Value>(text) {
                Ok(inner) if inner.is_object() => return Err(SpecError::Stringified),
                Ok(inner) => {
                    return Err(SpecError::StringifiedNotObject {
                        found: json_kind(&inner).to_string(),
                    });
                }
                Err(err) => {
                    let near = excerpt_around(text, error_offset(text, &err));
                    return Err(SpecError::StringifiedBroken {
                        detail: err.to_string(),
                        near,
                    });
                }
            },
            None => value,
        };
        if !value.is_object() {
            return Err(SpecError::NotAnObject);
        }
        // Node kinds are an internally-tagged enum, which serde cannot run
        // `deny_unknown_fields` on — so the closed vocabulary is enforced here,
        // before the typed parse. Without this a stray `"style"` would be
        // silently dropped and the model would never learn the boundary.
        check_node_keys(value, "screen")?;
        let spec: ScreenSpec =
            parse_tracked(value).map_err(|err| refine_or_malformed(value, &err))?;
        spec.validate()?;
        Ok(spec)
    }

    fn validate(&self) -> Result<(), SpecError> {
        if let Some(notice) = &self.notice
            && notice.chars().count() > 500
        {
            return Err(invalid("notice", "exceeds 500 char cap"));
        }
        let mut node_count = 0usize;
        if let Some(mock) = &self.mock {
            validate_node(mock, "mock", &mut node_count)?;
        }
        let mut ids = HashSet::new();
        for (index, variant) in self.variants.iter().enumerate() {
            let path = format!("variants[{index}]");
            if variant.id.is_empty() || variant.id.len() > 64 {
                return Err(invalid(&format!("{path}.id"), "must be 1-64 chars"));
            }
            if !ids.insert(variant.id.as_str()) {
                return Err(invalid(
                    &format!("{path}.id"),
                    &format!("duplicate id `{}`", variant.id),
                ));
            }
            if let Some(mock) = &variant.mock {
                validate_node(mock, &format!("{path}.mock"), &mut node_count)?;
            }
            if variant.mock.is_none() && self.mock.is_none() {
                return Err(invalid(&path, "needs a mock tree here or on the screen"));
            }
            if let Some(theme) = &variant.theme {
                validate_theme(theme, &format!("{path}.theme"))?;
            }
            if variant.emph.len() > 32 {
                return Err(invalid(&format!("{path}.emph"), "exceeds 32 entries"));
            }
            if variant.content.len() > 64 {
                return Err(invalid(&format!("{path}.content"), "exceeds 64 entries"));
            }
            // `emph` / `content` address nodes by id in the tree this variant
            // actually renders. A dangling id is a model typo worth failing on:
            // silently ignoring it would show a candidate with no highlight.
            if let Some(tree) = variant.mock.as_ref().or(self.mock.as_ref()) {
                let mut node_ids = HashSet::new();
                crate::spec_render::collect_ids(tree, &mut node_ids);
                for id in &variant.emph {
                    if !node_ids.contains(id.as_str()) {
                        return Err(invalid(
                            &format!("{path}.emph"),
                            &format!("unknown node id `{id}`"),
                        ));
                    }
                }
                for id in variant.content.keys() {
                    if !node_ids.contains(id.as_str()) {
                        return Err(invalid(
                            &format!("{path}.content"),
                            &format!("unknown node id `{id}`"),
                        ));
                    }
                }
            }
        }
        if node_count > MAX_NODES {
            return Err(invalid("mock", &format!("exceeds {MAX_NODES} node cap")));
        }
        Ok(())
    }

    /// Depth-first walk over every node in the shared tree and each variant's
    /// override. The renderer and validators both want this.
    pub fn for_each_node(&self, mut visit: impl FnMut(&MockNode)) {
        if let Some(mock) = &self.mock {
            mock.for_each_node(&mut visit);
        }
        for variant in &self.variants {
            if let Some(mock) = &variant.mock {
                mock.for_each_node(&mut visit);
            }
        }
    }
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

fn invalid(field: &str, reason: &str) -> SpecError {
    SpecError::InvalidField {
        field: field.into(),
        reason: reason.into(),
    }
}

/// The published node vocabulary — one list. The rejection text and the
/// tool description are both checked against it (see `tool_texts` and
/// `screen_spec` tests), so the model is never told a name the parser
/// rejects, nor denied a name it accepts.
pub const NODE_KINDS: &[&str] = &[
    "frame",
    "cols",
    "rows",
    "stack",
    "box",
    "nav",
    "sidebar",
    "input",
    "button",
    "swatches",
    "text",
    "placeholder",
    "kpi",
    "hero",
    "sparkline",
    "ring",
    "bars",
    "area",
    "viz",
    "phone_shell",
    "topbar",
    "tabbar",
    "toolbar",
    "dialog",
    "list_row",
    "switch",
    "segment",
    "table",
    "tree",
];

/// Allowed JSON keys per node kind. This table *is* the closed vocabulary: a
/// key that is not listed here is not a knob the schema has, no matter how
/// reasonable it looks — that is what keeps "structure and tokens in, CSS out"
/// from eroding one extra field at a time.
fn node_keys(kind: &str) -> Result<&'static [&'static str], SpecError> {
    Ok(match kind {
        "frame" => &["kind", "id", "title", "body"],
        "cols" | "rows" => &["kind", "id", "ratio", "children"],
        "stack" => &["kind", "id", "count", "child"],
        "box" => &["kind", "id", "w", "h", "text"],
        "nav" | "sidebar" => &["kind", "id", "items"],
        "input" | "button" | "placeholder" => &["kind", "id", "text"],
        "swatches" => &["kind", "id"],
        "text" => &["kind", "id", "text"],
        "kpi" => &["kind", "id", "label", "value", "delta"],
        "hero" => &["kind", "id", "text"],
        "sparkline" => &["kind", "id", "shape"],
        "ring" => &["kind", "id", "pct", "label"],
        "bars" => &["kind", "id", "items"],
        "area" => &["kind", "id", "label"],
        "viz" => &["kind", "id", "form", "label"],
        "phone_shell" => &["kind", "id", "body"],
        "topbar" => &["kind", "id", "title", "back", "action"],
        "tabbar" => &["kind", "id", "items", "active"],
        "toolbar" => &["kind", "id", "items"],
        "dialog" => &["kind", "id", "title", "text", "body", "actions"],
        "list_row" => &["kind", "id", "title", "note", "meta", "chevron", "avatar"],
        "switch" => &["kind", "id", "label", "on"],
        "segment" => &["kind", "id", "items", "active"],
        "table" => &["kind", "id", "cols", "rows"],
        "tree" => &["kind", "id", "items"],
        other => {
            return Err(invalid(
                "screen.mock",
                &format!(
                    "unknown node kind `{other}` (kinds: {})",
                    NODE_KINDS.join(", ")
                ),
            ));
        }
    })
}

/// Walk the raw JSON of every node (shared tree + each variant override),
/// rejecting keys the kind does not declare.
fn check_node_keys(value: &serde_json::Value, path: &str) -> Result<(), SpecError> {
    let obj = value
        .as_object()
        .ok_or_else(|| invalid(path, "must be a JSON object"))?;
    if let Some(mock) = obj.get("mock") {
        check_one_node(mock, &format!("{path}.mock"))?;
    }
    if let Some(variants) = obj.get("variants").and_then(|v| v.as_array()) {
        for (index, variant) in variants.iter().enumerate() {
            if let Some(mock) = variant.get("mock") {
                check_one_node(mock, &format!("{path}.variants[{index}].mock"))?;
            }
        }
    }
    Ok(())
}

fn check_one_node(value: &serde_json::Value, path: &str) -> Result<(), SpecError> {
    let obj = value
        .as_object()
        .ok_or_else(|| invalid(path, "node must be a JSON object"))?;
    let kind = obj
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid(path, "node needs a string `kind`"))?;
    let allowed = node_keys(kind).map_err(|err| match err {
        SpecError::InvalidField { reason, .. } => invalid(path, &reason),
        other => other,
    })?;
    for key in obj.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(invalid(
                path,
                &format!(
                    "unknown field `{key}` for kind `{kind}` (allowed: {})",
                    allowed.join(", ")
                ),
            ));
        }
    }
    if let Some(body) = obj.get("body") {
        check_one_node(body, &format!("{path}.body"))?;
    }
    if let Some(child) = obj.get("child") {
        check_one_node(child, &format!("{path}.child"))?;
    }
    if let Some(children) = obj.get("children").and_then(|v| v.as_array()) {
        for (index, child) in children.iter().enumerate() {
            check_one_node(child, &format!("{path}.children[{index}]"))?;
        }
    }
    if let Some(item_keys) = match kind {
        "bars" => Some(&["label", "value"][..]),
        "tree" => Some(&["label", "depth"][..]),
        _ => None,
    } && let Some(items) = obj.get("items").and_then(|v| v.as_array())
    {
        for (index, item) in items.iter().enumerate() {
            let item = item
                .as_object()
                .ok_or_else(|| invalid(&format!("{path}.items[{index}]"), "must be an object"))?;
            for key in item.keys() {
                if !item_keys.contains(&key.as_str()) {
                    return Err(invalid(
                        &format!("{path}.items[{index}]"),
                        &format!("unknown field `{key}` (allowed: {})", item_keys.join(", ")),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_node(node: &MockNode, path: &str, count: &mut usize) -> Result<(), SpecError> {
    *count += 1;
    if *count > MAX_NODES {
        return Err(invalid(path, &format!("exceeds {MAX_NODES} node cap")));
    }
    match node {
        MockNode::Frame { body, title, .. } => {
            if let Some(title) = title {
                check_copy(title, &format!("{path}.title"))?;
            }
            validate_node(body, &format!("{path}.body"), count)?;
        }
        MockNode::Cols {
            ratio, children, ..
        }
        | MockNode::Rows {
            ratio, children, ..
        } => {
            if children.is_empty() {
                return Err(invalid(path, "needs at least one child"));
            }
            if children.len() > 12 {
                return Err(invalid(path, "exceeds 12 children cap"));
            }
            if !ratio.is_empty() && ratio.len() != children.len() {
                return Err(invalid(path, "ratio length must match children"));
            }
            for (index, weight) in ratio.iter().enumerate() {
                let positive = weight
                    .parse::<f64>()
                    .is_ok_and(|number| number.is_finite() && number > 0.0);
                if !positive {
                    return Err(invalid(
                        &format!("{path}.ratio[{index}]"),
                        "ratio weights must be positive numbers (like 1 or 0.7)",
                    ));
                }
            }
            for (index, child) in children.iter().enumerate() {
                validate_node(child, &format!("{path}.children[{index}]"), count)?;
            }
        }
        MockNode::Stack {
            count: n, child, ..
        } => {
            if *n == 0 || *n > 12 {
                return Err(invalid(path, "count must be 1-12"));
            }
            validate_node(child, &format!("{path}.child"), count)?;
        }
        MockNode::Region { w, h, text, .. } => {
            check_proportion(w.as_deref(), &format!("{path}.w"))?;
            check_proportion(h.as_deref(), &format!("{path}.h"))?;
            if let Some(text) = text {
                check_copy(text, &format!("{path}.text"))?;
            }
        }
        MockNode::Nav { items, .. } | MockNode::Sidebar { items, .. } => check_items(items, path)?,
        MockNode::Input { text, .. }
        | MockNode::Button { text, .. }
        | MockNode::Placeholder { text, .. } => {
            if let Some(text) = text {
                check_copy(text, &format!("{path}.text"))?;
            }
        }
        MockNode::Swatches { .. } => {}
        MockNode::Text { text, .. } => check_copy(text, &format!("{path}.text"))?,
        MockNode::Kpi {
            label,
            value,
            delta,
            ..
        } => {
            if let Some(label) = label {
                check_copy(label, &format!("{path}.label"))?;
            }
            check_copy(value, &format!("{path}.value"))?;
            if let Some(delta) = delta {
                check_copy(delta, &format!("{path}.delta"))?;
            }
        }
        MockNode::Hero { text, .. } => check_copy(text, &format!("{path}.text"))?,
        MockNode::Sparkline { .. } => {}
        MockNode::Ring { pct, label, .. } => {
            if *pct > 100 {
                return Err(invalid(&format!("{path}.pct"), "must be 0-100"));
            }
            if let Some(label) = label {
                check_copy(label, &format!("{path}.label"))?;
            }
        }
        MockNode::Bars { items, .. } => {
            if items.is_empty() || items.len() > 10 {
                return Err(invalid(path, "items must be 1-10"));
            }
            for (index, item) in items.iter().enumerate() {
                check_copy(&item.label, &format!("{path}.items[{index}].label"))?;
            }
        }
        MockNode::Area { label, .. } | MockNode::Viz { label, .. } => {
            if let Some(label) = label {
                check_copy(label, &format!("{path}.label"))?;
            }
        }
        MockNode::PhoneShell { body, .. } => {
            validate_node(body, &format!("{path}.body"), count)?;
        }
        MockNode::Topbar {
            title,
            action,
            back,
            ..
        } => {
            if let Some(title) = title {
                check_copy(title, &format!("{path}.title"))?;
            }
            if let Some(action) = action {
                check_copy(action, &format!("{path}.action"))?;
            }
            if *back && title.is_none() {
                // A back chevron with nothing to go back to is chrome without
                // a subject; the title is what makes the row read as a screen.
                return Err(invalid(path, "a `back` topbar needs a `title`"));
            }
        }
        MockNode::Tabbar { items, active, .. } | MockNode::Segment { items, active, .. } => {
            check_items(items, path)?;
            check_active(active, items.len(), path)?;
        }
        MockNode::Toolbar { items, .. } => check_items(items, path)?,
        MockNode::Dialog {
            title,
            text,
            body,
            actions,
            ..
        } => {
            check_copy(title, &format!("{path}.title"))?;
            if let Some(text) = text {
                check_copy(text, &format!("{path}.text"))?;
            }
            if text.is_none() && body.is_none() {
                return Err(invalid(
                    path,
                    "needs `text` or `body` — a dialog says something",
                ));
            }
            if let Some(body) = body {
                validate_node(body, &format!("{path}.body"), count)?;
            }
            if actions.len() > 3 {
                return Err(invalid(path, "exceeds 3 actions cap"));
            }
            for (index, action) in actions.iter().enumerate() {
                check_copy(action, &format!("{path}.actions[{index}]"))?;
            }
        }
        MockNode::ListRow {
            title, note, meta, ..
        } => {
            check_copy(title, &format!("{path}.title"))?;
            if let Some(note) = note {
                check_copy(note, &format!("{path}.note"))?;
            }
            if let Some(meta) = meta {
                check_copy(meta, &format!("{path}.meta"))?;
            }
        }
        MockNode::Switch { label, .. } => {
            if let Some(label) = label {
                check_copy(label, &format!("{path}.label"))?;
            }
        }
        MockNode::Table { cols, rows, .. } => {
            if cols.is_empty() || cols.len() > 8 {
                return Err(invalid(path, "cols must be 1-8"));
            }
            for (index, col) in cols.iter().enumerate() {
                check_copy(col, &format!("{path}.cols[{index}]"))?;
            }
            if rows.len() > 10 {
                return Err(invalid(path, "exceeds 10 rows cap"));
            }
            for (index, row) in rows.iter().enumerate() {
                if row.len() != cols.len() {
                    return Err(invalid(
                        &format!("{path}.rows[{index}]"),
                        "cell count must match cols",
                    ));
                }
                for (cell_index, cell) in row.iter().enumerate() {
                    // Cells may be blank — a real table has empty ones.
                    check_cell(cell, &format!("{path}.rows[{index}][{cell_index}]"))?;
                }
            }
        }
        MockNode::Tree { items, .. } => {
            if items.is_empty() || items.len() > 12 {
                return Err(invalid(path, "items must be 1-12"));
            }
            for (index, item) in items.iter().enumerate() {
                check_copy(&item.label, &format!("{path}.items[{index}].label"))?;
                if let Some(depth) = item.depth
                    && depth > 4
                {
                    return Err(invalid(
                        &format!("{path}.items[{index}].depth"),
                        "must be 0-4",
                    ));
                }
            }
        }
    }
    Ok(())
}

/// A labelled item list (`nav`-family): 1-8 entries, each one line of copy.
fn check_items(items: &[String], path: &str) -> Result<(), SpecError> {
    if items.is_empty() || items.len() > 8 {
        return Err(invalid(path, "items must be 1-8"));
    }
    for (index, item) in items.iter().enumerate() {
        check_copy(item, &format!("{path}.items[{index}]"))?;
    }
    Ok(())
}

/// `active` selects an index; a dangling one would render a bar with nothing
/// chosen and teach the model nothing.
fn check_active(active: &Option<u8>, len: usize, path: &str) -> Result<(), SpecError> {
    if let Some(active) = active
        && usize::from(*active) >= len
    {
        return Err(invalid(
            &format!("{path}.active"),
            &format!("must index a real item (0-{})", len.saturating_sub(1)),
        ));
    }
    Ok(())
}

fn validate_theme(theme: &Theme, path: &str) -> Result<(), SpecError> {
    let colors = [
        ("bg", &theme.bg),
        ("panel", &theme.panel),
        ("border", &theme.border),
        ("accent", &theme.accent),
        ("accent2", &theme.accent2),
        ("text", &theme.text),
        ("text_bright", &theme.text_bright),
    ];
    for (name, value) in colors {
        if let Some(value) = value {
            check_color(value, &format!("{path}.{name}"))?;
        }
    }
    for (name, value) in [
        ("frost", theme.frost),
        ("elevation", theme.elevation),
        ("edge", theme.edge),
    ] {
        if let Some(value) = value
            && !(1..=3).contains(&value)
        {
            return Err(invalid(&format!("{path}.{name}"), "must be 1-3"));
        }
    }
    if let Some(glow) = theme.glow
        && glow > 2
    {
        return Err(invalid(&format!("{path}.glow"), "must be 0-2"));
    }
    Ok(())
}

/// Slot copy: one line, bounded, no control characters.
fn check_copy(text: &str, field: &str) -> Result<(), SpecError> {
    if text.is_empty() {
        return Err(invalid(field, "must be non-empty"));
    }
    check_cell(text, field)
}

/// A cell of mock data: same bounds as [`check_copy`], but blank is allowed —
/// a real table has empty cells.
fn check_cell(text: &str, field: &str) -> Result<(), SpecError> {
    if text.chars().any(char::is_control) {
        return Err(invalid(field, "must not contain control characters"));
    }
    if text.chars().count() > 120 {
        return Err(invalid(field, "exceeds 120 char cap"));
    }
    Ok(())
}

/// Structural proportion: `NN%`, `NN`, `N.N`, or the bare number the model
/// may send instead. Not CSS — in an open-tracked band the renderer turns
/// this into the child's grid track; elsewhere it sizes the box.
fn check_proportion(value: Option<&str>, field: &str) -> Result<(), SpecError> {
    let Some(value) = value else {
        return Ok(());
    };
    let number = value.trim().strip_suffix('%').unwrap_or(value.trim());
    let ok = number
        .parse::<f32>()
        .map(|n| n.is_finite() && n > 0.0 && n <= 100.0)
        .unwrap_or(false);
    if !ok {
        return Err(invalid(
            field,
            "must be a positive number up to 100, optionally with `%`",
        ));
    }
    Ok(())
}

/// A color token: `#rgb` / `#rrggbb` / `#rrggbbaa`, or a CSS `rgb()/rgba()/hsl()/hsla()`
/// triple. Kept strict because these values are interpolated into a `style`
/// attribute: any quote, semicolon or brace would be style smuggling.
fn check_color(value: &str, field: &str) -> Result<(), SpecError> {
    let trimmed = value.trim();
    let charset_ok = trimmed.len() <= 64
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "#%.,() +-".contains(c));
    let ok = charset_ok
        && if let Some(hex) = trimmed.strip_prefix('#') {
            let len = hex.len();
            (len == 3 || len == 4 || len == 6 || len == 8)
                && hex.chars().all(|c| c.is_ascii_hexdigit())
        } else {
            let lower = trimmed.to_ascii_lowercase();
            (lower.starts_with("rgb(")
                || lower.starts_with("rgba(")
                || lower.starts_with("hsl(")
                || lower.starts_with("hsla("))
                && trimmed.ends_with(')')
        };
    if !ok {
        return Err(invalid(
            field,
            "must be #rgb/#rrggbb/#rrggbbaa or rgb()/hsl() notation",
        ));
    }
    Ok(())
}
