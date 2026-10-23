//! The silent-no-op guard: every field the vocabulary declares must leave a
//! trace in the rendered HTML.
//!
//! The share-as-track bug shipped past the suite because nothing asserted a
//! parsed field *did* anything — `h: "18"` parsed, validated, and rendered as
//! if absent. This sweep is one row per declared field: give the field a
//! distinctive value, render, demand its marker. A field that parses but
//! leaves no trace fails here, not in a live session.
//!
//! Two faces, two sweeps (decision A): these rows guard the greybox face
//! (`wireframe`, where every kind renders as `m-*` markup) and
//! `island_fields_leave_a_trace` guards the antd face (color/texture on a
//! wide form, where mapped leaves leave their trace in the slot payload).
//! A field that traces in one face and not the other fails one of the two.

use phi_canvas::{ScreenSpec, render_spec};
use serde_json::json;

fn rendered(node: serde_json::Value) -> String {
    let spec = ScreenSpec::parse(&json!({
        "fidelity": "wireframe",
        "mock": node
    }))
    .expect("spec renders");
    render_spec(&spec, &[])
}

/// Render `node` and demand `needle` — the row label `why` names the field.
fn assert_trace(node: serde_json::Value, needle: &str, why: &str) {
    let html = rendered(node);
    assert!(
        html.contains(needle),
        "{why}: expected {needle:?} in output\n{html}"
    );
}

#[test]
fn layout_fields_leave_a_trace() {
    assert_trace(
        json!({ "kind": "frame", "title": "窗口标题", "body": { "kind": "text", "text": "内文" } }),
        "m-frame-hd\">窗口标题",
        "frame.title",
    );
    assert_trace(
        json!({ "kind": "cols", "ratio": [1, 3], "children": [
            { "kind": "text", "text": "a" }, { "kind": "text", "text": "b" }
        ] }),
        "minmax(0, 3fr)",
        "cols.ratio",
    );
    assert_trace(
        json!({ "kind": "rows", "ratio": [2, 5], "children": [
            { "kind": "text", "text": "a" }, { "kind": "text", "text": "b" }
        ] }),
        "minmax(0, 5fr)",
        "rows.ratio",
    );
    assert_trace(
        json!({ "kind": "stack", "count": 3, "child": { "kind": "text", "text": "s" } }),
        "--rows:3",
        "stack.count",
    );
    assert_trace(
        json!({ "kind": "box", "w": "24%", "h": 40, "text": "面板1" }),
        "--w:24%",
        "box.w (cross-axis share)",
    );
    assert_trace(
        json!({ "kind": "box", "w": "24%", "h": 40, "text": "面板1" }),
        "--h:40%",
        "box.h (numeric share)",
    );
    assert_trace(
        json!({ "kind": "box", "w": "24%", "h": 40, "text": "面板1" }),
        "面板1",
        "box.text",
    );
}

#[test]
fn data_widget_fields_leave_a_trace() {
    assert_trace(
        json!({ "kind": "kpi", "label": "收入", "value": "¥2.1M", "delta": "+4.2%" }),
        "m-kpi-l\">收入",
        "kpi.label",
    );
    assert_trace(
        json!({ "kind": "kpi", "label": "收入", "value": "¥2.1M", "delta": "+4.2%" }),
        "m-kpi-v\">¥2.1M",
        "kpi.value",
    );
    assert_trace(
        json!({ "kind": "kpi", "label": "收入", "value": "¥2.1M", "delta": "+4.2%" }),
        "m-kpi-d\">+4.2%",
        "kpi.delta",
    );
    assert_trace(
        json!({ "kind": "hero", "text": "128,640" }),
        "128,640",
        "hero.text",
    );
    assert_trace(
        json!({ "kind": "sparkline", "shape": "down" }),
        "m-spark-down",
        "sparkline.shape",
    );
    assert_trace(
        json!({ "kind": "ring", "pct": 86, "label": "完成" }),
        "--pct:86",
        "ring.pct",
    );
    assert_trace(
        json!({ "kind": "ring", "pct": 86, "label": "完成" }),
        "完成",
        "ring.label",
    );
    assert_trace(
        json!({ "kind": "bars", "items": [{ "label": "甲", "value": 3 }, { "label": "乙" }] }),
        "--w:100",
        "bars.items[].value (max width)",
    );
    assert_trace(
        json!({ "kind": "bars", "items": [{ "label": "甲", "value": 3 }, { "label": "乙" }] }),
        "甲",
        "bars.items[].label",
    );
    assert_trace(
        json!({ "kind": "area", "label": "趋势" }),
        "趋势",
        "area.label",
    );
    assert_trace(
        json!({ "kind": "viz", "form": "funnel", "label": "转化" }),
        "m-viz-funnel",
        "viz.form",
    );
    assert_trace(
        json!({ "kind": "viz", "form": "funnel", "label": "转化" }),
        "转化",
        "viz.label",
    );
}

