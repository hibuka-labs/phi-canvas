//! Screen envelope: the structured metadata `canvas_push` carries alongside
//! `body_html`. Status-bar counts, click→label mapping, and multiselect
//! semantics all read from here — never from scraping the HTML.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Hard cap on `body_html` size. The line-count target in the product spec is
/// a token-cost guideline, not a budget; this cap only bounds disk/memory abuse.
pub const MAX_BODY_HTML_BYTES: usize = 256 * 1024;

/// Maximum length of an option id (ASCII, HTML-attribute-safe charset).
pub const MAX_OPTION_ID_LEN: usize = 64;

/// Maximum length of the single-line text fields (title / subtitle / label).
/// They surface in one-line tool return texts and the frame chrome.
pub const MAX_TEXT_LEN: usize = 300;

/// One selectable option on a screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionItem {
    /// Stable id echoed back in click events. Restricted charset: the host
    /// renders it into `data-choice` attributes and maps events back to labels.
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Everything a screen needs besides the HTML body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenEnvelope {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Empty vec = display-only screen: shown for looking at, clicks carry no
    /// selection semantics.
    pub options: Vec<OptionItem>,
    /// Kept out of sidecar metadata on disk (stored as its own fragment file).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body_html: String,
    #[serde(default)]
    pub multiselect: bool,
}

/// Why an envelope was rejected.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum EnvelopeError {
    #[error("envelope must be a JSON object")]
    NotAnObject,
    #[error("missing required field `{0}`")]
    MissingField(&'static str),
    #[error("invalid envelope: {0}")]
    Malformed(String),
    #[error("invalid field `{field}`: {reason}")]
    InvalidField { field: String, reason: String },
}

impl ScreenEnvelope {
    /// Parse tool-call arguments into an envelope, enforcing the field contract.
    pub fn parse(value: &Value) -> Result<Self, EnvelopeError> {
        let obj = value.as_object().ok_or(EnvelopeError::NotAnObject)?;
        for required in ["title", "options"] {
            if !obj.contains_key(required) {
                return Err(EnvelopeError::MissingField(required));
            }
        }
        let env: ScreenEnvelope = serde_json::from_value(value.clone())
            .map_err(|err| EnvelopeError::Malformed(err.to_string()))?;
        env.validate()?;
        Ok(env)
    }

    /// A screen with no options is for looking at only.
    pub fn is_display_only(&self) -> bool {
        self.options.is_empty()
    }

    fn validate(&self) -> Result<(), EnvelopeError> {
        validate_single_line_text("title", &self.title)?;
        if let Some(subtitle) = &self.subtitle {
            validate_single_line_text("subtitle", subtitle)?;
        }
        let mut seen = HashSet::new();
        for (index, option) in self.options.iter().enumerate() {
            if !is_valid_option_id(&option.id) {
                return Err(EnvelopeError::InvalidField {
                    field: format!("options[{index}].id"),
                    reason: format!(
                        "must be 1-{MAX_OPTION_ID_LEN} chars of [A-Za-z0-9_-], got {:?}",
                        option.id
                    ),
                });
            }
            if !seen.insert(option.id.as_str()) {
                return Err(EnvelopeError::InvalidField {
                    field: format!("options[{index}].id"),
                    reason: format!("duplicate id `{}`", option.id),
                });
            }
            validate_single_line_text(&format!("options[{index}].label"), &option.label)?;
        }
        if self.body_html.len() > MAX_BODY_HTML_BYTES {
            return Err(EnvelopeError::InvalidField {
                field: "body_html".into(),
                reason: format!("exceeds {MAX_BODY_HTML_BYTES} byte cap"),
            });
        }
        Ok(())
    }
}

fn validate_single_line_text(field: &str, text: &str) -> Result<(), EnvelopeError> {
    if text.trim().is_empty() {
        return Err(EnvelopeError::InvalidField {
            field: field.into(),
            reason: "must be non-empty".into(),
        });
    }
    if text.chars().any(char::is_control) {
        return Err(EnvelopeError::InvalidField {
            field: field.into(),
            reason: "must not contain control characters".into(),
        });
    }
    if text.chars().count() > MAX_TEXT_LEN {
        return Err(EnvelopeError::InvalidField {
            field: field.into(),
            reason: format!("exceeds {MAX_TEXT_LEN} char cap"),
        });
    }
    Ok(())
}

fn is_valid_option_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_OPTION_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
