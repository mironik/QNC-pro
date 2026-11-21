//! One range of a clip's sound through the engine: only the sound over the clip's floor (6 dB
//! over its noise, never higher than -42 dBFS), in pieces of 30 s cut at quiet places, each
//! piece heard from two seconds earlier so the words a cut took from the piece before are
//! heard again, and the words already written are dropped. Pure over the samples given.

use std::sync::atomic::AtomicBool;

use qnc_transcript_store::{Line, Word};

/// Seconds of one piece.
pub const PIECE_SECONDS: usize = 30;
/// Seconds of sound before a piece that it is given again.
pub const OVERLAP_SECONDS: usize = 2;
/// Frames after the last written word within which the same word is that word heard again.
const ECHO_FRAMES: u64 = 25;

/// Hears `samples` (16 kHz, starting at clip frame `from`, `fps` frames a second) piece by
/// piece through `hear`, which gives the lines of one piece as clip frames; the lines of the
/// range, the language of the first piece that named one, and the engine of the last.
pub(crate) fn hear_range(samples: &[f32], from: u64, to: u64, fps: f64, cancel: &AtomicBool, hear: &mut dyn FnMut(&[f32], f64) -> Result<(Vec<Line>, String, String), String>) -> Result<(Vec<Line>, String, String), String> {
    let rate = qnc_transcribe_jobs::WHISPER_RATE as usize;
    let floor = qnc_transcribe_jobs::noise_dbfs(samples, rate).map_or(qnc_transcribe_jobs::FLOOR_CEILING_DBFS, qnc_transcribe_jobs::clip_floor_dbfs);
    let (mut lines, mut language, mut made_by) = (Vec::new(), String::new(), String::new());
    for span in qnc_transcribe_jobs::loud_spans(samples, rate, floor) {
        for piece in qnc_transcribe_jobs::quiet_pieces(&samples[span.clone()], rate, PIECE_SECONDS) {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(qnc_text::text("transcribe.cancelled").into());
            }
            let range = span.start + piece.start..span.start + piece.end;
            let early = (OVERLAP_SECONDS * rate).min(range.start);
            let piece_start = from as f64 / fps + (range.start - early) as f64 / rate as f64;
            let (heard, piece_language, by) = hear(&samples[range.start - early..range.end], piece_start)?;
            let (until, last) = kept_until(&lines);
            let heard = after_kept(heard, (until, &last));
            let heard = qnc_transcript_store::lines_cut(&heard, from, to, true);
            lines.extend(heard);
            if language.is_empty() {
                language = piece_language;
            }
            made_by = by;
        }
    }
    Ok((lines, language, made_by))
}

/// Where the last word written so far ends (clip frame) and its text.
fn kept_until(lines: &[Line]) -> (u64, String) {
    let last = lines.iter().flat_map(|line| line.words.iter()).max_by_key(|w| w.to);
    let until = lines.iter().flat_map(|line| line.words.iter().map(|w| w.to).chain([line.to])).max().unwrap_or(0);
    (until, last.map(|w| w.text.clone()).unwrap_or_default())
}

/// One word, its case and punctuation aside.
fn same_word(a: &str, b: &str) -> bool {
    let bare = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>();
    let a = bare(a);
    !a.is_empty() && a == bare(b)
}

