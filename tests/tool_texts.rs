//! Tool-layer contract (M3): the four return texts, implicit open binding the
//! server without ever launching a browser, and the schema handoff.

use std::sync::{Arc, Mutex};

use phi_canvas::tools::{
    BrowserLauncher, CanvasTools, LaunchDecision, TEXT_CLOSED, TEXT_SPEC_OVER_HTML, ToolCallError,
    tool_descriptors,
};
use serde_json::json;

/// Records every launch request and answers with a fixed decision.
struct Spy {
    calls: Mutex<Vec<String>>,
    decision: LaunchDecision,
}

impl Spy {
    fn new(decision: LaunchDecision) -> Arc<Self> {
        Arc::new(Self {
            calls: Mutex::new(Vec::new()),
            decision,
        })
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl BrowserLauncher for Spy {
    fn launch(&self, url: &str) -> LaunchDecision {
        self.calls.lock().unwrap().push(url.to_string());
        self.decision
    }
}

fn tools() -> (tempfile::TempDir, CanvasTools) {
    let tmp = tempfile::tempdir().unwrap();
    let tools = CanvasTools::new(tmp.path().join("canvas"), "sess-tools");
    (tmp, tools)
}

fn tools_with_spy(decision: LaunchDecision) -> (tempfile::TempDir, CanvasTools, Arc<Spy>) {
    let tmp = tempfile::tempdir().unwrap();
    let spy = Spy::new(decision);
    let tools =
        CanvasTools::new(tmp.path().join("canvas"), "sess-tools").with_launcher(spy.clone());
    (tmp, tools, spy)
}

fn push_args() -> serde_json::Value {
    json!({
        "title": "大屏顶部导航，哪个布局更专业？",
        "subtitle": "考虑信息密度和视觉层级",
        "options": [
            { "id": "a", "label": "侧边栏导航" },
            { "id": "b", "label": "顶部横向导航" },
            { "id": "c", "label": "混合式" }
        ],
        "body_html": "<div class=\"mockup\"><div class=\"mock-nav\">nav</div></div>"
    })
}

#[tokio::test]
async fn open_without_launcher_awaits_consent() {
    let (_tmp, tools) = tools();

    let text = tools.open().await.unwrap();
    let port = tools.canvas().await.expect("canvas bound").port();
    assert_eq!(
        text,
        format!("canvas: http://127.0.0.1:{port} | state: open | awaiting consent")
    );
}

#[tokio::test]
async fn open_reports_browser_launched_when_seam_launches() {
    let (_tmp, tools, spy) = tools_with_spy(LaunchDecision::Launched);

    let text = tools.open().await.unwrap();
    let port = tools.canvas().await.expect("canvas bound").port();
    let url = format!("http://127.0.0.1:{port}");
    assert_eq!(
        text,
        format!("canvas: {url} | state: open | browser launched")
    );
    assert_eq!(
        spy.calls(),
        vec![url],
        "exactly one launch, of the bound URL"
    );
}

#[tokio::test]
async fn open_is_idempotent_no_rebind_and_no_second_launch() {
    let (_tmp, tools, spy) = tools_with_spy(LaunchDecision::Launched);

    let first = tools.open().await.unwrap();
    let second = tools.open().await.unwrap();
    assert_eq!(first, second, "same URL and same launch state");
    assert_eq!(
        spy.calls().len(),
        1,
        "idempotent open must not launch again"
    );
}

#[tokio::test]
async fn implicit_open_on_push_binds_the_server_but_never_launches() {
    // Acceptance row 14 companion (negative list #1): a chained canvas_push
    // must not pop a browser — the launch always waits for the y consent.
    let (_tmp, tools, spy) = tools_with_spy(LaunchDecision::Launched);

    let text = tools.push(&push_args()).await.unwrap();
    let port = tools
        .canvas()
        .await
        .expect("implicit open must bind")
        .port();
    assert_eq!(
        text,
        format!(
            "canvas: http://127.0.0.1:{port} | screen: 大屏顶部导航，哪个布局更专业？ | options: 3 | awaiting selection"
        )
    );
    assert!(
        spy.calls().is_empty(),
        "implicit open must never call the launcher"
    );
}

#[tokio::test]
async fn push_display_only_screen_returns_display_text() {
    let (_tmp, tools) = tools();

    let text = tools
        .push(&json!({ "title": "配色方向", "options": [], "body_html": "<p>x</p>" }))
        .await
        .unwrap();
    let port = tools.canvas().await.expect("canvas bound").port();
    assert_eq!(
        text,
        format!("canvas: http://127.0.0.1:{port} | screen: 配色方向 | display only")
    );
}

#[tokio::test]
async fn push_after_close_rebinds_to_a_new_port_without_launching() {
    let (_tmp, tools, spy) = tools_with_spy(LaunchDecision::Launched);

    let first = tools.push(&push_args()).await.unwrap();
    tools.close().await.unwrap();

    let second = tools.push(&push_args()).await.unwrap();
    assert_ne!(
        first, second,
        "rebind must move the URL (port 0 never reuses)"
    );
    assert!(
        second
            .contains("| screen: 大屏顶部导航，哪个布局更专业？ | options: 3 | awaiting selection"),
        "recovered push keeps the M3 text shape: {second}"
    );
    assert!(
        spy.calls().is_empty(),
        "recovery through push must not launch a browser either"
    );
}

#[tokio::test]
async fn close_tool_returns_the_locked_closed_text() {
    let (_tmp, tools) = tools();

    tools.open().await.unwrap();
    assert_eq!(tools.close().await.unwrap(), TEXT_CLOSED);
    assert_eq!(TEXT_CLOSED, "canvas: closed", "M3 locked text");
}

#[tokio::test]
async fn close_of_a_never_opened_canvas_is_still_closed_text() {
    let (_tmp, tools) = tools();
    assert_eq!(tools.close().await.unwrap(), "canvas: closed");
}

#[tokio::test]
async fn malformed_envelope_surfaces_as_a_tool_error() {
    let (_tmp, tools) = tools();

    let err = tools.push(&json!({ "options": [] })).await.unwrap_err();
    assert!(
        matches!(err, ToolCallError::Envelope(_)),
        "missing title is an envelope error, got {err:?}"
    );
}

#[test]
fn tool_descriptors_cover_the_three_tools() {
    let descriptors = tool_descriptors();
    let names: Vec<&str> = descriptors.iter().map(|d| d.name).collect();
    assert_eq!(
        names,
        vec!["canvas_open", "canvas_push", "canvas_close"],
        "three tools, in the spec's order"
    );

    for descriptor in &descriptors {
        assert!(
            !descriptor.description.trim().is_empty(),
            "{} needs a description",
            descriptor.name
        );
        assert_eq!(
            descriptor.schema["type"], "object",
            "{} schema is a JSON object",
            descriptor.name
        );
    }

    let push = descriptors
        .iter()
        .find(|d| d.name == "canvas_push")
        .unwrap();
    let required: Vec<&str> = push.schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        required.contains(&"title") && required.contains(&"options"),
        "push schema must require title + options: {required:?}"
    );
    for field in ["title", "subtitle", "options", "body_html", "multiselect"] {
        assert!(
            push.schema["properties"].get(field).is_some(),
            "push schema must describe `{field}`"
        );
    }
}

#[tokio::test]
async fn open_after_a_crash_rebinds_like_a_close_does() {
    // The crash shape (1C row 16): the server task is gone but no close was
    // recorded. `open` must treat it as a dead canvas and rebind — otherwise
    // recovery would report the corpse's URL.
    let (_tmp, tools, spy) = tools_with_spy(LaunchDecision::Launched);
    let first_url = tools.open().await.unwrap();
    tools.canvas().await.expect("canvas bound").shutdown().await;

    let second = tools.open().await.unwrap();
    assert_ne!(
        first_url, second,
        "a crashed canvas must rebind to a new port"
    );
    assert_eq!(
        spy.calls().len(),
        2,
        "the rebind is a new URL — the launcher is asked again"
    );
}

#[tokio::test]
async fn rebind_never_consults_the_launcher() {
    // Ctrl+B is the user's own request: the host shows the browser itself, so
    // the host-only rebind path must not route through the consent seam.
    let (_tmp, tools, spy) = tools_with_spy(LaunchDecision::Launched);
    tools.canvas().await; // nothing bound yet
    let url = tools.rebind().await.unwrap();
    assert!(
        url.starts_with("http://127.0.0.1:"),
        "rebind reports the URL"
    );
    assert!(
        spy.calls().is_empty(),
        "a host-driven rebind never raises the consent gate"
    );
}

#[tokio::test]
async fn launch_decision_can_be_settled_after_the_fact() {
    // The consent answer arrives asynchronously (the user presses y on a
    // popup), so the host reports the outcome back — later `canvas_open`
    // calls on the same live canvas must not claim "awaiting consent" after
    // a browser is already open.
    let (_tmp, tools, _spy) = tools_with_spy(LaunchDecision::AwaitingConsent);
    let text = tools.open().await.unwrap();
    assert!(text.ends_with("awaiting consent"));

    tools.set_launch_decision(LaunchDecision::Launched).await;
    let text = tools.open().await.unwrap();
    assert!(
        text.ends_with("browser launched"),
        "settled decision stays honest: {text}"
    );
}

// ---- `screen` on the push path (gate 2) ----

fn spec_args() -> serde_json::Value {
    json!({
        "title": "配色方向",
        "options": [
            { "id": "deep-blue", "label": "深空蓝" },
            { "id": "neon-purple", "label": "暗紫霓虹" }
        ],
        "screen": {
            "fidelity": "color",
            "mock": {
                "kind": "cols",
                "children": [
                    { "kind": "hero", "id": "hero", "text": "128,640" },
                    { "kind": "button", "id": "cta", "text": "导出" }
                ]
            },
            "variants": [
                { "id": "deep-blue", "theme": { "bg": "#0A1428" }, "emph": ["hero"] },
                { "id": "neon-purple", "theme": { "bg": "#12071F" }, "emph": ["cta"] }
            ]
        }
    })
}

fn stored_fragments(store_root: &std::path::Path) -> Vec<String> {
    let dir = store_root.join("screens");
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|e| e == "html") {
                out.push(std::fs::read_to_string(entry.path()).unwrap());
            }
        }
    }
    out.sort();
    out
}

