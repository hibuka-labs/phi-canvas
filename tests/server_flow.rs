//! Server flow: bind/push/subscribe/close, and acceptance rows 3 + 4.

mod common;

use std::time::Duration;

use common::{get, post_json, send};
use phi_canvas::{Canvas, CanvasConfig, CanvasError, CloseReason, EventAction, ScreenEnvelope};
use serde_json::json;

fn envelope() -> ScreenEnvelope {
    ScreenEnvelope::parse(&json!({
        "title": "大屏顶部导航，哪个布局更专业？",
        "subtitle": "考虑信息密度和视觉层级",
        "options": [
            { "id": "a", "label": "侧边栏导航" },
            { "id": "b", "label": "顶部横向导航" },
            { "id": "c", "label": "混合式" }
        ],
        "body_html": "<div class=\"mockup\"><div class=\"mock-nav\">nav</div></div>",
        "multiselect": false
    }))
    .unwrap()
}

async fn open_canvas(tmp: &tempfile::TempDir) -> std::sync::Arc<Canvas> {
    let config = CanvasConfig::new(tmp.path().join("canvas"), "sess-test");
    Canvas::open(config).await.expect("canvas opens")
}

#[tokio::test]
async fn open_binds_kernel_port_and_writes_server_info() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;

    let port = canvas.port();
    assert!(port > 0, "kernel-assigned port must be usable");
    assert_eq!(canvas.url(), format!("http://127.0.0.1:{port}"));
    assert!(canvas.is_open());

    let raw = std::fs::read_to_string(tmp.path().join("canvas/server-info")).unwrap();
    let info: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(info["port"], port);
    assert_eq!(info["url"], canvas.url());
    assert_eq!(info["session_id"], "sess-test");
    assert!(
        !raw.contains(canvas.session_token()),
        "row 4 companion: server-info must not carry the token"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn push_assigns_incrementing_screen_seq() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;

    let first = canvas.push(envelope()).await.unwrap();
    assert_eq!(first.screen, 1);
    assert_eq!(first.title, "大屏顶部导航，哪个布局更专业？");
    assert_eq!(first.option_count, 3);
    assert!(!first.display_only);
    assert_eq!(first.url, canvas.url());

    let display = ScreenEnvelope::parse(
        &json!({ "title": "配色方向", "options": [], "body_html": "<p>x</p>" }),
    )
    .unwrap();
    let second = canvas.push(display).await.unwrap();
    assert_eq!(second.screen, 2);
    assert!(second.display_only);
    assert_eq!(second.option_count, 0);

    // Host-side lookup (M2 synthesis): the mailbox maps event option ids back
    // to labels through the stored envelope.
    let stored = canvas
        .screen_envelope(first.screen)
        .expect("pushed screen must be retrievable");
    assert_eq!(stored, envelope());
    assert_eq!(stored.options[1].id, "b");
    assert_eq!(stored.options[1].label, "顶部横向导航");
    assert!(canvas.screen_envelope(99).is_none(), "unknown seq → None");

    canvas.shutdown().await;
}

#[tokio::test]
async fn rebind_recovers_the_last_screen_from_the_store() {
    // 1C row 16 recovery: Ctrl+B starts a fresh server task (the old one is
    // dead), and the canvas must come back on the screen the user was looking
    // at — an empty "等待第一屏" is a broken recovery.
    let tmp = tempfile::tempdir().unwrap();
    let config = CanvasConfig::new(tmp.path().join("canvas"), "sess-test");
    let first = Canvas::open(config.clone()).await.unwrap();
    first.push(envelope()).await.unwrap();
    let last = ScreenEnvelope::parse(
        &json!({ "title": "配色方向", "options": [], "body_html": "<p>chosen</p>" }),
    )
    .unwrap();
    first.push(last).await.unwrap();
    first.shutdown().await; // crash shape: no close reason recorded

    let rebound = Canvas::open(config).await.unwrap();
    assert_ne!(rebound.port(), first.port(), "port 0 never reuses");
    let seqs: Vec<u64> = rebound.pushed_screens().iter().map(|r| r.screen).collect();
    assert_eq!(seqs, vec![1, 2], "history comes back");

    let (status, page) = send(rebound.port(), &get(rebound.port(), "/")).await;
    assert_eq!(status, 200);
    assert!(
        page.contains("配色方向") && !page.contains("等待第一屏"),
        "the rebound page serves the last screen, not the empty state"
    );

    let third = rebound.push(envelope()).await.unwrap();
    assert_eq!(third.screen, 3, "seq continues after the restored history");

    rebound.shutdown().await;
}

