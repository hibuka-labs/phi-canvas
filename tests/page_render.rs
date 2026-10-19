//! Frame rendering: the page a human actually looks at (acceptance row 5 is
//! the human's 30-second pass; these pin its structure so regressions are
//! caught mechanically).

mod common;

use common::{body_of, get, send};
use phi_canvas::{Canvas, CanvasConfig, CloseReason, ScreenEnvelope};
use serde_json::json;

async fn open_canvas(tmp: &tempfile::TempDir) -> std::sync::Arc<Canvas> {
    let config = CanvasConfig::new(tmp.path().join("canvas"), "sess-page");
    Canvas::open(config).await.expect("canvas opens")
}

fn options_screen() -> ScreenEnvelope {
    ScreenEnvelope::parse(&json!({
        "title": "大屏顶部导航，哪个布局更专业？",
        "subtitle": "考虑信息密度和视觉层级",
        "options": [
            { "id": "a", "label": "侧边栏导航", "note": "筛选常驻" },
            { "id": "b", "label": "顶部横向导航" }
        ],
        "body_html": "<div class=\"mockup\"><div class=\"mock-nav\">NAV</div></div>",
        "multiselect": false
    }))
    .unwrap()
}

#[tokio::test]
async fn page_renders_title_options_and_body() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(options_screen()).await.unwrap();
    let port = canvas.port();

    let (status, response) = send(port, &get(port, "/")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);

    assert!(
        body.contains("大屏顶部导航，哪个布局更专业？"),
        "title rendered"
    );
    assert!(body.contains("考虑信息密度和视觉层级"), "subtitle rendered");
    assert!(
        body.contains("data-choice=\"a\"") && body.contains("data-choice=\"b\""),
        "options wired"
    );
    assert!(
        body.contains("侧边栏导航") && body.contains("顶部横向导航"),
        "labels rendered"
    );
    assert!(body.contains("mock-nav"), "body_html inserted verbatim");

    canvas.shutdown().await;
}

#[tokio::test]
async fn metadata_is_html_escaped() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    let env = ScreenEnvelope::parse(&json!({
        "title": "<script>alert(1)</script>",
        "options": [{ "id": "a", "label": "<b>bold</b>" }],
        "body_html": "<p>keep</p>"
    }))
    .unwrap();
    canvas.push(env).await.unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);

    assert!(
        !body.contains("<script>alert(1)</script>"),
        "metadata must never be raw markup"
    );
    assert!(body.contains("&lt;script&gt;"), "escaped title present");
    assert!(
        body.contains("&lt;b&gt;bold&lt;/b&gt;"),
        "escaped label present"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn token_is_in_meta_but_never_in_urls() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(options_screen()).await.unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);

    assert!(
        body.contains(&format!(
            r#"name="canvas-token" content="{}""#,
            canvas.session_token()
        )),
        "token is served in the page meta for POSTs"
    );
    assert!(
        !body.contains("?token=") && !body.contains("&token="),
        "no token in URLs"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn display_only_screen_has_no_option_panel() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    let env = ScreenEnvelope::parse(&json!({
        "title": "配色方向",
        "options": [],
        "body_html": "<div class=\"mockup\"><div class=\"placeholder\">色板</div></div>"
    }))
    .unwrap();
    canvas.push(env).await.unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);

    assert!(body.contains("色板"), "body rendered");
    assert!(
        !body.contains("data-choice="),
        "display-only: no clickable options"
    );
    assert!(
        !body.contains("data-action=\"skip\""),
        "display-only: no skip button"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn multiselect_renders_confirm_and_skip_buttons() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    let mut env = options_screen();
    env.multiselect = true;
    canvas.push(env).await.unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);

    assert!(
        body.contains("data-action=\"confirm\""),
        "multiselect needs confirm"
    );
    assert!(
        body.contains("data-action=\"skip\""),
        "skip is always available with options"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn single_select_renders_skip_but_not_confirm() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(options_screen()).await.unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/")).await;
    let body = body_of(&response);
    assert_eq!(status, 200);

    assert!(body.contains("data-action=\"skip\""));
    assert!(
        !body.contains("data-action=\"confirm\""),
        "confirm is multiselect-only"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn specific_screen_url_serves_that_screen() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(options_screen()).await.unwrap();
    let second = ScreenEnvelope::parse(&json!({
        "title": "第二屏",
        "options": [],
        "body_html": "<p>second</p>"
    }))
    .unwrap();
    canvas.push(second).await.unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/screens/1")).await;
    assert_eq!(status, 200);
    assert!(
        body_of(&response).contains("大屏顶部导航"),
        "screen 1 reachable by seq"
    );

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/screens/2")).await;
    assert_eq!(status, 200);
    assert!(body_of(&response).contains("第二屏"));

    let (status, _) = send(canvas.port(), &get(canvas.port(), "/screens/9")).await;
    assert_eq!(status, 404);

    canvas.shutdown().await;
}

#[tokio::test]
async fn poll_reports_state_for_heartbeat_and_reload() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/api/poll")).await;
    assert_eq!(status, 200);
    let state: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();
    assert_eq!(state["seq"], 0, "no screen yet");
    assert_eq!(state["state"], "empty");

    canvas.push(options_screen()).await.unwrap();
    let (_, response) = send(canvas.port(), &get(canvas.port(), "/api/poll")).await;
    let state: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();
    assert_eq!(state["seq"], 1);
    assert_eq!(state["state"], "open");
    assert_eq!(state["options"], 2);
    assert_eq!(state["display_only"], false);

    canvas.shutdown().await;
}

