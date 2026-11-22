//! Fuzzy matching for the launcher: the query's characters must appear in order.

/// Score in `0.0..=1.0` of `query` against `target`, ignoring case; `None` when they don't match
/// or match too loosely.
/// Consecutive characters and word starts score higher, and so do shorter targets.
pub fn score(query: &str, target: &str) -> Option<f32> {
    let query: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    if query.is_empty() {
        return Some(0.0);
    }
    let original: Vec<char> = target.chars().collect();
    let lower: Vec<char> = original.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    if query.len() > lower.len() {
        return None;
    }
    let word_start = |j: usize| {
        j == 0 || !original[j - 1].is_alphanumeric() || (original[j - 1].is_lowercase() && original[j].is_uppercase())
    };
    // A matched character is worth 1, or 2 when it follows the previous match or starts a word.
    let worth = |j: usize, consecutive: bool| if consecutive || word_start(j) { 2.0 } else { 1.0 };

    // best[j]: best score with the query so far matched and its last character at `j`.
    let mut best: Vec<Option<f32>> = lower.iter().enumerate().map(|(j, c)| (*c == query[0]).then(|| worth(j, false))).collect();
    for &wanted in &query[1..] {
        let mut next = vec![None; lower.len()];
        // Best `score + GAP_CHAR_PENALTY * k` over matches at k <= j - 2, so a match at `j` after a
        // gap from `k` scores `score - GAP_PENALTY - GAP_CHAR_PENALTY * (j - k - 1)`.
        let mut before_gap: Option<f32> = None;
        for j in 1..lower.len() {
            if j >= 2 {
                before_gap = max(before_gap, best[j - 2].map(|score| score + GAP_CHAR_PENALTY * (j - 2) as f32));
            }
            if lower[j] != wanted {
                continue;
            }
            let consecutive = best[j - 1].map(|score| score + worth(j, true));
            let after_gap = before_gap.map(|score| score - GAP_CHAR_PENALTY * (j - 1) as f32 - GAP_PENALTY + worth(j, false));
            next[j] = max(consecutive, after_gap);
        }
        best = next;
    }
    let raw = best.into_iter().flatten().reduce(f32::max)?;
    let ratio = (raw / (2.0 * query.len() as f32)).clamp(0.0, 1.0);
    let score = ratio * (query.len() as f32 / lower.len() as f32).powf(LENGTH_WEIGHT);
    (score >= MIN_SCORE).then_some(score)
}

fn max(a: Option<f32>, b: Option<f32>) -> Option<f32> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        _ => a.or(b),
    }
}

// Subtracted for each gap between matched characters, plus for each character it skips.
const GAP_PENALTY: f32 = 0.5;
const GAP_CHAR_PENALTY: f32 = 0.05;
// Weaker matches, letters scattered across words, are dropped.
const MIN_SCORE: f32 = 0.3;
// How much a longer target lowers the score.
const LENGTH_WEIGHT: f32 = 0.15;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn characters_must_appear_in_order() {
        assert!(score("ffx", "Firefox").is_some());
        assert!(score("xff", "Firefox").is_none());
        assert!(score("firefoxes", "Firefox").is_none());
    }

    #[test]
    fn case_is_ignored() {
        assert_eq!(score("FIRE", "firefox"), score("fire", "Firefox"));
    }

    #[test]
    fn scores_stay_between_zero_and_one() {
        for (query, target) in [("f", "Firefox"), ("firefox", "Firefox"), ("fx", "Firefox Developer Edition")] {
            let score = score(query, target).unwrap();
            assert!((0.0..=1.0).contains(&score), "{query} in {target}: {score}");
        }
        assert_eq!(score("firefox", "firefox"), Some(1.0));
    }

    #[test]
    fn consecutive_characters_beat_scattered_ones() {
        assert!(score("fox", "Firefox").unwrap() > score("fio", "Firefox").unwrap());
    }

    #[test]
    fn word_starts_beat_the_middle_of_words() {
        assert!(score("vs", "Visual Studio").unwrap() > score("ua", "Visual Studio").unwrap());
    }

    #[test]
    fn shorter_targets_win_ties() {
        assert!(score("code", "Code").unwrap() > score("code", "Code - OSS Insiders").unwrap());
    }

    #[test]
    fn letters_scattered_across_words_do_not_match() {
        assert!(score("calc", "Recent folders and file search").is_none());
        assert!(score("ffx", "Firefox").is_some());
        assert!(score("gpu term", "Fast, GPU-based terminal").is_some());
    }

    #[test]
    fn an_empty_query_matches_everything() {
        assert_eq!(score("", "Firefox"), Some(0.0));
    }
}