#[tokio::test]
async fn pushed_screens_snapshots_history_for_a_late_subscriber() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;

    // Pushes made before anyone subscribes must still be visible — this is
    // exactly the tool-vs-tick race the host polls this for (1C).
    let first = canvas.push(envelope()).await.unwrap();
    let display = ScreenEnvelope::parse(
        &json!({ "title": "配色方向", "options": [], "body_html": "<p>x</p>" }),
    )
    .unwrap();
    let second = canvas.push(display).await.unwrap();

    let snapshot = canvas.pushed_screens();
    assert_eq!(snapshot.len(), 2);
    assert_eq!(snapshot[0].screen, first.screen);
    assert_eq!(snapshot[0].option_count, 3);
    assert!(!snapshot[0].display_only);
    assert_eq!(snapshot[1].screen, second.screen);
    assert_eq!(snapshot[1].title, "配色方向");
    assert!(snapshot[1].display_only);
    assert!(
        snapshot.iter().all(|r| r.url == canvas.url()),
        "every receipt carries the canvas URL"
    );

    // A later push extends the snapshot without reordering (oldest first).
    let third = canvas.push(envelope()).await.unwrap();
    let snapshot = canvas.pushed_screens();
    assert_eq!(
        snapshot.iter().map(|r| r.screen).collect::<Vec<_>>(),
        vec![first.screen, second.screen, third.screen]
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn valid_click_reaches_subscribers_and_persists() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    let receipt = canvas.push(envelope()).await.unwrap();
    let mut rx = canvas.subscribe();

    let body = format!(
        r#"{{"type":"select","screen":{},"option_id":"b"}}"#,
        receipt.screen
    );
    let request = post_json(canvas.port(), "/events", canvas.session_token(), &body);
    let (status, response) = send(canvas.port(), &request).await;
    assert_eq!(status, 204, "accepted click: {response}");

    let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("event must reach the subscriber")
        .expect("channel open");
    assert_eq!(event.seq, 1);
    assert_eq!(event.screen, receipt.screen);
    assert_eq!(
        event.action,
        EventAction::Select {
            option_id: "b".into()
        }
    );

    let raw = std::fs::read_to_string(tmp.path().join("canvas/events")).unwrap();
    assert_eq!(raw.lines().count(), 1);
    assert!(raw.contains(r#""option_id":"b""#));

    canvas.shutdown().await;
}

#[tokio::test]
async fn click_rejects_unknown_screen_and_unknown_option() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    let receipt = canvas.push(envelope()).await.unwrap();

    let body = r#"{"type":"select","screen":99,"option_id":"a"}"#.to_string();
    let request = post_json(canvas.port(), "/events", canvas.session_token(), &body);
    let (status, _) = send(canvas.port(), &request).await;
    assert_eq!(status, 404, "unknown screen");

    let body = format!(
        r#"{{"type":"select","screen":{},"option_id":"zz"}}"#,
        receipt.screen
    );
    let request = post_json(canvas.port(), "/events", canvas.session_token(), &body);
    let (status, _) = send(canvas.port(), &request).await;
    assert_eq!(status, 400, "unknown option id");

    canvas.shutdown().await;
}

#[tokio::test]
async fn session_token_never_lands_on_disk() {
    // Acceptance row 3: after a full flow (open → push → click → close),
    // no file under the store root may contain the session token.
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    let token = canvas.session_token().to_string();

    let receipt = canvas.push(envelope()).await.unwrap();
    let body = format!(
        r#"{{"type":"select","screen":{},"option_id":"a"}}"#,
        receipt.screen
    );
    let request = post_json(canvas.port(), "/events", &token, &body);
    let (status, _) = send(canvas.port(), &request).await;
    assert_eq!(status, 204);
    canvas.close(CloseReason::UserClosed).await.unwrap();

    let mut seen = 0usize;
    let mut stack = vec![tmp.path().join("canvas")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            seen += 1;
            let bytes = std::fs::read(&path).unwrap();
            let text = String::from_utf8_lossy(&bytes);
            assert!(
                !text.contains(&token),
                "token leaked into {}",
                path.display()
            );
        }
    }
    assert!(
        seen >= 4,
        "flow must have written server-info/events/screens, saw {seen} files"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn click_action_must_match_screen_mode() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    let single = canvas.push(envelope()).await.unwrap();
    let display = canvas
        .push(
            ScreenEnvelope::parse(
                &json!({ "title": "配色方向", "options": [], "body_html": "<p>x</p>" }),
            )
            .unwrap(),
        )
        .await
        .unwrap();

    let body = format!(r#"{{"type":"confirm","screen":{}}}"#, single.screen);
    let request = post_json(canvas.port(), "/events", canvas.session_token(), &body);
    let (status, _) = send(canvas.port(), &request).await;
    assert_eq!(status, 400, "confirm requires a multiselect screen");

    let body = format!(r#"{{"type":"skip","screen":{}}}"#, display.screen);
    let request = post_json(canvas.port(), "/events", canvas.session_token(), &body);
    let (status, _) = send(canvas.port(), &request).await;
    assert_eq!(status, 400, "skip on a display-only screen is meaningless");

    let body = format!(
        r#"{{"type":"select","screen":{},"option_id":"a"}}"#,
        display.screen
    );
    let request = post_json(canvas.port(), "/events", canvas.session_token(), &body);
    let (status, _) = send(canvas.port(), &request).await;
    assert_eq!(status, 400, "display-only screens take no selection");

    canvas.shutdown().await;
}

#[tokio::test]
async fn close_stops_accepting_pushes_and_clicks() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(envelope()).await.unwrap();
    canvas.close(CloseReason::SessionEnded).await.unwrap();

    assert!(!canvas.is_open());
    assert!(
        matches!(canvas.push(envelope()).await, Err(CanvasError::Closed)),
        "push after close is an error"
    );

    let body = r#"{"type":"skip","screen":1}"#.to_string();
    let request = post_json(canvas.port(), "/events", canvas.session_token(), &body);
    let (status, _) = send(canvas.port(), &request).await;
    assert_eq!(status, 409, "clicks after close are rejected");

    canvas.shutdown().await;
}

#[tokio::test]
async fn shutdown_stops_the_listener() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    let port = canvas.port();
    canvas.shutdown().await;

    let probe = tokio::net::TcpStream::connect(("127.0.0.1", port)).await;
    assert!(probe.is_err(), "listener must be gone after shutdown");
}
