//! The mapping layer's data half: mock `kind` → antd component binding.
//!
//! 「形状归词表，质感归组件库」 — the vocabulary stays library-neutral, the
//! library face lives here. This table is data on purpose: swapping the
//! library swaps this table plus the JS adapter recipe file
//! (`runtime/src/main.jsx`), never the spec, the renderer core, or the
//! server. The judgment bar for the whole P1 design: if a swap needs a change
//! to `spec.rs` / the layout tree / `store` / `server`, the ceiling got welded
//! shut.
//!
//! Two declared groups cover the closed vocabulary (`crate::spec::NODE_KINDS`):
//!
//! - [`ANTD_MAP`]: leaf components with a real antd face — these render as
//!   hydrated islands (see `m-antd-slot` in [`crate::spec_render`]);
//! - [`GREYBOX_KINDS`]: layout structure, copy, device chrome and decorative
//!   chart shapes — shape is the vocabulary's job (grid/CSS), some have no
//!   antd equivalent at all (phone chrome, swatches, decorative spark shapes).
//!
//! The reconciliation test keeps the three lists in lockstep: every
//! `NODE_KINDS` entry lands in exactly one group, so nothing can silently
//! lose its face.

/// kind → antd component (dotted paths allowed, e.g. `List.Item`).
pub const ANTD_MAP: &[(&str, &str)] = &[
    ("nav", "Menu"),
    ("sidebar", "Menu"),
    ("input", "Input"),
    ("button", "Button"),
    // The admin-dashboard tile: `Card` carries the face and the recipe puts a
    // `Statistic` inside it — a bare number did not read as antd (live note).
    ("kpi", "Card"),
    ("hero", "Statistic"),
    ("ring", "Progress"),
    ("bars", "Progress"),
    // Inline furniture inside a variant card — a real `Modal` would float over
    // the whole canvas. `Card` carries the dialog silhouette honestly.
    ("dialog", "Card"),
    ("list_row", "List.Item"),
    ("switch", "Switch"),
    ("segment", "Segmented"),
    ("table", "Table"),
    ("tree", "Tree"),
    // A tool strip of buttons — the action row an admin screen always has.
    ("toolbar", "Space"),
];

/// Kinds that keep the `m-*` mock renderer on every fidelity. Layout nodes
/// are the shape layer; copy is copy; chart shapes carry no data to chart
/// with; phone chrome is the antd-mobile instance's job (scope matrix).
pub const GREYBOX_KINDS: &[&str] = &[
    "frame",
    "cols",
    "rows",
    "stack",
    "box",
    "text",
    "placeholder",
    "swatches",
    "sparkline",
    "area",
    "viz",
    "phone_shell",
    "topbar",
    "tabbar",
];

/// The antd component bound to a kind, or `None` when the kind is greybox.
pub fn component_for(kind: &str) -> Option<&'static str> {
    ANTD_MAP
        .iter()
        .find(|(name, _)| *name == kind)
        .map(|(_, component)| *component)
}

/// Whether a kind hydrates as an antd island when the screen is on the antd
/// path (fidelity `color`/`texture`, wide-form — see `spec_render`).
pub fn is_antd_kind(kind: &str) -> bool {
    component_for(kind).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::NODE_KINDS;

    /// The three lists are one partition of the published vocabulary: a new
    /// node kind must land in exactly one group, and a map entry cannot name
    /// a kind the parser rejects.
    #[test]
    fn map_and_greybox_partition_node_kinds() {
        for kind in NODE_KINDS {
            let in_map = component_for(kind).is_some();
            let in_greybox = GREYBOX_KINDS.contains(kind);
            assert!(
                in_map ^ in_greybox,
                "kind `{kind}` must be in exactly one of ANTD_MAP / GREYBOX_KINDS"
            );
        }
        for (kind, component) in ANTD_MAP {
            assert!(
                NODE_KINDS.contains(kind),
                "ANTD_MAP names unknown kind `{kind}`"
            );
            assert!(!component.is_empty(), "ANTD_MAP `{kind}` has empty binding");
        }
        for kind in GREYBOX_KINDS {
            assert!(
                NODE_KINDS.contains(kind),
                "GREYBOX_KINDS names unknown kind `{kind}`"
            );
        }
    }

    #[test]
    fn dotted_paths_resolve_shape() {
        assert_eq!(component_for("list_row"), Some("List.Item"));
        assert_eq!(component_for("table"), Some("Table"));
        assert_eq!(component_for("frame"), None);
    }
}
