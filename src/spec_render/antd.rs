//! The antd face: mapped leaves become hydrate slots carrying the binding
//! table's component names and resolved props, so the runtime never sees the
//! mock schema and the layout tree never sees the library.

use std::collections::HashSet;

use crate::spec::MockNode;

use super::{RenderCtx, escape_attr};

/// The wire kind name (`box`, not `Region`) — read from the same serde tag the
/// parser used, so the two can never drift.
fn node_kind(node: &MockNode) -> Option<String> {
    serde_json::to_value(node)
        .ok()?
        .get("kind")?
        .as_str()
        .map(str::to_string)
}
/// One antd island: the div the bundled runtime hydrates (`runtime/src/main.jsx`
/// mounts every `.m-antd-slot[data-antd]`). The payload is the binding table's
/// component name plus resolved props — the JS adapter never sees the mock
/// schema, and the layout tree never sees the library. `None` means greybox.
pub(crate) fn antd_slot(node: &MockNode, ctx: &RenderCtx) -> Option<String> {
    if !ctx.antd {
        return None;
    }
    let kind = node_kind(node)?;
    let component = crate::antd_map::component_for(&kind)?;
    let props = slot_props(node, ctx)?;
    let emph = node_id(node)
        .map(|id| ctx.emph.contains(id))
        .unwrap_or(false);
    let payload = serde_json::json!({
        "kind": kind,
        "component": component,
        "props": props,
        "emph": emph,
    });
    let class = if emph {
        "m-antd-slot m-emph"
    } else {
        "m-antd-slot"
    };
    Some(format!(
        "<div class=\"{class}\" data-antd-kind=\"{kind}\" data-antd='{}'></div>",
        escape_attr(&payload.to_string()),
    ))
}
/// Recipe input for one island: the node's fields with `content` overrides
/// applied exactly where the greybox renderer applies them (the primary copy
/// slot), so the two faces never disagree on what the user meant to say.
/// `None` when the node's shape has no honest island form (a `dialog` carrying
/// a nested body node — the body is mock structure, not card content).
fn slot_props(node: &MockNode, ctx: &RenderCtx) -> Option<serde_json::Value> {
    use serde_json::json;

    let override_text = node_id(node).and_then(|id| ctx.content.get(id)).cloned();
    let text_of = |own: &str| override_text.clone().unwrap_or_else(|| own.to_string());
    Some(match node {
        MockNode::Input { text, .. } | MockNode::Button { text, .. } => json!({
            "text": text.as_deref().map(text_of),
        }),
        MockNode::Nav { items, .. }
        | MockNode::Sidebar { items, .. }
        | MockNode::Toolbar { items, .. } => json!({
            "items": items,
        }),
        MockNode::Kpi {
            label,
            value,
            delta,
            ..
        } => json!({
            "label": label,
            "value": text_of(value),
            "delta": delta,
        }),
        MockNode::Hero { text, .. } => json!({
            "text": text_of(text),
        }),
        MockNode::Ring { pct, label, .. } => json!({
            "pct": pct,
            "label": label.as_deref().map(text_of),
        }),
        MockNode::Bars { items, .. } => json!({
            "items": items
                .iter()
                .map(|item| json!({ "label": item.label, "value": item.value }))
                .collect::<Vec<_>>(),
        }),
        MockNode::Dialog {
            title,
            text,
            body,
            actions,
            ..
        } => {
            if body.is_some() {
                return None;
            }
            // Greybox semantics: the message is the primary slot; the title is
            // rewritten only when there is no message to carry the override.
            json!({
                "title": if text.is_some() { title.clone() } else { text_of(title) },
                "text": text.as_deref().map(text_of),
                "actions": actions,
            })
        }
        MockNode::ListRow {
            title,
            note,
            meta,
            chevron,
            avatar,
            ..
        } => json!({
            "title": text_of(title),
            "note": note,
            "meta": meta,
            "chevron": chevron,
            "avatar": avatar,
        }),
        MockNode::Switch { label, on, .. } => json!({
            "label": label.as_deref().map(text_of),
            "on": on,
        }),
        MockNode::Segment { items, active, .. } => json!({
            "items": items,
            "active": active,
        }),
        MockNode::Table { cols, rows, .. } => json!({
            "cols": cols,
            "rows": rows,
        }),
        MockNode::Tree { items, .. } => json!({
            "items": items
                .iter()
                .map(|item| json!({ "label": item.label, "depth": item.depth }))
                .collect::<Vec<_>>(),
        }),
        // Mapped kinds only reach this function (`antd_slot` looks the kind
        // up first); a new map entry without a recipe arm falls back to
        // greybox rather than rendering a hollow island.
        _ => return None,
    })
}
pub fn node_id(node: &MockNode) -> Option<&str> {
    match node {
        MockNode::Frame { id, .. }
        | MockNode::Cols { id, .. }
        | MockNode::Rows { id, .. }
        | MockNode::Stack { id, .. }
        | MockNode::Region { id, .. }
        | MockNode::Nav { id, .. }
        | MockNode::Sidebar { id, .. }
        | MockNode::Input { id, .. }
        | MockNode::Button { id, .. }
        | MockNode::Swatches { id, .. }
        | MockNode::Text { id, .. }
        | MockNode::Placeholder { id, .. }
        | MockNode::Kpi { id, .. }
        | MockNode::Hero { id, .. }
        | MockNode::Sparkline { id, .. }
        | MockNode::Ring { id, .. }
        | MockNode::Bars { id, .. }
        | MockNode::Area { id, .. }
        | MockNode::Viz { id, .. }
        | MockNode::PhoneShell { id, .. }
        | MockNode::Topbar { id, .. }
        | MockNode::Tabbar { id, .. }
        | MockNode::Toolbar { id, .. }
        | MockNode::Dialog { id, .. }
        | MockNode::ListRow { id, .. }
        | MockNode::Switch { id, .. }
        | MockNode::Segment { id, .. }
        | MockNode::Table { id, .. }
        | MockNode::Tree { id, .. } => id.as_deref(),
    }
}
/// Collect every node id in one tree. Used by validation to reject ambiguous
/// or dangling `emph` / `content` references.
pub fn collect_ids(node: &MockNode, out: &mut HashSet<String>) {
    if let Some(id) = node_id(node) {
        out.insert(id.to_string());
    }
    match node {
        MockNode::Frame { body, .. }
        | MockNode::PhoneShell { body, .. }
        | MockNode::Dialog {
            body: Some(body), ..
        } => collect_ids(body, out),
        MockNode::Cols { children, .. } | MockNode::Rows { children, .. } => {
            for child in children {
                collect_ids(child, out);
            }
        }
        MockNode::Stack { child, .. } => collect_ids(child, out),
        _ => {}
    }
}
