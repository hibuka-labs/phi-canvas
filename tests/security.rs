//! Acceptance row 2: forged requests are rejected.
//!
//! Threat: any web page can POST to `localhost:PORT`; a click is a design
//! answer (answer-injection channel, M4). Checks: session token (constant
//! time), Origin, Host, and no token-in-URL channel.

mod common;

use common::{get, post_json, raw_request, send};
use phi_canvas::{Canvas, CanvasConfig, ScreenEnvelope};
use serde_json::json;

async fn open_canvas(tmp: &tempfile::TempDir) -> std::sync::Arc<Canvas> {
    let config = CanvasConfig::new(tmp.path().join("canvas"), "sess-sec");
    Canvas::open(config).await.expect("canvas opens")
}

fn envelope() -> ScreenEnvelope {
    ScreenEnvelope::parse(&json!({
        "title": "pick",
        "options": [{ "id": "a", "label": "one" }],
        "body_html": ""
    }))
    .unwrap()
}

const CLICK: &str = r#"{"type":"select","screen":1,"option_id":"a"}"#;

#[tokio::test]
async fn post_without_token_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(envelope()).await.unwrap();

    let request = raw_request(
        "POST",
        "/events",
        &[
            ("Host", format!("127.0.0.1:{}", canvas.port())),
            ("Content-Type", "application/json".into()),
        ],
        CLICK,
    );
    let (status, _) = send(canvas.port(), &request).await;
    assert_eq!(status, 401, "POST requires the session token");

    canvas.shutdown().await;
}

#[tokio::test]
async fn post_with_wrong_token_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(envelope()).await.unwrap();

    let mut forged = canvas.session_token().to_string();
    forged.replace_range(0..1, if forged.starts_with('0') { "1" } else { "0" });
    let request = post_json(canvas.port(), "/events", &forged, CLICK);
    let (status, _) = send(canvas.port(), &request).await;
    assert_eq!(status, 401);

    canvas.shutdown().await;
}

#[tokio::test]
async fn post_with_foreign_origin_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(envelope()).await.unwrap();

    let request = raw_request(
        "POST",
        "/events",
        &[
            ("Host", format!("127.0.0.1:{}", canvas.port())),
            ("Origin", "https://evil.example".into()),
            ("Content-Type", "application/json".into()),
            ("X-Canvas-Token", canvas.session_token().into()),
        ],
        CLICK,
    );
    let (status, _) = send(canvas.port(), &request).await;
    assert_eq!(
        status, 403,
        "cross-origin POST refused even with a valid token"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn foreign_host_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(envelope()).await.unwrap();
    let port = canvas.port();

    for (method, target, body) in [
        ("GET", "/".to_string(), String::new()),
        ("POST", "/events".to_string(), CLICK.to_string()),
    ] {
        let request = raw_request(
            method,
            &target,
            &[
                ("Host", "evil.example".into()),
                ("Content-Type", "application/json".into()),
                ("X-Canvas-Token", canvas.session_token().into()),
            ],
            &body,
        );
        let (status, _) = send(port, &request).await;
        assert_eq!(
            status, 403,
            "{method} {target} with forged Host must be refused"
        );
    }

    canvas.shutdown().await;
}

#[tokio::test]
async fn token_in_url_query_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(envelope()).await.unwrap();
    let port = canvas.port();
    let token = canvas.session_token();

    // Attack: smuggle the token through the URL (history/Referer/log leak).
    let target = format!("/events?token={token}");
    let request = raw_request(
        "POST",
        &target,
        &[
            ("Host", format!("127.0.0.1:{port}")),
            ("Content-Type", "application/json".into()),
        ],
        CLICK,
    );
    let (status, _) = send(port, &request).await;
    assert!(
        status == 400 || status == 401 || status == 403,
        "token-in-query refused, got {status}"
    );

    // Even a fully valid POST is refused once a query string rides along.
    let request = post_json(port, &target, token, CLICK);
    let (status, _) = send(port, &request).await;
    assert_eq!(status, 400, "query strings are never an auth channel");

    canvas.shutdown().await;
}

#[tokio::test]
async fn missing_host_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(envelope()).await.unwrap();

    let request = raw_request("GET", "/", &[], "");
    let (status, _) = send(canvas.port(), &request).await;
    assert!(
        status >= 400,
        "requests without Host must be refused, got {status}"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn get_needs_no_token_but_post_does() {
    // The token is injected into the served page at request time (M4), so GET
    // is Host-gated only. This pins that model — POST is the gated write path.
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(envelope()).await.unwrap();
    let port = canvas.port();

    let (status, _) = send(port, &get(port, "/")).await;
    assert_eq!(status, 200, "page loads without a token");

    let request = post_json(port, "/events", "", CLICK);
    let (status, _) = send(port, &request).await;
    assert_eq!(status, 401, "empty token is not a token");

    canvas.shutdown().await;
}

#[tokio::test]
async fn foreign_origin_on_get_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    let port = canvas.port();

    let request = raw_request(
        "GET",
        "/",
        &[
            ("Host", format!("127.0.0.1:{port}")),
            ("Origin", "https://evil.example".into()),
        ],
        "",
    );
    let (status, _) = send(port, &request).await;
    assert_eq!(status, 403);

    canvas.shutdown().await;
}

#[tokio::test]
async fn localhost_host_variant_is_accepted() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(envelope()).await.unwrap();
    let port = canvas.port();

    let request = raw_request("GET", "/", &[("Host", format!("localhost:{port}"))], "");
    let (status, _) = send(port, &request).await;
    assert_eq!(status, 200);

    canvas.shutdown().await;
}
