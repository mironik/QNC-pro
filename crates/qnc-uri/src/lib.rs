//! The QNC resource URI: `qnc://local/<kind>/<id...>` or `qnc://lan|intranet/<authority>/<kind>/<id...>`.
//! It is the public identity of a database, a source, a medium or a project; a raw OS path never
//! is. The rules of the old QNC are kept to the letter so records written there read here.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QncUri {
    pub environment: String,
    pub authority: Option<String>,
    pub resource_kind: String,
    pub resource_id: String,
}

/// Does the text look like a path of the operating system (a drive, a UNC path, a root, `~`).
pub fn looks_like_raw_os_path(value: &str) -> bool {
    if value.starts_with("qnc://") {
        return false;
    }
    let bytes = value.as_bytes();
    let windows_drive = bytes.len() >= 3 && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/') && bytes[0].is_ascii_alphabetic();
    windows_drive || value.starts_with("\\\\") || value.starts_with('/') || value.starts_with("~/")
}

pub fn parse(input: &str) -> Result<QncUri, String> {
    let input = input.trim();
    if looks_like_raw_os_path(input) {
        return Err(format!("raw OS path is not a public QNC URI: {input}"));
    }
    let Some(rest) = input.strip_prefix("qnc://") else {
        return Err(format!("QNC URI must start with qnc://: {input}"));
    };
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.iter().any(|part| part.is_empty()) {
        return Err(format!("QNC URI has an empty path segment: {input}"));
    }
    match parts.first().copied() {
        Some("local") => {
            if parts.len() < 3 {
                return Err(format!("local QNC URI requires kind and id: {input}"));
            }
            Ok(QncUri { environment: "local".into(), authority: None, resource_kind: parts[1].into(), resource_id: parts[2..].join("/") })
        }
        Some("lan") | Some("intranet") => {
            if parts.len() < 4 {
                return Err(format!("LAN/intranet QNC URI requires authority, kind and id: {input}"));
            }
            Ok(QncUri { environment: parts[0].into(), authority: Some(parts[1].into()), resource_kind: parts[2].into(), resource_id: parts[3..].join("/") })
        }
        Some(environment) => Err(format!("unsupported QNC environment '{environment}'")),
        None => Err(format!("empty QNC URI: {input}")),
    }
}

/// An id: 1..=256 bytes of letters, digits, `_` and `-`.
pub fn valid_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 256 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return Err(format!("invalid id: {id:?}"));
    }
    Ok(())
}

/// A resource URI as a record may hold it: parsed, trimmed, no `:` in kind or id, no `.`/`..`
/// segments, no control characters, `\`, `?` or `#`, at most 8192 bytes.
pub fn validate_resource_uri(uri: &str) -> Result<(), String> {
    let p = parse(uri)?;
    let safe = uri.len() <= 8192
        && uri == uri.trim()
        && !p.resource_kind.contains(':')
        && !p.resource_id.contains(':')
        && uri.split('/').all(|s| s != "." && s != ".." && !s.chars().any(|c| c.is_control() || matches!(c, '\\' | '?' | '#')));
    if safe {
        Ok(())
    } else {
        Err(format!("not an OS-neutral QNC resource URI: {uri}"))
    }
}

/// A segment of a path as a file URI carries it: every byte that is not a letter or digit
/// percent-encoded (the old QNC's rule, so `Mironik 1483.MXF` is `Mironik%201483%2EMXF`).
pub fn encode_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The URI of a file inside a source: `<source_uri>/file/<encoded path>`; the path is
/// relative, `/`-separated, without `.` or `..`.
pub fn file_uri(source_uri: &str, relative_path: &str) -> Result<String, String> {
    let source = parse(source_uri)?;
    if source.resource_kind != "source" || source.resource_id.contains('/') {
        return Err(format!("not a source URI: {source_uri}"));
    }
    let segments: Vec<&str> = relative_path.split('/').collect();
    if segments.iter().any(|s| matches!(*s, "" | "." | "..") || s.contains(['\\', ':']) || s.chars().any(char::is_control)) {
        return Err(format!("not a relative path inside the source: {relative_path}"));
    }
    let encoded: Vec<String> = segments.iter().map(|s| encode_segment(s)).collect();
    Ok(format!("{}/file/{}", source_uri.trim_end_matches('/'), encoded.join("/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uri_parses_by_environment_and_a_raw_path_or_a_bad_id_is_refused() {
        let local = parse("qnc://local/source/volume-7e80f1fb").unwrap();
        assert_eq!((local.environment.as_str(), local.resource_kind.as_str(), local.resource_id.as_str()), ("local", "source", "volume-7e80f1fb"));
        let lan = parse("qnc://lan/newsroom/db/project_workspace/p1").unwrap();
        assert_eq!((lan.authority.as_deref(), lan.resource_id.as_str()), (Some("newsroom"), "project_workspace/p1"));
        assert!(parse("C:\\cards").is_err());
        assert!(parse("qnc://local/source").is_err());
        assert!(parse("qnc://cloud/x/y").is_err());
        assert!(valid_id("clip-1df27557").is_ok());
        assert!(valid_id("a/b").is_err());
        assert!(validate_resource_uri("qnc://local/source/x/file/a%2Eb").is_ok());
        assert!(validate_resource_uri("qnc://local/source/x/../y").is_err());
        assert!(validate_resource_uri("qnc://local/source/x/a:b").is_err());
        assert_eq!(file_uri("qnc://local/source/volume-67870d0c", "PRIVATE/XDROOT/Sub/Mironik 1483S03.MP4").unwrap(), "qnc://local/source/volume-67870d0c/file/PRIVATE/XDROOT/Sub/Mironik%201483S03%2EMP4");
        assert!(file_uri("qnc://local/db/x", "a").is_err() && file_uri("qnc://local/source/x", "../a").is_err());
    }
}
