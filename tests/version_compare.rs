//! Version comparison (gate 2 ③): the design so far is a rail on every screen,
//! the compare view lays two versions side by side or wipes one over the other,
//! and a field-level list says what actually changed.
//!
//! Interstitial screens (the waiting screen) never enter that history — the
//! user scrolls the design, not the pauses between it.

mod common;

use common::{body_of, get, send};
use phi_canvas::{Canvas, CanvasConfig, ScreenEnvelope};
use serde_json::json;

async fn open_canvas(tmp: &tempfile::TempDir) -> std::sync::Arc<Canvas> {
    let config = CanvasConfig::new(tmp.path().join("canvas"), "sess-versions");
    Canvas::open(config).await.expect("canvas opens")
}

fn screen(title: &str, body: &str) -> ScreenEnvelope {
    ScreenEnvelope::parse(&json!({
        "title": title,
        "options": [],
        "body_html": body
    }))
    .unwrap()
}

fn waiting() -> ScreenEnvelope {
    // Built the way the host builds it: not a model call, so `interstitial` is
    // set directly.
    let mut env = screen("继续在终端讨论…", "<p>已定</p>");
    env.interstitial = true;
    env
}

/// Two takes on one layout that differ only in a theme knob.
fn spec_screen(title: &str, glow: u64) -> ScreenEnvelope {
    let mut env = ScreenEnvelope::parse(&json!({
        "title": title,
        "options": [{ "id": "a", "label": "就这版" }],
        "screen": {
            "fidelity": "texture",
            "mock": {
                "kind": "rows",
                "children": [{ "kind": "kpi", "id": "rev", "label": "收入", "value": "128,640" }]
            },
            "variants": [{
                "id": "a",
                "theme": {
                    "bg": "#0A1428", "panel": "#122647", "border": "#1D4ED8",
                    "accent": "#00D4FF", "accent2": "#00E5FF",
                    "text": "#7FA6D9", "text_bright": "#E6F1FF",
                    "glow": glow
                }
            }]
        }
    }))
    .unwrap();
    env.materialize_screen().unwrap();
    env
}

// ── the rail (回看) ────────────────────────────────────────────────────────

