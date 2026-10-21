//! Session-directory persistence: `screens/`, `events`, `server-info`.
//! Also pins the event wire shapes accepted from `POST /events`.

use phi_canvas::{CanvasEvent, CanvasStore, EventAction, EventInput, ScreenEnvelope, ServerInfo};
use serde_json::{Value, json};

fn env() -> ScreenEnvelope {
    ScreenEnvelope::parse(&json!({
        "title": "大屏顶部导航，哪个布局更专业？",
        "subtitle": "考虑信息密度和视觉层级",
        "options": [
            { "id": "a", "label": "侧边栏导航", "note": "筛选常驻" },
            { "id": "b", "label": "顶部横向导航" }
        ],
        "body_html": "<div class=\"mockup\"><div class=\"mock-nav\"></div></div>",
        "multiselect": true
    }))
    .unwrap()
}

#[test]
fn open_creates_layout() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("canvas").join("sess1");
    let store = CanvasStore::open(&root).unwrap();

    assert!(root.is_dir(), "store root created");
    assert!(root.join("screens").is_dir(), "screens/ created");
    assert_eq!(store.root(), root);
}

#[test]
fn server_info_persists_all_fields_and_no_token_key() {
    let tmp = tempfile::tempdir().unwrap();
    let store = CanvasStore::open(tmp.path().join("c")).unwrap();

    store
        .write_server_info(&ServerInfo {
            session_id: "sess1".into(),
            url: "http://127.0.0.1:43123".into(),
            port: 43123,
            started_at_ms: 1_760_000_000_000,
        })
        .unwrap();

    let raw = std::fs::read_to_string(store.root().join("server-info")).unwrap();
    let parsed: Value = serde_json::from_str(&raw).unwrap();
    let obj = parsed.as_object().unwrap();

    assert_eq!(obj["session_id"], "sess1");
    assert_eq!(obj["url"], "http://127.0.0.1:43123");
    assert_eq!(obj["port"], 43123);
    assert_eq!(obj["started_at_ms"], 1_760_000_000_000u64);
    assert!(
        !obj.contains_key("token") && !raw.contains("token"),
        "server-info must not carry any token field: {raw}"
    );
}

#[test]
fn screen_is_stored_as_body_fragment_plus_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    let store = CanvasStore::open(tmp.path().join("c")).unwrap();
    let mut env = env();

    store.save_screen(1, &mut env).unwrap();

    let fragment = std::fs::read_to_string(store.root().join("screens/0001.html")).unwrap();
    assert_eq!(
        fragment, env.body_html,
        "fragment file holds exactly body_html"
    );

    let meta_raw = std::fs::read_to_string(store.root().join("screens/0001.json")).unwrap();
    let meta: Value = serde_json::from_str(&meta_raw).unwrap();
    assert_eq!(meta["title"], "大屏顶部导航，哪个布局更专业？");
    assert_eq!(meta["multiselect"], true);
    assert_eq!(meta["options"][1]["label"], "顶部横向导航");
    assert!(
        !meta_raw.contains("mock-nav"),
        "metadata file must not duplicate the body: {meta_raw}"
    );
}

#[test]
fn screen_roundtrip_restores_full_envelope() {
    let tmp = tempfile::tempdir().unwrap();
    let store = CanvasStore::open(tmp.path().join("c")).unwrap();
    let mut env = env();

    store.save_screen(7, &mut env).unwrap();
    let loaded = store.load_screen(7).unwrap();
    assert_eq!(loaded, env);
}

#[test]
fn load_all_screens_replays_saved_screens_in_seq_order() {
    // Rebind recovery reads the whole directory back (1C row 16): gaps are
    // allowed, a corrupt sidecar is skipped, order is ascending by seq.
    let tmp = tempfile::tempdir().unwrap();
    let store = CanvasStore::open(tmp.path().join("c")).unwrap();
    let mut env = env();

    store.save_screen(1, &mut env).unwrap();
    store.save_screen(3, &mut env).unwrap();
    std::fs::write(store.root().join("screens/0002.json"), b"{not json").unwrap();

    let loaded = store.load_all_screens();
    let seqs: Vec<u64> = loaded.iter().map(|(seq, _)| *seq).collect();
    assert_eq!(
        seqs,
        vec![1, 3],
        "ascending by seq, corrupt sidecar skipped"
    );
    assert_eq!(loaded[0].1, env, "full envelope reassembles");
}

