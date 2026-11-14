//! Bounds and shape checks: the per-kind key tables, copy/color/proportion
//! checks, and `NODE_KINDS` — the published vocabulary the tool text and the
//! parser are both pinned to.

use super::types::{MAX_NODES, MockNode, SpecError, Theme};

pub(super) fn invalid(field: &str, reason: &str) -> SpecError {
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
pub(super) fn check_node_keys(value: &serde_json::Value, path: &str) -> Result<(), SpecError> {
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

pub(super) fn validate_node(
    node: &MockNode,
    path: &str,
    count: &mut usize,
) -> Result<(), SpecError> {
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
                check_bar_value(item.value, &format!("{path}.items[{index}].value"))?;
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

pub(super) fn validate_theme(theme: &Theme, path: &str) -> Result<(), SpecError> {
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

/// A bar magnitude: any finite number, zero allowed (a real zero bar).
/// Relative only — the renderer divides by the max — so there is no cap,
/// just nothing negative or non-finite.
fn check_bar_value(value: Option<f64>, field: &str) -> Result<(), SpecError> {
    match value {
        Some(value) if !value.is_finite() || value < 0.0 => {
            Err(invalid(field, "must be a non-negative number"))
        }
        _ => Ok(()),
    }
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