#[tokio::test]
async fn empty_canvas_shows_waiting_page() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/")).await;
    assert_eq!(status, 200);
    assert!(body_of(&response).contains("等待第一屏"));

    canvas.shutdown().await;
}

#[tokio::test]
async fn close_renders_farewell_page() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(options_screen()).await.unwrap();
    canvas.close(CloseReason::UserClosed).await.unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/")).await;
    assert_eq!(status, 200, "graceful close keeps serving the last screen");
    assert!(body_of(&response).contains("画布已关闭"));

    canvas.shutdown().await;
}

#[tokio::test]
async fn session_ended_farewell_uses_session_wording() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(options_screen()).await.unwrap();
    canvas.close(CloseReason::SessionEnded).await.unwrap();

    let (_, response) = send(canvas.port(), &get(canvas.port(), "/")).await;
    assert!(body_of(&response).contains("会话已结束"));

    canvas.shutdown().await;
}

#[tokio::test]
async fn marker_like_text_in_metadata_renders_literally() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    let env = ScreenEnvelope::parse(&json!({
        "title": "use {{TOKEN}} as text",
        "options": [],
        "body_html": ""
    }))
    .unwrap();
    canvas.push(env).await.unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);

    assert!(
        body.contains("use {{TOKEN}} as text"),
        "markers in metadata are literal"
    );
    let token_occurrences = body.matches(canvas.session_token()).count();
    assert_eq!(
        token_occurrences, 1,
        "token appears exactly once (the meta tag)"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn assets_include_heartbeat_and_component_classes() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    let port = canvas.port();

    let (status, response) = send(port, &get(port, "/assets/helper.js")).await;
    assert_eq!(status, 200);
    let js = body_of(&response);
    assert!(js.contains("/api/poll"), "heartbeat poll wired");
    assert!(
        js.contains("连接已断开"),
        "disconnect rendering shipped in helper.js"
    );

    let (status, response) = send(port, &get(port, "/assets/components.css")).await;
    assert_eq!(status, 200);
    let css = body_of(&response);
    for class in [
        ".options",
        ".card",
        ".mockup",
        ".split",
        ".mock-nav",
        ".mock-sidebar",
        ".placeholder",
    ] {
        assert!(css.contains(class), "component class {class} shipped");
    }

    canvas.shutdown().await;
}
