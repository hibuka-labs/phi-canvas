//! Acceptance row 1: envelope parsing — title / options / multiselect / missing fields.

use phi_canvas::{EnvelopeError, ScreenEnvelope};
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

// ---- `screen` vs `body_html` (gate 2: structured spec + escape hatch) ----

fn spec_args() -> serde_json::Value {
    json!({
        "title": "配色方向",
        "options": [
            { "id": "deep-blue", "label": "深空蓝" },
            { "id": "neon-purple", "label": "暗紫霓虹" }
        ],
        "screen": {
            "fidelity": "color",
            "mock": {
                "kind": "cols",
                "children": [
                    { "kind": "hero", "id": "hero", "text": "128,640" },
                    { "kind": "button", "id": "cta", "text": "导出" }
                ]
            },
            "variants": [
                { "id": "deep-blue", "theme": { "bg": "#0A1428" }, "emph": ["hero"] },
                { "id": "neon-purple", "theme": { "bg": "#12071F" }, "emph": ["cta"] }
            ]
        }
    })
}

#[test]
fn screen_alone_materializes_the_body() {
    let mut env = ScreenEnvelope::parse(&spec_args()).expect("envelope");
    assert!(env.has_spec());
    assert!(
        env.body_html.is_empty(),
        "body is not authored by the model"
    );

    let both = env.materialize_screen().expect("render");
    assert!(!both, "no body_html was given");
    assert!(env.body_html.contains("m-screen"), "{}", env.body_html);
    assert!(
        env.body_html.contains("--m-bg:#0A1428"),
        "{}",
        env.body_html
    );
    assert!(
        env.screen.is_some(),
        "the spec stays on the envelope for the sidecar"
    );
}

#[test]
fn body_html_alone_is_untouched() {
    let mut env = ScreenEnvelope::parse(&full()).expect("envelope");
    let before = env.body_html.clone();
    let both = env.materialize_screen().expect("render");
    assert!(!both);
    assert_eq!(env.body_html, before);
}

#[test]
fn sending_both_keeps_the_spec_and_reports_it() {
    // The design contract: the spec wins and the caller says so — the model's
    // markup is not silently shown, and it is not silently discarded either.
    let mut v = spec_args();
    v["body_html"] = json!("<div class=\"cards\">hand written</div>");
    let mut env = ScreenEnvelope::parse(&v).expect("envelope");

    let both = env.materialize_screen().expect("render");
    assert!(both, "both were given");
    assert!(env.body_html.contains("m-screen"), "{}", env.body_html);
    assert!(
        !env.body_html.contains("hand written"),
        "the spec wins over body_html"
    );
}

#[test]
fn variant_ids_must_name_a_real_option() {
    let mut v = spec_args();
    v["screen"]["variants"][0]["id"] = json!("deep-blur");
    let err = ScreenEnvelope::parse(&v).unwrap_err();
    assert!(format!("{err}").contains("matches no option id"), "{err}");
}

#[test]
fn display_only_variants_may_use_free_form_ids() {
    // No options → nothing to align to; the id is just the card caption.
    let v = json!({
        "title": "质感方向",
        "options": [],
        "screen": {
            "fidelity": "texture",
            "mock": { "kind": "text", "text": "card" },
            "variants": [
                { "id": "玻璃拟态", "theme": { "surface": "glass", "frost": 2 } },
                { "id": "硬朗工业", "theme": { "surface": "metal", "frost": 1 } }
            ]
        }
    });
    ScreenEnvelope::parse(&v).expect("envelope");
}

#[test]
fn a_malformed_spec_surfaces_as_an_envelope_error() {
    let mut v = spec_args();
    v["screen"]["fidelity"] = json!("photoreal");
    assert!(ScreenEnvelope::parse(&v).is_err());

    let mut v = spec_args();
    v["screen"]["mock"]["kind"] = json!("chartjs");
    assert!(ScreenEnvelope::parse(&v).is_err());
}

// ── reference (参考物贴片) ──────────────────────────────────────────────────

