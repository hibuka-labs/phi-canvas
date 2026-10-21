//! ScreenSpec parsing and validation: the closed vocabulary, the token
//! ranges, and the reference contract behind `emph` / `content`.

use phi_canvas::{Fidelity, MockNode, ScreenSpec, SpecError};
use serde_json::{Value, json};

/// A spec that exercises every layer: shared tree, two themed variants,
/// emphasis, and a copy override. Reused as the happy path.
fn full() -> Value {
    json!({
        "fidelity": "color",
        "mock": {
            "kind": "frame",
            "id": "shell",
            "title": "数据总览",
            "body": {
                "kind": "cols",
                "ratio": [1, 2],
                "children": [
                    {
                        "kind": "sidebar",
                        "id": "rail",
                        "items": ["首页", "报表"]
                    },
                    {
                        "kind": "rows",
                        "children": [
                            {
                                "kind": "cols",
                                "children": [
                                    { "kind": "kpi", "id": "rev", "label": "收入", "value": "128,640", "delta": "+12.4%" },
                                    { "kind": "ring", "id": "goal", "pct": 86, "label": "完成度" }
                                ]
                            },
                            { "kind": "hero", "id": "hero", "text": "128,640" },
                            { "kind": "sparkline", "id": "trend", "shape": "up" },
                            { "kind": "bars", "id": "top", "items": [
                                { "label": "华东", "value": 90 },
                                { "label": "华南", "value": 60 }
                            ] },
                            { "kind": "viz", "id": "map", "form": "map", "label": "区域分布" }
                        ]
                    }
                ]
            }
        },
        "notice": "mock 数据是装饰性的",
        "variants": [
            {
                "id": "deep-blue",
                "theme": {
                    "bg": "#0A1428", "panel": "#122647", "border": "#1D4ED8",
                    "accent": "#00D4FF", "accent2": "#00E5FF",
                    "text": "#7FA6D9", "text_bright": "#E6F1FF",
                    "surface": "glass", "frost": 2, "elevation": 3, "edge": 2, "glow": 1
                },
                "emph": ["hero"],
                "content": { "hero": "96,120" }
            },
            {
                "id": "neon-purple",
                "theme": { "bg": "#12071F", "accent": "#C026D3" },
                "emph": ["goal"]
            }
        ]
    })
}

fn parse(v: Value) -> Result<ScreenSpec, SpecError> {
    ScreenSpec::parse(&v)
}

#[test]
fn parses_the_full_vocabulary() {
    let spec = parse(full()).expect("full spec");
    assert_eq!(spec.fidelity, Fidelity::Color);
    assert_eq!(spec.variants.len(), 2);
    assert_eq!(spec.variants[0].emph, vec!["hero".to_string()]);
    assert_eq!(
        spec.variants[0].content.get("hero").map(String::as_str),
        Some("96,120")
    );
    assert_eq!(
        spec.variants[1].theme.as_ref().unwrap().accent.as_deref(),
        Some("#C026D3")
    );
    assert!(spec.variants[0].theme.as_ref().unwrap().surface.is_some());
}

#[test]
fn wire_names_match_the_documented_vocabulary() {
    // `box` and `viz.form` are the published names; the Rust field for the
    // latter is `form` because `kind` is the discriminator.
    let spec = parse(json!({
        "fidelity": "wireframe",
        "mock": {
            "kind": "cols",
            "children": [
                { "kind": "box", "w": "24%", "h": "28%", "text": "面板1" },
                { "kind": "placeholder", "text": "内容区" },
                { "kind": "viz", "form": "funnel", "label": "转化" },
                { "kind": "sparkline", "shape": "down" },
                { "kind": "input", "text": "搜索" },
                { "kind": "button", "text": "导出" },
                { "kind": "text", "text": "图例" },
                { "kind": "swatches" }
            ]
        }
    }))
    .expect("wire names");

    let mut kinds = Vec::new();
    spec.for_each_node(|node| {
        kinds.push(match node {
            MockNode::Cols { .. } => "cols",
            MockNode::Region { .. } => "box",
            MockNode::Placeholder { .. } => "placeholder",
            MockNode::Viz { .. } => "viz",
            MockNode::Sparkline { .. } => "sparkline",
            MockNode::Input { .. } => "input",
            MockNode::Button { .. } => "button",
            MockNode::Text { .. } => "text",
            MockNode::Swatches { .. } => "swatches",
            other => panic!("unexpected node {other:?}"),
        });
    });
    assert_eq!(
        kinds,
        vec![
            "cols",
            "box",
            "placeholder",
            "viz",
            "sparkline",
            "input",
            "button",
            "text",
            "swatches"
        ]
    );
}

