//! Spec rendering: what the model's structure + tokens turn into, and what
//! must never leak out of them (theme colors at wireframe, knobs outside
//! texture, unescaped copy).

use phi_canvas::render_spec;
use phi_canvas::{OptionItem, ScreenSpec};
use serde_json::json;

fn options() -> Vec<OptionItem> {
    vec![
        OptionItem {
            id: "deep-blue".into(),
            label: "深空蓝".into(),
            note: Some("冷、克制".into()),
        },
        OptionItem {
            id: "neon-purple".into(),
            label: "暗紫霓虹".into(),
            note: None,
        },
    ]
}

fn parse(v: serde_json::Value) -> ScreenSpec {
    ScreenSpec::parse(&v).expect("spec")
}

fn themed(fidelity: &str) -> ScreenSpec {
    parse(json!({
        "fidelity": fidelity,
        "mock": {
            "kind": "cols",
            "children": [
                { "kind": "text", "id": "tag", "text": "图例" },
                { "kind": "hero", "id": "hero", "text": "128,640" },
                { "kind": "button", "id": "cta", "text": "导出" }
            ]
        },
        "variants": [
            {
                "id": "deep-blue",
                "theme": {
                    "bg": "#0A1428", "accent": "#00D4FF",
                    "surface": "glass", "frost": 2, "elevation": 3, "edge": 2, "glow": 1
                },
                "emph": ["hero"]
            },
            {
                "id": "neon-purple",
                "theme": { "bg": "#12071F", "accent": "#C026D3" },
                "emph": ["cta"]
            }
        ]
    }))
}

#[test]
fn variants_render_as_cards_scoped_by_their_own_theme() {
    let html = render_spec(&themed("color"), &options());
    assert!(html.contains("fidelity-color"), "{html}");
    assert!(html.contains("m-variant"), "{html}");
    // Two cards, each carrying its own palette as custom properties.
    assert!(html.contains("--m-bg:#0A1428"), "{html}");
    assert!(html.contains("--m-bg:#12071F"), "{html}");
    assert!(html.contains("--m-accent:#00D4FF"), "{html}");
    assert!(html.contains("--m-accent:#C026D3"), "{html}");
}

#[test]
fn captions_come_from_the_options_panel() {
    let html = render_spec(&themed("color"), &options());
    assert!(html.contains(">深空蓝</div>"), "{html}");
    assert!(html.contains(">暗紫霓虹</div>"), "{html}");
    // The option's note is not retyped into the picture — the host panel owns it.
    assert!(!html.contains("冷、克制"), "{html}");
}

#[test]
fn variant_cards_are_clickable_only_for_real_options() {
    let html = render_spec(&themed("color"), &options());
    assert!(html.contains("data-choice=\"deep-blue\""), "{html}");
    assert!(html.contains("data-choice=\"neon-purple\""), "{html}");

    let orphan = parse(json!({
        "fidelity": "color",
        "mock": { "kind": "text", "text": "hi" },
        "variants": [{ "id": "not-an-option" }]
    }));
    let html = render_spec(&orphan, &options());
    assert!(!html.contains("data-choice"), "{html}");
    assert!(html.contains("not-an-option"), "{html}"); // still captioned by id
}

#[test]
fn emphasis_lands_on_the_variants_own_node_only() {
    let html = render_spec(&themed("color"), &options());
    let first = html.split("data-choice=\"neon-purple\"").next().unwrap();
    let second = html.split("data-choice=\"neon-purple\"").nth(1).unwrap();

    // hero/button are mapped kinds, so at color wide they are antd slots and
    // the emphasis marker rides the slot element itself.
    assert!(
        first.contains("m-antd-slot m-emph\" data-antd-kind=\"hero\""),
        "{first}"
    );
    assert!(
        first.contains("m-antd-slot\" data-antd-kind=\"button\""),
        "{first}"
    );
    assert!(
        !first.contains("m-antd-slot m-emph\" data-antd-kind=\"button\""),
        "{first}"
    );
    assert!(
        second.contains("m-antd-slot m-emph\" data-antd-kind=\"button\""),
        "{second}"
    );
    assert!(
        second.contains("m-antd-slot\" data-antd-kind=\"hero\""),
        "{second}"
    );
    assert!(
        !second.contains("m-antd-slot m-emph\" data-antd-kind=\"hero\""),
        "{second}"
    );
}

