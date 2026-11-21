//! What a transcript is: lines of speech with their words and the clip frames they are said
//! in; a queued range of a clip waiting to be heard; the rule that cuts lines to a range.

use serde::{Deserialize, Serialize};

/// One word and the clip frames it is said in (`[from, to)`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Word {
    pub from: u64,
    pub to: u64,
    pub text: String,
}

/// One line of speech with its words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Line {
    pub from: u64,
    pub to: u64,
    pub text: String,
    pub words: Vec<Word>,
}

/// What was said in clip frames `[from_frame, to_frame)` of a clip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipTranscript {
    pub clip_id: String,
    pub from_frame: u64,
    pub to_frame: u64,
    /// The language the text is in (a code).
    pub language: String,
    /// The engine and model that heard it.
    pub engine: String,
    pub lines: Vec<Line>,
    /// Heard by the fine engine of the language: a later transcript of the fast engine never
    /// says over it, it only fills what the fine one lacks.
    #[serde(default)]
    pub fine: bool,
}

/// Which engine a queued range is heard with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueQuality {
    Fast,
    Best,
}

impl QueueQuality {
    pub fn code(self) -> &'static str {
        match self {
            QueueQuality::Fast => "fast",
            QueueQuality::Best => "best",
        }
    }

    pub fn of(code: &str) -> Option<QueueQuality> {
        match code {
            "fast" => Some(QueueQuality::Fast),
            "best" => Some(QueueQuality::Best),
            _ => None,
        }
    }
}

/// Where a queued range is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueState {
    Waiting,
    Running,
    Done,
    Failed,
}

impl QueueState {
    pub fn code(self) -> &'static str {
        match self {
            QueueState::Waiting => "waiting",
            QueueState::Running => "running",
            QueueState::Done => "done",
            QueueState::Failed => "failed",
        }
    }

    pub fn of(code: &str) -> Option<QueueState> {
        match code {
            "waiting" => Some(QueueState::Waiting),
            "running" => Some(QueueState::Running),
            "done" => Some(QueueState::Done),
            "failed" => Some(QueueState::Failed),
            _ => None,
        }
    }

    /// Heard, or given up: nothing waits for it any more.
    pub fn is_over(self) -> bool {
        matches!(self, QueueState::Done | QueueState::Failed)
    }
}

/// A range of a clip asked to be transcribed in the background; the queue is in the project's
/// database, so it outlives the program and another one sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Queued {
    pub clip_id: String,
    pub from_frame: u64,
    pub to_frame: u64,
    pub quality: QueueQuality,
    pub state: QueueState,
    /// Why it failed.
    pub error: String,
    /// How long its transcription took, when done (milliseconds).
    #[serde(default)]
    pub took_ms: u64,
}

/// The lines of `lines` cut to what lies inside clip frames `[from, to)` (`inside`) or outside
/// them: a word belongs where its middle is. A line with words keeps those on the asked side,
/// one line for each run of them; its text is those words when some went. A line without words
/// goes by its middle. Inside, a kept word is clamped to the range.
pub fn lines_cut(lines: &[Line], from: u64, to: u64, inside: bool) -> Vec<Line> {
    let wanted = |a: u64, b: u64| (from..to).contains(&((a + b) / 2)) == inside;
    let mut cut = Vec::new();
    for line in lines {
        if line.words.is_empty() {
            if wanted(line.from, line.to) {
                cut.push(line.clone());
            }
            continue;
        }
        if line.words.iter().all(|word| wanted(word.from, word.to)) {
            cut.push(line.clone());
            continue;
        }
        let mut run: Vec<Word> = Vec::new();
        let mut flush = |run: &mut Vec<Word>| {
            if let (Some(first), Some(last)) = (run.first(), run.last()) {
                let text = run.iter().map(|word| word.text.as_str()).collect::<Vec<_>>().join(" ");
                cut.push(Line { from: first.from, to: last.to, text, words: std::mem::take(run) });
            }
        };
        for word in &line.words {
            if wanted(word.from, word.to) {
                run.push(word.clone());
            } else {
                flush(&mut run);
            }
        }
        flush(&mut run);
    }
    if inside {
        for line in &mut cut {
            for word in &mut line.words {
                (word.from, word.to) = (word.from.clamp(from, to), word.to.clamp(from, to));
            }
            (line.from, line.to) = (line.from.clamp(from, to), line.to.clamp(from, to));
        }
    }
    cut
}

/// The share of clip frames `[from, to)` the words of `lines` cover.
pub fn spoken_share(lines: &[Line], from: u64, to: u64) -> f32 {
    if to <= from {
        return 0.0;
    }
    let mut spans: Vec<(u64, u64)> = lines.iter().flat_map(|line| line.words.iter()).map(|word| (word.from.max(from), word.to.min(to))).filter(|(a, b)| a < b).collect();
    spans.sort_unstable();
    let (mut covered, mut until) = (0u64, from);
    for (a, b) in spans {
        let a = a.max(until);
        if b > a {
            covered += b - a;
            until = b;
        }
    }
    covered as f32 / (to - from) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(from: u64, to: u64, text: &str) -> Word {
        Word { from, to, text: text.into() }
    }

    #[test]
    fn a_line_running_past_a_range_keeps_the_words_on_the_asked_side() {
        let line = Line { from: 10, to: 40, text: "a b c".into(), words: vec![word(10, 14, "a"), word(20, 24, "b"), word(30, 40, "c")] };
        let inside = lines_cut(&[line.clone()], 18, 50, true);
        assert_eq!(inside.len(), 1);
        assert_eq!(inside[0].text, "b c");
        assert_eq!((inside[0].from, inside[0].to), (20, 40));
        let outside = lines_cut(&[line], 18, 50, false);
        assert_eq!(outside[0].text, "a");
        assert!((spoken_share(&inside, 18, 50) - 14.0 / 32.0).abs() < 1e-6);
    }
}
