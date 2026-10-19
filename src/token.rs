//! Per-session token guarding the answer-injection channel (M4).
//!
//! Rules the rest of the crate must keep:
//! - never written to disk (`screens/` / `events` / `server-info`),
//! - never placed in a URL (history / Referer / access log leak),
//! - injected into the served page body at request time only,
//! - compared in constant time on `POST /events`.

use std::fmt;

/// 32 CSPRNG bytes, carried as lowercase hex.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionToken {
    hex: String,
}

impl SessionToken {
    /// Draw a fresh token from the OS CSPRNG.
    pub fn generate() -> Self {
        let mut raw = [0u8; 32];
        getrandom::fill(&mut raw).expect("OS CSPRNG must be available");
        Self {
            hex: raw.iter().map(|b| format!("{b:02x}")).collect(),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.hex
    }

    /// Constant-time equality. Length is public (fixed 64), so the length
    /// shortcut does not leak anything an attacker cannot already assume.
    pub fn verify(&self, presented: &str) -> bool {
        let expected = self.hex.as_bytes();
        let got = presented.as_bytes();
        if expected.len() != got.len() {
            return false;
        }
        let mut diff = 0u8;
        for (a, b) in expected.iter().zip(got.iter()) {
            diff |= a ^ b;
        }
        diff == 0
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionToken(****)")
    }
}