#[test]
fn content_overrides_replace_the_primary_slot() {
    let spec = parse(json!({
        "fidelity": "color",
        "mock": {
            "kind": "rows",
            "children": [
                { "kind": "kpi", "id": "rev", "label": "收入", "value": "0" },
                { "kind": "text", "id": "tag", "text": "图例" }
            ]
        },
        "variants": [
            { "id": "deep-blue", "content": { "rev": "128,640", "tag": "A 组" } },
            { "id": "neon-purple", "content": { "rev": "96,120" } }
        ]
    }));
    let html = render_spec(&spec, &options());
    assert!(html.contains("128,640"), "{html}");
    assert!(html.contains("A 组"), "{html}");
    assert!(html.contains("96,120"), "{html}");
    // kpi's primary slot is its value; the label stays as authored.
    assert!(html.contains("收入"), "{html}");
}

#[test]
fn wireframe_drops_theme_colors() {
    let html = render_spec(&themed("wireframe"), &options());
    assert!(html.contains("fidelity-wireframe"), "{html}");
    assert!(!html.contains("--m-bg:"), "{html}");
    assert!(!html.contains("--m-accent:"), "{html}");
    assert!(!html.contains("m-surface-"), "{html}");
}

#[test]
fn color_fidelity_shows_tokens_but_not_texture_knobs() {
    let html = render_spec(&themed("color"), &options());
    assert!(html.contains("--m-bg:#0A1428"), "{html}");
    assert!(!html.contains("--m-frost:"), "{html}");
    assert!(!html.contains("--m-elevation:"), "{html}");
    assert!(!html.contains("--m-edge:"), "{html}");
    assert!(!html.contains("--m-glow:"), "{html}");
    assert!(!html.contains("m-surface-"), "{html}");
}

#[test]
fn texture_fidelity_emits_the_four_measured_knobs_and_surface() {
    let html = render_spec(&themed("texture"), &options());
    assert!(html.contains("fidelity-texture"), "{html}");
    assert!(html.contains("--m-frost:2"), "{html}");
    assert!(html.contains("--m-elevation:3"), "{html}");
    assert!(html.contains("--m-edge:2"), "{html}");
    assert!(html.contains("--m-glow:1"), "{html}");
    // Surface is a CSS hook: a class, since custom properties cannot be
    // branched on without container-style queries.
    assert!(html.contains("m-surface-glass"), "{html}");
    // The second variant declared no surface.
    assert!(!html.contains("m-surface-matte"), "{html}");
}

#[test]
fn geometry_arrives_as_named_custom_properties() {
    let spec = parse(json!({
        "fidelity": "wireframe",
        "mock": {
            "kind": "cols",
            "ratio": [1, 2, 1],
            "children": [
                { "kind": "box", "w": "24%", "h": "28%", "text": "面板1" },
                { "kind": "ring", "pct": 86, "label": "完成度" },
                { "kind": "stack", "count": 3, "child": { "kind": "text", "text": "行" } }
            ]
        }
    }));
    let html = render_spec(&spec, &[]);
    assert!(
        html.contains("--ratio:minmax(0, 1fr) minmax(0, 2fr) minmax(0, 1fr)"),
        "{html}"
    );
    assert!(html.contains("--w:24%"), "{html}");
    assert!(html.contains("--h:28%"), "{html}");
    assert!(html.contains("--pct:86"), "{html}");
    assert!(html.contains("--rows:3"), "{html}");
    // Three stacked copies of the child.
    assert_eq!(html.matches("行").count(), 3, "{html}");
}

#[test]
fn copy_is_escaped() {
    let spec = parse(json!({
        "fidelity": "wireframe",
        "mock": { "kind": "text", "text": "<script>alert(1)</script>" }
    }));
    let html = render_spec(&spec, &[]);
    assert!(!html.contains("<script>"), "{html}");
    assert!(html.contains("&lt;script&gt;"), "{html}");
}

