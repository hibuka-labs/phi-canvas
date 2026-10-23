//! Frame rendering: the page a human actually looks at (acceptance row 5 is
//! the human's 30-second pass; these pin its structure so regressions are
//! caught mechanically).

mod common;

use common::{body_of, get, post_json, send};
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
async fn the_next_screen_shows_a_receipt_of_the_pick() {
    // The helper reloads the page on every screen change, so the click's own
    // highlight dies with the old page. The receipt is server-rendered so the
    // next screen still answers "what did I pick?".
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    let first = canvas.push(options_screen()).await.unwrap();
    let port = canvas.port();

    let body = format!(
        r#"{{"type":"select","screen":{},"option_id":"a"}}"#,
        first.screen
    );
    let request = post_json(port, "/events", canvas.session_token(), &body);
    let (status, _) = send(port, &request).await;
    assert_eq!(status, 204, "click accepted");

    // The pick's own screen keeps its question clean — the receipt is about
    // the *previous* answer, so it only appears on the screen after.
    let (_, response) = send(port, &get(port, "/")).await;
    assert!(
        !body_of(&response).contains("上一问已选"),
        "no receipt on the screen that was just answered"
    );

    canvas.push(options_screen()).await.unwrap();
    let (_, response) = send(port, &get(port, "/")).await;
    let body = body_of(&response);
    assert!(
        body.contains("上一问已选：侧边栏导航"),
        "the pick survives the page turn: {body}"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn a_skip_receipt_says_skip() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    let first = canvas.push(options_screen()).await.unwrap();
    let port = canvas.port();

    let body = format!(r#"{{"type":"skip","screen":{}}}"#, first.screen);
    let request = post_json(port, "/events", canvas.session_token(), &body);
    let (status, _) = send(port, &request).await;
    assert_eq!(status, 204, "skip accepted");

    canvas.push(options_screen()).await.unwrap();
    let (_, response) = send(port, &get(port, "/")).await;
    assert!(
        body_of(&response).contains("上一问已选：跳过"),
        "a skip is named as 跳过"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn empty_canvas_shows_waiting_page() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);
    assert!(body.contains("等待第一屏"));
    // The page promises the first screen "会自动出现在这里" — so it must chase
    // the current screen. Live marks the page as one helper.js reloads on a
    // seq change; a waiting page marked look-only never picked up the push
    // that landed after the browser opened (it needed Ctrl+B to change).
    assert!(
        body.contains("canvas-live\" content=\"1"),
        "the waiting page chases the first screen"
    );
    assert!(
        body.contains("/assets/helper.js"),
        "the poller that does the chasing is on the page"
    );

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
    let body = body_of(&response);
    assert!(body.contains("画布已关闭"));
    // A farewell is the end of the road — nothing to chase.
    assert!(
        body.contains("canvas-live\" content=\"0"),
        "the farewell page does not chase"
    );

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

// ── reference beside the candidates (参考物贴片) ───────────────────────────

fn reference_screen(src: &str, caption: Option<&str>) -> ScreenEnvelope {
    let mut reference = json!({ "src": src });
    if let Some(caption) = caption {
        reference["caption"] = json!(caption);
    }
    ScreenEnvelope::parse(&json!({
        "title": "参考这个方向改，哪个更像？",
        "options": [
            { "id": "a", "label": "更接近" },
            { "id": "b", "label": "差一点" }
        ],
        "body_html": "<div class=\"cards\"><div class=\"card\">A</div></div>",
        "reference": reference
    }))
    .unwrap()
}

#[tokio::test]
async fn a_reference_renders_beside_the_candidates() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    let src = "data:image/png;base64,aGVsbG8=";
    canvas
        .push(reference_screen(src, Some("正泰首页 · 2026-10")))
        .await
        .unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);

    assert!(
        body.contains("class=\"reference\""),
        "reference block present"
    );
    assert!(
        body.contains(&format!("src=\"{src}\"")),
        "src handed to the page"
    );
    assert!(
        body.contains("<figcaption>正泰首页 · 2026-10</figcaption>"),
        "caption rendered"
    );
    // The reference and the body share a stage row — 「摆在旁边」 is a layout,
    // not a second page.
    assert!(body.contains("class=\"stage\""), "stage wrapper present");
    assert!(
        body.contains("class=\"cards\""),
        "candidates still rendered"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn a_screen_without_a_reference_has_no_reference_block() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(options_screen()).await.unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);

    assert!(
        !body.contains("class=\"reference\""),
        "no reference block without a reference"
    );
    assert!(body.contains("mock-nav"), "body unaffected");

    canvas.shutdown().await;
}

#[tokio::test]
async fn reference_caption_and_src_are_html_escaped() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas
        .push(reference_screen(
            "https://x.test/a\".png",
            Some("<b>bold</b>"),
        ))
        .await
        .unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);

    assert!(
        !body.contains("<b>bold</b>"),
        "caption must never be raw markup"
    );
    assert!(body.contains("&lt;b&gt;bold&lt;/b&gt;"), "escaped caption");
    assert!(
        !body.contains("src=\"https://x.test/a\".png\""),
        "a quote in src must not break out of the attribute"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn the_reference_route_serves_the_copied_bytes_and_nothing_else() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;

    // A local file: the host copies it, the page loads the route.
    let png = tmp.path().join("ref.png");
    std::fs::write(&png, b"\x89PNG-bytes-here").unwrap();
    canvas
        .push(reference_screen(png.to_str().unwrap(), None))
        .await
        .unwrap();

    let port = canvas.port();
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect");
    stream
        .write_all(common::get(port, "/screens/1/reference").as_bytes())
        .await
        .unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    let head = String::from_utf8_lossy(&buf).into_owned();

    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert!(
        head.to_ascii_lowercase()
            .contains("content-type: image/png"),
        "served as an image: {head}"
    );
    assert!(
        buf.windows(b"\x89PNG-bytes-here".len())
            .any(|w| w == b"\x89PNG-bytes-here"),
        "the copied bytes are what get served"
    );

    // No copy for that seq → 404, not a walk of the filesystem.
    let (status, _) = send(port, &common::get(port, "/screens/999/reference")).await;
    assert_eq!(status, 404);

    canvas.shutdown().await;
}

