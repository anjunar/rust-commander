use std::path::Path;

use super::{describe_connect_error, format_fingerprint, known_hosts_candidates};

#[test]
fn returns_known_hosts_candidates_in_priority_order() {
    let candidates = known_hosts_candidates(Path::new("/tmp/home"));
    assert_eq!(candidates[0], Path::new("/tmp/home/.ssh/known_hosts"));
    assert_eq!(candidates[1], Path::new("/tmp/home/.ssh/known_hosts2"));
}

#[test]
fn formats_connect_timeout_for_user_facing_error() {
    let error = std::io::Error::new(std::io::ErrorKind::TimedOut, "timed out");
    assert_eq!(
        describe_connect_error("example.com:22", &error),
        "Could not reach remote host example.com:22: connection timed out"
    );
}

#[test]
fn formats_sha256_fingerprint_as_hex() {
    assert_eq!(format_fingerprint(&[0x01, 0xab, 0xff]), "01abff");
}