#[test]
fn a_mock_only_screen_renders_one_uncaptioned_card() {
    let spec = parse(json!({
        "fidelity": "color",
        "mock": { "kind": "hero", "text": "128,640" },
        "notice": "字体层级后续再定"
    }));
    let html = render_spec(&spec, &[]);
    assert!(html.contains("m-variant"), "{html}");
    assert!(!html.contains("m-variant-cap"), "{html}");
    assert!(!html.contains("data-choice"), "{html}");
    assert!(html.contains("m-notice"), "{html}");
    assert!(html.contains("字体层级后续再定"), "{html}");
}

#[test]
fn variant_mock_overrides_the_shared_tree() {
    let spec = parse(json!({
        "fidelity": "wireframe",
        "mock": { "kind": "text", "id": "shared", "text": "共享" },
        "variants": [
            { "id": "deep-blue" },
            { "id": "neon-purple", "mock": { "kind": "text", "id": "own", "text": "自绘" } }
        ]
    }));
    let html = render_spec(&spec, &options());
    let first = html.split("data-choice=\"neon-purple\"").next().unwrap();
    let second = html.split("data-choice=\"neon-purple\"").nth(1).unwrap();
    assert!(first.contains("共享"), "{first}");
    assert!(!first.contains("自绘"), "{first}");
    assert!(second.contains("自绘"), "{second}");
    assert!(!second.contains("共享"), "{second}");
}

#[test]
fn swatches_follow_the_variants_theme() {
    let spec = parse(json!({
        "fidelity": "color",
        "mock": { "kind": "swatches" },
        "variants": [{ "id": "deep-blue", "theme": { "accent": "#00D4FF", "bg": "#0A1428" } }]
    }));
    let html = render_spec(&spec, &options());
    assert!(html.contains("--chip:#0A1428"), "{html}");
    assert!(html.contains("--chip:#00D4FF"), "{html}");
    assert_eq!(html.matches("m-chip").count(), 2, "{html}");
}

/// `needle` must occur in `css` as a whole name — `.m-text` must not be
/// satisfied by `.m-text-bright`, nor `var(--w` by `var(--width)`.
fn css_uses(css: &str, needle: &str) -> bool {
    let mut from = 0;
    while let Some(at) = css[from..].find(needle) {
        let end = from + at + needle.len();
        match css[end..].chars().next() {
            Some(c) if c.is_alphanumeric() || c == '-' || c == '_' => from = end,
            _ => return true,
        }
    }
    false
}

