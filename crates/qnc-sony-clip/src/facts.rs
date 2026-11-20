//! The facts themselves: index attributes and sidecar elements to the saved representation.

use qnc_media_model::*;
use qnc_sony_index::IndexedMedia;

use crate::{fact, notice, Notice};

pub(crate) fn representation(camera: &IndexedMedia, media_uri: &str, id: &str, prefix: &str, notices: &mut Vec<Notice>) -> MediaRepresentation {
    let mut media = MediaRepresentation { media_uri: media_uri.into(), container: None, duration_seconds: None, streams_complete: None, streams: Vec::new(), tags: crate::empty_tags() };
    for (key, value) in &camera.attributes {
        let locator = format!("{}/@{key}", camera.locator);
        media.tags.insert(format!("sony.index:{locator}"), fact(id, &locator, value.clone()));
    }
    if let Some(kind) = camera.attributes.get("type") {
        match kind.as_str() {
            "MXF" | "MP4" => media.container = Some(fact(id, &format!("{}/@type", camera.locator), kind.to_ascii_lowercase())),
            _ => notice(notices, "unsupported", &format!("{prefix}.container"), "unmapped camera container label retained as raw fact"),
        }
    }
    if let Some(codec) = camera.attributes.get("videoType") {
        ensure_video(&mut media);
        let locator = format!("{}/@videoType", camera.locator);
        media.streams[0].codec = codec_fact(codec, id, &locator, &format!("{prefix}.codec"), notices);
        // Sony video type labels state the frame size: AVC_Proxy_1920_1080_HP@L42.
        if let Some((width, height)) = label_dimensions(codec) {
            let video = ensure_video(&mut media);
            video.width = Some(fact(id, &locator, width));
            video.height = Some(fact(id, &locator, height));
        }
    }
    if let Some(aspect) = camera.attributes.get("aspectRatio") {
        set_pixel_aspect(ensure_video(&mut media), aspect, id, &format!("{}/@aspectRatio", camera.locator));
    }
    if let Some(fps) = camera.attributes.get("fps") {
        let video = ensure_video(&mut media);
        let locator = format!("{}/@fps", camera.locator);
        video.frame_rate = fps_fact(fps, id, &locator, &format!("{prefix}.frame_rate"), notices);
        if video.frame_rate.is_some() {
            video.scan_mode = Some(fact(id, &locator, ScanMode::Progressive));
        }
    }
    media
}

fn codec_fact(value: &str, id: &str, locator: &str, field: &str, notices: &mut Vec<Notice>) -> Option<Fact<Signal<String>>> {
    if value.starts_with("AVC50_") || value.starts_with("AVC_Proxy_") {
        Some(fact(id, locator, Signal::Known("h264".into())))
    } else {
        notice(notices, "unsupported", field, "unmapped video codec label retained as raw fact");
        None
    }
}

fn progressive_rate(value: &str) -> Option<FrameTimebase> {
    let value = value.strip_suffix('p')?;
    let (integer, fraction) = value.split_once('.').unwrap_or((value, "0"));
    if integer.is_empty() || !integer.bytes().all(|c| c.is_ascii_digit()) || fraction.is_empty() || !fraction.bytes().all(|c| c == b'0') {
        return None;
    }
    FrameTimebase::new(integer.parse().ok()?, 1).ok()
}

fn fps_fact(value: &str, id: &str, locator: &str, field: &str, notices: &mut Vec<Notice>) -> Option<Fact<FrameTimebase>> {
    match progressive_rate(value) {
        Some(rate) => Some(fact(id, locator, rate)),
        None => {
            notice(notices, "unsupported", field, "FPS semantics outside verified integer-progressive mapping; raw value retained");
            None
        }
    }
}

pub(crate) fn positive(value: &str, field: &str, notices: &mut Vec<Notice>) -> Option<i64> {
    match value.parse::<i64>() {
        Ok(value) if value > 0 => Some(value),
        _ => {
            notice(notices, "invalid", field, "expected positive integer within signed 64-bit range");
            None
        }
    }
}

