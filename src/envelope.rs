//! Screen envelope: the structured metadata `canvas_push` carries alongside
//! the screen body. Status-bar counts, click→label mapping, and multiselect
//! semantics all read from here — never from scraping the HTML.
//!
//! Two ways to describe the body, exactly one of them used per screen:
//! - `screen` — a [`ScreenSpec`]: structure + appearance tokens, rendered host-
//!   side. This is the default path (cheaper tokens, field-level version diff).
//! - `body_html` — raw markup, the escape hatch for screens the vocabulary
//!   cannot express. Spliced in verbatim.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::spec::{ScreenSpec, SpecError};

/// Hard cap on `body_html` size. The line-count target in the product spec is
/// a token-cost guideline, not a budget; this cap only bounds disk/memory abuse.
pub const MAX_BODY_HTML_BYTES: usize = 256 * 1024;

/// Maximum length of an option id (ASCII, HTML-attribute-safe charset).
pub const MAX_OPTION_ID_LEN: usize = 64;

/// Maximum length of the single-line text fields (title / subtitle / label).
/// They surface in one-line tool return texts and the frame chrome.
pub const MAX_TEXT_LEN: usize = 300;

/// Maximum length of the `escape` tag: one or two words naming the missing
/// shape, not a sentence about it.
pub const MAX_ESCAPE_TAG_LEN: usize = 40;

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

/// The reference image shown beside the candidates ("参考物贴片").
///
/// The contract is **one PNG and a name for where it is** — sources plug in
/// under that and nothing here is tied to how the image was produced: a file
/// on disk the host copies beside the screen, an http(s) URL the page loads
/// itself, or an inline `data:` URI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    /// A filesystem path the host can read, or an http(s) / `data:` URI.
    pub src: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// Host-set: what the page actually loads. A local file is copied into the
    /// session store and served from there, so the page never reaches at the
    /// user's disk and a rebind still finds the bytes. `None` means the image
    /// could not be resolved — the screen renders without it and the tool text
    /// says so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<String>,
}

/// Hard cap on `reference.src`. Paths and URLs are tiny; the binding case is
/// an inline `data:` URI, which is the whole image in base64.
pub const MAX_REFERENCE_SRC_BYTES: usize = 1024 * 1024;

