//! The thirteen pictures of a clip's filmstrip from its copy in the project: the clip is cut
//! in thirteen parts and each part's first frame is a place. A frame-wrapped MXF original with
//! an index gives the intra picture nearest each place as stored bytes, decoded by one ffmpeg
//! call (each picture an input of its own, since a camera's key pictures are not IDR); any
//! other medium gets the key picture at or before each place through one ffmpeg call with a
//! seek per place. Every picture becomes a JPEG of `THUMB` size, padded black; the caller
//! writes the files where the project keeps them.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub const COUNT: usize = 13;
/// The size of a thumbnail (a decoded picture is the thumbnail as it is).
pub const THUMB: [u32; 2] = [160, 90];
pub const PROGRAM: &str = "ffmpeg";
/// A decode of thirteen pictures that takes longer is given up.
const TIMEOUT: Duration = Duration::from_secs(120);

/// The frames of a clip of `frames` frames the filmstrip shows, in order.
pub fn places(frames: u64) -> Vec<u64> {
    (0..COUNT).map(|index| ((index as u128 * frames as u128) / COUNT as u128) as u64).collect()
}

/// The folder name of a clip's filmstrip from the clip's name: ASCII letters, digits, `-`, `_`
/// and `.`; anything else `_`; never empty, `.` or `..`.
pub fn folder_name(clip_name: &str) -> String {
    let mut out: String = clip_name.trim().chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' }).collect();
    out.truncate(120);
    if out.is_empty() || matches!(out.as_str(), "." | "..") {
        "clip".into()
    } else {
        out
    }
}

/// The file name of picture `index` shown at `seek_sec` of the clip: `000_0_00.jpg`.
pub fn file_name(index: usize, seek_sec: f64) -> String {
    format!("{index:03}_{}.jpg", format!("{:.2}", seek_sec.max(0.0)).replace('.', "_"))
}

/// The codec name ffmpeg demuxes a stored picture of the original by: `h264` or `mpeg2video`
/// decode on their own from the stored bytes, anything else does not.
pub fn stored_picture_codec(codec: &str) -> Option<&'static str> {
    ["h264", "mpeg2video"].into_iter().find(|known| *known == codec)
}

/// The stored intra pictures of `path` nearest each place, through the file's own index;
/// `None` when the file carries no usable index or a place has no intra picture.
pub fn intra_pictures(path: &Path, places: &[u64], cancel: &AtomicBool) -> Result<Option<Vec<Vec<u8>>>, String> {
    let Some(index) = qnc_mxf_index::MxfIndex::read(path)? else {
        return Ok(None);
    };
    let mut file = fs::File::open(path).map_err(|error| format!("filmstrip original open: {error}"))?;
    let mut pictures = Vec::with_capacity(places.len());
    for place in places {
        if cancel.load(Ordering::Relaxed) {
            return Err("filmstrip cancelled".into());
        }
        let Some(entry) = index.intra_nearest(*place) else {
            return Ok(None);
        };
        let Some(picture) = qnc_mxf_index::read_package(&mut file, &index, entry, true, false)?.picture else {
            return Ok(None);
        };
        pictures.push(picture);
    }
    Ok(Some(pictures))
}

/// The JPEG bytes of `pictures` (stored pictures of `codec`), one each in order, decoded by
/// one ffmpeg run; a picture equal to the one before it is decoded once and shared.
pub fn decode_stored(codec: &str, pictures: &[Vec<u8>], cancel: &AtomicBool) -> Result<Vec<Vec<u8>>, String> {
    if pictures.is_empty() {
        return Err("no pictures to decode".into());
    }
    let temp = scratch("stored")?;
    let result = (|| {
        let mut unique: Vec<&Vec<u8>> = Vec::new();
        let mut of = Vec::with_capacity(pictures.len());
        for picture in pictures {
            if unique.last() != Some(&picture) {
                unique.push(picture);
            }
            of.push(unique.len() - 1);
        }
        let mut command = qnc_launch::command(&qnc_launch::beside_application(PROGRAM)?, qnc_launch::Priority::Below);
        command.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"]);
        for (index, picture) in unique.iter().enumerate() {
            let input = temp.join(format!("{index:03}.{codec}"));
            fs::write(&input, picture).map_err(|error| format!("filmstrip picture write: {error}"))?;
            command.args(["-f", codec, "-threads", "1", "-i"]).arg(&input);
        }
        for index in 0..unique.len() {
            output(&mut command, index, &temp.join(format!("{index:03}.jpg")));
        }
        run(command, cancel)?;
        of.iter().map(|index| fs::read(temp.join(format!("{index:03}.jpg"))).map_err(|error| format!("filmstrip picture missing: {error}"))).collect()
    })();
    let _ = fs::remove_dir_all(&temp);
    result
}