fn merge<T: PartialEq>(target: &mut Option<Fact<T>>, incoming: Option<Fact<T>>, field: &str, notices: &mut Vec<Notice>) {
    match (&target, &incoming) {
        (Some(a), Some(b)) if a.value != b.value => {
            *target = None;
            notice(notices, "conflict", field, "camera index and sidecar disagree; neither value is silently preferred");
        }
        _ => *target = incoming,
    }
}

/// The sidecar's facts onto the original; `true` when it proves a normal progressive
/// recording whose exact frame count index and sidecar agree on.
pub(crate) fn apply_sidecar(root: &qnc_xml::Node, camera: &IndexedMedia, media: &mut MediaRepresentation, notices: &mut Vec<Notice>) -> Result<bool, String> {
    let id = "sidecar-original";
    let mut attributes = std::collections::BTreeMap::new();
    root.all_attributes(&mut attributes);
    for (locator, value) in attributes {
        media.tags.insert(format!("sony.sidecar:{locator}"), fact(id, &locator, value));
    }
    if let Some((node, value)) = root.one("CreationDate")?.and_then(|n| n.attributes.get("value").map(|v| (n, v))) {
        media.tags.insert("creation_time".into(), fact(id, &format!("{}/@value", node.locator), value.clone()));
    }
    let frame = root.at(&["VideoFormat", "VideoFrame"])?;
    if let Some(frame) = frame {
        if let Some(codec) = frame.attributes.get("videoCodec") {
            ensure_video(media);
            let incoming = codec_fact(codec, id, &format!("{}/@videoCodec", frame.locator), "original.codec", notices);
            merge(&mut media.streams[0].codec, incoming, "original.codec", notices);
            if camera.attributes.get("videoType").is_some_and(|c| c != codec) {
                media.streams[0].codec = None;
                notice(notices, "conflict", "original.codec", "raw camera video codec declarations disagree");
            }
        }
        if let Some(fps) = frame.attributes.get("formatFps") {
            let locator = format!("{}/@formatFps", frame.locator);
            let incoming = fps_fact(fps, id, &locator, "original.frame_rate", notices);
            let video = ensure_video(media);
            merge(&mut video.frame_rate, incoming, "original.frame_rate", notices);
            video.scan_mode = video.frame_rate.as_ref().map(|_| fact(id, &locator, ScanMode::Progressive));
        }
    }
    if let Some(layout) = root.at(&["VideoFormat", "VideoLayout"])? {
        let video = ensure_video(media);
        for (attribute, target) in [("pixel", &mut video.width), ("numOfVerticalLine", &mut video.height)] {
            if let Some(value) = layout.attributes.get(attribute) {
                let value = positive(value, attribute, notices).and_then(|v| u32::try_from(v).ok());
                *target = value.map(|value| fact(id, &format!("{}/@{attribute}", layout.locator), value));
            }
        }
        if let Some(aspect) = layout.attributes.get("aspectRatio") {
            set_pixel_aspect(video, aspect, id, &format!("{}/@aspectRatio", layout.locator));
        }
    }
    let normal = root.one("RecordingMode")?.and_then(|n| n.attributes.get("type")).is_some_and(|t| t == "normal");
    let format_rate = frame.and_then(|n| n.attributes.get("formatFps")).and_then(|s| progressive_rate(s));
    let capture_rate = frame.and_then(|n| n.attributes.get("captureFps")).and_then(|s| progressive_rate(s));
    let index_rate = camera.attributes.get("fps").and_then(|s| progressive_rate(s));
    let supported = normal && format_rate.is_some() && format_rate == capture_rate && format_rate == index_rate && camera.attributes.get("offset").is_some_and(|o| o == "0");
    if !supported {
        return Ok(false);
    }
    let Some(duration) = root.one("Duration")? else { return Ok(false) };
    let (Some(side_duration), Some(index_duration)) = (duration.attributes.get("value"), camera.attributes.get("dur")) else { return Ok(false) };
    let a = positive(side_duration, "original.frame_count", notices);
    let b = positive(index_duration, "original.frame_count", notices);
    let Some((a, b)) = a.zip(b) else { return Ok(false) };
    if a != b {
        notice(notices, "conflict", "original.frame_count", "Duration and index dur disagree");
        return Ok(false);
    }
    promote_video_count(media, id, &format!("{}/@value", duration.locator), a);
    Ok(true)
}

