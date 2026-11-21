//! What the browser shows of a path: labels, crumbs, identity hashes (no filesystem access).

use std::path::{Path, PathBuf};

pub fn display_path_label(path: &Path) -> String {
    trim_path_suffix_for_display(&clean_os_path_display(&path.to_string_lossy()))
}

pub fn display_private_path(path: &Path) -> String {
    clean_os_path_display(&path.to_string_lossy())
}

pub fn display_entry_name(name: &str) -> String {
    trim_path_suffix_for_display(&clean_os_path_display(name))
}

fn clean_os_path_display(value: &str) -> String {
    let trimmed = value.trim();
    if let Some(rest) = trimmed.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{rest}")
    } else if let Some(rest) = trimmed.strip_prefix("\\\\?\\") {
        rest.to_string()
    } else if let Some(rest) = trimmed.strip_prefix("//?/UNC/") {
        format!("//{rest}")
    } else if let Some(rest) = trimmed.strip_prefix("//?/") {
        rest.to_string()
    } else {
        trimmed.to_string()
    }
}

fn trim_path_suffix_for_display(value: &str) -> String {
    if value == "/" || value == "\\" {
        return value.to_string();
    }
    if is_windows_drive_root(value) {
        return value[..2].to_string();
    }
    value.trim_end_matches(['\\', '/']).to_string()
}

fn is_windows_drive_root(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

pub(crate) fn browser_crumb_paths(path: &Path) -> Vec<(String, PathBuf)> {
    let clean = clean_os_path_display(&path.to_string_lossy());
    if clean.is_empty() {
        return Vec::new();
    }

    if is_windows_drive_rooted(&clean) {
        let drive = clean[..2].to_string();
        let mut out = vec![(drive.clone(), PathBuf::from(format!("{drive}\\")))];
        let rest = clean[3..].trim_matches(['\\', '/']);
        let mut current = format!("{drive}\\");
        for part in rest.split(['\\', '/']).filter(|part| !part.is_empty()) {
            if !current.ends_with('\\') {
                current.push('\\');
            }
            current.push_str(part);
            out.push((part.to_string(), PathBuf::from(current.clone())));
        }
        return out;
    }

    if clean.starts_with("\\\\") {
        let mut out = Vec::new();
        let mut current = String::from("\\\\");
        for part in clean
            .trim_start_matches('\\')
            .split('\\')
            .filter(|part| !part.is_empty())
        {
            if current != "\\\\" {
                current.push('\\');
            }
            current.push_str(part);
            out.push((part.to_string(), PathBuf::from(current.clone())));
        }
        return out;
    }

    if clean.starts_with('/') {
        let mut out = vec![("/".to_string(), PathBuf::from("/"))];
        let mut current = String::from("/");
        for part in clean
            .trim_start_matches('/')
            .split('/')
            .filter(|part| !part.is_empty())
        {
            if !current.ends_with('/') {
                current.push('/');
            }
            current.push_str(part);
            out.push((part.to_string(), PathBuf::from(current.clone())));
        }
        return out;
    }

    let mut out = Vec::new();
    let mut current = String::new();
    for part in clean.split(['\\', '/']).filter(|part| !part.is_empty()) {
        if !current.is_empty() {
            current.push(std::path::MAIN_SEPARATOR);
        }
        current.push_str(part);
        out.push((part.to_string(), PathBuf::from(current.clone())));
    }
    out
}

fn is_windows_drive_rooted(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

pub(crate) fn normalize_path_for_identity(path: &Path) -> String {
    clean_os_path_display(&path.to_string_lossy())
        .replace('\\', "/")
        .to_lowercase()
}

pub(crate) fn fnv1a64(value: &str) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut hash = OFFSET;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

