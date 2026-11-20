//! One run of ffprobe on one local file: the stream and format summary as JSON, and, only
//! when the summary does not know the colour or scan of a picture stream, the first decoded
//! pictures of the file in the same probe (XDCAM MXF keeps the colour in the MPEG-2 headers).
//! The report's `format.filename` becomes the media URI, so the report binds to the medium,
//! never to a path. The program is the ffprobe beside the application; the run is bounded in
//! time and in output. Who probes what, and that a medium is probed once, is the caller's.

use std::io::Read;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use qnc_launch::Priority;

pub const PROGRAM: &str = "ffprobe";
/// The demuxers a probe may use: single files of the cameras QNC reads.
pub const DEMUXERS: &str = "mxf,mov";
pub const PROBE_SIZE_BYTES: u64 = 2 * 1024 * 1024;
pub const ANALYZE_DURATION_US: u64 = 1_000_000;
/// The packets read from the start for the colour and scan of the first pictures (an MP4
/// interleaves its sound in long runs: 32 gave no picture, 128 leaves room).
pub const FIRST_PACKETS: &str = "%+#128";
pub const MAX_STDERR_BYTES: usize = 65_536;

/// The report of a probe: the JSON text, bound to the medium.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub media_uri: String,
    pub json: String,
}

fn summary_arguments() -> Vec<&'static str> {
    vec!["-show_format", "-show_streams", "-show_programs", "-show_chapters", "-show_data", "-show_program_version", "-show_library_versions", "-show_error", "-show_optional_fields", "always"]
}

fn frame_arguments() -> Vec<&'static str> {
    vec!["-show_frames", "-read_intervals", FIRST_PACKETS, "-show_entries", "frame=stream_index,media_type,color_primaries,color_transfer,color_space,color_range,interlaced_frame,top_field_first"]
}

/// ffprobe's report of `file`, bound to `media_uri`, within `timeout`.
pub fn probe(file: &Path, media_uri: &str, timeout: Duration) -> Result<Report, String> {
    let program = qnc_launch::beside_application(PROGRAM)?;
    let mut json = run(&program, &summary_arguments(), file, timeout)?;
    if qnc_ffprobe_read::summary_lacks_picture_facts(&json) {
        let frames = run(&program, &frame_arguments(), file, timeout)?;
        if let (Some(object), Some(frames)) = (json.as_object_mut(), frames.get("frames")) {
            object.insert("frames".into(), frames.clone());
        }
    }
    let format = json.get_mut("format").and_then(|v| v.as_object_mut()).ok_or_else(|| qnc_text::text("probe.invalid_output").to_string())?;
    format.insert("filename".into(), media_uri.into());
    Ok(Report { media_uri: media_uri.into(), json: serde_json::to_string(&json).map_err(|_| qnc_text::text("probe.invalid_output").to_string())? })
}

fn run(program: &Path, what: &[&str], file: &Path, timeout: Duration) -> Result<serde_json::Value, String> {
    let probe_size = PROBE_SIZE_BYTES.to_string();
    let analyze = ANALYZE_DURATION_US.to_string();
    let mut command = qnc_launch::command(program, Priority::Below);
    command
        .args(["-v", "error", "-probesize", &probe_size, "-analyzeduration", &analyze, "-threads", "1", "-protocol_whitelist", "file", "-format_whitelist", DEMUXERS])
        .args(what)
        .args(["-of", "json", "-i"])
        .arg(file)
        .env_remove("FFREPORT")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command.spawn().map_err(|e| qnc_text::fill("probe.not_started", &[("error", &e.to_string())]))?;
    let stdout = child.stdout.take().ok_or_else(|| qnc_text::text("probe.invalid_output").to_string())?;
    let stderr = child.stderr.take().ok_or_else(|| qnc_text::text("probe.invalid_output").to_string())?;
    let out = capture(stdout, qnc_ffprobe_read::MAX_DOCUMENT_BYTES);
    let err = capture(stderr, MAX_STDERR_BYTES);
    // ffprobe closes its output when it ends: the report arriving is the end, or the limit.
    let bytes = match out.recv_timeout(timeout) {
        Ok(bytes) => bytes,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(qnc_text::fill("probe.timed_out", &[("seconds", &timeout.as_secs().to_string())]));
        }
    };
    let status = child.wait().map_err(|e| qnc_text::fill("probe.failed", &[("error", &e.to_string())]))?;
    let error_text = err.recv_timeout(Duration::from_secs(5)).ok().flatten().map(|b: Vec<u8>| String::from_utf8_lossy(&b).trim().to_string()).unwrap_or_default();
    let Some(bytes) = bytes else { return Err(qnc_text::text("probe.output_limit").to_string()) };
    if !status.success() {
        return Err(qnc_text::fill("probe.failed", &[("error", &error_text)]));
    }
    let json: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| qnc_text::text("probe.invalid_output").to_string())?;
    if json.get("error").is_some() {
        return Err(qnc_text::fill("probe.failed", &[("error", &json["error"].to_string())]));
    }
    Ok(json)
}

/// Reads a pipe to its end in the background: the bytes, or `None` past `limit`.
fn capture(mut pipe: impl Read + Send + 'static, limit: usize) -> mpsc::Receiver<Option<Vec<u8>>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 8192];
        let over = loop {
            match pipe.read(&mut buffer) {
                Ok(0) | Err(_) => break false,
                Ok(n) => {
                    if bytes.len() + n > limit {
                        break true;
                    }
                    bytes.extend_from_slice(&buffer[..n]);
                }
            }
        };
        let _ = tx.send(if over { None } else { Some(bytes) });
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The original of Mironik 1483 as it lies in a project the tray made (a copy of the
    /// card; read only). Without the file or the ffprobe beside the build the test says so
    /// and passes.
    const FILE: &str = "C:/Users/Public/QNC-IngestBase/ingest_sony_ilme-fx6v_2026-11-20_13_28_0_2945922201e04bc8b69d791611a4b98a/original/PRIVATE/XDROOT/Clip/Mironik 1483.MXF";
    const MEDIA: &str = "qnc://local/source/project-ingest_sony_ilme-fx6v_2026-11-20_13_28_0_2945922201e04bc8b69d791611a4b98a/file/original/PRIVATE/XDROOT/Clip/Mironik%201483%2EMXF";

    #[test]
    fn the_probe_of_a_real_mxf_reads_as_the_saved_description_and_a_missing_file_is_an_error() {
        if !Path::new(FILE).is_file() || qnc_launch::beside_application(PROGRAM).is_err() {
            eprintln!("skipped: no project copy or no ffprobe beside the build");
            return;
        }
        let report = probe(Path::new(FILE), MEDIA, Duration::from_secs(30)).unwrap();
        let parsed = qnc_ffprobe_read::read(&report.json, MEDIA, "qnc://local/artifact/probe-t", "ffprobe-0").unwrap();
        assert_eq!(qnc_media_model::single_video(&parsed.media).unwrap().exact_frame_count(), Some(362));
        let missing = probe(Path::new("C:/nowhere/x.mxf"), MEDIA, Duration::from_secs(30)).unwrap_err();
        assert!(missing.contains("ffprobe"), "{missing}");
    }
}
