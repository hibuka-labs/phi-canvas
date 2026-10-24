//! The canvas server: process-local tokio task bound to `127.0.0.1:0` (M1),
//! with the M4 security posture (loopback-only, session token, Origin/Host
//! checks, token never in a URL and never on disk).
//!
//! Lifecycle notes (M1):
//! - `close` is graceful: a farewell screen is served to any open tab and the
//!   listener keeps answering until the session ends or `shutdown` runs.
//! - binding with port 0 means a rebind (the recovery path) always moves the
//!   port; the host relaunches the browser with the new URL.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use axum::Router;
use axum::extract::{Json, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::json;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::assets;
use crate::envelope::ScreenEnvelope;
use crate::event::{CanvasEvent, EventAction, EventInput};
use crate::store::{self, CanvasStore, ServerInfo, StoreError};
use crate::token::SessionToken;

/// Where the session directory lives and what to call it, plus the idle
/// lifecycle knob. `idle_timeout` is parameterized so tests can inject a tiny
/// value instead of waiting out the production default (30 minutes).
#[derive(Debug, Clone)]
pub struct CanvasConfig {
    pub store_root: PathBuf,
    pub session_id: String,
    /// No push and no click for this long closes the canvas (`CloseReason::Idle`).
    /// `None` disables the watcher (long-lived stores, tests that drive close).
    pub idle_timeout: Option<Duration>,
}

/// The production idle budget (M1: "30 分钟空闲自动 close").
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

impl CanvasConfig {
    pub fn new(store_root: impl Into<PathBuf>, session_id: impl Into<String>) -> Self {
        Self {
            store_root: store_root.into(),
            session_id: session_id.into(),
            idle_timeout: Some(DEFAULT_IDLE_TIMEOUT),
        }
    }

    /// Test seam (row 19): shrink or disable the idle watcher.
    pub fn with_idle_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.idle_timeout = timeout;
        self
    }
}

/// Why the canvas went away. Drives the farewell screen wording (M1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    SessionEnded,
    UserClosed,
    Idle,
}

/// What a push stored — the tool layer turns this into its return text (M3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushReceipt {
    pub screen: u64,
    pub url: String,
    pub title: String,
    pub option_count: usize,
    pub display_only: bool,
    /// Host-set pause marker (`ScreenEnvelope::interstitial`) — the receipt
    /// carries it so a replayed history still names its pauses.
    pub interstitial: bool,
}

#[derive(Debug, Error)]
pub enum CanvasError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("canvas is closed")]
    Closed,
    #[error("failed to bind 127.0.0.1:0 — {0}")]
    Bind(String),
}

struct CanvasState {
    next_seq: u64,
    screens: BTreeMap<u64, ScreenEnvelope>,
    current: Option<u64>,
    closed: Option<CloseReason>,
    notice_seq: Option<u64>,
    /// The last answer a click gave — the receipt line the *next* page load
    /// shows. The page reloads on every screen change (`window.location.
    /// reload` in the helper), so a click's highlight dies with the old page;
    /// this is what keeps "what did I pick?" answerable. A restore starts
    /// without one: the events log is history, not state to rebuild.
    last_pick: Option<LastPick>,
}

/// A pick the receipt line names: the screen it landed on (the receipt is for
/// *later* screens) and the label the user saw.
struct LastPick {
    screen: u64,
    label: String,
}

impl CanvasState {
    fn new() -> Self {
        Self {
            next_seq: 1,
            screens: BTreeMap::new(),
            current: None,
            closed: None,
            notice_seq: None,
            last_pick: None,
        }
    }

    /// Rebuild from the session store (screens saved by an earlier server
    /// task). Seq continues after the last stored screen and `current` is the
    /// last one pushed — a rebind is a recovery of the same canvas, not a
    /// blank slate (1C row 16: Ctrl+B must bring the screen back).
    fn restored(screens: Vec<(u64, ScreenEnvelope)>) -> Self {
        let mut state = Self::new();
        for (seq, env) in screens {
            state.next_seq = state.next_seq.max(seq + 1);
            state.current = Some(seq);
            state.screens.insert(seq, env);
        }
        state
    }