#[test]
fn reference_is_optional_and_carries_src_and_caption() {
    let v = json!({
        "title": "参考这个方向改，哪个更像？",
        "options": [{ "id": "a", "label": "更接近" }],
        "body_html": "<div class=\"mockup\"></div>",
        "reference": { "src": "/tmp/zhentai.png", "caption": "正泰首页 · 2026-10" }
    });
    let env = ScreenEnvelope::parse(&v).expect("envelope");
    let reference = env.reference.expect("reference");
    assert_eq!(reference.src, "/tmp/zhentai.png");
    assert_eq!(reference.caption.as_deref(), Some("正泰首页 · 2026-10"));
    // The host sets this at save time; the model never does.
    assert!(reference.resolved.is_none());
}

#[test]
fn reference_absent_leaves_the_field_none() {
    let v = json!({
        "title": "配色方向，选一个？",
        "options": [{ "id": "a", "label": "深空蓝" }],
        "body_html": "<div class=\"mockup\"></div>"
    });
    let env = ScreenEnvelope::parse(&v).expect("envelope");
    assert!(env.reference.is_none());
}

#[test]
fn reference_requires_src() {
    let v = json!({
        "title": "参考这个方向改",
        "options": [{ "id": "a", "label": "更接近" }],
        "body_html": "<div></div>",
        "reference": { "caption": "no source" }
    });
    assert!(ScreenEnvelope::parse(&v).is_err());

    let v = json!({
        "title": "参考这个方向改",
        "options": [{ "id": "a", "label": "更接近" }],
        "body_html": "<div></div>",
        "reference": { "src": "   " }
    });
    assert!(ScreenEnvelope::parse(&v).is_err());
}

#[test]
fn reference_src_rejects_control_characters() {
    let v = json!({
        "title": "参考这个方向改",
        "options": [{ "id": "a", "label": "更接近" }],
        "body_html": "<div></div>",
        "reference": { "src": "/tmp/a\u{0000}b.png" }
    });
    assert!(ScreenEnvelope::parse(&v).is_err());
}

#[test]
fn reference_caption_must_be_a_single_line() {
    let v = json!({
        "title": "参考这个方向改",
        "options": [{ "id": "a", "label": "更接近" }],
        "body_html": "<div></div>",
        "reference": { "src": "/tmp/a.png", "caption": "one\ntwo" }
    });
    assert!(ScreenEnvelope::parse(&v).is_err());
}

// ── host-set fields ────────────────────────────────────────────────────────

#[test]
fn interstitial_is_the_hosts_to_set_and_never_the_models() {
    // A tool call must not be able to park its own screen out of the version
    // rail by declaring itself interstitial.
    let mut v = full();
    v["interstitial"] = json!(true);
    let env = ScreenEnvelope::parse(&v).expect("parses");
    assert!(!env.interstitial, "parse clears the host-set flag");
}

// ── the escape ledger ─────────────────────────────────────────────────────

#[test]
fn an_escape_tag_travels_with_a_body_html_push() {
    // The tag is bookkeeping, never rendered: it names the shape the closed
    // vocabulary could not hold so the same shape showing up twice is greppable
    // evidence for what to add next.
    let mut v = full();
    v["escape"] = json!("timeline");
    let env = ScreenEnvelope::parse(&v).expect("parses");
    assert_eq!(env.escape.as_deref(), Some("timeline"));

    let again = serde_json::from_value::<ScreenEnvelope>(serde_json::to_value(&env).unwrap())
        .expect("round trip");
    assert_eq!(again, env);
}

#[test]
fn an_escape_tag_has_no_business_on_a_screen_push() {
    // A `screen` is the vocabulary working — there is nothing escaped, so the
    // tag would be a lie in the ledger.
    let mut v = spec_args();
    v["escape"] = json!("timeline");
    let err = ScreenEnvelope::parse(&v).expect_err("escape on a screen");
    assert!(matches!(err, EnvelopeError::InvalidField { .. }), "{err}");
    assert!(format!("{err}").contains("escape"), "{err}");
}