fn stored_sidecars(store_root: &std::path::Path) -> Vec<serde_json::Value> {
    let dir = store_root.join("screens");
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|e| e == "json") {
                out.push(
                    serde_json::from_str(&std::fs::read_to_string(entry.path()).unwrap()).unwrap(),
                );
            }
        }
    }
    out
}

#[tokio::test]
async fn push_with_a_spec_renders_the_body_and_keeps_the_locked_text() {
    let tmp = tempfile::tempdir().unwrap();
    let store_root = tmp.path().join("canvas");
    let tools = CanvasTools::new(store_root.clone(), "sess-spec");

    let text = tools.push(&spec_args()).await.unwrap();
    assert!(
        text.contains("| screen: 配色方向 | options: 2 | awaiting selection"),
        "the M3 format is unchanged: {text}"
    );
    assert!(
        !text.contains(TEXT_SPEC_OVER_HTML),
        "no note when only one of the two was given"
    );

    let fragments = stored_fragments(&store_root);
    assert_eq!(fragments.len(), 1);
    assert!(fragments[0].contains("m-screen"), "{}", fragments[0]);
    assert!(fragments[0].contains("--m-bg:#0A1428"), "{}", fragments[0]);
    assert!(fragments[0].contains("m-hero m-emph"), "{}", fragments[0]);

    // The spec rides along in the sidecar so version comparison can diff it.
    let sidecars = stored_sidecars(&store_root);
    assert_eq!(sidecars.len(), 1);
    assert!(
        sidecars[0]["screen"]["fidelity"] == json!("color"),
        "{sidecars:?}"
    );
    assert!(
        sidecars[0]["body_html"].is_null() || sidecars[0]["body_html"] == json!(""),
        "the heavy rendered body does not live in the sidecar: {sidecars:?}"
    );
}

