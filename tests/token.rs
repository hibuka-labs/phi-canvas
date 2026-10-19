//! Session token behavior: generation, constant-time verify, no accidental leak.

use phi_canvas::SessionToken;

#[test]
fn generate_is_64_hex_chars() {
    let token = SessionToken::generate();
    let s = token.as_str();
    assert_eq!(s.len(), 64, "32 random bytes as lowercase hex");
    assert!(
        s.bytes().all(|b| b.is_ascii_hexdigit()),
        "hex only, got {s}"
    );
}

#[test]
fn generate_is_random_per_call() {
    let a = SessionToken::generate();
    let b = SessionToken::generate();
    assert_ne!(a.as_str(), b.as_str());
}

#[test]
fn verify_accepts_own_value() {
    let token = SessionToken::generate();
    assert!(token.verify(token.as_str()));
}

#[test]
fn verify_rejects_wrong_values() {
    let token = SessionToken::generate();
    let own = token.as_str().to_string();

    assert!(!token.verify(""), "empty");
    assert!(!token.verify(&own[..32]), "truncated");
    assert!(!token.verify(&format!("{own}x")), "extended");
    assert!(
        !token.verify(SessionToken::generate().as_str()),
        "other token"
    );

    let mut flipped = own.clone().into_bytes();
    flipped[0] = if flipped[0] == b'0' { b'1' } else { b'0' };
    let flipped = String::from_utf8(flipped).unwrap();
    assert!(!token.verify(&flipped), "single-char tamper");
}

#[test]
fn debug_output_does_not_leak_the_token() {
    let token = SessionToken::generate();
    let dbg = format!("{token:?}");
    assert!(
        !dbg.contains(token.as_str()),
        "Debug must redact, got {dbg}"
    );
}