#[test]
fn chrome_fields_leave_a_trace() {
    assert_trace(
        json!({ "kind": "phone_shell", "body": { "kind": "text", "text": "屏" } }),
        "m-phone-status",
        "phone_shell draws the status bar around body",
    );
    assert_trace(
        json!({ "kind": "topbar", "title": "设置", "back": true, "action": "编辑" }),
        "m-topbar-back",
        "topbar.back",
    );
    assert_trace(
        json!({ "kind": "topbar", "title": "设置", "back": true, "action": "编辑" }),
        "m-topbar-t\">设置",
        "topbar.title",
    );
    assert_trace(
        json!({ "kind": "topbar", "title": "设置", "back": true, "action": "编辑" }),
        "m-topbar-a\">编辑",
        "topbar.action",
    );
    assert_trace(
        json!({ "kind": "tabbar", "items": ["首页", "我的"], "active": 1 }),
        "m-tab-on",
        "tabbar.active",
    );
    assert_trace(
        json!({ "kind": "toolbar", "items": ["文件"] }),
        "文件",
        "toolbar.items",
    );
    assert_trace(
        json!({ "kind": "dialog", "title": "删除文件？", "text": "不可撤销", "actions": ["取消", "删除"] }),
        "m-dialog-hd\">删除文件？",
        "dialog.title",
    );
    assert_trace(
        json!({ "kind": "dialog", "title": "删除文件？", "text": "不可撤销", "actions": ["取消", "删除"] }),
        "m-dialog-bd\">不可撤销",
        "dialog.text",
    );
    assert_trace(
        json!({ "kind": "dialog", "title": "删除文件？", "text": "不可撤销", "actions": ["取消", "删除"] }),
        "m-dialog-key\">删除",
        "dialog.actions (last is the key one)",
    );
    assert_trace(
        json!({ "kind": "dialog", "title": "t", "body": { "kind": "text", "text": "体" }, "actions": ["好"] }),
        "体",
        "dialog.body",
    );
    assert_trace(
        json!({ "kind": "nav", "items": ["总览"] }),
        "总览",
        "nav.items",
    );
    assert_trace(
        json!({ "kind": "sidebar", "items": ["项目"] }),
        "项目",
        "sidebar.items",
    );
}

#[test]
fn control_fields_leave_a_trace() {
    assert_trace(
        json!({ "kind": "list_row", "title": "通知", "note": "推送", "meta": "开", "chevron": true, "avatar": true }),
        "m-row-t\">通知",
        "list_row.title",
    );
    assert_trace(
        json!({ "kind": "list_row", "title": "通知", "note": "推送", "meta": "开", "chevron": true, "avatar": true }),
        "m-row-n\">推送",
        "list_row.note",
    );
    assert_trace(
        json!({ "kind": "list_row", "title": "通知", "note": "推送", "meta": "开", "chevron": true, "avatar": true }),
        "m-row-meta\">开",
        "list_row.meta",
    );
    assert_trace(
        json!({ "kind": "list_row", "title": "通知", "note": "推送", "meta": "开", "chevron": true, "avatar": true }),
        "m-row-ch",
        "list_row.chevron",
    );
    assert_trace(
        json!({ "kind": "list_row", "title": "通知", "note": "推送", "meta": "开", "chevron": true, "avatar": true }),
        "m-row-av",
        "list_row.avatar",
    );
    assert_trace(
        json!({ "kind": "switch", "label": "深色模式", "on": true }),
        "深色模式",
        "switch.label",
    );
    assert_trace(
        json!({ "kind": "switch", "label": "深色模式", "on": true }),
        "m-switch-t m-on",
        "switch.on",
    );
    assert_trace(
        json!({ "kind": "segment", "items": ["日", "周"], "active": 1 }),
        "m-seg-on",
        "segment.active",
    );
    assert_trace(
        json!({ "kind": "input", "text": "搜索" }),
        "搜索",
        "input.text",
    );
    assert_trace(
        json!({ "kind": "button", "text": "导出" }),
        "导出",
        "button.text",
    );
}

