//! Names and ids. A fresh id is 32 characters of letters and digits that never repeat in this
//! process (time, process and a counter mixed); a slug is a name as the old QNC turned it into
//! the start of a project id; a folder name is an id made safe for a file system.

use std::sync::atomic::{AtomicU64, Ordering};

/// A fresh id: 32 of `0-9a-v`, never the same twice in this process.
pub fn fresh() -> String {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let count = COUNT.fetch_add(1, Ordering::Relaxed);
    let mut x = (nanos as u64) ^ (u64::from(std::process::id()) << 32) ^ count.rotate_left(17) ^ 0x9E37_79B9_7F4A_7C15;
    let mut out = String::with_capacity(32);
    for _ in 0..32 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.push(char::from(b"0123456789abcdefghijklmnopqrstuv"[(x & 31) as usize]));
    }
    out
}

/// The slug of a name: lower case, letters, digits, `-` and `_`, at most 40 characters,
/// `projekt` when nothing is left (the old QNC's rule).
pub fn slug(name: &str) -> String {
    let mut base: String = name.trim().to_lowercase().chars().map(|ch| if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' { ch } else { '_' }).collect();
    while base.contains("__") {
        base = base.replace("__", "_");
    }
    base = base.trim_matches('_').chars().take(40).collect();
    if base.is_empty() {
        base = "projekt".to_string();
    }
    base
}

/// A folder name of an id: letters, digits, `-` and `_`, at most 80 characters.
pub fn folder_name(id: &str) -> String {
    let mut output: String = id.trim().chars().map(|ch| if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' { ch } else { '_' }).collect();
    output.truncate(80);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_fresh_and_names_become_safe_slugs() {
        let a = fresh();
        let b = fresh();
        assert_ne!(a, b);
        assert!(a.len() == 32 && a.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_eq!(slug("Breaking news 2026-11-20 13:28"), "breaking_news_2026-11-20_13_28");
        assert_eq!(slug("   "), "projekt");
        assert_eq!(folder_name("a/b:c"), "a_b_c");
    }
}