/// The exact video count at a constant rate (XML dur/Duration count video edit units).
pub(crate) fn promote_video_count(media: &mut MediaRepresentation, id: &str, locator: &str, count: i64) {
    let video = ensure_video(media);
    video.frame_count = Some(fact(id, locator, FrameCount::Exact(count as u64)));
    video.frame_rate_mode = Some(fact(id, locator, FrameRateMode::Constant));
}

/// Width and height stated by a Sony video type label (`<codec>_<width>_<height>_...`).
fn label_dimensions(label: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = label.split('_').collect();
    parts.windows(2).find_map(|pair| {
        let width: u32 = pair[0].parse().ok()?;
        let height: u32 = pair[1].parse().ok()?;
        (width >= 16 && height >= 16).then_some((width, height))
    })
}

/// Pixel aspect of a display aspect `W:H` over the stated frame size, reduced.
fn set_pixel_aspect(video: &mut VideoMetadata, aspect: &str, id: &str, locator: &str) {
    let (Some(width), Some(height)) = (&video.width, &video.height) else { return };
    let Some((aw, ah)) = aspect.split_once(':').and_then(|(a, b)| Some((a.trim().parse::<i64>().ok()?, b.trim().parse::<i64>().ok()?))) else { return };
    if aw <= 0 || ah <= 0 {
        return;
    }
    let (numerator, denominator) = (aw * i64::from(height.value), ah * i64::from(width.value));
    let divisor = gcd(numerator, denominator);
    video.sample_aspect_ratio = Some(fact(id, locator, Rational { numerator: numerator / divisor, denominator: denominator / divisor }));
}

/// Length in seconds of an exact frame count at a constant rate, reduced.
pub(crate) fn promote_duration(media: &mut MediaRepresentation, field: &str, notices: &mut Vec<Notice>) {
    let Some(video) = video_ref(media) else { return };
    let (Some(rate), Some(frames)) = (&video.frame_rate, video.exact_frame_count()) else { return };
    let (Some(count), Some((evidence_id, locator))) = (i64::try_from(frames).ok(), video.frame_count.as_ref().map(|f| (f.evidence_id.clone(), f.locator.clone()))) else {
        notice(notices, "invalid", field, "frame count out of range");
        return;
    };
    let (numerator, denominator) = (count * rate.value.fps_den, rate.value.fps_num);
    if denominator <= 0 {
        return;
    }
    let divisor = gcd(numerator, denominator);
    media.duration_seconds = Some(fact(&evidence_id, &locator, Rational { numerator: numerator / divisor, denominator: denominator / divisor }));
}

fn gcd(mut a: i64, mut b: i64) -> i64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.abs().max(1)
}

pub(crate) fn video_ref(media: &MediaRepresentation) -> Option<&VideoMetadata> {
    media.streams.iter().find_map(|s| match &s.details {
        StreamDetails::Video(v) => Some(v.as_ref()),
        _ => None,
    })
}

fn ensure_video(media: &mut MediaRepresentation) -> &mut VideoMetadata {
    if media.streams.is_empty() {
        media.streams.push(MediaStream {
            index: None,
            codec: None,
            profile: None,
            time_base: None,
            start_pts: None,
            duration_ts: None,
            details: StreamDetails::Video(Box::new(VideoMetadata {
                width: None,
                height: None,
                frame_rate: None,
                frame_rate_mode: None,
                frame_count: None,
                scan_mode: None,
                pixel_format: None,
                sample_aspect_ratio: None,
                rotation_degrees: None,
                color: ColorMetadata { primaries: None, transfer: None, matrix: None, range: None },
            })),
        });
    }
    match &mut media.streams[0].details {
        StreamDetails::Video(v) => v,
        _ => unreachable!("only partial video descriptions are constructed here"),
    }
}
