//! Canvas lifecycle (1C mechanism): push-receipt feed, server-task liveness,
//! and the parameterized idle auto-close (row 19's injectable timeout).

mod common;

use std::time::Duration;

use phi_canvas::{Canvas, CanvasConfig, CloseReason, ScreenEnvelope};
use serde_json::json;

fn envelope() -> ScreenEnvelope {
    ScreenEnvelope::parse(&json!({
        "title": "布局选择",
        "options": [
            { "id": "a", "label": "侧边栏导航" },
            { "id": "b", "label": "顶部横向导航" }
        ]
    }))
    .unwrap()
}

fn display() -> ScreenEnvelope {
    ScreenEnvelope::parse(&json!({
        "title": "参考 mockup",
        "options": [],
        "body_html": "<div class=\"mockup\">x</div>"
    }))
    .unwrap()
}

async fn open_canvas(tmp: &tempfile::TempDir, idle: Option<Duration>) -> std::sync::Arc<Canvas> {
    let config = CanvasConfig::new(tmp.path().join("canvas"), "sess-life").with_idle_timeout(idle);
    Canvas::open(config).await.expect("canvas opens")
}

#[tokio::test]
async fn subscribe_pushes_reports_every_screen() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp, None).await;
    let mut rx = canvas.subscribe_pushes();

    let first = canvas.push(envelope()).await.unwrap();
    let second = canvas.push(display()).await.unwrap();

    let got_first = rx.recv().await.unwrap();
    assert_eq!(got_first.screen, first.screen);
    assert_eq!(got_first.option_count, 2);
    assert!(!got_first.display_only);
    assert_eq!(got_first.title, "布局选择");

    let got_second = rx.recv().await.unwrap();
    assert_eq!(got_second.screen, second.screen);
    assert!(got_second.display_only, "empty options = display-only");

    canvas.shutdown().await;
}

#[tokio::test]
async fn server_task_liveness_tracks_the_join_handle() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp, None).await;

    assert!(canvas.is_alive(), "serving = alive");
    assert!(canvas.is_open());
    assert_eq!(canvas.close_reason(), None);
    assert!(!canvas.is_failed());

    // Graceful close keeps the listener (farewell view) — still alive,
    // and not a failure.
    canvas.close(CloseReason::UserClosed).await.unwrap();
    assert!(!canvas.is_open());
    assert_eq!(canvas.close_reason(), Some(CloseReason::UserClosed));
    assert!(canvas.is_alive(), "farewell serving is not a failure");
    assert!(!canvas.is_failed());
    canvas.shutdown().await;
}

#[tokio::test]
async fn task_death_without_close_is_the_failure_shape() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp, None).await;

    // A bare `shutdown` is the crash shape: task gone, no close recorded.
    // (Session exit closes with SessionEnded first, precisely to avoid it.)
    canvas.shutdown().await;
    assert!(!canvas.is_alive(), "aborted task = dead");
    assert_eq!(canvas.close_reason(), None, "no lifecycle close happened");
    assert!(canvas.is_failed(), "task death without close = failure");
}

#[tokio::test]
async fn idle_timeout_closes_the_canvas() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp, Some(Duration::from_millis(200))).await;

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(canvas.is_open(), "still inside the idle budget");

    tokio::time::sleep(Duration::from_millis(450)).await;
    assert!(!canvas.is_open(), "idle close fires without interaction");
    assert_eq!(canvas.close_reason(), Some(CloseReason::Idle));

    canvas.shutdown().await;
}

#[tokio::test]
async fn click_activity_postpones_the_idle_close() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp, Some(Duration::from_millis(300))).await;
    let receipt = canvas.push(envelope()).await.unwrap();

    // A click every 100ms keeps resetting the clock; the canvas must survive
    // well past the original 300ms budget, then close shortly after the last
    // click.
    for _ in 0..4 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let body = serde_json::json!({
            "type": "select",
            "screen": receipt.screen,
            "option_id": "a"
        })
        .to_string();
        let req = common::post_json(canvas.port(), "/events", canvas.session_token(), &body);
        let (status, _) = common::send(canvas.port(), &req).await;
        assert_eq!(status, 204, "click accepted");
    }
    assert!(canvas.is_open(), "activity kept it open past 300ms");

    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(!canvas.is_open(), "quiet after the last click ⇒ close");
    assert_eq!(canvas.close_reason(), Some(CloseReason::Idle));

    canvas.shutdown().await;
}

#[tokio::test]
async fn push_activity_postpones_the_idle_close() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp, Some(Duration::from_millis(400))).await;

    for _ in 0..3 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        canvas.push(display()).await.unwrap();
        assert!(canvas.is_open(), "agent pushes count as activity");
    }

    tokio::time::sleep(Duration::from_millis(800)).await;
    assert!(!canvas.is_open());
    assert_eq!(canvas.close_reason(), Some(CloseReason::Idle));

    canvas.shutdown().await;
}
