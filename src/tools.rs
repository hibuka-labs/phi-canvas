//! The three canvas tools: schema handoff plus the locked M3 return texts.
//!
//! This is mechanism only. The host adapts these into its tool registry and
//! owns everything with product semantics: the `y` consent gate, actually
//! opening a browser (`BrowserLauncher`), the click mailbox (M2), and status
//! bar wording (1C).
//!
//! Lifecycle contract (M1 + M3):
//! - `canvas_push` on a not-open canvas implicitly opens one — but the
//!   implicit open only binds the server. A browser must never appear without
//!   the user's consent, so the launcher is never consulted from `push`.
//! - `canvas_open` is idempotent while the canvas is up: same URL, no rebind,
//!   no second launch. After a close it rebinds (port 0 ⇒ new port) and asks
//!   the launcher again — that is the documented recovery path.
//! - `canvas_close` is graceful and idempotent; the listener keeps serving a
//!   farewell screen until the host calls `Canvas::shutdown`.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;
use thiserror::Error;
use tokio::sync::Mutex;

use crate::envelope::{EnvelopeError, ScreenEnvelope};
use crate::server::{Canvas, CanvasConfig, CanvasError, CloseReason};

pub const TOOL_OPEN: &str = "canvas_open";
pub const TOOL_PUSH: &str = "canvas_push";
pub const TOOL_CLOSE: &str = "canvas_close";

/// The locked `canvas_close` return text (M3).
pub const TEXT_CLOSED: &str = "canvas: closed";

/// What the host did about showing the URL to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchDecision {
    /// The browser (or equivalent) now shows the canvas URL.
    Launched,
    /// Nothing was launched yet — the consent gate is still pending (M3).
    AwaitingConsent,
}

/// Host seam for browser launch. The `y` consent gate and platform quirks
/// (focus stealing, WSL, headless fallback) live behind this — phi-canvas
/// only formats the resulting state into tool text.
pub trait BrowserLauncher: Send + Sync {
    fn launch(&self, url: &str) -> LaunchDecision;
}

/// Schema + description for the host's tool registry. `schema` is the
/// parameters object (the shape `agent-base`'s `Tool::schema` returns).
pub struct ToolDescriptor {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Value,
}