#[test]
fn an_escape_tag_names_a_shape_not_a_note() {
    let mut v = full();
    v["escape"] = json!("x".repeat(41));
    let err = ScreenEnvelope::parse(&v).expect_err("a note is not a tag");
    assert!(matches!(err, EnvelopeError::InvalidField { .. }), "{err}");

    let mut v = full();
    v["escape"] = json!("  ");
    assert!(
        ScreenEnvelope::parse(&v).is_err(),
        "a blank tag is not a tag"
    );
}

#[test]
fn a_type_fumble_names_the_field_not_just_the_type() {
    // A genuine type fumble (a boolean where a `ratio` weight or a `box` size
    // wants a number) must name the field — a pathless "expected u32" costs a
    // full re-push per guess.
    let mut v = spec_args();
    v["screen"]["mock"]["ratio"] = json!([true, 1]);
    let err = ScreenEnvelope::parse(&v).expect_err("boolean weight");
    let msg = format!("{err}");
    assert!(msg.contains("screen.mock.ratio"), "{msg}");
    assert!(msg.contains("invalid type"), "{msg}");

    let mut v = spec_args();
    v["screen"]["mock"]["children"][0] = json!({ "kind": "box", "id": "b", "w": true });
    let err = ScreenEnvelope::parse(&v).expect_err("boolean w");
    let msg = format!("{err}");
    assert!(msg.contains("screen.mock.children[0].w"), "{msg}");
}

#[test]
fn a_float_ratio_weight_is_a_weight_not_a_fumble() {
    // Observed live: `ratio: [1, 5, 0.7]` — the model reaching for a finer
    // split than whole numbers allow. Weights are proportions; the push round
    // must not be spent re-typing them as integers.
    let mut v = spec_args();
    v["screen"]["mock"]["ratio"] = json!([1, 0.7]);
    ScreenEnvelope::parse(&v).expect("float weights are weights");

    let mut v = spec_args();
    v["screen"]["mock"]["ratio"] = json!(["1", "3"]);
    ScreenEnvelope::parse(&v).expect("numeric-string weights are weights");
}

#[test]
fn a_non_positive_ratio_weight_is_rejected_by_name() {
    // A zero or negative track is degenerate CSS — refuse it with the field,
    // not with a render that silently collapses a child.
    let mut v = spec_args();
    v["screen"]["mock"]["ratio"] = json!([0, 1]);
    let err = ScreenEnvelope::parse(&v).expect_err("zero weight");
    let msg = format!("{err}");
    assert!(msg.contains("mock.ratio[0]"), "{msg}");
    assert!(msg.contains("positive"), "{msg}");

    let mut v = spec_args();
    v["screen"]["mock"]["ratio"] = json!([1, -2]);
    let err = ScreenEnvelope::parse(&v).expect_err("negative weight");
    let msg = format!("{err}");
    assert!(msg.contains("mock.ratio[1]"), "{msg}");
}

#[test]
fn a_numeric_box_share_is_not_a_fumble() {
    // Observed twice in live runs: the model writes `h: 18` where the schema
    // stringifies percentages. `18` and `"18%"` mean the same share of the
    // parent — accept the number instead of spending a push round on it.
    let mut v = spec_args();
    // Keep the id the fixture's `emph` points at — this test is about the
    // type of `w`/`h`, not about reference validation.
    v["screen"]["mock"]["children"][0] = json!({ "kind": "box", "id": "hero", "w": 14, "h": 18 });
    ScreenEnvelope::parse(&v).expect("numeric w/h are proportions");
}

#[test]
fn a_stray_node_field_is_rejected_on_the_push_path() {
    // The closed vocabulary is only closed if the push path enforces it —
    // `ScreenEnvelope::parse` must run the spec checks, not just the derive
    // (which silently drops fields a tagged enum does not declare).
    let mut v = spec_args();
    v["screen"]["mock"]["style"] = json!("border-radius: 8px");
    let err = ScreenEnvelope::parse(&v).expect_err("stray field");
    let msg = format!("{err}");
    assert!(msg.contains("style"), "{msg}");
}