/// The JPEG bytes of the key picture at or before each of `seek_secs` of the medium `path`,
/// in order, by one ffmpeg run with a seek per picture.
pub fn decode_seeked(path: &Path, seek_secs: &[f64], cancel: &AtomicBool) -> Result<Vec<Vec<u8>>, String> {
    if seek_secs.is_empty() {
        return Err("no pictures to decode".into());
    }
    let temp = scratch("seeked")?;
    let result = (|| {
        let mut command = qnc_launch::command(&qnc_launch::beside_application(PROGRAM)?, qnc_launch::Priority::Below);
        // Source timestamps are kept: a key picture just before the place is a picture, not a
        // negative time dropped (the last place of a short clip has none after it).
        command.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-noautorotate", "-copyts"]);
        for seek in seek_secs {
            command.args(["-ss", &format!("{:.6}", seek.max(0.0)), "-noaccurate_seek", "-skip_frame", "nokey", "-threads", "1", "-i"]).arg(path);
        }
        for index in 0..seek_secs.len() {
            output(&mut command, index, &temp.join(format!("{index:03}.jpg")));
        }
        run(command, cancel)?;
        (0..seek_secs.len()).map(|index| fs::read(temp.join(format!("{index:03}.jpg"))).map_err(|error| format!("filmstrip picture missing: {error}"))).collect()
    })();
    let _ = fs::remove_dir_all(&temp);
    result
}

fn output(command: &mut std::process::Command, input: usize, file: &Path) {
    let [width, height] = THUMB;
    let scale = format!("scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color=black");
    command.args(["-map", &format!("{input}:v:0"), "-an", "-sn", "-dn", "-frames:v", "1", "-vf", &scale, "-q:v", "2", "-pix_fmt", "yuvj420p", "-strict", "unofficial"]).arg(file);
}

fn scratch(kind: &str) -> Result<PathBuf, String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!("qnc-filmstrip-{kind}-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    fs::create_dir_all(&dir).map_err(|error| format!("filmstrip temp dir: {error}"))?;
    Ok(dir)
}

/// Runs ffmpeg to its end, or kills it when cancelled or past the timeout.
fn run(mut command: std::process::Command, cancel: &AtomicBool) -> Result<(), String> {
    command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
    let mut child = command.spawn().map_err(|error| format!("ffmpeg not started: {error}"))?;
    let stderr = child.stderr.take();
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            if status.success() {
                return Ok(());
            }
            let mut said = String::new();
            if let Some(mut err) = stderr {
                let _ = std::io::Read::read_to_string(&mut err, &mut said);
            }
            return Err(format!("ffmpeg ended with {status}: {}", said.trim()));
        }
        if cancel.load(Ordering::Relaxed) || started.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(if cancel.load(Ordering::Relaxed) { "filmstrip cancelled".into() } else { "ffmpeg took too long".into() });
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_places_are_the_first_frames_of_thirteen_equal_parts_and_the_names_are_plain() {
        assert_eq!(places(26), [0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24]);
        assert_eq!(places(5).len(), COUNT);
        assert_eq!(file_name(3, 1.5), "003_1_50.jpg");
        assert_eq!(folder_name("Mironik 3608"), "Mironik_3608");
        assert_eq!(folder_name(" "), "clip");
        assert_eq!(stored_picture_codec("h264"), Some("h264"));
        assert_eq!(stored_picture_codec("hevc"), None);
    }
}