/// The antd island runtime is a per-page guest: a body carrying `m-antd-slot`
/// loads it (and the asset is served), a greybox body never sees the script —
/// a wireframe page must stay exactly the page it was.
#[tokio::test]
async fn island_pages_load_the_runtime_and_greybox_pages_do_not() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;

    let islands = ScreenEnvelope::parse(&json!({
        "title": "上色",
        "options": [{ "id": "a", "label": "方案 A" }],
        "body_html": "<div class=\"m-antd-slot\" data-antd-kind=\"button\" data-antd='{}'></div>"
    }))
    .unwrap();
    canvas.push(islands).await.unwrap();
    let port = canvas.port();

    let (_, response) = send(port, &get(port, "/")).await;
    assert!(
        body_of(&response).contains("/assets/antd-runtime.js"),
        "island body pulls the runtime: {}",
        body_of(&response)
    );

    let (status, asset) = send(port, &get(port, "/assets/antd-runtime.js")).await;
    assert_eq!(status, 200);
    let js = body_of(&asset);
    assert!(
        js.len() > 100_000,
        "the bundled runtime is the real bundle: {} bytes",
        js.len()
    );

    let greybox = ScreenEnvelope::parse(&json!({
        "title": "线框",
        "options": [{ "id": "a", "label": "方案 A" }],
        "body_html": "<div class=\"m-kpi\">收入</div>"
    }))
    .unwrap();
    canvas.push(greybox).await.unwrap();

    let (_, response) = send(port, &get(port, "/")).await;
    let body = body_of(&response);
    assert!(
        !body.contains("antd-runtime.js"),
        "greybox body must not pull the runtime: {body}"
    );

    canvas.shutdown().await;
}

/// Viewing mode (M4): a historical version shows what was asked, not a
/// second chance to answer it — the question is drawn as a record (no
/// `data-choice` anywhere — including the body's own pick markers, which is
/// what spec variant cards carry) and the page is marked not-live so the
/// helper neither posts clicks nor chases the current screen.
#[tokio::test]
async fn a_historical_version_renders_as_a_record_not_a_pick_screen() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    // The body carries its own pick markers, like a spec's variant cards do.
    let spatial = ScreenEnvelope::parse(&json!({
        "title": "大屏顶部导航，哪个布局更专业？",
        "options": [{ "id": "a", "label": "侧边栏导航" }, { "id": "b", "label": "顶部横向导航" }],
        "body_html": "<div class=\"cards\"><div class=\"card\" data-choice=\"a\">A</div><div class=\"card\" data-choice=\"b\">B</div></div>"
    }))
    .unwrap();
    let first = canvas.push(spatial).await.unwrap();
    canvas.push(options_screen()).await.unwrap();
    let port = canvas.port();

    let (_, response) = send(port, &get(port, &format!("/screens/{}", first.screen))).await;
    let body = body_of(&response);
    assert!(
        body.contains("大屏顶部导航，哪个布局更专业？"),
        "the question is still on the page: {body}"
    );
    assert!(
        !body.contains("data-choice"),
        "a record is not clickable — body pick markers included: {body}"
    );
    assert!(
        body.contains("data-view=\"a\""),
        "the marker is renamed, not deleted: {body}"
    );
    assert!(
        !body.contains("data-action=\"skip\""),
        "no pick actions on a look-back: {body}"
    );
    assert!(
        body.contains("canvas-live\" content=\"0"),
        "the helper must see a not-live page: {body}"
    );
    assert!(
        body.contains("<body class=\"view-only\">"),
        "and the page wears the record face: {body}"
    );
    assert!(body.contains("查看中"), "the indicator says so: {body}");

    let (_, response) = send(port, &get(port, "/")).await;
    let body = body_of(&response);
    assert!(
        body.contains("data-choice=\"a\""),
        "the screen on offer is still clickable: {body}"
    );
    assert!(
        body.contains("canvas-live\" content=\"1"),
        "and marked live: {body}"
    );

    canvas.shutdown().await;
}