#[tokio::test]
async fn the_rail_lists_the_design_versions_and_marks_the_current_one() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas
        .push(screen("v1 · 侧边栏", "<p>one</p>"))
        .await
        .unwrap();
    canvas
        .push(screen("v2 · 深色", "<p>two</p>"))
        .await
        .unwrap();
    canvas
        .push(screen("v3 · 玻璃", "<p>three</p>"))
        .await
        .unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/screens/2")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);

    assert!(body.contains("class=\"versions\""), "rail rendered");
    for title in ["v1 · 侧边栏", "v2 · 深色", "v3 · 玻璃"] {
        assert!(body.contains(title), "rail carries {title}");
    }
    assert!(
        body.contains(r#"class="version current" href="/screens/2""#),
        "the screen being looked at is marked"
    );
    assert!(
        body.contains(r#"class="version" href="/screens/1""#),
        "the others are plain links"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn a_single_version_has_no_rail() {
    // History of one is just the present.
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(screen("v1", "<p>one</p>")).await.unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/")).await;
    assert_eq!(status, 200);
    assert!(
        !body_of(&response).contains("class=\"versions\""),
        "no rail with one version"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn the_rail_offers_the_way_back_when_the_pause_is_current() {
    // The pause screen (host waiting push at turn end) supersedes the design
    // before the user ever looks. Then one design version is history, not the
    // present — and the pause must still lead back to it.
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas
        .push(screen("v1 · 侧边栏", "<p>one</p>"))
        .await
        .unwrap();
    canvas.push(waiting()).await.unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);

    assert!(
        body.contains(r#"href="/screens/1""#),
        "the pause must offer the way back to the design: {body}"
    );
    assert!(
        !body.contains("/compare/"),
        "one version is not a compare pair"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn a_look_back_screen_links_forward_to_the_live_one() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas
        .push(screen("v1 · 侧边栏", "<p>one</p>"))
        .await
        .unwrap();
    canvas.push(waiting()).await.unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/screens/1")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);

    assert!(body.contains("查看中"), "it is a look back: {body}");
    assert!(
        body.contains(r#"href="/""#),
        "and it links forward to the screen on offer: {body}"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn interstitial_screens_stay_out_of_the_history() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(screen("v1", "<p>one</p>")).await.unwrap();
    canvas.push(waiting()).await.unwrap();
    canvas.push(screen("v2", "<p>two</p>")).await.unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);

    assert!(
        body.contains("v1") && body.contains("v2"),
        "the design is there"
    );
    assert!(
        !body.contains("继续在终端讨论…"),
        "the waiting screen is not a version"
    );
    assert!(
        body.contains(r#"class="version" href="/screens/1""#)
            && body.contains(r#"class="version current" href="/screens/3""#),
        "the rail jumps over the interstitial seq"
    );

    canvas.shutdown().await;
}

// ── the launch target (what "come look at the design" lands on) ─────────────

#[tokio::test]
async fn push_receipts_name_the_pauses() {
    // The host decides what "come look at the design" lands on — it can only
    // do that if a receipt says which pushes are pauses.
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;

    let design = canvas.push(screen("v1", "<p>one</p>")).await.unwrap();
    assert!(!design.interstitial, "a design push is a version");

    let pause = canvas.push(waiting()).await.unwrap();
    assert!(pause.interstitial, "the pause says so on its receipt");

    canvas.shutdown().await;
}

#[tokio::test]
async fn replayed_receipts_still_name_the_pauses() {
    // The recovery path opens a fresh server over the same store and replays
    // `pushed_screens` into the host's bookkeeping — the pause distinction has
    // to survive that, or the relaunch target lands back on the pause.
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(screen("v1", "<p>one</p>")).await.unwrap();
    canvas.push(waiting()).await.unwrap();
    canvas.shutdown().await;

    let rebound = open_canvas(&tmp).await;
    let receipts = rebound.pushed_screens();
    assert_eq!(receipts.len(), 2, "history is recovered: {receipts:?}");
    assert!(!receipts[0].interstitial, "the design");
    assert!(receipts[1].interstitial, "and the pause");

    rebound.shutdown().await;
}

// ── the compare view (并排 / 滑块) ─────────────────────────────────────────

#[tokio::test]
async fn compare_shows_the_two_most_recent_versions_by_default() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas
        .push(screen("v1 · 侧边栏", "<p class=\"one\">one</p>"))
        .await
        .unwrap();
    canvas
        .push(screen("v2 · 深色", "<p class=\"two\">two</p>"))
        .await
        .unwrap();
    canvas
        .push(screen("v3 · 玻璃", "<p class=\"three\">three</p>"))
        .await
        .unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/compare")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);

    assert!(
        body.contains("v2 · 深色") && body.contains("v3 · 玻璃"),
        "defaults"
    );
    assert!(
        !body.contains("<p class=\"one\">"),
        "v1 is not in the default pair"
    );
    assert!(
        body.contains("<p class=\"two\">") && body.contains("<p class=\"three\">"),
        "both bodies rendered"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn compare_rails_the_pair_the_user_asks_for() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas
        .push(screen("v1", "<p class=\"one\">one</p>"))
        .await
        .unwrap();
    canvas
        .push(screen("v2", "<p class=\"two\">two</p>"))
        .await
        .unwrap();
    canvas
        .push(screen("v3", "<p class=\"three\">three</p>"))
        .await
        .unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/compare/1/3")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);
    assert!(body.contains("<p class=\"one\">") && body.contains("<p class=\"three\">"));

    // A seq that is not a version falls back instead of rendering a hole.
    let (status, response) = send(canvas.port(), &get(canvas.port(), "/compare/99/98")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);
    assert!(
        body.contains("<p class=\"two\">") && body.contains("<p class=\"three\">"),
        "unknown seqs fall back to the recent pair"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn compare_has_both_layouts() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas
        .push(screen("v1", "<p class=\"one\">one</p>"))
        .await
        .unwrap();
    canvas
        .push(screen("v2", "<p class=\"two\">two</p>"))
        .await
        .unwrap();
    let port = canvas.port();

    // Side by side is two panes; the slider is one stack with a handle.
    let (status, response) = send(port, &get(port, "/compare/1/2/split")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);
    assert!(body.contains("class=\"compare split\""), "{body}");
    assert!(
        body.matches("class=\"pane\"").count() == 2,
        "one pane each side"
    );

    let (status, response) = send(port, &get(port, "/compare/1/2/slide")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);
    assert!(
        body.contains("class=\"wipe-stack\""),
        "stacked for the wipe"
    );
    assert!(body.contains("wipe-cover"), "the older one is clipped");
    assert!(body.contains("id=\"wipe-range\""), "the handle is there");
    assert!(
        body.contains("/assets/compare.js"),
        "the slider needs its own helper"
    );

    // Both modes are offered from either page.
    assert!(body.contains("/compare/1/2/split") && body.contains("/compare/1/2/slide"));

    // An unknown mode is a bad URL, not a silent fallback.
    let (status, _) = send(port, &get(port, "/compare/1/2/nonsense")).await;
    assert_eq!(status, 404);

    canvas.shutdown().await;
}

#[tokio::test]
async fn compare_never_takes_a_query_string() {
    // The M4 guard rejects every `?…` outright — that is how the session token
    // stays out of URLs — so the pair and the layout travel as path segments.
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(screen("v1", "<p>one</p>")).await.unwrap();
    canvas.push(screen("v2", "<p>two</p>")).await.unwrap();

    let (status, _) = send(canvas.port(), &get(canvas.port(), "/compare?a=1&b=2")).await;
    assert_eq!(status, 400, "query strings are refused everywhere");

    canvas.shutdown().await;
}

#[tokio::test]
async fn compare_is_looking_not_picking() {
    // History the user browses must not be able to post a choice: no token on
    // the page, no click pipeline in its script.
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(screen("v1", "<p>one</p>")).await.unwrap();
    canvas.push(screen("v2", "<p>two</p>")).await.unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/compare")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);
    assert!(!body.contains("canvas-token"), "no token on a compare page");
    assert!(!body.contains("helper.js"), "no click pipeline either");

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/assets/compare.js")).await;
    assert_eq!(status, 200);
    let js = body_of(&response);
    assert!(
        !js.contains("fetch(") && !js.contains("XMLHttpRequest"),
        "compare.js cannot post anything: {js}"
    );

    canvas.shutdown().await;
}

#[tokio::test]
async fn compare_needs_two_versions() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(screen("v1", "<p>one</p>")).await.unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/compare")).await;
    assert_eq!(status, 200);
    assert!(body_of(&response).contains("还没有可对比的版本"));

    canvas.shutdown().await;
}

// ── 改了什么 (field-level change list) ─────────────────────────────────────

#[tokio::test]
async fn compare_lists_the_fields_that_changed() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(spec_screen("v1 · 无光", 0)).await.unwrap();
    canvas.push(spec_screen("v2 · 发光", 2)).await.unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/compare")).await;
    assert_eq!(status, 200);
    let body = body_of(&response);

    assert!(body.contains("改了什么"), "the change list is offered");
    assert!(
        body.contains("glow") && body.contains("0") && body.contains("2"),
        "the knob that moved is named: {body}"
    );
    assert!(body.contains("title"), "and so is the title that moved");

    canvas.shutdown().await;
}

#[tokio::test]
async fn two_identical_screens_have_nothing_to_report() {
    let tmp = tempfile::tempdir().unwrap();
    let canvas = open_canvas(&tmp).await;
    canvas.push(spec_screen("v1", 1)).await.unwrap();
    // Same metadata, different body: the body is compared by looking at it.
    canvas.push(spec_screen("v1", 1)).await.unwrap();

    let (status, response) = send(canvas.port(), &get(canvas.port(), "/compare")).await;
    assert_eq!(status, 200);
    assert!(!body_of(&response).contains("改了什么"));

    canvas.shutdown().await;
}