/// The renderer and `components.css` are one design. A class emitted on one
/// side and missing on the other renders as unstyled markup — or dead CSS —
/// and no unit test of the HTML alone would notice. This walks a spec that
/// exercises every node kind and every theme token, then checks both sides.
#[test]
fn every_emitted_class_and_custom_property_is_in_the_stylesheet() {
    let css = phi_canvas::assets::COMPONENTS_CSS;
    let spec = parse(json!({
        "fidelity": "texture",
        "mock": {
            "kind": "frame", "id": "frame", "title": "控制台",
            "body": {
                "kind": "rows", "id": "rows", "ratio": [1, 2],
                "children": [
                    { "kind": "nav", "id": "nav", "items": ["总览", "报表"] },
                    {
                        "kind": "cols", "id": "cols", "ratio": [1, 1, 1, 1, 1],
                        "children": [
                            { "kind": "sidebar", "id": "sidebar", "items": ["项目"] },
                            {
                                "kind": "stack", "id": "stack", "count": 2,
                                "child": { "kind": "box", "id": "box", "w": "24%", "h": "40%", "text": "区域" }
                            },
                            {
                                "kind": "rows",
                                "children": [
                                    { "kind": "kpi", "id": "kpi", "label": "收入", "value": "¥2.1M", "delta": "+4.2%" },
                                    { "kind": "hero", "id": "hero", "text": "128,640" },
                                    { "kind": "sparkline", "id": "spark", "shape": "down" },
                                    { "kind": "ring", "id": "ring", "pct": 86, "label": "86%" },
                                    { "kind": "bars", "id": "bars", "items": [{ "label": "A", "value": 3 }, { "label": "B" }] },
                                    { "kind": "area", "id": "area", "label": "趋势" },
                                    { "kind": "viz", "id": "viz", "form": "map", "label": "分布" },
                                    { "kind": "input", "id": "input", "text": "搜索" },
                                    { "kind": "button", "id": "button", "text": "导出" },
                                    { "kind": "swatches", "id": "swatches" },
                                    { "kind": "text", "id": "text", "text": "说明" },
                                    { "kind": "placeholder", "id": "ph", "text": "内容区" }
                                ]
                            },
                            {
                                "kind": "phone_shell", "id": "phone",
                                "body": {
                                    "kind": "rows",
                                    "children": [
                                        { "kind": "topbar", "id": "topbar", "title": "设置", "back": true, "action": "编辑" },
                                        { "kind": "list_row", "id": "row", "title": "通知", "note": "推送与声音", "meta": "开", "chevron": true, "avatar": true },
                                        { "kind": "switch", "id": "switch", "label": "深色模式", "on": true },
                                        { "kind": "segment", "id": "segment", "items": ["日", "周"], "active": 1 },
                                        { "kind": "tabbar", "id": "tabbar", "items": ["首页", "我的"], "active": 0 }
                                    ]
                                }
                            },
                            {
                                "kind": "rows",
                                "children": [
                                    { "kind": "toolbar", "id": "toolbar", "items": ["文件", "编辑"] },
                                    { "kind": "table", "id": "table", "cols": ["名称", "状态"], "rows": [["api", "就绪"], ["web", ""]] },
                                    { "kind": "tree", "id": "tree", "items": [{ "label": "src", "depth": 0 }, { "label": "main", "depth": 1 }] },
                                    {
                                        "kind": "dialog", "id": "dialog", "title": "删除文件？",
                                        "body": { "kind": "text", "id": "dialog-body", "text": "此操作不可撤销" },
                                        "actions": ["取消", "删除"]
                                    }
                                ]
                            }
                        ]
                    }
                ]
            }
        },
        "notice": "字体层级后续再定",
        "variants": [
            {
                "id": "deep-blue",
                "theme": {
                    "bg": "#0A1428", "panel": "#0E1B33", "border": "#243552",
                    "accent": "#00D4FF", "accent2": "#7C3AED",
                    "text": "#C7D2E5", "text_bright": "#F2F7FF",
                    "surface": "matte", "frost": 2, "elevation": 3, "edge": 2, "glow": 1
                },
                "emph": ["hero", "kpi"],
                "content": { "hero": "99,001" }
            }
        ]
    }));
    let html = render_spec(&spec, &options());

    // Every class token the renderer put in a class attribute must be named
    // as a selector in the stylesheet.
    let mut missing = Vec::new();
    for (start, _) in html.match_indices("class=\"") {
        let rest = &html[start + 7..];
        let end = rest.find('"').expect("closed class attribute");
        for token in rest[..end].split_whitespace() {
            if !css_uses(css, &format!(".{token}")) {
                missing.push(token.to_string());
            }
        }
    }
    assert!(
        missing.is_empty(),
        "classes rendered but not styled: {missing:?}\n{html}"
    );

    // Every custom property the renderer set must actually be read somewhere.
    let mut unconsumed = Vec::new();
    for (start, _) in html.match_indices("style=\"") {
        let rest = &html[start + 7..];
        let end = rest.find('"').expect("closed style attribute");
        for decl in rest[..end].split(';') {
            let Some(name) = decl.split(':').next() else {
                continue;
            };
            let name = name.trim().strip_prefix("--").unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            if !css_uses(css, &format!("var(--{name}")) {
                unconsumed.push(name.to_string());
            }
        }
    }
    assert!(
        unconsumed.is_empty(),
        "custom properties set but never consumed: {unconsumed:?}\n{html}"
    );
}

#[test]
fn mobile_chrome_renders_its_shapes() {
    let spec = parse(json!({
        "fidelity": "color",
        "mock": {
            "kind": "phone_shell",
            "body": {
                "kind": "rows",
                "children": [
                    { "kind": "topbar", "title": "设置", "back": true, "action": "编辑" },
                    { "kind": "list_row", "title": "通知", "note": "推送与声音", "meta": "开", "chevron": true, "avatar": true },
                    { "kind": "switch", "label": "深色模式", "on": true },
                    { "kind": "segment", "items": ["日", "周"], "active": 1 },
                    { "kind": "tabbar", "items": ["首页", "我的"], "active": 0 }
                ]
            }
        }
    }));
    let html = render_spec(&spec, &[]);

    for needle in [
        "m-phone",
        "m-phone-status",
        "m-phone-home",
        "m-topbar-back",
        "m-topbar-a",
        "m-row-av",
        "m-row-ch",
        "m-switch-t m-on",
        "m-seg m-seg-on",
        "m-tab m-tab-on",
    ] {
        assert!(html.contains(needle), "missing `{needle}`:\n{html}");
    }
    // The selected tab is the first, the selected segment the second.
    assert!(
        html.contains("<span class=\"m-tab m-tab-on\"><i></i>首页</span>"),
        "{html}"
    );
    assert!(html.contains("<span class=\"m-seg\">日</span>"), "{html}");
    assert!(
        html.contains("<span class=\"m-seg m-seg-on\">周</span>"),
        "{html}"
    );
}