#[tokio::test]
async fn push_with_both_screen_and_body_html_appends_the_note() {
    let tmp = tempfile::tempdir().unwrap();
    let tools = CanvasTools::new(tmp.path().join("canvas"), "sess-both");

    let mut args = spec_args();
    args["body_html"] = json!("<div class=\"cards\">hand written</div>");
    let text = tools.push(&args).await.unwrap();
    assert!(text.starts_with("canvas: "), "base text is intact: {text}");
    assert!(
        text.ends_with(TEXT_SPEC_OVER_HTML),
        "the note is appended, the format is not rewritten: {text}"
    );
}

#[tokio::test]
async fn a_bad_spec_is_a_tool_error_not_a_crash() {
    let (_tmp, tools) = tools();
    let mut args = spec_args();
    args["screen"]["mock"]["kind"] = json!("chartjs");
    let err = tools.push(&args).await.unwrap_err();
    assert!(!err.to_string().is_empty());

    let mut args = spec_args();
    args["screen"]["fidelity"] = json!("photoreal");
    assert!(tools.push(&args).await.is_err());
}

#[test]
fn the_screen_description_names_exactly_the_accepted_kinds() {
    // The description is the model's only contract for the closed vocabulary:
    // a kind named there but rejected by the parser (or accepted but unnamed)
    // burns a live push round. This pins the description against NODE_KINDS
    // and NODE_KINDS against the parser's key table.
    let push = tool_descriptors()
        .into_iter()
        .find(|d| d.name == "canvas_push")
        .expect("canvas_push descriptor");
    let description = push.schema["properties"]["screen"]["description"]
        .as_str()
        .expect("screen description is text");

    let start = description
        .find("closed kind set")
        .expect("kind list marker");
    let end = description.find("A kind only takes").expect("end marker");
    let mut listed: Vec<String> = description[start..end]
        .split('\'')
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, name)| name.to_string())
        .filter(|name| !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        .collect();
    listed.sort();
    let mut published: Vec<String> = phi_canvas::spec::NODE_KINDS
        .iter()
        .map(|kind| kind.to_string())
        .collect();
    published.sort();
    assert_eq!(
        listed, published,
        "the screen description's kind names must be exactly NODE_KINDS"
    );

    // Every published name must reach the parser's key table — nothing the
    // description tells the model to send may die as "unknown node kind".
    // (Kinds with no required fields parse empty — that is fine.)
    for kind in phi_canvas::spec::NODE_KINDS {
        if let Err(err) = phi_canvas::ScreenSpec::parse(&json!({
            "fidelity": "wireframe",
            "mock": { "kind": kind }
        })) {
            assert!(
                !err.to_string().contains("unknown node kind"),
                "{kind} is published but the parser rejects it: {err}"
            );
        }
    }
}