    /// Close bookkeeping shared by the graceful path and the idle watcher:
    /// first close wins, later attempts are no-ops (M1 idempotence).
    fn close(&mut self, reason: CloseReason) -> bool {
        if self.closed.is_some() {
            return false;
        }
        self.closed = Some(reason);
        self.notice_seq = Some(self.next_seq);
        self.next_seq += 1;
        true
    }
}

struct Inner {
    port: u16,
    url: String,
    token: SessionToken,
    store: CanvasStore,
    state: RwLock<CanvasState>,
    events_tx: broadcast::Sender<CanvasEvent>,
    event_seq: AtomicU64,
    task: Mutex<Option<JoinHandle<()>>>,
    /// Push receipts for the host's status bar / waiting-screen policy (1C).
    pushes_tx: broadcast::Sender<PushReceipt>,
    /// Last real interaction (push or click) in `store::now_ms()` — the idle
    /// watcher's only liveness signal. Page views and client poll heartbeats
    /// deliberately do not count: a forgotten tab is exactly the case the
    /// 30-minute close exists for.
    last_activity_ms: AtomicU64,
    /// The idle watcher's handle (aborted together with the server task).
    idle_task: Mutex<Option<JoinHandle<()>>>,
}

/// Handle to a running canvas server.
pub struct Canvas {
    inner: Arc<Inner>,
}

impl Canvas {
    /// Bind `127.0.0.1:0`, write `server-info`, spawn the server task.
    pub async fn open(config: CanvasConfig) -> Result<Arc<Canvas>, CanvasError> {
        let store = CanvasStore::open(&config.store_root)?;
        // Screens persisted by earlier server tasks in this session — the
        // recovery source for a rebind (see CanvasState::restored).
        let store_screens = store.load_all_screens();
        let token = SessionToken::generate();

        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|err| CanvasError::Bind(err.to_string()))?;
        let port = listener
            .local_addr()
            .map_err(|err| CanvasError::Bind(err.to_string()))?
            .port();
        let url = format!("http://127.0.0.1:{port}");

        store.write_server_info(&ServerInfo {
            session_id: config.session_id.clone(),
            url: url.clone(),
            port,
            started_at_ms: store::now_ms(),
        })?;

        let (events_tx, _) = broadcast::channel(256);
        let (pushes_tx, _) = broadcast::channel(64);
        let inner = Arc::new(Inner {
            port,
            url,
            token,
            store,
            state: RwLock::new(CanvasState::restored(store_screens)),
            events_tx,
            event_seq: AtomicU64::new(0),
            task: Mutex::new(None),
            pushes_tx,
            last_activity_ms: AtomicU64::new(store::now_ms()),
            idle_task: Mutex::new(None),
        });

        let app = router(inner.clone());
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        *inner.task.lock().unwrap() = Some(task);

        if let Some(timeout) = config.idle_timeout {
            *inner.idle_task.lock().unwrap() = Some(spawn_idle_watcher(inner.clone(), timeout));
        }