#[test]
fn desktop_chrome_renders_its_shapes() {
    // Greybox shapes on purpose: at color wide the mapped leaves (table/tree/
    // dialog) island into antd faces, so their greybox markup is guarded here
    // at wireframe fidelity instead.
    let spec = parse(json!({
        "fidelity": "wireframe",
        "mock": {
            "kind": "rows",
            "children": [
                { "kind": "toolbar", "items": ["文件", "编辑"] },
                { "kind": "table", "cols": ["名称", "状态"], "rows": [["api", "就绪"], ["web", ""]] },
                { "kind": "tree", "items": [{ "label": "src", "depth": 1 }, { "label": "main", "depth": 3 }] },
                { "kind": "dialog", "title": "删除文件？", "text": "此操作不可撤销", "actions": ["取消", "删除"] }
            ]
        }
    }));
    let html = render_spec(&spec, &[]);

    assert!(html.contains("m-toolbar"), "{html}");
    assert!(html.contains("m-tr m-th"), "{html}");
    assert!(
        html.contains("<span></span>"),
        "a blank cell is a real cell: {html}"
    );
    assert!(
        html.contains("style=\"--d:3\""),
        "tree nesting is geometry: {html}"
    );
    assert!(html.contains("m-dialog-hd"), "{html}");
    assert!(html.contains("m-dialog-act"), "{html}");
    // The last action is the key one.
    assert!(html.contains("m-dialog-act m-dialog-key\">删除"), "{html}");
    assert!(!html.contains("m-dialog-act m-dialog-key\">取消"), "{html}");
}

#[test]
fn a_dialog_message_is_the_slot_a_variant_can_rewrite() {
    let spec = parse(json!({
        "fidelity": "color",
        "mock": { "kind": "dialog", "id": "confirm", "title": "删除文件？", "text": "此操作不可撤销", "actions": ["取消", "删除"] },
        "variants": [{ "id": "deep-blue", "content": { "confirm": "删除整个目录？" } }]
    }));
    let html = render_spec(&spec, &options());

    assert!(html.contains("删除整个目录？"), "{html}");
    assert!(!html.contains("此操作不可撤销"), "{html}");
    assert!(
        html.contains("删除文件？"),
        "the title stays as authored: {html}"
    );
}

#[test]
fn a_list_row_title_is_the_slot_a_variant_can_rewrite() {
    let spec = parse(json!({
        "fidelity": "color",
        "mock": { "kind": "list_row", "id": "row", "title": "通知", "note": "推送与声音", "chevron": true },
        "variants": [{ "id": "deep-blue", "content": { "row": "消息提醒" } }]
    }));
    let html = render_spec(&spec, &options());

    assert!(html.contains("消息提醒"), "{html}");
    assert!(!html.contains(">通知<"), "{html}");
    assert!(
        html.contains("推送与声音"),
        "the note stays as authored: {html}"
    );
}

#[test]
fn unweighted_tracks_flex_by_child_role() {
    // Regression: unweighted tracks used to be one equal fraction each, so a
    // phone shell rendered as three giant bands (topbar / list / tabbar at a
    // third apiece) and the model escaped to `body_html` — `escape:
    // "phone-frame"` was in the ledger. Chrome hugs its content; content
    // containers fill what is left.
    let spec = parse(json!({
        "fidelity": "wireframe",
        "mock": {
            "kind": "phone_shell",
            "body": {
                "kind": "rows",
                "children": [
                    { "kind": "topbar", "title": "设置", "back": true },
                    {
                        "kind": "rows",
                        "children": [
                            { "kind": "list_row", "title": "通知", "meta": "已开启" },
                            { "kind": "text", "text": "通用" }
                        ]
                    },
                    { "kind": "tabbar", "items": ["首页", "我的"], "active": 1 }
                ]
            }
        },
        "variants": [{ "id": "deep-blue" }]
    }));
    let html = render_spec(&spec, &options());
    assert!(html.contains("--ratio:auto minmax(0, 1fr) auto"), "{html}");
    assert!(html.contains("--ratio:auto auto"), "{html}");
}