/// Tool-call failure to hand back to the model as a tool error.
#[derive(Debug, Error)]
pub enum ToolCallError {
    #[error(transparent)]
    Canvas(#[from] CanvasError),
    #[error(transparent)]
    Envelope(#[from] EnvelopeError),
}

struct ToolState {
    canvas: Option<Arc<Canvas>>,
    decision: LaunchDecision,
}

/// Process-wide tool state for one session: the live canvas plus how the URL
/// was last delivered to the user.
pub struct CanvasTools {
    config: CanvasConfig,
    state: Mutex<ToolState>,
    launcher: Option<Arc<dyn BrowserLauncher>>,
}

impl CanvasTools {
    pub fn new(store_root: impl Into<PathBuf>, session_id: impl Into<String>) -> Self {
        Self {
            config: CanvasConfig::new(store_root, session_id),
            state: Mutex::new(ToolState {
                canvas: None,
                decision: LaunchDecision::AwaitingConsent,
            }),
            launcher: None,
        }
    }

    /// Wire the host's launch/consent seam. Without it, open reports
    /// `awaiting consent` and never touches a browser.
    pub fn with_launcher(mut self, launcher: Arc<dyn BrowserLauncher>) -> Self {
        self.launcher = Some(launcher);
        self
    }

    /// `canvas_open`: bind (or report) the canvas URL. Idempotent while the
    /// canvas is up; after a close or a crash this rebinds to a fresh port
    /// (M1). The launcher is consulted on every (re)bind — the browser waits
    /// for the user's yes even here (M3).
    pub async fn open(&self) -> Result<String, ToolCallError> {
        let (text, _, _) = self.bind(true).await?;
        Ok(text)
    }

    /// Host recovery path (Ctrl+B, 1C row 16): bind a fresh server after a
    /// close/crash (or report the live one) and return its URL — deliberately
    /// without consulting the launcher. Ctrl+B is the user's own request; the
    /// host shows the browser itself. Never call this from a tool.
    pub async fn rebind(&self) -> Result<String, ToolCallError> {
        let (_, url, _) = self.bind(false).await?;
        Ok(url)
    }

    /// Host seam: the consent gate resolves asynchronously, so the host
    /// reports the outcome here to keep the `canvas_open` return text honest
    /// for later calls on the same live canvas (M3).
    pub async fn set_launch_decision(&self, decision: LaunchDecision) {
        self.state.lock().await.decision = decision;
    }

    /// Bind a live canvas (idempotent) or open one. Returns the locked M3
    /// tool text, the URL, and whether this call actually bound a new server.
    async fn bind(&self, consult_launcher: bool) -> Result<(String, String, bool), ToolCallError> {
        let mut state = self.state.lock().await;
        if let Some(canvas) = &state.canvas
            && is_live(canvas)
        {
            return Ok((
                text_open(&canvas.url(), state.decision),
                canvas.url(),
                false,
            ));
        }
        let canvas = Canvas::open(self.config.clone()).await?;
        let decision = if consult_launcher {
            match &self.launcher {
                Some(launcher) => launcher.launch(&canvas.url()),
                None => LaunchDecision::AwaitingConsent,
            }
        } else {
            // Host-driven rebind: the caller owns the launch decision.
            state.decision
        };
        let text = text_open(&canvas.url(), decision);
        let url = canvas.url();
        state.canvas = Some(canvas);
        state.decision = decision;
        Ok((text, url, true))
    }

    /// `canvas_push`: parse the M3 envelope and show one screen. Implicitly
    /// opens (binds only — never launches) when no live canvas exists.
    pub async fn push(&self, args: &Value) -> Result<String, ToolCallError> {
        let env = ScreenEnvelope::parse(args)?;
        let mut state = self.state.lock().await;
        let live = state.canvas.as_ref().is_some_and(|canvas| is_live(canvas));
        if !live {
            state.canvas = Some(Canvas::open(self.config.clone()).await?);
            state.decision = LaunchDecision::AwaitingConsent;
        }
        let canvas = state
            .canvas
            .as_ref()
            .expect("live canvas ensured above")
            .clone();
        let receipt = canvas.push(env).await?;
        Ok(if receipt.display_only {
            text_display_only(&receipt.url, &receipt.title)
        } else {
            text_awaiting_selection(&receipt.url, &receipt.title, receipt.option_count)
        })
    }

    /// `canvas_close`: graceful close, idempotent (M1/M3).
    pub async fn close(&self) -> Result<String, ToolCallError> {
        let state = self.state.lock().await;
        if let Some(canvas) = &state.canvas
            && canvas.is_open()
        {
            canvas.close(CloseReason::UserClosed).await?;
        }
        Ok(TEXT_CLOSED.to_string())
    }

    /// Host access: the click mailbox subscribes here (M2), and session exit
    /// calls `shutdown` on the handle (M1).
    pub async fn canvas(&self) -> Option<Arc<Canvas>> {
        self.state.lock().await.canvas.clone()
    }
}

/// A canvas is usable while it is open *and* its server task is up — a crash
/// (task gone, no close recorded) must rebind like a graceful close does (M1).
fn is_live(canvas: &Canvas) -> bool {
    canvas.is_open() && !canvas.is_failed()
}

/// Locked `canvas_open` texts (M3).
pub fn text_open(url: &str, decision: LaunchDecision) -> String {
    match decision {
        LaunchDecision::Launched => format!("canvas: {url} | state: open | browser launched"),
        LaunchDecision::AwaitingConsent => {
            format!("canvas: {url} | state: open | awaiting consent")
        }
    }
}

/// Locked `canvas_push` text for a screen with options (M3).
pub fn text_awaiting_selection(url: &str, title: &str, option_count: usize) -> String {
    format!("canvas: {url} | screen: {title} | options: {option_count} | awaiting selection")
}

/// Locked `canvas_push` text for a display-only screen (M3).
pub fn text_display_only(url: &str, title: &str) -> String {
    format!("canvas: {url} | screen: {title} | display only")
}

/// The three tool descriptors: names, descriptions, and parameter schemas.
pub fn tool_descriptors() -> Vec<ToolDescriptor> {
    vec![
        ToolDescriptor {
            name: TOOL_OPEN,
            description: "Open the design canvas in the user's browser. Use when the next \
                question is visual (layout, styling, comparison) and a picture would be \
                clearer than words. Requires the user's consent before a browser appears; \
                the return text says whether it launched or is still awaiting consent. \
                Idempotent while the canvas is open — calling it again returns the same URL.",
            schema: serde_json::json!({
                "type": "object",
                "properties": {},
            }),
        },
        ToolDescriptor {
            name: TOOL_PUSH,
            description: "Push one design screen to the canvas and wait for the user's pick. \
                `title` states the question in one line; `options` are the candidates \
                (`id` echoed back on click, `label` shown); `body_html` is a small HTML \
                fragment for mockups/comparisons (optionally with `data-choice` attributes \
                for spatial picks); `options: []` makes it a display-only screen; \
                `multiselect: true` adds confirm/skip instead of single-select. Opening is \
                implicit (server only — the browser waits for the user's consent).",
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "The question this screen asks, one line."
                    },
                    "subtitle": {
                        "type": "string",
                        "description": "Optional one-line context under the title."
                    },
                    "options": {
                        "type": "array",
                        "description": "Candidates to pick from. Empty = display-only screen.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {
                                    "type": "string",
                                    "description": "Stable id ([A-Za-z0-9_-]), echoed back on click."
                                },
                                "label": {
                                    "type": "string",
                                    "description": "Short label shown in the options panel."
                                },
                                "note": {
                                    "type": "string",
                                    "description": "Optional one-line elaboration under the label."
                                }
                            },
                            "required": ["id", "label"]
                        }
                    },
                    "body_html": {
                        "type": "string",
                        "description": "Optional HTML fragment (mockups, comparison cards)."
                    },
                    "multiselect": {
                        "type": "boolean",
                        "description": "True for multi-pick screens (adds confirm). Default false."
                    }
                },
                "required": ["title", "options"]
            }),
        },
        ToolDescriptor {
            name: TOOL_CLOSE,
            description: "Close the canvas. Open tabs show a farewell screen and the tab can \
                be closed. Use when the design question is settled or the session is ending.",
            schema: serde_json::json!({
                "type": "object",
                "properties": {},
            }),
        },
    ]
}
