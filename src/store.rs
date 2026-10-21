//! Session-directory persistence: `screens/`, `events`, `server-info`.
//!
//! Layout under the store root (host chooses it — conventionally
//! `~/.phimint/canvas/<session>/`):
//!
//! ```text
//! server-info          JSON: session_id, url, port, started_at_ms
//! events               JSONL: one CanvasEvent per line
//! screens/0001.html    body_html fragment exactly as pushed
//! screens/0001.json    envelope metadata (title/subtitle/options/multiselect)
//! ```
//!
//! The metadata/fragment split keeps heavy HTML separable from the light
//! decision record (see the sizing open question in the design doc). Nothing
//! written here ever contains the session token.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::envelope::ScreenEnvelope;
use crate::event::CanvasEvent;

/// Startup facts the host (and any tooling) reads to find the canvas.
/// Deliberately has no token field — see the module docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInfo {
    pub session_id: String,
    pub url: String,
    pub port: u16,
    pub started_at_ms: u64,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("corrupt screen {seq}: {reason}")]
    CorruptScreen { seq: u64, reason: String },
    #[error("serialization error: {0}")]
    Serialize(String),
}

pub struct CanvasStore {
    root: PathBuf,
}

impl CanvasStore {
    /// Create (if needed) and attach to a session canvas directory.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        create_dir_all(&root.join("screens"))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn write_server_info(&self, info: &ServerInfo) -> Result<(), StoreError> {
        let path = self.root.join("server-info");
        let body =
            serde_json::to_vec_pretty(info).map_err(|e| StoreError::Serialize(e.to_string()))?;
        write_file(&path, &body)
    }

    /// Persist one screen as an HTML fragment plus sidecar metadata.
    ///
    /// Takes `&mut` because a reference that names a file is resolved here:
    /// the bytes are copied beside the screen and `reference.resolved` is set
    /// to the route that serves them, so neither the page nor a later rebind
    /// goes near the user's disk. A named file that cannot be read is an
    /// error, not a silent drop — a reference the user asked for must not
    /// quietly disappear from the screen.
    pub fn save_screen(&self, seq: u64, env: &mut ScreenEnvelope) -> Result<(), StoreError> {
        if let Some(reference) = env.reference.as_mut() {
            reference.resolved = if page_loads_src(&reference.src) {
                Some(reference.src.clone())
            } else {
                let bytes = read_file(Path::new(&reference.src))?;
                write_file(&self.reference_path(seq), &bytes)?;
                Some(format!("/screens/{seq}/reference"))
            };
        }

        write_file(&self.fragment_path(seq), env.body_html.as_bytes())?;

        let mut meta = env.clone();
        meta.body_html = String::new();
        let body = serde_json::to_vec(&meta).map_err(|e| StoreError::Serialize(e.to_string()))?;
        write_file(&self.meta_path(seq), &body)
    }

    /// The copied reference image for one screen.
    pub fn reference_path(&self, seq: u64) -> PathBuf {
        self.root.join("screens").join(format!("{seq:04}.ref.png"))
    }

    /// Read the copied reference image, if one was stored for this screen.
    pub fn read_reference(&self, seq: u64) -> Option<Vec<u8>> {
        read_file(&self.reference_path(seq)).ok()
    }

    /// Reassemble a saved screen (metadata + fragment).
    pub fn load_screen(&self, seq: u64) -> Result<ScreenEnvelope, StoreError> {
        let meta_raw = read_file(&self.meta_path(seq))?;
        let mut env: ScreenEnvelope =
            serde_json::from_slice(&meta_raw).map_err(|e| StoreError::CorruptScreen {
                seq,
                reason: format!("metadata: {e}"),
            })?;
        let fragment = read_file(&self.fragment_path(seq))?;
        env.body_html = String::from_utf8(fragment).map_err(|e| StoreError::CorruptScreen {
            seq,
            reason: format!("fragment is not UTF-8: {e}"),
        })?;
        Ok(env)
    }

    /// Every saved screen, ascending by seq. Rebinding (Ctrl+B recovery after
    /// a crash) rebuilds canvas state from this — the store is the canvas's
    /// memory across server tasks, so a reopen comes back on the screen the
    /// user was looking at instead of an empty "waiting for first screen".
    /// A corrupt sidecar is skipped, not fatal: one bad file must not take
    /// recovery down.
    pub fn load_all_screens(&self) -> Vec<(u64, ScreenEnvelope)> {
        let dir = self.root.join("screens");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(seq) = name
                .to_str()
                .and_then(|n| n.strip_suffix(".json"))
                .and_then(|stem| stem.parse::<u64>().ok())
            else {
                continue;
            };
            if let Ok(env) = self.load_screen(seq) {
                out.push((seq, env));
            }
        }
        out.sort_by_key(|(seq, _)| *seq);
        out
    }

    /// Append one click record to the `events` JSONL.
    pub fn append_event(&self, event: &CanvasEvent) -> Result<(), StoreError> {
        let path = self.root.join("events");
        let mut line =
            serde_json::to_vec(event).map_err(|e| StoreError::Serialize(e.to_string()))?;
        line.push(b'\n');
        append_file(&path, &line)
    }

    fn fragment_path(&self, seq: u64) -> PathBuf {
        self.root.join("screens").join(format!("{seq:04}.html"))
    }

    fn meta_path(&self, seq: u64) -> PathBuf {
        self.root.join("screens").join(format!("{seq:04}.json"))
    }
}

/// Wall-clock milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn create_dir_all(path: &Path) -> Result<(), StoreError> {
    std::fs::create_dir_all(path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn write_file(path: &Path, body: &[u8]) -> Result<(), StoreError> {
    std::fs::write(path, body).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn append_file(path: &Path, body: &[u8]) -> Result<(), StoreError> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|source| StoreError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    file.write_all(body).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn read_file(path: &Path) -> Result<Vec<u8>, StoreError> {
    std::fs::read(path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Whether the page can load this reference `src` on its own.
///
/// Everything else is a filesystem path the host reads. The canvas is served
/// from `127.0.0.1` and a browser may not reach at the user's disk, so a local
/// file has to be copied into the store and served from there instead.
fn page_loads_src(src: &str) -> bool {
    src.starts_with("data:") || src.starts_with("http://") || src.starts_with("https://")
}