#[test]
fn several_content_bands_hug_instead_of_equalizing() {
    // Regression (session 20261023_41c0f4df): four unweighted content bands
    // each took `minmax(0, 1fr)`, and equal fr rows in a content-sized grid
    // all grow to the tallest band's content — the title line and KPI row
    // rendered as tall as the data table, with the gap under every short
    // band. Several content bands now hug; only a lone one fills.
    let spec = parse(json!({
        "fidelity": "color",
        "mock": {
            "kind": "frame",
            "title": "运营中心",
            "body": {
                "kind": "rows",
                "children": [
                    {
                        "kind": "cols",
                        "children": [
                            { "kind": "text", "text": "今日运营概览" },
                            { "kind": "toolbar", "items": ["今天", "导出"] }
                        ]
                    },
                    {
                        "kind": "cols",
                        "children": [
                            { "kind": "kpi", "label": "GMV", "value": "¥128,640" },
                            { "kind": "kpi", "label": "订单", "value": "3,482" }
                        ]
                    },
                    {
                        "kind": "cols",
                        "children": [
                            { "kind": "area", "label": "趋势" },
                            { "kind": "bars", "items": [{ "label": "私域", "value": 58 }] }
                        ]
                    },
                    {
                        "kind": "table",
                        "cols": ["渠道", "GMV"],
                        "rows": [["信息流", "¥52,400"]]
                    }
                ]
            }
        },
        "variants": [{ "id": "deep-blue" }]
    }));
    let html = render_spec(&spec, &options());
    assert!(
        html.contains("--ratio:auto auto auto auto"),
        "content bands must hug, got: {html}"
    );
}

#[test]
fn compact_widgets_hug_their_band() {
    // A 34px sparkline and a 74px ring are not bands — sharing a track with a
    // KPI card stretched half the card's height into empty space.
    let spec = parse(json!({
        "fidelity": "wireframe",
        "mock": {
            "kind": "rows",
            "children": [
                {
                    "kind": "rows",
                    "children": [
                        { "kind": "kpi", "label": "GMV", "value": "¥128,640" },
                        { "kind": "sparkline", "shape": "up" }
                    ]
                },
                {
                    "kind": "rows",
                    "children": [
                        { "kind": "ring", "pct": 68, "label": "目标" },
                        { "kind": "bars", "items": [{ "label": "私域", "value": 58 }] }
                    ]
                }
            ]
        },
        "variants": [{ "id": "a" }]
    }));
    let html = render_spec(&spec, &options());
    // The KPI card is the lone content band and keeps the fill track; the
    // sparkline hugs. The ring/bars stack has no content band at all.
    assert!(html.contains("--ratio:minmax(0, 1fr) auto"), "{html}");
    assert!(html.contains("--ratio:auto auto"), "{html}");
}

#[test]
fn explicit_ratio_weights_still_win() {
    let spec = parse(json!({
        "fidelity": "wireframe",
        "mock": {
            "kind": "cols",
            "ratio": [1, 3],
            "children": [
                { "kind": "sidebar", "items": ["概览"] },
                {
                    "kind": "rows",
                    "children": [{ "kind": "text", "text": "正文" }]
                }
            ]
        },
        "variants": [{ "id": "deep-blue" }]
    }));
    let html = render_spec(&spec, &options());
    assert!(
        html.contains("--ratio:minmax(0, 1fr) minmax(0, 3fr)"),
        "{html}"
    );
}