#[test]
fn mobile_chrome_kinds_use_the_documented_wire_names() {
    let spec = parse(json!({
        "fidelity": "wireframe",
        "mock": {
            "kind": "phone_shell",
            "body": {
                "kind": "rows",
                "children": [
                    { "kind": "topbar", "title": "设置", "back": true, "action": "编辑" },
                    { "kind": "list_row", "title": "通知", "note": "推送与声音", "meta": "开", "chevron": true, "avatar": true },
                    { "kind": "switch", "label": "深色模式", "on": true },
                    { "kind": "segment", "items": ["日", "周", "月"], "active": 1 },
                    { "kind": "tabbar", "items": ["首页", "我的"], "active": 0 }
                ]
            }
        }
    }))
    .expect("mobile chrome kinds");

    let mut kinds = Vec::new();
    spec.for_each_node(|node| {
        kinds.push(match node {
            MockNode::PhoneShell { .. } => "phone_shell",
            MockNode::Rows { .. } => "rows",
            MockNode::Topbar { .. } => "topbar",
            MockNode::ListRow { .. } => "list_row",
            MockNode::Switch { .. } => "switch",
            MockNode::Segment { .. } => "segment",
            MockNode::Tabbar { .. } => "tabbar",
            other => panic!("unexpected node {other:?}"),
        });
    });
    assert_eq!(
        kinds,
        vec![
            "phone_shell",
            "rows",
            "topbar",
            "list_row",
            "switch",
            "segment",
            "tabbar"
        ]
    );
}

#[test]
fn desktop_chrome_kinds_use_the_documented_wire_names() {
    let spec = parse(json!({
        "fidelity": "wireframe",
        "mock": {
            "kind": "frame",
            "title": "工作台",
            "body": {
                "kind": "cols",
                "children": [
                    { "kind": "tree", "items": [
                        { "label": "src", "depth": 0 },
                        { "label": "main.rs", "depth": 1 }
                    ] },
                    {
                        "kind": "rows",
                        "children": [
                            { "kind": "toolbar", "items": ["文件", "编辑", "视图"] },
                            { "kind": "table", "cols": ["名称", "状态"], "rows": [["api", "就绪"], ["web", ""]] },
                            {
                                "kind": "dialog",
                                "title": "删除文件？",
                                "body": { "kind": "text", "text": "此操作不可撤销" },
                                "actions": ["取消", "删除"]
                            }
                        ]
                    }
                ]
            }
        }
    }))
    .expect("desktop chrome kinds");

    let mut kinds = Vec::new();
    spec.for_each_node(|node| {
        kinds.push(match node {
            MockNode::Frame { .. } => "frame",
            MockNode::Cols { .. } => "cols",
            MockNode::Rows { .. } => "rows",
            MockNode::Tree { .. } => "tree",
            MockNode::Toolbar { .. } => "toolbar",
            MockNode::Table { .. } => "table",
            MockNode::Dialog { .. } => "dialog",
            MockNode::Text { .. } => "text",
            other => panic!("unexpected node {other:?}"),
        });
    });
    assert_eq!(
        kinds,
        vec![
            "frame", "cols", "tree", "rows", "toolbar", "table", "dialog", "text"
        ]
    );
}

#[test]
fn a_selected_tab_or_segment_must_index_a_real_item() {
    for kind in ["tabbar", "segment"] {
        let err = parse(json!({
            "fidelity": "wireframe",
            "mock": { "kind": kind, "items": ["a", "b"], "active": 2 }
        }))
        .expect_err("dangling active");
        assert!(matches!(err, SpecError::InvalidField { .. }), "{err}");
        assert!(format!("{err}").contains("active"), "{err}");
    }
}

#[test]
fn a_table_row_holds_exactly_one_cell_per_column() {
    let err = parse(json!({
        "fidelity": "wireframe",
        "mock": { "kind": "table", "cols": ["名称", "状态"], "rows": [["api"]] }
    }))
    .expect_err("ragged row");
    assert!(matches!(err, SpecError::InvalidField { .. }), "{err}");
    assert!(format!("{err}").contains("cell count"), "{err}");
}