/// Everything a screen needs besides its body.
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
    /// Structured alternative to `body_html`. Mutually exclusive with it —
    /// see [`ScreenEnvelope::parse`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<ScreenSpec>,
    /// A reference image the candidates are judged against. Independent of
    /// `screen` / `body_html` — it is context, not the body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<Reference>,
    #[serde(default)]
    pub multiselect: bool,
    /// One or two words naming the shape that forced a `body_html` escape
    /// ("table", "phone-frame", "timeline"). Bookkeeping only — never rendered,
    /// never shown. This is the ledger: the same tag showing up twice is the
    /// evidence for what the closed vocabulary should grow into, which is why
    /// it is a field the model fills rather than something inferred from the
    /// markup afterwards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escape: Option<String>,
    /// Host-set: the screen is interstitial chrome between design screens —
    /// the waiting screen the host pushes at turn end — not design content.
    /// It gets a seq and a page like any other screen, but it is not a
    /// version: it stays out of the version rail and the compare pickers,
    /// so the history the user scrolls is the design, not the pauses.
    ///
    /// Never comes from the model: [`ScreenEnvelope::parse`] clears it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub interstitial: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
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
    #[error(transparent)]
    Spec(#[from] SpecError),
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
        // `screen` goes through ScreenSpec::parse — node-aware (closed
        // vocabulary, validation, and a type error naming the field) — instead
        // of the envelope derive, which can only stop at `screen.mock` because
        // a tagged node enum buffers its content. Everything else gets the
        // tracked parse so type errors name their field path too.
        let mut rest = value.clone();
        let screen = match rest.as_object_mut().and_then(|obj| obj.remove("screen")) {
            Some(raw) if !raw.is_null() => Some(ScreenSpec::parse(&raw)?),
            _ => None,
        };
        let mut env: ScreenEnvelope = crate::spec::parse_tracked(&rest)
            .map_err(|err| EnvelopeError::Malformed(err.to_string()))?;
        env.screen = screen;
        // Host-set fields are not the model's to declare, even if it spells
        // them out. A tool call cannot park itself out of the version rail.
        env.interstitial = false;
        env.validate()?;
        Ok(env)
    }

    /// A screen with no options is for looking at only.
    pub fn is_display_only(&self) -> bool {
        self.options.is_empty()
    }

    /// How the screen body is authored, for tool text and diagnostics.
    pub fn has_spec(&self) -> bool {
        self.screen.is_some()
    }

    /// Replace `body_html` with the host rendering of `screen`, if present.
    ///
    /// Called once on the push path. The spec stays on the envelope so the
    /// sidecar can serve field-level version comparison later (gate 2 ③),
    /// while the store keeps writing one HTML fragment per screen — store,
    /// rebind, and serve paths are unchanged.
    ///
    /// Returns `true` when the model handed in *both* `screen` and
    /// `body_html`: the spec wins and the caller is expected to say so in the
    /// tool text rather than silently dropping the model's markup.
    pub fn materialize_screen(&mut self) -> Result<bool, SpecError> {
        let Some(spec) = self.screen.clone() else {
            return Ok(false);
        };
        let both = !self.body_html.is_empty();
        let rendered = crate::spec_render::render_spec(&spec, &self.options);
        if rendered.len() > MAX_BODY_HTML_BYTES {
            return Err(SpecError::InvalidField {
                field: "screen".into(),
                reason: format!("renders past the {MAX_BODY_HTML_BYTES} byte cap"),
            });
        }
        self.body_html = rendered;
        Ok(both)
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
        if let Some(escape) = &self.escape {
            if self.screen.is_some() {
                return Err(EnvelopeError::InvalidField {
                    field: "escape".into(),
                    reason: "is the tag for a `body_html` escape; drop it on a `screen`".into(),
                });
            }
            validate_single_line_text("escape", escape)?;
            if escape.chars().count() > MAX_ESCAPE_TAG_LEN {
                return Err(EnvelopeError::InvalidField {
                    field: "escape".into(),
                    reason: format!(
                        "exceeds {MAX_ESCAPE_TAG_LEN} char cap — it is a tag, not a note"
                    ),
                });
            }
        }
        if let Some(reference) = &self.reference {
            if reference.src.trim().is_empty() {
                return Err(EnvelopeError::InvalidField {
                    field: "reference.src".into(),
                    reason: "must be non-empty".into(),
                });
            }
            if reference.src.len() > MAX_REFERENCE_SRC_BYTES {
                return Err(EnvelopeError::InvalidField {
                    field: "reference.src".into(),
                    reason: format!("exceeds {MAX_REFERENCE_SRC_BYTES} byte cap"),
                });
            }
            if reference.src.chars().any(char::is_control) {
                return Err(EnvelopeError::InvalidField {
                    field: "reference.src".into(),
                    reason: "must not contain control characters".into(),
                });
            }
            if let Some(caption) = &reference.caption {
                validate_single_line_text("reference.caption", caption)?;
            }
        }
        if let Some(spec) = &self.screen {
            // Variants name the candidates they picture. A variant that no
            // option claims would be a picture nobody can pick.
            for (index, variant) in spec.variants.iter().enumerate() {
                if !self.options.is_empty()
                    && !self.options.iter().any(|option| option.id == variant.id)
                {
                    return Err(EnvelopeError::InvalidField {
                        field: format!("screen.variants[{index}].id"),
                        reason: format!(
                            "`{}` matches no option id ({:?})",
                            variant.id,
                            self.options
                                .iter()
                                .map(|option| option.id.as_str())
                                .collect::<Vec<_>>()
                        ),
                    });
                }
            }
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