        Ok(Arc::new(Canvas { inner }))
    }

    pub fn url(&self) -> String {
        self.inner.url.clone()
    }

    pub fn port(&self) -> u16 {
        self.inner.port
    }

    /// Host-side only: needed to open the page and to forge tests. Never log
    /// or persist this — it is a credential.
    pub fn session_token(&self) -> &str {
        self.inner.token.as_str()
    }

    pub fn is_open(&self) -> bool {
        self.inner.state.read().unwrap().closed.is_none()
    }

    /// Why the canvas closed — drives the host status bar (`画布已关闭 · 可重开`
    /// vs. an idle auto-close). `None` while it is still serving.
    pub fn close_reason(&self) -> Option<CloseReason> {
        self.inner.state.read().unwrap().closed
    }

    /// Server-task liveness: `true` while the accept loop runs. A graceful
    /// close still serves the farewell view, so it stays alive — only
    /// `shutdown`/a crash ends the task.
    pub fn is_alive(&self) -> bool {
        let task = self.inner.task.lock().unwrap();
        task.as_ref().is_some_and(|handle| !handle.is_finished())
    }

    /// The crash shape (1C failure detection): the server task is gone but no
    /// lifecycle close was ever recorded. Session exit avoids this by closing
    /// (`CloseReason::SessionEnded`) before `shutdown`.
    pub fn is_failed(&self) -> bool {
        !self.is_alive() && self.inner.state.read().unwrap().closed.is_none()
    }

    /// Subscribe to click events (the host's mailbox drains this — M2).
    pub fn subscribe(&self) -> broadcast::Receiver<CanvasEvent> {
        self.inner.events_tx.subscribe()
    }

    /// Subscribe to push receipts (1C: status-bar counts + waiting-screen
    /// freshness). Fires for tool pushes and for host pushes alike — the host
    /// tags its own waiting pushes so it never treats them as fresh content.
    pub fn subscribe_pushes(&self) -> broadcast::Receiver<PushReceipt> {
        self.inner.pushes_tx.subscribe()
    }

    /// Host-side lookup: the click mailbox maps an event's `option_id` back to
    /// its label through the stored envelope (M2 synthesis). Cloned out —
    /// `ScreenEnvelope` is a plain data struct.
    pub fn screen_envelope(&self, seq: u64) -> Option<ScreenEnvelope> {
        self.inner.state.read().unwrap().screens.get(&seq).cloned()
    }

    /// Host-side snapshot of every pushed screen, oldest first (1C).
    ///
    /// [`Self::subscribe_pushes`] only carries pushes made *after* the
    /// subscription, and a tool's first push always races the host's tick
    /// (the tool holds its state lock across `push`), so the host polls this
    /// instead and feeds its bookkeeping. Recording is idempotent by seq, so
    /// polling overlap costs nothing.
    pub fn pushed_screens(&self) -> Vec<PushReceipt> {
        let state = self.inner.state.read().unwrap();
        state
            .screens
            .iter()
            .map(|(seq, env)| PushReceipt {
                screen: *seq,
                url: self.inner.url.clone(),
                title: env.title.clone(),
                option_count: env.options.len(),
                display_only: env.is_display_only(),
                interstitial: env.interstitial,
            })
            .collect()
    }

    /// Store a screen and make it the current view.
    pub async fn push(&self, mut env: ScreenEnvelope) -> Result<PushReceipt, CanvasError> {
        let receipt = {
            let mut state = self.inner.state.write().unwrap();
            if state.closed.is_some() {
                return Err(CanvasError::Closed);
            }
            let seq = state.next_seq;
            state.next_seq += 1;
            self.inner.store.save_screen(seq, &mut env)?;
            let receipt = PushReceipt {
                screen: seq,
                url: self.inner.url.clone(),
                title: env.title.clone(),
                option_count: env.options.len(),
                display_only: env.is_display_only(),
                interstitial: env.interstitial,
            };
            state.screens.insert(seq, env);
            state.current = Some(seq);
            receipt
        };
        self.inner
            .last_activity_ms
            .store(store::now_ms(), Ordering::Relaxed);
        let _ = self.inner.pushes_tx.send(receipt.clone());
        Ok(receipt)
    }

    /// Graceful close (M1): serve a farewell view, stop accepting writes, keep
    /// the listener up so open tabs land on the farewell instead of a dead tab.
    pub async fn close(&self, reason: CloseReason) -> Result<(), CanvasError> {
        self.inner.state.write().unwrap().close(reason);
        Ok(())
    }

    /// Stop the listener and end the server task (session exit / switch).
    /// Aborts the idle watcher too — a shutdown canvas must not linger.
    ///
    /// Deliberately does *not* record a close reason: the host performs
    /// `close(SessionEnded)` first for the farewell page, and a bare
    /// `shutdown` is exactly the crash shape `is_failed` reports.
    pub async fn shutdown(&self) {
        let task = self.inner.task.lock().unwrap().take();
        let idle = self.inner.idle_task.lock().unwrap().take();
        if let Some(idle) = idle {
            idle.abort();
        }
        if let Some(task) = task {
            task.abort();
            let _ = task.await;
        }
    }
}