#[test]
fn table_cells_may_be_blank_but_not_hostile() {
    parse(json!({
        "fidelity": "wireframe",
        "mock": { "kind": "table", "cols": ["名称"], "rows": [[""], ["ok"]] }
    }))
    .expect("a blank cell is a real table cell");

    let err = parse(json!({
        "fidelity": "wireframe",
        "mock": { "kind": "table", "cols": ["名称"], "rows": [["a\nb"]] }
    }))
    .expect_err("control character in a cell");
    assert!(matches!(err, SpecError::InvalidField { .. }), "{err}");
}

#[test]
fn tree_depth_is_nesting_bounded_to_something_a_mock_can_hold() {
    parse(json!({
        "fidelity": "wireframe",
        "mock": { "kind": "tree", "items": [{ "label": "src", "depth": 4 }] }
    }))
    .expect("depth 4 nests four levels");

    let err = parse(json!({
        "fidelity": "wireframe",
        "mock": { "kind": "tree", "items": [{ "label": "src", "depth": 9 }] }
    }))
    .expect_err("depth 9 is a document, not a mock");
    assert!(format!("{err}").contains("depth"), "{err}");
}

#[test]
fn a_dialog_says_something() {
    let err = parse(json!({
        "fidelity": "wireframe",
        "mock": { "kind": "dialog", "title": "确认", "actions": ["取消", "删除"] }
    }))
    .expect_err("a dialog with no message");
    assert!(matches!(err, SpecError::InvalidField { .. }), "{err}");
    assert!(format!("{err}").contains("text"), "{err}");
}

#[test]
fn chrome_kinds_stay_closed_vocabulary() {
    let err = parse(json!({
        "fidelity": "wireframe",
        "mock": { "kind": "list_row", "title": "通知", "style": "rounded" }
    }))
    .expect_err("stray field on a new kind");
    assert!(format!("{err}").contains("unknown field `style`"), "{err}");

    let err = parse(json!({
        "fidelity": "wireframe",
        "mock": {
            "kind": "tree",
            "items": [{ "label": "src", "depth": 0, "color": "#fff" }]
        }
    }))
    .expect_err("stray key on a tree item");
    assert!(format!("{err}").contains("unknown field `color`"), "{err}");
}

#[test]
fn round_trips_through_json() {
    let spec = parse(full()).expect("full spec");
    let value = serde_json::to_value(&spec).expect("serialize");
    let again = ScreenSpec::parse(&value).expect("reparsed");
    assert_eq!(spec, again);
}

#[test]
fn non_object_is_rejected() {
    assert_eq!(ScreenSpec::parse(&json!([])), Err(SpecError::NotAnObject));
}

#[test]
fn missing_fidelity_is_rejected() {
    let err = parse(json!({ "mock": { "kind": "text", "text": "hi" } })).unwrap_err();
    assert!(matches!(err, SpecError::Malformed(_)), "{err}");
}

#[test]
fn unknown_node_kind_is_rejected() {
    let err = parse(json!({
        "fidelity": "wireframe",
        "mock": { "kind": "chartjs" }
    }))
    .unwrap_err();
    assert!(matches!(err, SpecError::InvalidField { .. }), "{err}");
    assert!(format!("{err}").contains("chartjs"), "{err}");
}

#[test]
fn unknown_node_fields_are_rejected() {
    // The closed vocabulary has to be closed in practice, not just on paper:
    // a silently dropped `style` would teach the model to keep sending one.
    let err = parse(json!({
        "fidelity": "wireframe",
        "mock": { "kind": "box", "text": "hi", "style": "border-radius:18px" }
    }))
    .unwrap_err();
    assert!(format!("{err}").contains("unknown field `style`"), "{err}");

    let err = parse(json!({
        "fidelity": "texture",
        "mock": { "kind": "text", "text": "hi" },
        "variants": [{ "id": "a", "theme": { "radius": "18px" } }]
    }))
    .unwrap_err();
    assert!(matches!(err, SpecError::Malformed(_)), "{err}");
}

#[test]
fn bad_colors_are_rejected() {
    for bad in [
        "red",
        "#12",
        "rgb(1,2,3); background: url(x)",
        "rgb(1,2,3)\" onmouseover=\"x",
        "#gggggg",
    ] {
        let err = parse(json!({
            "fidelity": "color",
            "mock": { "kind": "text", "text": "hi" },
            "variants": [{ "id": "a", "theme": { "bg": bad } }]
        }))
        .unwrap_err();
        assert!(
            matches!(err, SpecError::InvalidField { .. }),
            "{bad} -> {err}"
        );
    }
}