#[test]
fn dense_content_fields_leave_a_trace() {
    assert_trace(
        json!({ "kind": "table", "cols": ["名称", "状态"], "rows": [["api", "就绪"], ["web", ""]] }),
        "m-th",
        "table.cols",
    );
    assert_trace(
        json!({ "kind": "table", "cols": ["名称", "状态"], "rows": [["api", "就绪"], ["web", ""]] }),
        "就绪",
        "table.rows",
    );
    assert_trace(
        json!({ "kind": "tree", "items": [{ "label": "src", "depth": 2 }] }),
        "--d:2",
        "tree.items[].depth",
    );
    assert_trace(
        json!({ "kind": "tree", "items": [{ "label": "src", "depth": 2 }] }),
        "src",
        "tree.items[].label",
    );
    assert_trace(
        json!({ "kind": "text", "text": "说明" }),
        "说明",
        "text.text",
    );
    assert_trace(
        json!({ "kind": "placeholder", "text": "占位区" }),
        "占位区",
        "placeholder.text",
    );
}

/// The antd face's sweep: every field of every mapped kind must reach the
/// island payload the runtime hydrates. A field dropped here would render the
/// default component and nobody would notice until a live session.
#[test]
fn island_fields_leave_a_trace() {
    let html = rendered_islands(json!({
        "kind": "rows",
        "children": [
            {
                "kind": "rows",
                "children": [
                    { "kind": "nav", "items": ["总览"] },
                    { "kind": "sidebar", "items": ["项目A"] },
                    { "kind": "input", "text": "搜索词" },
                    { "kind": "button", "text": "导出键" },
                    { "kind": "kpi", "label": "收入标", "value": "值2.1", "delta": "变4.2" },
                    { "kind": "hero", "text": "大数99" },
                    { "kind": "ring", "pct": 86, "label": "完成率" },
                    { "kind": "bars", "items": [{ "label": "甲乙", "value": 3 }] }
                ]
            },
            {
                "kind": "rows",
                "children": [
                    { "kind": "dialog", "title": "删除题", "text": "消息体", "actions": ["取消行", "删除行"] },
                    { "kind": "list_row", "title": "行题", "note": "行注", "meta": "行元", "chevron": true, "avatar": true },
                    { "kind": "switch", "label": "开关标", "on": true },
                    { "kind": "segment", "items": ["日", "周"], "active": 1 },
                    { "kind": "table", "cols": ["列名"], "rows": [["格值"]] },
                    { "kind": "tree", "items": [{ "label": "树项", "depth": 2 }] },
                    { "kind": "toolbar", "items": ["工具项"] }
                ]
            }
        ]
    }));

    for needle in [
        // The binding table's face: the runtime resolves these names.
        "\"component\":\"Menu\"",
        "\"component\":\"Input\"",
        "\"component\":\"Button\"",
        "\"component\":\"Card\"",
        "\"component\":\"Statistic\"",
        "\"component\":\"Progress\"",
        "\"component\":\"List.Item\"",
        "\"component\":\"Switch\"",
        "\"component\":\"Segmented\"",
        "\"component\":\"Table\"",
        "\"component\":\"Tree\"",
        "\"component\":\"Space\"",
        // One marker per field — the payload is greppable because the slot
        // attribute keeps JSON's quotes literal.
        "\"items\":[\"总览\"]",
        "\"items\":[\"项目A\"]",
        "\"text\":\"搜索词\"",
        "\"text\":\"导出键\"",
        "\"label\":\"收入标\"",
        "\"value\":\"值2.1\"",
        "\"delta\":\"变4.2\"",
        "\"text\":\"大数99\"",
        "\"pct\":86",
        "\"label\":\"完成率\"",
        "\"label\":\"甲乙\"",
        "\"value\":3",
        "\"title\":\"删除题\"",
        "\"text\":\"消息体\"",
        "\"actions\":[\"取消行\",\"删除行\"]",
        "\"title\":\"行题\"",
        "\"note\":\"行注\"",
        "\"meta\":\"行元\"",
        "\"chevron\":true",
        "\"avatar\":true",
        "\"label\":\"开关标\"",
        "\"on\":true",
        "\"items\":[\"日\",\"周\"]",
        "\"active\":1",
        "\"cols\":[\"列名\"]",
        "\"rows\":[[\"格值\"]]",
        "\"label\":\"树项\"",
        "\"depth\":2",
        "\"items\":[\"工具项\"]",
    ] {
        assert!(html.contains(needle), "missing {needle:?}:\n{html}");
    }
}