#[test]
fn unweighted_cols_widen_controls_instead_of_squeezing_them() {
    // The track of a `cols` band is width. Only side panels hug there — a
    // switch that hugged its column shrank to one CJK character per line in a
    // quick-toggles row. Controls take their share; the sidebar stays narrow.
    let spec = parse(json!({
        "fidelity": "wireframe",
        "mock": {
            "kind": "cols",
            "children": [
                { "kind": "sidebar", "items": ["设置"] },
                {
                    "kind": "cols",
                    "children": [
                        { "kind": "switch", "label": "深色模式", "on": true },
                        { "kind": "switch", "label": "免打扰", "on": false },
                        { "kind": "switch", "label": "自动同步", "on": true }
                    ]
                }
            ]
        },
        "variants": [{ "id": "deep-blue" }]
    }));
    let html = render_spec(&spec, &options());
    assert!(html.contains("--ratio:auto minmax(0, 1fr)"), "{html}");
    assert!(
        html.contains("--ratio:minmax(0, 1fr) minmax(0, 1fr) minmax(0, 1fr)"),
        "{html}"
    );
}

#[test]
fn a_declared_share_becomes_the_band_track() {
    // `h: 18` in a `rows` stack means "this row is 18% of the band". A
    // percentage height on the box itself would resolve against an auto-sized
    // track and silently do nothing — the share has to BE the track. Numbers
    // and percent strings mean the same share.
    let spec = parse(json!({
        "fidelity": "wireframe",
        "mock": {
            "kind": "rows",
            "children": [
                { "kind": "topbar", "title": "设置" },
                { "kind": "box", "h": 18, "text": "头像 · 昵称" },
                { "kind": "box", "h": "12%", "text": "快捷" },
                { "kind": "text", "text": "通用" }
            ]
        },
        "variants": [{ "id": "a" }]
    }));
    let html = render_spec(&spec, &options());
    assert!(html.contains("--ratio:auto 18% 12% auto"), "{html}");
    // The share must not also land on the box: 18% of an 18% track is not
    // "18% of the band".
    assert!(!html.contains("--h:"), "{html}");
}

#[test]
fn a_share_on_the_cross_axis_still_sizes_the_box() {
    // In a `cols` band the track is width — a box's `w` is the track but its
    // `h` remains a size on the box inside that cell.
    let spec = parse(json!({
        "fidelity": "wireframe",
        "mock": {
            "kind": "cols",
            "children": [
                { "kind": "box", "w": 25, "h": 40, "text": "面板" },
                { "kind": "text", "text": "正文" }
            ]
        },
        "variants": [{ "id": "a" }]
    }));
    let html = render_spec(&spec, &options());
    assert!(html.contains("--ratio:25% minmax(0, 1fr)"), "{html}");
    assert!(html.contains("--h:40%"), "{html}");
    assert!(!html.contains("--w:"), "{html}");
}

#[test]
fn float_ratio_weights_render_as_tracks() {
    // Observed live: `ratio: [1, 5, 0.7]` — a finer split than whole numbers.
    // Numeric strings are the same weights: `"3.0"` renders as `3fr`.
    let spec = parse(json!({
        "fidelity": "wireframe",
        "mock": {
            "kind": "cols",
            "ratio": [1, 5, 0.7],
            "children": [
                { "kind": "text", "text": "a" },
                { "kind": "text", "text": "b" },
                { "kind": "text", "text": "c" }
            ]
        },
        "variants": [{ "id": "a" }]
    }));
    let html = render_spec(&spec, &options());
    assert!(
        html.contains("--ratio:minmax(0, 1fr) minmax(0, 5fr) minmax(0, 0.7fr)"),
        "{html}"
    );

    let spec = parse(json!({
        "fidelity": "wireframe",
        "mock": {
            "kind": "cols",
            "ratio": ["3.0", "1"],
            "children": [
                { "kind": "text", "text": "a" },
                { "kind": "text", "text": "b" }
            ]
        },
        "variants": [{ "id": "a" }]
    }));
    let html = render_spec(&spec, &options());
    assert!(
        html.contains("--ratio:minmax(0, 3fr) minmax(0, 1fr)"),
        "{html}"
    );
}

