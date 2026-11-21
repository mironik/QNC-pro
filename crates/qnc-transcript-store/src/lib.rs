//! The transcripts of a project in its own database: what was said in a range of a clip's
//! frames, line by line with every word and the frames it is said in, the language and the
//! engine that heard it; and the queue of the ranges asked to be heard. Kept in the project
//! database so a range is heard once and every form, and another computer, reads it. The one
//! owner of those tables; written only through the database writer.

mod model;
mod read;
mod store;

pub use model::{lines_cut, spoken_share, ClipTranscript, Line, QueueQuality, QueueState, Queued, Word};
pub use read::{heard, read_clip, read_queue, transcribed};
pub use store::{TranscriptTables, MODULE, QUEUE, TRANSCRIPTS};

use qnc_db::Writer;
use serde_json::json;

/// Keeps a transcript (it replaces one of the same range; the newest says over older ones).
pub fn save(writer: &Writer, transcript: &ClipTranscript) -> Result<(), String> {
    writer.execute(MODULE, json!({ "save": transcript })).map(|_| ())
}

/// Queues ranges, waiting (a range asked before is replaced).
pub fn enqueue(writer: &Writer, ranges: &[Queued]) -> Result<(), String> {
    writer.execute(MODULE, json!({ "enqueue": ranges })).map(|_| ())
}

/// A queued range is now in `state`, with why it failed and how long it took.
pub fn set_queued(writer: &Writer, entry: &Queued, state: QueueState, error: &str, took_ms: u64) -> Result<(), String> {
    let set = Queued { state, error: error.to_string(), took_ms, ..entry.clone() };
    writer.execute(MODULE, json!({ "set_queued": set })).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use qnc_db::Reader;

    fn word(from: u64, to: u64, text: &str) -> Word {
        Word { from, to, text: text.into() }
    }

    fn line(words: Vec<Word>) -> Line {
        let text = words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
        Line { from: words[0].from, to: words[words.len() - 1].to, text, words }
    }

    #[test]
    fn a_newer_transcript_takes_the_words_of_an_older_one_over_its_range_and_the_queue_is_heard_out() {
        let dir = std::env::temp_dir().join(format!("qnc-transcript-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("project.db");
        qnc_db::create(&db).unwrap();
        let reader = Reader::open(&db).unwrap();
        assert!(read_clip(&reader, "c").unwrap().is_empty(), "no table yet is no transcript");
        let writer = Writer::start(&db, vec![Box::new(TranscriptTables)]).unwrap();
        let old = ClipTranscript { clip_id: "c".into(), from_frame: 0, to_frame: 300, language: "hr".into(), engine: "e".into(), lines: vec![line(vec![word(10, 20, "ovo"), word(20, 30, "je"), word(150, 160, "prije"), word(160, 170, "brkova")])], fine: false };
        save(&writer, &old).unwrap();
        let newer = ClipTranscript { clip_id: "c".into(), from_frame: 100, to_frame: 200, language: "hr".into(), engine: "e".into(), lines: vec![line(vec![word(150, 161, "prije"), word(161, 170, "brkova,")])], fine: false };
        save(&writer, &newer).unwrap();
        let read = read_clip(&reader, "c").unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].lines[0].text, "ovo je", "the old one lost its words in the new range");
        assert_eq!(read[1].lines[0].text, "prije brkova,");
        assert!(save(&writer, &ClipTranscript { from_frame: 5, to_frame: 5, ..old.clone() }).is_err(), "an empty range is refused");
        let fine = ClipTranscript { clip_id: "c".into(), from_frame: 0, to_frame: 50, language: "hr".into(), engine: "f".into(), lines: vec![line(vec![word(10, 20, "Ovo"), word(20, 30, "je")])], fine: true };
        save(&writer, &fine).unwrap();
        save(&writer, &ClipTranscript { fine: false, lines: vec![line(vec![word(12, 22, "ovo")])], ..fine.clone() }).unwrap();
        let read = read_clip(&reader, "c").unwrap();
        assert!(read.iter().any(|t| t.fine && t.lines[0].text == "Ovo je"), "a fast one never says over a fine one: {read:?}");
        // The queue.
        let ask = |from, to| Queued { clip_id: "c".into(), from_frame: from, to_frame: to, quality: QueueQuality::Fast, state: QueueState::Waiting, error: String::new(), took_ms: 0 };
        enqueue(&writer, &[ask(0, 150), ask(150, 300)]).unwrap();
        assert_eq!(read_queue(&reader).unwrap().len(), 2);
        assert!(heard(&reader).unwrap().is_empty());
        set_queued(&writer, &ask(0, 150), QueueState::Done, "", 12).unwrap();
        assert!(heard(&reader).unwrap().is_empty(), "a clip queued in pieces is heard when every piece is");
        set_queued(&writer, &ask(150, 300), QueueState::Failed, "no sound", 0).unwrap();
        assert_eq!(heard(&reader).unwrap().len(), 1);
        assert_eq!(transcribed(&reader).unwrap().len(), 1);
        writer.execute(MODULE, json!({ "clear_done": true })).unwrap();
        assert_eq!(read_queue(&reader).unwrap().len(), 1, "the failed one stays, with why");
        writer.execute(MODULE, json!({ "forget_clip": "c" })).unwrap();
        assert!(read_clip(&reader, "c").unwrap().is_empty());
        drop(writer);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
