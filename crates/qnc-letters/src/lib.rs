//! Letters on a spoken word. An aligner gives, for the text of a word, the clip frame each
//! letter starts at (a short peak), sometimes none for a sign it has not (a comma). A letter
//! lasts until the next one starts, the last until the end of the word's sound. Letters belong
//! to a word only when they lie within `reach` frames of where the word was heard: letters laid
//! onto another word's voice ("broju" once kept letters that lay on the next
//! sentence's "broj") are not kept, and the word is one to align again. Pure.

use serde::{Deserialize, Serialize};

/// How far from its word's heard frames a letter may lie (frames; 0.5 s at 50 fps).
pub const REACH: u64 = 25;

/// One letter of a word and the clip frame it starts at; `None` for a sign without a sound.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Letter {
    pub letter: char,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<f64>,
}

/// The letters of one word of a clip: the word as the transcript has it (its first frame and
/// text), the frame after its last sound, and its letters in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WordLetters {
    pub word_from: u64,
    pub word_to: u64,
    pub text: String,
    pub letters: Vec<Letter>,
}

impl WordLetters {
    /// Every letter with the clip frames `[from, to)` it lasts; `None` for a sign without one.
    pub fn spans(&self) -> Vec<(char, Option<(u64, u64)>)> {
        let starts: Vec<Option<u64>> = self.letters.iter().map(|l| l.at.map(|at| at.round() as u64)).collect();
        self.letters
            .iter()
            .enumerate()
            .map(|(index, letter)| {
                let span = starts[index].map(|from| {
                    let next = starts[index + 1..].iter().flatten().next().copied();
                    let to = next.unwrap_or(self.word_to);
                    (from, to.max(from + 1))
                });
                (letter.letter, span)
            })
            .collect()
    }

    /// The letter said at clip frame `at`, if any.
    pub fn letter_at(&self, at: u64) -> Option<char> {
        self.spans().into_iter().find_map(|(letter, span)| span.filter(|&(from, to)| at >= from && at < to).map(|_| letter))
    }

    /// Do these letters belong to this word: every timed letter within `reach` of its sound.
    pub fn fits(&self, reach: u64) -> bool {
        let low = self.word_from.saturating_sub(reach) as f64;
        let high = (self.word_to.max(self.word_from + 1) + reach) as f64;
        self.letters.iter().all(|l| l.at.is_none_or(|at| at >= low && at <= high))
    }
}

/// The letters kept for the word that starts at `from` with `text`.
pub fn letters_of<'a>(kept: &'a [WordLetters], from: u64, text: &str) -> Option<&'a WordLetters> {
    let text = text.trim();
    kept.iter().find(|w| w.word_from == from && w.text == text)
}

/// What an aligner gave, sorted: the words whose letters fit (to keep) and those whose letters
/// lie away from their sound (to drop and align again).
pub fn admit(given: Vec<WordLetters>, reach: u64) -> (Vec<WordLetters>, Vec<WordLetters>) {
    given.into_iter().partition(|word| word.fits(reach))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(from: u64, to: u64, text: &str, starts: &[Option<f64>]) -> WordLetters {
        let letters = text.chars().zip(starts).map(|(letter, &at)| Letter { letter, at }).collect();
        WordLetters { word_from: from, word_to: to, text: text.to_string(), letters }
    }

    #[test]
    fn a_letter_lasts_until_the_next_one_the_last_until_the_words_end_and_a_sign_has_no_time() {
        let w = word(100, 112, "a,b", &[Some(100.4), None, Some(105.0)]);
        assert_eq!(w.spans(), vec![('a', Some((100, 105))), (',', None), ('b', Some((105, 112)))]);
        assert_eq!(w.letter_at(104), Some('a'));
        assert_eq!(w.letter_at(111), Some('b'));
        assert_eq!(w.letter_at(112), None);
    }

    #[test]
    fn letters_laid_on_another_words_voice_are_not_kept() {
        let fitting = word(956, 967, "broju", &[Some(956.0), Some(958.0), Some(961.0), Some(963.0), Some(965.0)]);
        let away = word(956, 967, "broju", &[Some(1012.0), Some(1014.0), Some(1016.0), Some(1018.0), Some(1020.0)]);
        assert!(fitting.fits(REACH));
        assert!(!away.fits(REACH));
        let (kept, dropped) = admit(vec![fitting.clone(), away.clone()], REACH);
        assert_eq!(kept, vec![fitting.clone()]);
        assert_eq!(dropped, vec![away]);
        assert_eq!(letters_of(&kept, 956, " broju "), Some(&fitting));
        assert_eq!(letters_of(&kept, 957, "broju"), None);
    }
}