#[test]
fn good_colors_are_accepted() {
    for good in [
        "#fff",
        "#112233",
        "#11223344",
        "rgb(10, 20, 30)",
        "hsl(210, 50%, 40%)",
    ] {
        parse(json!({
            "fidelity": "color",
            "mock": { "kind": "text", "text": "hi" },
            "variants": [{ "id": "a", "theme": { "bg": good } }]
        }))
        .unwrap_or_else(|err| panic!("{good} -> {err}"));
    }
}

#[test]
fn texture_knobs_are_range_checked() {
    let err = parse(json!({
        "fidelity": "texture",
        "mock": { "kind": "text", "text": "hi" },
        "variants": [{ "id": "a", "theme": { "frost": 4 } }]
    }))
    .unwrap_err();
    assert!(matches!(err, SpecError::InvalidField { .. }), "{err}");

    let err = parse(json!({
        "fidelity": "texture",
        "mock": { "kind": "text", "text": "hi" },
        "variants": [{ "id": "a", "theme": { "glow": 3 } }]
    }))
    .unwrap_err();
    assert!(matches!(err, SpecError::InvalidField { .. }), "{err}");
}

#[test]
fn ring_pct_and_bar_items_are_bounded() {
    let err = parse(json!({
        "fidelity": "color",
        "mock": { "kind": "ring", "pct": 140 }
    }))
    .unwrap_err();
    assert!(matches!(err, SpecError::InvalidField { .. }), "{err}");

    let err = parse(json!({
        "fidelity": "color",
        "mock": { "kind": "bars", "items": [] }
    }))
    .unwrap_err();
    assert!(matches!(err, SpecError::InvalidField { .. }), "{err}");
}

#[test]
fn ratio_must_match_children() {
    let err = parse(json!({
        "fidelity": "wireframe",
        "mock": {
            "kind": "cols",
            "ratio": [1, 2, 3],
            "children": [
                { "kind": "text", "text": "a" },
                { "kind": "text", "text": "b" }
            ]
        }
    }))
    .unwrap_err();
    assert!(matches!(err, SpecError::InvalidField { .. }), "{err}");
}

#[test]
fn proportions_are_numbers_not_css() {
    for good in ["24%", "24", "1.6"] {
        parse(json!({
            "fidelity": "wireframe",
            "mock": { "kind": "box", "w": good, "h": "30%" }
        }))
        .unwrap_or_else(|err| panic!("{good} -> {err}"));
    }
    for bad in ["24px", "50vw", "calc(100%-2px)"] {
        let err = parse(json!({
            "fidelity": "wireframe",
            "mock": { "kind": "box", "w": bad }
        }))
        .unwrap_err();
        assert!(
            matches!(err, SpecError::InvalidField { .. }),
            "{bad} -> {err}"
        );
    }
}

#[test]
fn copy_rejects_control_characters() {
    let err = parse(json!({
        "fidelity": "wireframe",
        "mock": { "kind": "text", "text": "hi\nthere" }
    }))
    .unwrap_err();
    assert!(matches!(err, SpecError::InvalidField { .. }), "{err}");
}

#[test]
fn duplicate_variant_ids_are_rejected() {
    let err = parse(json!({
        "fidelity": "color",
        "mock": { "kind": "text", "text": "hi" },
        "variants": [{ "id": "a" }, { "id": "a" }]
    }))
    .unwrap_err();
    assert!(matches!(err, SpecError::InvalidField { .. }), "{err}");
}

#[test]
fn a_variant_needs_some_mock_tree() {
    let err = parse(json!({
        "fidelity": "color",
        "variants": [{ "id": "a" }]
    }))
    .unwrap_err();
    assert!(matches!(err, SpecError::InvalidField { .. }), "{err}");
}

#[test]
fn emph_and_content_must_reference_real_nodes() {
    let err = parse(json!({
        "fidelity": "color",
        "mock": { "kind": "text", "id": "hero", "text": "hi" },
        "variants": [{ "id": "a", "emph": ["nope"] }]
    }))
    .unwrap_err();
    assert!(matches!(err, SpecError::InvalidField { .. }), "{err}");

    let err = parse(json!({
        "fidelity": "color",
        "mock": { "kind": "text", "id": "hero", "text": "hi" },
        "variants": [{ "id": "a", "content": { "nope": "x" } }]
    }))
    .unwrap_err();
    assert!(matches!(err, SpecError::InvalidField { .. }), "{err}");
}

