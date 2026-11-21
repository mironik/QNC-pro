//! Whether a clip is a statement (someone speaking on the first camera channel), the rule the
//! transcripts of an ingest follow: a clip of `WHOLE_SECONDS` or shorter is heard whole; a
//! longer one by its loudest `PROBE_SECONDS` (where the voice is, when there is one); the clip
//! is a statement when the words heard cover at least `SHARE` of what was heard. A statement is
//! then transcribed whole; a clip that is none is left to the AI shots. Pure over the samples
//! and the words given.

/// A clip this long or shorter is heard whole at once.
pub const WHOLE_SECONDS: usize = 20;
/// How many seconds of the first channel are heard to know whether a long clip is a statement.
pub const PROBE_SECONDS: usize = 15;
/// The share of the heard seconds the words must cover.
pub const SHARE: f32 = 0.3;

/// The loudest window of `window` samples in `samples`, by the energy of its tenths of a
/// second: the samples `[from, to)`. The whole when the samples are shorter than the window.
pub fn loudest_window(samples: &[f32], rate: usize, window: usize) -> std::ops::Range<usize> {
    if samples.len() <= window {
        return 0..samples.len();
    }
    let tenth = (rate / 10).max(1);
    let energy: Vec<f64> = samples.chunks(tenth).map(|chunk| chunk.iter().map(|s| f64::from(*s) * f64::from(*s)).sum()).collect();
    let span = (window / tenth).max(1);
    let mut best = (0usize, f64::MIN);
    let mut sum: f64 = energy.iter().take(span).sum();
    for first in 0..=energy.len().saturating_sub(span) {
        if first > 0 {
            sum += energy.get(first + span - 1).copied().unwrap_or_default() - energy[first - 1];
        }
        if sum > best.1 {
            best = (first, sum);
        }
    }
    let from = best.0 * tenth;
    from..(from + window).min(samples.len())
}

/// The share of the frames `[from, to)` the words `(from, to)` cover, overlaps counted once.
pub fn spoken_share(words: impl IntoIterator<Item = (u64, u64)>, from: u64, to: u64) -> f32 {
    if to <= from {
        return 0.0;
    }
    let mut spans: Vec<(u64, u64)> = words.into_iter().map(|(a, b)| (a.max(from), b.min(to))).filter(|(a, b)| a < b).collect();
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

/// Whether words covering `share` make a statement.
pub fn is_statement(share: f32) -> bool {
    share >= SHARE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_loudest_window_is_where_the_voice_is_and_the_share_counts_overlaps_once() {
        let rate = 100;
        let mut samples = vec![0.01f32; 10 * rate];
        samples[6 * rate..9 * rate].fill(0.5);
        let window = loudest_window(&samples, rate, 3 * rate);
        assert_eq!(window, 6 * rate..9 * rate);
        assert_eq!(loudest_window(&samples[..200], rate, 3 * rate), 0..200, "shorter than the window is the whole");
        assert!((spoken_share([(0, 10), (5, 15), (90, 110)], 0, 100) - 0.25).abs() < 1e-6);
        assert_eq!(spoken_share([(0, 10)], 10, 10), 0.0);
        assert!(is_statement(0.3) && !is_statement(0.29));
    }
}