fn rendered_islands(node: serde_json::Value) -> String {
    let spec = ScreenSpec::parse(&json!({
        "fidelity": "color",
        "mock": node
    }))
    .expect("spec renders");
    render_spec(&spec, &[])
}

/// Theme tokens on the antd face: every token a variant sets must ride the
/// card's `data-antd-theme` payload — that JSON is the ConfigProvider input,
/// which is where the antd chart/progress colors come from.
#[test]
fn theme_tokens_reach_the_antd_face() {
    let spec = ScreenSpec::parse(&json!({
        "fidelity": "color",
        "mock": {
            "kind": "rows",
            "children": [
                { "kind": "ring", "pct": 50, "label": "完成" },
                { "kind": "kpi", "label": "收入", "value": "1" }
            ]
        },
        "variants": [
            {
                "id": "deep",
                "theme": {
                    "bg": "#0A1428", "panel": "#0B1530", "border": "#1B2A4A",
                    "accent": "#00D4FF", "accent2": "#7C5CFF",
                    "text": "#8FA3C8", "text_bright": "#EAF2FF",
                    "surface": "glass", "frost": 2, "elevation": 3, "edge": 2, "glow": 1
                }
            }
        ]
    }))
    .expect("spec renders");
    let html = render_spec(&spec, &[]);
    for needle in [
        "\"bg\":\"#0A1428\"",
        "\"panel\":\"#0B1530\"",
        "\"border\":\"#1B2A4A\"",
        "\"accent\":\"#00D4FF\"",
        "\"accent2\":\"#7C5CFF\"",
        "\"text\":\"#8FA3C8\"",
        "\"text_bright\":\"#EAF2FF\"",
    ] {
        assert!(
            html.contains(needle),
            "theme never reaches the payload {needle:?}:\n{html}"
        );
    }
}

/// The chain is only real if both ends agree: the runtime must read every
/// theme key the card writes, every key it reads must be a real `Theme`
/// field, and the accent that colors antd Progress/Statistic must actually
/// be assigned to `colorPrimary` (chart colors ride that token).
#[test]
fn the_runtime_reads_the_theme_keys_the_card_writes() {
    let runtime = include_str!("../runtime/src/main.jsx");
    let theme_fields = [
        "bg",
        "panel",
        "border",
        "accent",
        "accent2",
        "text",
        "text_bright",
        "surface",
        "frost",
        "elevation",
        "edge",
        "glow",
    ];
    for key in [
        "bg",
        "panel",
        "border",
        "accent",
        "accent2",
        "text",
        "text_bright",
    ] {
        assert!(
            runtime.contains(&format!("theme.{key}")),
            "runtime never reads theme.{key}"
        );
    }
    for key in theme_fields {
        // Only keys the runtime actually reads need to be declared here —
        // scanning `theme.` mentions keeps a new read from landing on a
        // field this side stopped emitting.
        if runtime.contains(&format!("theme.{key}")) {
            assert!(
                theme_fields.contains(&key),
                "runtime reads unknown theme.{key}"
            );
        }
    }
    assert!(
        runtime.contains("token.colorPrimary = theme.accent"),
        "chart colors ride colorPrimary: the accent must be assigned to it"
    );
    let reads: Vec<String> = runtime
        .split("theme.")
        .skip(1)
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect()
        })
        .collect();
    for key in reads {
        assert!(
            theme_fields.contains(&key.as_str()),
            "runtime reads theme.{key}, which is not a Theme field"
        );
    }
}
