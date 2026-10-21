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
use crate::spec::SpecError;

pub const TOOL_OPEN: &str = "canvas_open";
pub const TOOL_PUSH: &str = "canvas_push";
pub const TOOL_CLOSE: &str = "canvas_close";

/// The locked `canvas_close` return text (M3).
pub const TEXT_CLOSED: &str = "canvas: closed";

/// Appended to a `canvas_push` text when the model sent both `screen` and
/// `body_html`. The spec wins; this note is how the model finds out its markup
/// was not the thing that got shown. The M3 formats above are unchanged.
pub const TEXT_SPEC_OVER_HTML: &str =
    " | note: both screen and body_html given - screen won, send only one";

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
    #[error(transparent)]
    Spec(#[from] SpecError),
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
        let mut env = ScreenEnvelope::parse(args)?;
        // Spec screens render host-side into the body slot; `body_html` stays
        // the escape hatch. When the model sends both, the spec wins and the
        // tool text says so.
        let both = env.materialize_screen()?;
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
        let mut text = if receipt.display_only {
            text_display_only(&receipt.url, &receipt.title)
        } else {
            text_awaiting_selection(&receipt.url, &receipt.title, receipt.option_count)
        };
        if both {
            text.push_str(TEXT_SPEC_OVER_HTML);
        }
        Ok(text)
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
                (`id` echoed back on click, `label` shown); `options: []` makes it a \
                display-only screen; `multiselect: true` adds confirm/skip instead of \
                single-select. Describe the picture with `screen` (structured mock + theme \
                tokens — preferred) or `body_html` (raw HTML escape hatch), never both. \
                `reference` pins one PNG beside the candidates when the user pointed at \
                something to match. Each push is kept as a version the user can look back \
                at and compare (side by side or a wipe slider) — number it in the title \
                when you iterate. \
                Opening is implicit (server only — the browser waits for the user's consent).",
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
                        "description": "Escape hatch: a raw HTML fragment (mockups, comparison cards). Send this OR `screen`, not both."
                    },
                    "escape": {
                        "type": "string",
                        "description": "Only with `body_html`: one or two words naming the shape the vocabulary could not hold ('table', 'phone-frame', 'timeline'). Never rendered — it is the ledger that says what the closed vocabulary is missing, so name the shape, not the screen. With `screen` it is rejected."
                    },
                    "reference": {
                        "type": "object",
                        "description": "One reference image the candidates are judged against, shown beside them. \
                            `src` is a filesystem path (the host copies it into the session store) or an \
                            http(s)/`data:` URI the page loads itself; `caption` is one line naming what it is. \
                            The source is pluggable — a screenshot, a file the user named, a pasted image — \
                            the canvas only needs one PNG and a name for where it is.",
                        "properties": {
                            "src": {
                                "type": "string",
                                "description": "Where the PNG is: a path on disk, or an http(s)/data: URI."
                            },
                            "caption": {
                                "type": "string",
                                "description": "One line: what this is a reference of."
                            }
                        },
                        "required": ["src"]
                    },
                    "screen": {
                        "type": "object",
                        "description": "Structured screen (preferred — send it as a real JSON object in the \
                            arguments, never as a stringified copy). Shape: \
                            {fidelity: 'wireframe'|'color'|'texture', mock?: <node>, notice?: string, \
                            variants?: [{id, mock?: <node>, theme?: <tokens>, emph?: [node_id], content?: {node_id: text}}]}. \
                            `variants[].id` must match an `options[].id`; the shared `mock` is reused \
                            per variant unless a variant overrides it. Node = {kind, id?, ...} with a \
                            closed kind set — layout: 'frame'|'cols'|'rows'|'stack'|'box'|'text'|'placeholder'; \
                            chrome: 'phone_shell'|'topbar'|'nav'|'sidebar'|'tabbar'|'toolbar'|'dialog'; \
                            controls: 'input'|'button'|'switch'|'segment'|'list_row'|'swatches'; \
                            data/content: 'kpi'|'hero'|'sparkline'|'ring'|'bars'|'area'|'viz'|'table'|'tree'. \
                            A kind only takes the fields it declares; a stray field is rejected. \
                            `viz` takes `form: 'map'|'funnel'|'graph'|'area'`, `sparkline` takes \
                            `shape: 'up'|'down'|'flat'`, `cols`/`rows` take `ratio` weights (numbers, \
                            decimals fine: `[1, 5, 0.7]`), `box` takes \
                            `w`/`h` percentages (number or string, `18` = `18%`; in a `cols`/`rows` band \
                            the share becomes the child's track), `phone_shell`/`dialog` take a nested `body` node, \
                            `tabbar`/`segment` take `items` + optional `active` index. \
                            Theme tokens: bg, panel, border, accent, accent2, text, text_bright; \
                            texture knobs (fidelity 'texture' only): surface 'glass'|'matte'|'metal', frost 1-3, \
                            elevation 1-3, edge 1-3, glow 0-2. No CSS and no style fields anywhere — \
                            structure + tokens only. `fidelity` follows the question being asked: \
                            layout = wireframe, styling = color, material/finish = texture."
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