#[test]
fn emph_resolves_against_the_variants_own_tree() {
    // When a variant overrides the mock, ids in `emph` refer to that override,
    // not the shared tree.
    parse(json!({
        "fidelity": "color",
        "mock": { "kind": "text", "id": "shared", "text": "hi" },
        "variants": [{
            "id": "a",
            "mock": { "kind": "text", "id": "own", "text": "yo" },
            "emph": ["own"]
        }]
    }))
    .expect("variant-local id");

    let err = parse(json!({
        "fidelity": "color",
        "mock": { "kind": "text", "id": "shared", "text": "hi" },
        "variants": [{
            "id": "a",
            "mock": { "kind": "text", "id": "own", "text": "yo" },
            "emph": ["shared"]
        }]
    }))
    .unwrap_err();
    assert!(matches!(err, SpecError::InvalidField { .. }), "{err}");
}

#[test]
fn node_cap_stops_a_runaway_tree() {
    fn wide(depth: usize) -> Value {
        if depth == 0 {
            return json!({ "kind": "text", "text": "x" });
        }
        json!({
            "kind": "cols",
            "children": (0..12).map(|_| wide(depth - 1)).collect::<Vec<_>>()
        })
    }
    // 1 + 12 + 144 + 1728 nodes: the walk must stop at the cap, not at the end.
    let err = parse(json!({ "fidelity": "wireframe", "mock": wide(3) })).unwrap_err();
    assert!(matches!(err, SpecError::InvalidField { .. }), "{err}");
    assert!(format!("{err}").contains("node cap"), "{err}");
}

#[test]
fn notice_is_length_capped() {
    let err = parse(json!({
        "fidelity": "wireframe",
        "mock": { "kind": "text", "text": "hi" },
        "notice": "x".repeat(501)
    }))
    .unwrap_err();
    assert!(matches!(err, SpecError::InvalidField { .. }), "{err}");
}

#[test]
fn a_stringified_screen_still_parses() {
    // Observed on the live push path: the whole `screen` object arrived as one
    // JSON string. Unwrapping is free; rejecting it spends a push round saying
    // what the model already said.
    let inner = json!({
        "fidelity": "wireframe",
        "mock": { "kind": "text", "text": "hi" }
    });
    let wrapped = Value::String(inner.to_string());
    let spec = ScreenSpec::parse(&wrapped).expect("double-encoded screen");
    assert_eq!(spec.fidelity, Fidelity::Wireframe);
}

#[test]
fn a_broken_stringified_screen_names_both_facts() {
    // Observed twice live: the model stringifies `screen` and the copy is
    // malformed — here a `variants` tail leaked without its `"variants": [`
    // key. "must be a JSON object" alone leaves the model re-sending the same
    // string, because it already believes it sent an object. Say it arrived as
    // a string, say the content is broken, and show where it broke.
    let broken = Value::String(
        "{\"fidelity\": \"wireframe\", \"mock\": {\"kind\": \"text\", \"text\": \"x\"}}, {\"id\": \"cards\"}]}]"
            .into(),
    );
    let err = ScreenSpec::parse(&broken).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("arrived as a string"), "{msg}");
    assert!(msg.contains("not valid JSON"), "{msg}");
    assert!(
        msg.contains("cards"),
        "the excerpt should point at the fumble: {msg}"
    );
}

#[test]
fn a_stringified_non_object_names_the_type() {
    let err = ScreenSpec::parse(&Value::String("[1, 2]".into())).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("a JSON array"), "{msg}");
    assert!(msg.contains("never stringified"), "{msg}");
}

#[test]
fn a_broken_stringified_screen_with_cjk_still_points_at_the_break() {
    // The excerpt window is sliced by serde's byte column while the content
    // carries CJK copy — a naive slice would panic on a char boundary (the
    // class of panic that bit `history.list_items`).
    let broken = Value::String(
        "{\"fidelity\": \"wireframe\", \"mock\": {\"kind\": \"text\", \"text\": \"账户设置页头像昵称与账号\"}}, {\"id\": \"卡片\"}]}]"
            .into(),
    );
    let err = ScreenSpec::parse(&broken).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("arrived as a string"), "{msg}");
    assert!(
        msg.contains("卡片") || msg.contains("mock"),
        "the excerpt should still show the fumble region: {msg}"
    );
}