fn router(inner: Arc<Inner>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/screens/{seq}", get(screen))
        .route("/screens/{seq}/reference", get(screen_reference))
        .route("/compare", get(compare_default))
        .route("/compare/{a}/{b}", get(compare_pair))
        .route("/compare/{a}/{b}/{mode}", get(compare_triple))
        .route("/api/poll", get(poll))
        .route("/assets/components.css", get(stylesheet))
        .route("/assets/helper.js", get(helper))
        .route("/assets/compare.js", get(compare_script))
        .route("/assets/antd-runtime.js", get(antd_runtime))
        .route("/events", post(post_event))
        .layer(middleware::from_fn_with_state(inner.clone(), guard))
        .with_state(inner)
}

/// Request guard: query strings, Host, and Origin (M4). Token lives with the
/// `POST /events` handler — GET serves the token into the page on purpose.
async fn guard(
    State(inner): State<Arc<Inner>>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if request.uri().query().is_some() {
        return (StatusCode::BAD_REQUEST, "query strings are not accepted").into_response();
    }

    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    let Some(host) = host else {
        return (StatusCode::BAD_REQUEST, "Host header is required").into_response();
    };
    if !allowed_hosts(inner.port)
        .iter()
        .any(|allowed| allowed == host)
    {
        return (StatusCode::FORBIDDEN, "foreign host").into_response();
    }

    if let Some(origin) = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        && !allowed_origins(inner.port)
            .iter()
            .any(|allowed| allowed == origin)
    {
        return (StatusCode::FORBIDDEN, "foreign origin").into_response();
    }

    next.run(request).await
}

fn allowed_hosts(port: u16) -> [String; 2] {
    [format!("127.0.0.1:{port}"), format!("localhost:{port}")]
}

fn allowed_origins(port: u16) -> [String; 2] {
    [
        format!("http://127.0.0.1:{port}"),
        format!("http://localhost:{port}"),
    ]
}

fn farewell_text(reason: CloseReason) -> (&'static str, &'static str) {
    match reason {
        CloseReason::SessionEnded => ("会话已结束", "这次设计讨论已经结束，可以回到终端继续。"),
        CloseReason::UserClosed => ("画布已关闭", "画布已被关闭，可以回到终端继续讨论。"),
        CloseReason::Idle => ("画布已关闭", "长时间未使用，画布已自动关闭。"),
    }
}

fn render_current(inner: &Inner) -> String {
    let state = inner.state.read().unwrap();
    if let Some(reason) = state.closed {
        let (title, message) = farewell_text(reason);
        return assets::render_notice(
            title,
            message,
            inner.token.as_str(),
            state.notice_seq.unwrap_or(0),
            false,
        );
    }
    match state
        .current
        .and_then(|seq| state.screens.get(&seq).map(|env| (seq, env)))
    {
        Some((seq, env)) => {
            let versions = version_entries(&state);
            let prior = state
                .last_pick
                .as_ref()
                .filter(|pick| pick.screen != seq)
                .map(|pick| pick.label.as_str());
            // The index route only ever shows the screen on offer.
            assets::render_screen(env, seq, inner.token.as_str(), &versions, true, prior)
        }
        // Live: the promise "会自动出现在这里" is the page's whole job — the
        // first push lands after the browser is already open (M3 opens on
        // `canvas_open`, the push comes when the model is done composing).
        None => assets::render_notice(
            "等待第一屏",
            "agent 推送方案后会自动出现在这里。",
            inner.token.as_str(),
            0,
            true,
        ),
    }
}

/// Design screens so far, ascending — the version rail and the compare
/// pickers (gate 2 ③). Interstitial screens are left out: the history the
/// user scrolls is the design, not the pauses between it.
fn version_entries(state: &CanvasState) -> Vec<assets::VersionEntry> {
    state
        .screens
        .iter()
        .filter(|(_, env)| !env.interstitial)
        .map(|(seq, env)| assets::VersionEntry {
            seq: *seq,
            title: env.title.clone(),
        })
        .collect()
}

async fn index(State(inner): State<Arc<Inner>>) -> Response {
    Html(render_current(&inner)).into_response()
}

