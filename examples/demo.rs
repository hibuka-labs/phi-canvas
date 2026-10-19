//! Human walkthrough for acceptance row 5: push a few screens and look at the
//! page for 30 seconds.
//!
//! ```
//! cargo run --example demo
//! ```
//!
//! The browser is launched here because the human started this demo at their
//! own terminal — that is the consent. Real consent gating (`y` before any
//! launch) is product work and lives in the host (1C).
//!
//! What to look at: options panel under the mockup, click an option and watch
//! this terminal print the event, then Ctrl-C to end (open tabs land on the
//! farewell screen).

use std::collections::HashMap;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use phi_canvas::tools::{BrowserLauncher, CanvasTools, LaunchDecision};
use serde_json::json;

struct DemoLauncher;

impl BrowserLauncher for DemoLauncher {
    fn launch(&self, url: &str) -> LaunchDecision {
        let started = if cfg!(target_os = "macos") {
            Command::new("open").arg(url).status()
        } else if cfg!(target_os = "windows") {
            Command::new("cmd").args(["/C", "start", "", url]).status()
        } else {
            Command::new("xdg-open").arg(url).status()
        };
        match started {
            Ok(status) if status.success() => LaunchDecision::Launched,
            _ => {
                eprintln!("浏览器没能自动打开，请手动访问：{url}");
                LaunchDecision::AwaitingConsent
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let store = tempfile::tempdir().expect("temp session dir");
    println!("session dir: {}", store.path().display());

    let tools =
        CanvasTools::new(store.path().join("canvas"), "demo").with_launcher(Arc::new(DemoLauncher));
    let labels: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));

    println!(">> canvas_open");
    println!("   {}", tools.open().await.expect("open"));

    println!(">> canvas_push (single select + mockup)");
    let text = tools
        .push(&json!({
            "title": "大屏顶部导航，哪个布局更专业？",
            "subtitle": "考虑信息密度和视觉层级",
            "options": [
                { "id": "a", "label": "侧边栏导航", "note": "筛选常驻，屏幕利用率高" },
                { "id": "b", "label": "顶部横向导航", "note": "视野开阔，筛选收进弹层" },
                { "id": "c", "label": "混合式", "note": "主导航横排，工具区进侧栏" }
            ],
            "body_html": "<div class=\"mockup\"><div class=\"mock-header\">运营大屏 · 概览</div><div class=\"mock-body\"><div class=\"split\"><div class=\"mock-sidebar\">筛选 · 图层 · 时间</div><div class=\"mock-content\"><div class=\"placeholder\">图表区</div></div></div></div></div>"
        }))
        .await
        .expect("push");
    println!("   {text}");
    record_labels(
        &labels,
        &[("a", "侧边栏导航"), ("b", "顶部横向导航"), ("c", "混合式")],
    );

    let canvas = tools.canvas().await.expect("canvas bound");
    let mut rx = canvas.subscribe();
    let mut pushed_multiselect = false;
    let mut pushed_display = false;

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            event = rx.recv() => {
                let Ok(event) = event else { break };
                let label = labels
                    .lock()
                    .unwrap()
                    .get(&option_id_of(&event.action))
                    .cloned()
                    .unwrap_or_else(|| format!("{:?}", event.action));
                println!("<< 事件 #{seq} 屏{screen} → {label}", seq = event.seq, screen = event.screen);

                if !pushed_multiselect {
                    pushed_multiselect = true;
                    println!(">> canvas_push (multiselect)");
                    let text = tools
                        .push(&json!({
                            "title": "这两版哪些细节值得保留？",
                            "subtitle": "可以多选",
                            "options": [
                                { "id": "keep-dense", "label": "高信息密度", "note": "一屏看完关键指标" },
                                { "id": "keep-breathe", "label": "留白节奏", "note": "区块间距更克制" },
                                { "id": "keep-contrast", "label": "强对比标题", "note": "视觉层级更清楚" }
                            ],
                            "body_html": "<div class=\"cards\"><div class=\"card\"><div class=\"card-body\"><h3>版本 A · 密集</h3><div class=\"placeholder\">mockup A</div></div></div><div class=\"card\"><div class=\"card-body\"><h3>版本 B · 呼吸</h3><div class=\"placeholder\">mockup B</div></div></div></div>"
                        }))
                        .await
                        .expect("push");
                    println!("   {text}");
                } else if !pushed_display {
                    pushed_display = true;
                    println!(">> canvas_push (display only)");
                    let text = tools
                        .push(&json!({
                            "title": "配色方向",
                            "options": [],
                            "body_html": "<div class=\"cards\"><div class=\"card\"><div class=\"card-image\" style=\"background:#2f6fed\"></div><div class=\"card-body\"><h3>冷静蓝</h3></div></div><div class=\"card\"><div class=\"card-image\" style=\"background:#1f9d6c\"></div><div class=\"card-body\"><h3>增长绿</h3></div></div><div class=\"card\"><div class=\"card-image\" style=\"background:#c2410c\"></div><div class=\"card-body\"><h3>预警橙</h3></div></div></div><div class=\"pros-cons\"><div class=\"pros\"><h3>冷静蓝</h3><p>专业、可信，长时间盯着不累。</p></div><div class=\"cons\"><h3>注意</h3><p>与竞品撞色率高，需要靠版式拉开差距。</p></div></div>"
                        }))
                        .await
                        .expect("push");
                    println!("   {text}");
                    println!("（展示屏没有选项 — 点完就看终端，Ctrl-C 退出）");
                }
            }
        }
    }

    println!(">> canvas_close");
    println!("   {}", tools.close().await.expect("close"));
    // Give any open tab a moment to land on the farewell screen before the
    // listener goes away with the process.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    canvas.shutdown().await;
    println!("demo 结束。");
}

fn record_labels(labels: &Mutex<HashMap<String, String>>, pairs: &[(&str, &str)]) {
    let mut map = labels.lock().unwrap();
    for (id, label) in pairs {
        map.insert((*id).to_string(), (*label).to_string());
    }
}

fn option_id_of(action: &phi_canvas::EventAction) -> String {
    match action {
        phi_canvas::EventAction::Select { option_id } => option_id.clone(),
        phi_canvas::EventAction::Confirm => "confirm".into(),
        phi_canvas::EventAction::Skip => "skip".into(),
    }
}
