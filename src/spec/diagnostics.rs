//! Turning a serde failure into a rejection that names the field and the
//! fumble: tracked-path refinement through node trees, teaching text for a node
//! in a string slot, and the excerpt around a stringified blob's break.

use super::de::{FieldError, parse_tracked};
use super::types::{MockNode, SpecError};
use super::validate::invalid;

/// A type error at a node boundary — `mock` or `variants[N].mock` — is all
/// serde can report: an internally-tagged enum buffers its content, so the
/// tracked path stops at the node and the field is lost. Name it.
pub(super) fn refine_or_malformed(root: &serde_json::Value, err: &FieldError) -> SpecError {
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
pub(super) fn json_kind(value: &serde_json::Value) -> &'static str {
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
pub(super) fn error_offset(text: &str, err: &serde_json::Error) -> usize {
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
pub(super) fn excerpt_around(text: &str, offset: usize) -> String {
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