async fn screen(State(inner): State<Arc<Inner>>, Path(seq): Path<u64>) -> Response {
    let state = inner.state.read().unwrap();
    match state.screens.get(&seq) {
        Some(env) => {
            let versions = version_entries(&state);
            // A historical version is a look back, not a second chance to
            // answer — only the screen on offer renders as clickable.
            let live = Some(seq) == state.current && state.closed.is_none();
            let prior = state
                .last_pick
                .as_ref()
                .filter(|pick| pick.screen != seq)
                .map(|pick| pick.label.as_str());
            Html(assets::render_screen(
                env,
                seq,
                inner.token.as_str(),
                &versions,
                live,
                prior,
            ))
            .into_response()
        }
        None => (StatusCode::NOT_FOUND, "unknown screen").into_response(),
    }
}

/// Compare two versions: pickers over the history, side by side or wiped.
/// Read-only by construction — this page has no token and cannot post events,
/// because comparing is looking, not picking.
///
/// The pair and the layout are path segments, not query parameters: the M4
/// guard rejects every query string outright (that is how the session token
/// is kept out of URLs), so nothing here may be passed as one.
async fn compare_default(State(inner): State<Arc<Inner>>) -> Response {
    compare_page(&inner, None, None, assets::CompareMode::SideBySide)
}

async fn compare_pair(State(inner): State<Arc<Inner>>, Path((a, b)): Path<(u64, u64)>) -> Response {
    compare_page(&inner, Some(a), Some(b), assets::CompareMode::SideBySide)
}

async fn compare_triple(
    State(inner): State<Arc<Inner>>,
    Path((a, b, mode)): Path<(u64, u64, String)>,
) -> Response {
    let mode = match mode.as_str() {
        "split" => assets::CompareMode::SideBySide,
        "slide" => assets::CompareMode::Wipe,
        _ => return (StatusCode::NOT_FOUND, "unknown compare mode").into_response(),
    };
    compare_page(&inner, Some(a), Some(b), mode)
}

fn compare_page(
    inner: &Inner,
    wanted_before: Option<u64>,
    wanted_after: Option<u64>,
    mode: assets::CompareMode,
) -> Response {
    let state = inner.state.read().unwrap();
    let versions = version_entries(&state);
    if versions.len() < 2 {
        return Html(assets::render_notice(
            "还没有可对比的版本",
            "至少推两屏设计之后，这里才能并排或拖滑块。",
            inner.token.as_str(),
            0,
            false,
        ))
        .into_response();
    }

    // Defaults are the two most recent versions: the pair the user came to
    // compare is nearly always "just now" against "the one before it".
    let fallback_before = versions[versions.len() - 2].seq;
    let fallback_after = versions[versions.len() - 1].seq;
    let known = |wanted: Option<u64>, fallback: u64| -> u64 {
        wanted
            .filter(|seq| versions.iter().any(|version| version.seq == *seq))
            .unwrap_or(fallback)
    };
    let before_seq = known(wanted_before, fallback_before);
    let after_seq = known(wanted_after, fallback_after);

    let (Some(before), Some(after)) = (
        state.screens.get(&before_seq),
        state.screens.get(&after_seq),
    ) else {
        return (StatusCode::NOT_FOUND, "unknown screen").into_response();
    };
    Html(assets::render_compare(
        (before_seq, before),
        (after_seq, after),
        &versions,
        mode,
    ))
    .into_response()
}

/// The reference image copied beside a screen (gate 2 ④). Only bytes the host
/// already read off disk are served here — the page never names a path of its
/// own, so this route cannot be used to walk the filesystem.
async fn screen_reference(State(inner): State<Arc<Inner>>, Path(seq): Path<u64>) -> Response {
    match inner.store.read_reference(seq) {
        Some(bytes) => ([(axum::http::header::CONTENT_TYPE, "image/png")], bytes).into_response(),
        None => (StatusCode::NOT_FOUND, "no reference image").into_response(),
    }
}

async fn poll(State(inner): State<Arc<Inner>>) -> Response {
    let state = inner.state.read().unwrap();
    let (seq, view_state, options, display_only) = if let Some(reason) = state.closed {
        let _ = reason;
        (state.notice_seq.unwrap_or(0), "closed", 0, true)
    } else if let Some(seq) = state.current {
        let env = state.screens.get(&seq);
        let options = env.map(|e| e.options.len()).unwrap_or(0);
        let display_only = env.map(|e| e.is_display_only()).unwrap_or(true);
        (seq, "open", options, display_only)
    } else {
        (0, "empty", 0, true)
    };
    Json(json!({
        "seq": seq,
        "state": view_state,
        "options": options,
        "display_only": display_only,
    }))
    .into_response()
}