/// The lines of a piece heard from earlier than its own start, without the words already
/// written: a word is new when it starts at or after the end of the last written word (one
/// frame of the engine's timing given), except that the first such word is dropped when it is
/// the last written word again within half a second. A line keeps its text when all its words
/// are new; one cut in two is written again from the words left.
fn after_kept(lines: Vec<Line>, (until, last): (u64, &str)) -> Vec<Line> {
    let mut first = true;
    let mut new = |from: u64, text: &str| {
        if from + 1 < until {
            return false;
        }
        let echo = std::mem::take(&mut first) && from < until + ECHO_FRAMES && same_word(text, last);
        !echo
    };
    lines
        .into_iter()
        .filter_map(|line| {
            if line.words.is_empty() {
                let text = line.text.clone();
                return new(line.from, &text).then_some(line);
            }
            let keep: Vec<bool> = line.words.iter().map(|w| new(w.from, &w.text)).collect();
            if keep.iter().all(|k| *k) {
                return Some(line);
            }
            let words: Vec<Word> = line.words.into_iter().zip(keep).filter_map(|(w, k)| k.then_some(w)).collect();
            let (first, last) = (words.first()?, words.last()?);
            Some(Line { from: first.from, to: last.to.max(first.from), text: words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" "), words })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(from: u64, to: u64, text: &str) -> Word {
        Word { from, to, text: text.into() }
    }

    fn line(words: Vec<Word>) -> Line {
        let text = words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
        Line { from: words[0].from, to: words[words.len() - 1].to, text, words }
    }

    #[test]
    fn a_piece_heard_again_from_earlier_writes_only_the_words_the_cut_took() {
        let written = vec![line(vec![word(100, 110, "meni"), word(110, 118, "je")])];
        let again = vec![line(vec![word(100, 111, "meni"), word(111, 118, "je"), word(120, 128, "žao")]), line(vec![word(140, 150, "otišao.")])];
        let (until, last) = kept_until(&written);
        let kept = after_kept(again, (until, &last));
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].text, "žao");
        assert_eq!(kept[1].text, "otišao.", "a line all new keeps its text");
        assert!(after_kept(vec![line(vec![word(100, 110, "meni")])], (118, "je")).is_empty());
        assert_eq!(after_kept(vec![line(vec![word(5, 9, "prvi")])], (0, "")).len(), 1, "the first piece keeps everything");
    }

    #[test]
    fn the_last_word_heard_again_a_little_later_is_not_written_twice() {
        let written = vec![line(vec![word(8364, 8368, "di"), word(8372, 8376, "sam"), word(8380, 8388, "i")])];
        let (until, last) = kept_until(&written);
        let again = vec![line(vec![word(8393, 8397, "i"), word(8397, 8401, "mi"), word(8417, 8421, "više")])];
        assert_eq!(after_kept(again, (until, &last))[0].text, "mi više");
        let said = vec![line(vec![word(9000, 9006, "ne,"), word(9010, 9016, "ne,")])];
        assert_eq!(after_kept(said, (until, &last))[0].text, "ne, ne,");
        let later = vec![line(vec![word(8388 + 40, 8388 + 44, "i")])];
        assert_eq!(after_kept(later, (until, &last)).len(), 1);
    }

    #[test]
    fn a_range_goes_to_the_engine_in_pieces_over_the_floor_with_their_start_in_clip_time() {
        let rate = qnc_transcribe_jobs::WHISPER_RATE as usize;
        // 2 s silence, 4 s speech, 2 s silence, from clip frame 500 at 50 fps.
        let mut samples = vec![0.0001f32; 2 * rate];
        samples.extend(vec![0.3f32; 4 * rate]);
        samples.extend(vec![0.0001f32; 2 * rate]);
        let mut starts = Vec::new();
        let (lines, language, by) = hear_range(&samples, 500, 900, 50.0, &AtomicBool::new(false), &mut |piece, start| {
            starts.push((piece.len(), start));
            let from = (start * 50.0).round() as u64;
            Ok((vec![line(vec![word(from, from + 10, "riječ")])], "hr".into(), "e".into()))
        })
        .unwrap();
        assert_eq!(starts.len(), 1, "one loud span, one piece");
        assert!((starts[0].1 - 10.0).abs() < 0.01, "heard from two seconds before the span, which here is the start of the samples: {}", starts[0].1);
        assert!(starts[0].0 >= 6 * rate, "the piece holds the speech with its lead and overlap: {}", starts[0].0);
        assert_eq!((lines.len(), language.as_str(), by.as_str()), (1, "hr", "e"));
    }
}
