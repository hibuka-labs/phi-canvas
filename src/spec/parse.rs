//! `ScreenSpec::parse`: the entry from raw JSON, and the validation walk that
//! runs after a successful typed parse.

use std::collections::HashSet;

use super::de::parse_tracked;
use super::diagnostics::{error_offset, excerpt_around, json_kind, refine_or_malformed};
use super::types::{MAX_NODES, MockNode, ScreenSpec, SpecError};
use super::validate::{check_node_keys, invalid, validate_node, validate_theme};

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
