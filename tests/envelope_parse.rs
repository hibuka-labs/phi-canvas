//! Acceptance row 1: envelope parsing — title / options / multiselect / missing fields.

use phi_canvas::ScreenEnvelope;
use serde_json::json;

fn full() -> serde_json::Value {
    json!({
        "title": "大屏顶部导航，哪个布局更专业？",
        "subtitle": "考虑信息密度和视觉层级",
        "options": [
            { "id": "a", "label": "侧边栏导航", "note": "筛选常驻，屏幕利用率高" },
            { "id": "b", "label": "顶部横向导航", "note": "视野开阔，筛选收进弹层" },
            { "id": "c", "label": "混合式" }
        ],
        "body_html": "<div class=\"cards\"></div>",
        "multiselect": false
    })
}

#[test]
fn parse_full_envelope() {
    let env = ScreenEnvelope::parse(&full()).expect("valid envelope");
    assert_eq!(env.title, "大屏顶部导航，哪个布局更专业？");
    assert_eq!(env.subtitle.as_deref(), Some("考虑信息密度和视觉层级"));
    assert_eq!(env.options.len(), 3);
    assert_eq!(env.options[0].id, "a");
    assert_eq!(env.options[0].label, "侧边栏导航");
    assert_eq!(
        env.options[0].note.as_deref(),
        Some("筛选常驻，屏幕利用率高")
    );
    assert_eq!(env.options[2].note, None);
    assert_eq!(env.body_html, "<div class=\"cards\"></div>");
    assert!(!env.multiselect);
}

#[test]
fn missing_title_is_rejected() {
    let mut v = full();
    v.as_object_mut().unwrap().remove("title");
    assert!(ScreenEnvelope::parse(&v).is_err(), "title is required");
}

#[test]
fn empty_title_is_rejected() {
    let mut v = full();
    v["title"] = json!("   ");
    assert!(ScreenEnvelope::parse(&v).is_err(), "blank title is invalid");
}

#[test]
fn missing_options_is_rejected() {
    let mut v = full();
    v.as_object_mut().unwrap().remove("options");
    assert!(
        ScreenEnvelope::parse(&v).is_err(),
        "options must be explicit (empty array = display-only screen)"
    );
}

#[test]
fn missing_body_defaults_to_empty_string() {
    let mut v = full();
    v.as_object_mut().unwrap().remove("body_html");
    let env = ScreenEnvelope::parse(&v).expect("body_html is optional");
    assert_eq!(env.body_html, "");
}

#[test]
fn multiselect_defaults_to_false() {
    let mut v = full();
    v.as_object_mut().unwrap().remove("multiselect");
    let env = ScreenEnvelope::parse(&v).expect("multiselect is optional");
    assert!(!env.multiselect);
}

#[test]
fn missing_subtitle_is_none() {
    let mut v = full();
    v.as_object_mut().unwrap().remove("subtitle");
    let env = ScreenEnvelope::parse(&v).expect("subtitle is optional");
    assert_eq!(env.subtitle, None);
}

#[test]
fn empty_options_is_display_only() {
    let mut v = full();
    v["options"] = json!([]);
    let env = ScreenEnvelope::parse(&v).expect("empty options is a display-only screen");
    assert!(env.is_display_only());
}

#[test]
fn non_empty_options_is_not_display_only() {
    let env = ScreenEnvelope::parse(&full()).unwrap();
    assert!(!env.is_display_only());
}

#[test]
fn duplicate_option_ids_are_rejected() {
    let v = json!({
        "title": "pick",
        "options": [
            { "id": "a", "label": "one" },
            { "id": "a", "label": "two" }
        ]
    });
    assert!(
        ScreenEnvelope::parse(&v).is_err(),
        "option ids must be unique"
    );
}

#[test]
fn option_id_charset_is_restricted() {
    for bad in ["", "a b", "a\"b", "a>b", "a<b", "中文", "a/b", "a&b"] {
        let v = json!({
            "title": "pick",
            "options": [{ "id": bad, "label": "one" }]
        });
        assert!(
            ScreenEnvelope::parse(&v).is_err(),
            "id {bad:?} must be rejected (HTML-attribute-safe charset only)"
        );
    }
    for ok in ["a", "A1", "side-bar", "side_bar", "opt42"] {
        let v = json!({
            "title": "pick",
            "options": [{ "id": ok, "label": "one" }]
        });
        assert!(
            ScreenEnvelope::parse(&v).is_ok(),
            "id {ok:?} should be accepted"
        );
    }
}

#[test]
fn option_label_must_be_non_empty() {
    let v = json!({
        "title": "pick",
        "options": [{ "id": "a", "label": "  " }]
    });
    assert!(ScreenEnvelope::parse(&v).is_err());
}

#[test]
fn missing_option_id_or_label_is_rejected() {
    let no_id = json!({ "title": "pick", "options": [{ "label": "one" }] });
    let no_label = json!({ "title": "pick", "options": [{ "id": "a" }] });
    assert!(ScreenEnvelope::parse(&no_id).is_err());
    assert!(ScreenEnvelope::parse(&no_label).is_err());
}

#[test]
fn control_characters_in_text_fields_are_rejected() {
    for field in ["title", "subtitle"] {
        let mut v = full();
        v[field] = json!("line1\nline2");
        assert!(
            ScreenEnvelope::parse(&v).is_err(),
            "{field} must be single-line"
        );
    }
    let v = json!({
        "title": "pick",
        "options": [{ "id": "a", "label": "bad\nlabel" }]
    });
    assert!(
        ScreenEnvelope::parse(&v).is_err(),
        "labels must be single-line"
    );
}

#[test]
fn body_html_size_cap() {
    let mut v = full();
    v["body_html"] = json!("x".repeat(300 * 1024));
    assert!(
        ScreenEnvelope::parse(&v).is_err(),
        "body_html over the 256 KiB cap must be rejected"
    );
}

#[test]
fn parse_from_json_object_only() {
    assert!(ScreenEnvelope::parse(&json!("string")).is_err());
    assert!(ScreenEnvelope::parse(&json!([1, 2])).is_err());
    assert!(ScreenEnvelope::parse(&json!(null)).is_err());
}

#[test]
fn single_line_text_fields_are_length_capped() {
    let long = "长".repeat(600);
    let mut v = full();
    v["title"] = json!(&long);
    assert!(ScreenEnvelope::parse(&v).is_err(), "title must be capped");

    let mut v = full();
    v["subtitle"] = json!(&long);
    assert!(
        ScreenEnvelope::parse(&v).is_err(),
        "subtitle must be capped"
    );

    let v = json!({
        "title": "pick",
        "options": [{ "id": "a", "label": long }]
    });
    assert!(ScreenEnvelope::parse(&v).is_err(), "label must be capped");
}

#[test]
fn option_id_length_is_capped() {
    let v = json!({
        "title": "pick",
        "options": [{ "id": "a".repeat(65), "label": "one" }]
    });
    assert!(ScreenEnvelope::parse(&v).is_err());
}
