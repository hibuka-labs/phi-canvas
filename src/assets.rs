//! Frame assembly: template assets compiled into the binary plus the
//! host-rendered options panel.
//!
//! Rendering contract:
//! - envelope metadata (title / subtitle / labels / notes) is HTML-escaped,
//! - `body_html` is spliced in verbatim (it is model-authored markup — that is
//!   the point of the body), but only ever after every template marker has
//!   been filled, so it cannot re-trigger substitution,
//! - the session token is injected into the page at serve time and nowhere
//!   else (never on disk, never in a URL).

use crate::envelope::ScreenEnvelope;

pub const FRAME_HTML: &str = include_str!("assets/frame.html");
pub const COMPONENTS_CSS: &str = include_str!("assets/components.css");
pub const HELPER_JS: &str = include_str!("assets/helper.js");

/// Fill `{{MARKER}}` placeholders in one left-to-right pass. Output is never
/// rescanned, so substituted values may contain `{{` without consequence.
fn fill_markers(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() + 512);
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                let name = &after[..end];
                match vars.iter().find(|(key, _)| *key == name) {
                    Some((_, value)) => out.push_str(value),
                    None => out.push_str(&rest[start..start + 2 + end + 2]),
                }
                rest = &after[end + 2..];
            }
            None => {
                out.push_str(&rest[start..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Minimal HTML text escaping for untrusted envelope metadata.
pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Render one screen: frame + escaped metadata + verbatim body + options panel.
pub fn render_screen(env: &ScreenEnvelope, seq: u64, token: &str) -> String {
    let subtitle = match &env.subtitle {
        Some(text) => format!("<p class=\"subtitle\">{}</p>", escape_html(text)),
        None => String::new(),
    };
    let indicator = if env.is_display_only() {
        "展示中 · 回到终端继续讨论"
    } else if env.multiselect {
        "点选后按「确认」提交，或「跳过」"
    } else {
        "点击选择，然后回到终端继续"
    };

    let (head, tail) = FRAME_HTML
        .split_once("{{BODY}}")
        .expect("frame template must contain a {{BODY}} marker");
    let vars = [
        ("TOKEN", token),
        ("SEQ", &seq.to_string()),
        ("TITLE", &escape_html(&env.title)),
        ("SUBTITLE", &subtitle),
        ("OPTIONS", &render_options(env)),
        ("INDICATOR", indicator),
    ];
    format!(
        "{}{}{}",
        fill_markers(head, &vars),
        env.body_html,
        fill_markers(tail, &vars)
    )
}

/// Host-rendered options panel — the canonical click surface. `body_html` may
/// also carry `data-choice="<id>"` for spatial picks, but the model is told
/// not to duplicate this panel.
fn render_options(env: &ScreenEnvelope) -> String {
    if env.is_display_only() {
        return String::new();
    }
    let mut items = String::new();
    for option in &env.options {
        let letter = option
            .id
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase())
            .unwrap_or('?');
        let note = match &option.note {
            Some(text) => format!("<p>{}</p>", escape_html(text)),
            None => String::new(),
        };
        items.push_str(&format!(
            r#"<div class="option" data-choice="{id}" role="button" tabindex="0">
  <div class="letter">{letter}</div>
  <div class="content"><h3>{label}</h3>{note}</div>
</div>"#,
            id = escape_html(&option.id),
            letter = letter,
            label = escape_html(&option.label),
            note = note,
        ));
    }

    let confirm = if env.multiselect {
        r#"<button class="btn" data-action="confirm">确认</button>"#
    } else {
        ""
    };
    format!(
        r#"<div class="options" data-multiselect="{multi}">
{items}</div>
<div class="actions">{confirm}<button class="btn btn-ghost" data-action="skip">跳过</button></div>"#,
        multi = env.multiselect,
        items = items,
        confirm = confirm,
    )
}

/// A frame with a single notice card (empty canvas, farewell screens).
pub fn render_notice(title: &str, message: &str, token: &str, seq: u64) -> String {
    let (head, tail) = FRAME_HTML
        .split_once("{{BODY}}")
        .expect("frame template must contain a {{BODY}} marker");
    let body = format!(
        r#"<div class="notice"><h2>{title}</h2><p>{message}</p></div>"#,
        title = escape_html(title),
        message = escape_html(message),
    );
    let vars = [
        ("TOKEN", token),
        ("SEQ", &seq.to_string()),
        ("TITLE", &escape_html(title)),
        ("SUBTITLE", ""),
        ("OPTIONS", ""),
        ("INDICATOR", "回到终端继续讨论"),
    ];
    format!(
        "{}{}{}",
        fill_markers(head, &vars),
        body,
        fill_markers(tail, &vars)
    )
}