#[test]
fn events_append_as_jsonl_in_order() {
    let tmp = tempfile::tempdir().unwrap();
    let store = CanvasStore::open(tmp.path().join("c")).unwrap();

    store
        .append_event(&CanvasEvent {
            seq: 1,
            screen: 1,
            at_ms: 100,
            action: EventAction::Select {
                option_id: "b".into(),
            },
        })
        .unwrap();
    store
        .append_event(&CanvasEvent {
            seq: 2,
            screen: 1,
            at_ms: 200,
            action: EventAction::Confirm,
        })
        .unwrap();

    let raw = std::fs::read_to_string(store.root().join("events")).unwrap();
    let lines: Vec<&str> = raw.lines().collect();
    assert_eq!(lines.len(), 2);

    let first: CanvasEvent = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(first.seq, 1);
    assert_eq!(
        first.action,
        EventAction::Select {
            option_id: "b".into()
        }
    );

    let second: CanvasEvent = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(second.action, EventAction::Confirm);
}

#[test]
fn event_input_parses_flat_wire() {
    let input: EventInput = serde_json::from_value(json!({
        "type": "select",
        "screen": 3,
        "option_id": "a"
    }))
    .unwrap();
    assert_eq!(input.screen, 3);
    assert_eq!(
        input.action,
        EventAction::Select {
            option_id: "a".into()
        }
    );

    let input: EventInput = serde_json::from_value(json!({ "type": "skip", "screen": 1 })).unwrap();
    assert_eq!(input.action, EventAction::Skip);

    let input: EventInput =
        serde_json::from_value(json!({ "type": "confirm", "screen": 1 })).unwrap();
    assert_eq!(input.action, EventAction::Confirm);
}

#[test]
fn event_input_rejects_unknown_or_incomplete_shapes() {
    assert!(
        serde_json::from_value::<EventInput>(json!({ "type": "nonsense", "screen": 1 })).is_err()
    );
    assert!(
        serde_json::from_value::<EventInput>(json!({ "type": "select", "screen": 1 })).is_err(),
        "select requires option_id"
    );
    assert!(
        serde_json::from_value::<EventInput>(json!({ "type": "select", "option_id": "a" }))
            .is_err(),
        "screen is required"
    );
}

// ── reference image persistence ────────────────────────────────────────────

fn ref_env(src: &str, caption: Option<&str>) -> ScreenEnvelope {
    let mut reference = serde_json::json!({ "src": src });
    if let Some(caption) = caption {
        reference["caption"] = json!(caption);
    }
    ScreenEnvelope::parse(&json!({
        "title": "参考这个方向改，哪个更像？",
        "options": [{ "id": "a", "label": "更接近" }],
        "body_html": "<div class=\"mockup\"></div>",
        "reference": reference
    }))
    .unwrap()
}

#[test]
fn a_local_reference_is_copied_beside_the_screen_and_resolved_to_a_route() {
    let tmp = tempfile::tempdir().unwrap();
    let store = CanvasStore::open(tmp.path().join("canvas")).unwrap();

    let png = tmp.path().join("zhentai.png");
    std::fs::write(&png, b"\x89PNG-not-really").unwrap();

    let mut env = ref_env(png.to_str().unwrap(), Some("正泰首页"));
    store.save_screen(3, &mut env).unwrap();

    // The page loads a route, never the user's path.
    assert_eq!(
        env.reference.as_ref().unwrap().resolved.as_deref(),
        Some("/screens/3/reference")
    );
    // …and the bytes are in the store, where a rebind will find them.
    assert_eq!(store.read_reference(3).unwrap(), b"\x89PNG-not-really");
    // The recorded source is still what the model sent.
    assert_eq!(env.reference.as_ref().unwrap().src, png.to_str().unwrap());

    // Round trip: the sidecar keeps the resolved URL.
    let loaded = store.load_screen(3).unwrap();
    assert_eq!(loaded.reference, env.reference);
}

#[test]
fn a_uri_reference_is_handed_to_the_page_verbatim() {
    let tmp = tempfile::tempdir().unwrap();
    let store = CanvasStore::open(tmp.path().join("canvas")).unwrap();

    for src in [
        "https://example.com/zhentai.png",
        "data:image/png;base64,aGVsbG8=",
    ] {
        let mut env = ref_env(src, None);
        store.save_screen(1, &mut env).unwrap();
        assert_eq!(
            env.reference.as_ref().unwrap().resolved.as_deref(),
            Some(src),
            "{src} must pass through"
        );
        // Nothing copied: the store holds no bytes for a remote reference.
        assert!(store.read_reference(1).is_none());
    }
}

#[test]
fn a_missing_reference_file_fails_the_save_rather_than_dropping_it() {
    // A reference the user asked for must not quietly vanish from the screen.
    let tmp = tempfile::tempdir().unwrap();
    let store = CanvasStore::open(tmp.path().join("canvas")).unwrap();

    let mut env = ref_env("/no/such/zhentai.png", None);
    let err = match store.save_screen(1, &mut env) {
        Ok(()) => panic!("an unreadable reference must fail the save"),
        Err(err) => err,
    };
    assert!(err.to_string().contains("zhentai.png"), "{err}");
}
