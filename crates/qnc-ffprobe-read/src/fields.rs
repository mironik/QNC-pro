//! The fields of the report: strings, numbers, ratios, colour, scan, tags.

use std::collections::BTreeMap;

use qnc_media_model::*;
use serde_json::Value;

use crate::{Error, Result};

pub(crate) struct Reader<'a> {
    pub id: &'a str,
}

impl Reader<'_> {
    pub fn fact<T>(&self, path: &str, value: T) -> Fact<T> {
        Fact { value, evidence_id: self.id.into(), locator: path.into() }
    }

    pub fn raw(&self, object: &Value, key: &str, path: &str) -> Result<Option<String>> {
        match object.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(v)) => Ok(Some(v.clone())),
            Some(Value::Number(v)) => Ok(Some(v.to_string())),
            _ => Err(Error::InvalidField(format!("{path}/{key}"))),
        }
    }

    pub fn string(&self, object: &Value, key: &str, path: &str) -> Result<Option<Fact<String>>> {
        Ok(self.raw(object, key, path)?.filter(|s| !missing(s)).map(|v| self.fact(&format!("{path}/{key}"), v)))
    }

    pub fn signal(&self, object: &Value, key: &str, path: &str) -> Result<Option<Fact<Signal<String>>>> {
        Ok(self.raw(object, key, path)?.map(|v| self.fact(&format!("{path}/{key}"), if missing(&v) { Signal::Unspecified } else { Signal::Known(v) })))
    }

    pub fn integer<T: std::str::FromStr>(&self, object: &Value, key: &str, path: &str) -> Result<Option<Fact<T>>> {
        self.string(object, key, path)?
            .map(|f| {
                let value = f.value.parse().map_err(|_| Error::InvalidField(f.locator.clone()))?;
                Ok(self.fact(&f.locator, value))
            })
            .transpose()
    }

    pub fn ratio(&self, object: &Value, key: &str, path: &str) -> Result<Option<Fact<Rational>>> {
        self.string(object, key, path)?
            .map(|f| {
                let value = rational(&f.value).ok_or_else(|| Error::InvalidField(f.locator.clone()))?;
                Ok(self.fact(&f.locator, value))
            })
            .transpose()
            .map(|r| r.filter(|f| f.value.numerator > 0 && f.value.denominator > 0))
    }

    /// A colour of a picture stream: the summary's, or the first decoded picture that knows it.
    fn color(&self, stream: &Value, key: &str, path: &str, index: u32, frames: &[Value]) -> Result<Option<Fact<Signal<String>>>> {
        let summary = self.signal(stream, key, path)?;
        if matches!(summary, Some(Fact { value: Signal::Known(_), .. })) {
            return Ok(summary);
        }
        for (i, frame) in frames.iter().enumerate() {
            if frame.get("stream_index").and_then(Value::as_u64) != Some(u64::from(index)) {
                continue;
            }
            let known = self.signal(frame, key, &format!("/frames/{i}"))?;
            if matches!(known, Some(Fact { value: Signal::Known(_), .. })) {
                return Ok(known);
            }
        }
        Ok(summary)
    }

    /// The scan from the first decoded pictures; `None` when none says it or they disagree.
    fn frame_scan(&self, index: u32, frames: &[Value]) -> Result<Option<Fact<ScanMode>>> {
        let mut found: Option<Fact<ScanMode>> = None;
        for (i, frame) in frames.iter().enumerate() {
            if frame.get("stream_index").and_then(Value::as_u64) != Some(u64::from(index)) {
                continue;
            }
            let path = format!("/frames/{i}");
            let Some(interlaced) = self.integer::<u8>(frame, "interlaced_frame", &path)? else { continue };
            let mode = match (interlaced.value, self.integer::<u8>(frame, "top_field_first", &path)?.map(|f| f.value)) {
                (0, _) => self.fact(&interlaced.locator, ScanMode::Progressive),
                (1, Some(1)) => self.fact(&format!("{path}/top_field_first"), ScanMode::InterlacedTopFieldFirst),
                (1, Some(0)) => self.fact(&format!("{path}/top_field_first"), ScanMode::InterlacedBottomFieldFirst),
                _ => return Err(Error::InvalidField(interlaced.locator)),
            };
            match &found {
                Some(first) if first.value != mode.value => return Ok(None),
                Some(_) => {}
                None => found = Some(mode),
            }
        }
        Ok(found)
    }

    pub fn video(&self, stream: &Value, path: &str, index: u32, frames: &[Value]) -> Result<VideoMetadata> {
        let rate = self.ratio(stream, "avg_frame_rate", path)?.or(self.ratio(stream, "r_frame_rate", path)?);
        let frame_rate = rate.as_ref().map(|r| self.fact(&r.locator, FrameTimebase { fps_num: r.value.numerator, fps_den: r.value.denominator }));
        let mut frame_count = self.integer::<u64>(stream, "nb_frames", path)?.filter(|f| f.value > 0).map(|f| self.fact(&f.locator, FrameCount::Exact(f.value)));
        if frame_count.is_none() {
            // A stream whose time base is one frame at a constant rate (MXF edit units)
            // counts its frames in `duration_ts`: exact, not estimated from seconds.
            let constant = self.ratio(stream, "avg_frame_rate", path)?.zip(self.ratio(stream, "r_frame_rate", path)?).is_some_and(|(avg, r)| avg.value == r.value);
            if let (true, Some(tb), Some(rate), Some(ticks)) = (constant, self.ratio(stream, "time_base", path)?, rate.as_ref(), self.integer::<u64>(stream, "duration_ts", path)?) {
                if ticks.value > 0 && i128::from(tb.value.numerator) * i128::from(rate.value.numerator) == i128::from(tb.value.denominator) * i128::from(rate.value.denominator) {
                    frame_count = Some(self.fact(&ticks.locator, FrameCount::Exact(ticks.value)));
                }
            }
        }
        if frame_count.is_none() {
            if let (Some(duration), Some(rate)) = (self.ratio(stream, "duration", path)?, rate.as_ref()) {
                let num = i128::from(duration.value.numerator) * i128::from(rate.value.numerator);
                let den = i128::from(duration.value.denominator) * i128::from(rate.value.denominator);
                if let Ok(n @ 1..=i64::MAX) = i64::try_from((num + den / 2) / den) {
                    frame_count = Some(self.fact(&format!("{path}/duration*{}", rate.locator), FrameCount::Estimated(n as u64)));
                }
            }
        }
        let scan_mode = self.string(stream, "field_order", path)?.and_then(|f| {
            let mode = match f.value.as_str() {
                "progressive" => ScanMode::Progressive,
                "tt" | "bt" => ScanMode::InterlacedTopFieldFirst,
                "bb" | "tb" => ScanMode::InterlacedBottomFieldFirst,
                _ => return None,
            };
            Some(self.fact(&f.locator, mode))
        });
        let scan_mode = match scan_mode {
            Some(mode) => Some(mode),
            None => self.frame_scan(index, frames)?,
        };
        let mut rotation = None;
        let mut declared = false;
        if let Some(sides) = stream.get("side_data_list") {
            let sides = sides.as_array().ok_or_else(|| Error::InvalidField(format!("{path}/side_data_list")))?;
            for (i, side) in sides.iter().enumerate() {
                if !side.is_object() {
                    return Err(Error::InvalidField(format!("{path}/side_data_list/{i}")));
                }
                declared |= side.get("rotation").is_some() || side.get("displaymatrix").is_some() || side.get("side_data_type").and_then(Value::as_str) == Some("Display Matrix");
                if let Some(value) = self.integer::<i32>(side, "rotation", &format!("{path}/side_data_list/{i}"))? {
                    if rotation.as_ref().is_some_and(|old: &Fact<i32>| old.value != value.value) {
                        return Err(Error::InvalidField(format!("{path}/rotation")));
                    }
                    rotation = Some(value);
                }
            }
        }
        declared |= stream.get("tags").is_some_and(|tags| tags.get("rotate").is_some());
        let rotation = match rotation {
            Some(value) => Some(self.fact(&value.locator, Signal::Known(value.value))),
            None if !declared => Some(self.fact(path, Signal::Unspecified)),
            None => None,
        };
        Ok(VideoMetadata {
            width: self.integer(stream, "width", path)?,
            height: self.integer(stream, "height", path)?,
            frame_rate,
            frame_rate_mode: Some(self.fact(path, FrameRateMode::Unknown)),
            frame_count,
            scan_mode,
            pixel_format: self.string(stream, "pix_fmt", path)?,
            sample_aspect_ratio: self.ratio(stream, "sample_aspect_ratio", path)?,
            rotation_degrees: rotation,
            color: ColorMetadata {
                primaries: self.color(stream, "color_primaries", path, index, frames)?,
                transfer: self.color(stream, "color_transfer", path, index, frames)?,
                matrix: self.color(stream, "color_space", path, index, frames)?,
                range: self.color(stream, "color_range", path, index, frames)?,
            },
        })
    }

    pub fn tags(&self, object: &Value, path: &str, out: &mut BTreeMap<String, Fact<String>>) -> Result<()> {
        if let Some(tags) = object.get("tags") {
            let tags = tags.as_object().ok_or(Error::InvalidDocument)?;
            for (key, value) in tags {
                let pointer = format!("{path}/tags/{}", key.replace('~', "~0").replace('/', "~1"));
                let value = value.as_str().ok_or_else(|| Error::InvalidField(pointer.clone()))?;
                out.insert(format!("ffprobe:{pointer}"), self.fact(&pointer, value.to_string()));
                if path == "/format" && matches!(key.as_str(), "creation_time" | "timecode") {
                    out.insert(key.clone(), self.fact(&pointer, value.to_string()));
                }
            }
        }
        if out.len() > 512 {
            return Err(Error::TooLarge);
        }
        Ok(())
    }
}

fn missing(s: &str) -> bool {
    matches!(s.trim().to_ascii_lowercase().as_str(), "" | "n/a" | "unknown" | "unspecified" | "none")
}

fn rational(s: &str) -> Option<Rational> {
    let (n, d) = if let Some((n, d)) = s.split_once('/').or_else(|| s.split_once(':')) {
        (n.parse::<i64>().ok()?, d.parse::<i64>().ok()?)
    } else if let Some((n, fraction)) = s.split_once('.') {
        if fraction.len() > 9 || fraction.is_empty() || !fraction.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let d = 10i64.checked_pow(fraction.len() as u32)?;
        let whole = n.parse::<i64>().ok()?;
        let f = fraction.parse::<i64>().ok()?;
        (whole.checked_mul(d)?.checked_add(if s.starts_with('-') { -f } else { f })?, d)
    } else {
        (s.parse::<i64>().ok()?, 1)
    };
    if n == 0 || d == 0 {
        return Some(Rational { numerator: n, denominator: d });
    }
    if n < 0 || d < 0 {
        return None;
    }
    let (mut a, mut b) = (n, d);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    Some(Rational { numerator: n / a, denominator: d / a })
}