async fn stylesheet() -> Response {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        assets::COMPONENTS_CSS,
    )
        .into_response()
}

async fn helper() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        assets::HELPER_JS,
    )
        .into_response()
}

async fn compare_script() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        assets::COMPARE_JS,
    )
        .into_response()
}

async fn antd_runtime() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        assets::ANTD_RUNTIME_JS,
    )
        .into_response()
}

async fn post_event(
    State(inner): State<Arc<Inner>>,
    headers: HeaderMap,
    Json(input): Json<EventInput>,
) -> Response {
    let presented = headers
        .get("x-canvas-token")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if !inner.token.verify(presented) {
        return (StatusCode::UNAUTHORIZED, "missing or bad session token").into_response();
    }

    let receipt_label = {
        let state = inner.state.read().unwrap();
        if state.closed.is_some() {
            return (StatusCode::CONFLICT, "canvas is closed").into_response();
        }
        let Some(env) = state.screens.get(&input.screen) else {
            return (StatusCode::NOT_FOUND, "unknown screen").into_response();
        };
        // Choices land only on the screen that is on offer. A pick from a
        // historical version (the rail is for looking) is not a decision —
        // this is the server-side half of the viewing-mode guard; the page
        // half is `render_options(live)`.
        if Some(input.screen) != state.current {
            return (StatusCode::CONFLICT, "this screen is not on offer").into_response();
        }
        let invalid = match &input.action {
            EventAction::Select { option_id } => {
                !env.options.iter().any(|option| &option.id == option_id)
            }
            EventAction::Confirm => !env.multiselect || env.options.is_empty(),
            EventAction::Skip => env.options.is_empty(),
        };
        if invalid {
            return (StatusCode::BAD_REQUEST, "action does not match this screen").into_response();
        }
        // What the next page load names as its receipt: a single-select pick
        // is a decision by itself, and a skip is one too. A multiselect answer
        // is a *set* — the terminal message carries it, not this line.
        match &input.action {
            EventAction::Select { option_id } if !env.multiselect => env
                .options
                .iter()
                .find(|option| &option.id == option_id)
                .map(|option| option.label.clone()),
            EventAction::Skip => Some("跳过".to_string()),
            _ => None,
        }
    };

    let seq = inner.event_seq.fetch_add(1, Ordering::SeqCst) + 1;
    let event = input.into_event(seq, store::now_ms());
    if let Err(err) = inner.store.append_event(&event) {
        return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
    }
    if let Some(label) = receipt_label {
        inner.state.write().unwrap().last_pick = Some(LastPick {
            screen: event.screen,
            label,
        });
    }
    // A click is real user activity — resets the idle clock (pushes do too).
    inner
        .last_activity_ms
        .store(store::now_ms(), Ordering::Relaxed);
    let _ = inner.events_tx.send(event);
    StatusCode::NO_CONTENT.into_response()
}

/// Idle watchdog (M1: 30 分钟空闲自动 close). Closes the canvas once no push
/// and no click has been seen for `timeout`, and exits when the canvas closes
/// or the server task dies. The check cadence is derived from the timeout so
/// tests with tiny budgets observe the close quickly without a busy loop.
fn spawn_idle_watcher(inner: Arc<Inner>, timeout: Duration) -> JoinHandle<()> {
    let interval = (timeout / 10).clamp(Duration::from_millis(20), Duration::from_secs(60));
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            if inner.state.read().unwrap().closed.is_some() {
                break;
            }
            let task_alive = {
                let task = inner.task.lock().unwrap();
                task.as_ref().is_some_and(|handle| !handle.is_finished())
            };
            if !task_alive {
                break;
            }
            let last = inner.last_activity_ms.load(Ordering::Relaxed);
            if store::now_ms().saturating_sub(last) >= timeout.as_millis() as u64 {
                inner.state.write().unwrap().close(CloseReason::Idle);
                break;
            }
        }
    })
}