#[test]
fn wide_form_variants_stack_full_row() {
    // A PC-web mock squeezed into a portrait column reads like a phone mock —
    // wide forms take the full row. The form factor is the model's call: it
    // wraps the mock in `phone_shell` or not, and the rail follows the content.
    // Variant b nests a `phone_shell` preview — a widget, not the form: only
    // the root decides.
    let spec = parse(json!({
        "fidelity": "wireframe",
        "mock": {
            "kind": "frame", "title": "控制台",
            "body": {
                "kind": "cols",
                "children": [
                    { "kind": "sidebar", "items": ["项目"] },
                    { "kind": "text", "text": "正文" }
                ]
            }
        },
        "variants": [
            { "id": "a" },
            {
                "id": "b",
                "mock": {
                    "kind": "frame", "title": "控制台",
                    "body": {
                        "kind": "rows",
                        "children": [
                            { "kind": "nav", "items": ["总览"] },
                            { "kind": "text", "text": "正文" },
                            {
                                "kind": "phone_shell",
                                "body": { "kind": "text", "text": "移动端预览" }
                            }
                        ]
                    }
                }
            }
        ]
    }));
    let html = render_spec(&spec, &options());
    assert_eq!(
        html.matches("m-variant-wide").count(),
        2,
        "both wide-form cards stack: {html}"
    );
}

#[test]
fn phone_trees_keep_the_side_by_side_rail() {
    // Phone mocks compare well side by side — no `m-variant-wide` there.
    let spec = parse(json!({
        "fidelity": "wireframe",
        "mock": {
            "kind": "phone_shell",
            "body": {
                "kind": "rows",
                "children": [
                    { "kind": "topbar", "title": "首页" },
                    { "kind": "text", "text": "内容" }
                ]
            }
        },
        "variants": [{ "id": "a" }, { "id": "b" }]
    }));
    let html = render_spec(&spec, &options());
    assert!(!html.contains("m-variant-wide"), "{html}");
}

/// Decision A's fence: the two faces never leak into each other. Wireframe
/// stays the greybox page it was (no slot, no theme payload), color wide
/// islands, and a phone-root tree stays greybox even at color — antd-mobile is
/// the mapping layer's next instance, not this one.
#[test]
fn the_two_faces_never_leak_into_each_other() {
    let mapped = json!({
        "kind": "rows",
        "children": [
            { "kind": "button", "text": "导出" },
            { "kind": "kpi", "label": "收入", "value": "1" }
        ]
    });
    let wireframe = render_spec(
        &parse(json!({ "fidelity": "wireframe", "mock": mapped.clone() })),
        &[],
    );
    assert!(!wireframe.contains("m-antd-slot"), "{wireframe}");
    assert!(!wireframe.contains("data-antd"), "{wireframe}");
    assert!(
        wireframe.contains("m-kpi"),
        "greybox still draws: {wireframe}"
    );

    let color = render_spec(
        &parse(json!({ "fidelity": "color", "mock": mapped.clone() })),
        &[],
    );
    assert!(color.contains("m-antd-slot"), "{color}");
    assert!(color.contains("data-antd-theme"), "{color}");

    let phone = render_spec(
        &parse(json!({
            "fidelity": "color",
            "mock": { "kind": "phone_shell", "body": mapped }
        })),
        &[],
    );
    assert!(
        !phone.contains("m-antd-slot"),
        "phone roots stay greybox: {phone}"
    );
    assert!(phone.contains("m-kpi"), "{phone}");
}

#[test]
fn bar_magnitudes_arrive_as_decimals_and_leave_as_percent_numbers() {
    let html = render_spec(
        &parse(json!({
            "fidelity": "wireframe",
            "mock": {
                "kind": "bars",
                "items": [
                    { "label": "甲", "value": 3.5 },
                    { "label": "乙", "value": "4.2" },
                    { "label": "丙", "value": "42%" }
                ]
            }
        })),
        &[],
    );
    // 3.5/42 ≈ 8%, "4.2"/42 = 10%, "42%" reads as 42 — the max, full width.
    assert!(html.contains("--w:8\""), "{html}");
    assert!(html.contains("--w:10\""), "{html}");
    assert!(html.contains("--w:100\""), "{html}");

    // The contract behind those numbers: `--w` is a unitless percent and the
    // stylesheet scales it. A plain `width: var(--w, 100%)` made `--w:100` an
    // invalid width — the biggest bar drew nothing while a valueless row took
    // the whole track.
    assert!(
        phi_canvas::assets::COMPONENTS_CSS.contains("calc(var(--w"),
        "components.css must read --w as a percent number"
    );
}
