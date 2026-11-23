//! The launcher's Emojis page: every emoji by Unicode group, a fuzzy search over names and
//! shortcodes, use counts for MOST USED and one skin tone for all emojis that have tones.

use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::time::Duration;

use emojis::{Emoji, EmojiVersion, Group, SkinTone};
use tokio::process::Command;

use crate::fuzzy;

pub const COLUMNS: usize = 6;
/// Emojis in the MOST USED row.
pub const MOST_USED: usize = 6;
pub const TILE_HEIGHT: f32 = 84.0;
pub const HEADER_HEIGHT: f32 = 22.0;
pub const GAP: f32 = 8.0;
/// The tones Alt+Left/Right cycles through, the yellow default first.
const TONES: [SkinTone; 6] = [SkinTone::Default, SkinTone::Light, SkinTone::MediumLight, SkinTone::Medium, SkinTone::MediumDark, SkinTone::Dark];
/// Newest emojis drawn in a skin tone. Noto Color Emoji (2026-09) has no toned glyphs for
/// Emoji 18.0's 🫹 and 🫺, which then show as the yellow hand beside a tone swatch.
const NEWEST_TONED: EmojiVersion = EmojiVersion::new(17, 0);
/// Wait after the launcher closes, so the window below has the keyboard again before typing.
const REFOCUS: Duration = Duration::from_millis(150);

/// One line of the grid: a section header, or a row of emojis given as a range into the
/// shown emojis.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Header(&'static str),
    /// `counts`: the tiles show how often each was picked (MOST USED and search results).
    Row { emojis: Range<usize>, counts: bool },
}

impl Line {
    /// Height of the line plus the gap under it.
    pub fn height(&self) -> f32 {
        match self {
            Line::Header(_) => HEADER_HEIGHT + GAP,
            Line::Row { .. } => TILE_HEIGHT + GAP,
        }
    }
}

/// What the Emojis page shows and remembers.
#[derive(Default)]
pub struct Picker {
    /// Picks by emoji (in its default tone).
    pub counts: HashMap<String, u32>,
    /// Index into the tones, 0 for the default yellow.
    pub tone: usize,
    pub shown: Vec<&'static Emoji>,
    pub lines: Vec<Line>,
    /// First line on screen.
    pub first_line: usize,
}

impl Picker {
    /// With the counts and tone kept in the state file.
    pub fn load() -> Self {
        let json = std::fs::read_to_string(path()).ok().and_then(|content| serde_json::from_str(&content).ok()).unwrap_or_default();
        let (counts, tone) = parse_state(&json);
        Self { counts, tone, ..Self::default() }
    }

    pub fn save(&self) {
        let path = path();
        let written = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, state_json(&self.counts, self.tone).to_string()));
        if let Err(err) = written {
            eprintln!("zehll: cannot save {}: {err}", path.display());
        }
    }

    /// Lays the grid out again for `query`, back at its top.
    pub fn refresh(&mut self, query: &str) {
        let (shown, lines) = layout(query, &self.counts);
        self.shown = shown;
        self.lines = lines;
        self.first_line = 0;
    }

    /// The emoji at `index` as it is typed: in the chosen tone when it has tones.
    pub fn text(&self, index: usize) -> Option<&'static str> {
        self.shown.get(index).map(|emoji| with_tone(emoji, self.tone).as_str())
    }
}

/// Title of a group's header, as in the design.
pub fn group_title(group: Group) -> &'static str {
    match group {
        Group::SmileysAndEmotion => "SMILEYS & EMOTION",
        Group::PeopleAndBody => "PEOPLE & BODY",
        Group::AnimalsAndNature => "ANIMALS & NATURE",
        Group::FoodAndDrink => "FOOD & DRINK",
        Group::TravelAndPlaces => "TRAVEL & PLACES",
        Group::Activities => "ACTIVITIES",
        Group::Objects => "OBJECTS",
        Group::Symbols => "SYMBOLS",
        Group::Flags => "FLAGS",
    }
}

/// The shown emojis and the grid's lines: with an empty search, MOST USED (when anything was
/// picked) and then every group under its header; else the matches as headerless rows.
pub fn layout(query: &str, counts: &HashMap<String, u32>) -> (Vec<&'static Emoji>, Vec<Line>) {
    let mut shown = Vec::new();
    let mut lines = Vec::new();
    let mut section = |title: Option<&'static str>, emojis: Vec<&'static Emoji>, counts: bool| {
        if emojis.is_empty() {
            return;
        }
        if let Some(title) = title {
            lines.push(Line::Header(title));
        }
        let start = shown.len();
        shown.extend(emojis);
        for row in (start..shown.len()).step_by(COLUMNS) {
            lines.push(Line::Row { emojis: row..(row + COLUMNS).min(shown.len()), counts });
        }
    };
    if !query.trim().is_empty() {
        section(None, search(query.trim(), counts), true);
        return (shown, lines);
    }
    section(Some("MOST USED"), most_used(counts), true);
    for group in Group::iter() {
        section(Some(group_title(group)), group.emojis().collect(), false);
    }
    (shown, lines)
}

/// Keywords of each emoji from emojilib, the list ii searches: "thunder" finds ⚡ although its
/// Unicode name is "high voltage".
const KEYWORDS: &str = include_str!("../assets/emoji/keywords.tsv");

/// Keywords by emoji (without variation selectors) from `keywords.tsv` lines: the emoji, a tab,
/// then comma-separated keywords. `#` lines are comments.
fn parse_keywords(text: &'static str) -> HashMap<&'static str, Vec<&'static str>> {
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once('\t'))
        .map(|(emoji, words)| (emoji, words.split(',').filter(|word| !word.is_empty()).collect()))
        .collect()
}

/// The keywords of `emoji`, in its default tone; empty when emojilib has none.
fn keywords(emoji: &Emoji) -> &'static [&'static str] {
    static TABLE: std::sync::OnceLock<HashMap<&'static str, Vec<&'static str>>> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| parse_keywords(KEYWORDS));
    let key = base(emoji).replace('\u{fe0f}', "");
    table.get(key.as_str()).map_or(&[], Vec::as_slice)
}

/// The most picked emojis, most first, at most `MOST_USED`.
pub fn most_used(counts: &HashMap<String, u32>) -> Vec<&'static Emoji> {
    let mut picked: Vec<(&'static Emoji, u32)> = counts.iter().filter(|(_, count)| **count > 0).filter_map(|(emoji, count)| Some((emojis::get(emoji)?, *count))).collect();
    // Equal counts in a fixed order, so the row doesn't shuffle between openings.
    picked.sort_by_key(|(emoji, count)| (std::cmp::Reverse(*count), emoji.as_str()));
    picked.into_iter().take(MOST_USED).map(|(emoji, _)| emoji).collect()
}

/// Emojis matching `query` by name or shortcode (or, weaker, by keyword, then by group), best
/// first; equal scores go to the more picked one, then keep Unicode's order.
pub fn search(query: &str, counts: &HashMap<String, u32>) -> Vec<&'static Emoji> {
    let mut ranked: Vec<(f32, u32, &'static Emoji)> = emojis::iter()
        .filter_map(|emoji| {
            let own = std::iter::once(emoji.name()).chain(emoji.shortcodes()).filter_map(|field| fuzzy::score(query, field)).reduce(f32::max);
            // A keyword is a looser description than the name, so it ranks a bit lower.
            let keyword = keywords(emoji).iter().filter_map(|field| fuzzy::score(query, field)).reduce(f32::max).map(|score| score * 0.8);
            // A group match alone ranks under any match of the emoji's own words.
            let group = fuzzy::score(query, group_title(emoji.group())).map(|score| score * 0.5);
            let score = own.into_iter().chain(keyword).chain(group).reduce(f32::max)?;
            Some((score, count(counts, emoji), emoji))
        })
        .collect();
    // Stable: equal scores and counts keep Unicode's order.
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1)));
    ranked.into_iter().map(|(_, _, emoji)| emoji).collect()
}

/// How often `emoji` was picked, whatever its tone.
pub fn count(counts: &HashMap<String, u32>, emoji: &Emoji) -> u32 {
    counts.get(base(emoji)).copied().unwrap_or(0)
}

/// Counts one more pick of `emoji`, under its default tone.
pub fn bump(counts: &mut HashMap<String, u32>, emoji: &Emoji) {
    *counts.entry(base(emoji).to_owned()).or_default() += 1;
}

/// `emoji` in its default tone.
fn base(emoji: &Emoji) -> &str {
    emoji.with_skin_tone(SkinTone::Default).unwrap_or(emoji).as_str()
}

/// `emoji` in the tone at `tone`, or as it is when it has no tones.
pub fn with_tone(emoji: &'static Emoji, tone: usize) -> &'static Emoji {
    match has_tones(emoji) {
        true => emoji.with_skin_tone(TONES[tone % TONES.len()]).unwrap_or(emoji),
        false => emoji,
    }
}

/// Whether `emoji` is offered in skin tones: it has them and the font draws them.
pub fn has_tones(emoji: &Emoji) -> bool {
    emoji.skin_tones().is_some() && emoji.emoji_version() <= NEWEST_TONED
}

/// The previous or next tone, wrapping.
pub fn cycle_tone(tone: usize, delta: i32) -> usize {
    (tone as i64 + delta as i64).rem_euclid(TONES.len() as i64) as usize
}

/// The line whose row holds the emoji at `index`.
pub fn line_of(lines: &[Line], index: usize) -> Option<usize> {
    lines.iter().position(|line| matches!(line, Line::Row { emojis, .. } if emojis.contains(&index)))
}

/// The emoji `delta` rows up or down from `index`, in the same column or the row's last one;
/// headers are skipped and `index` stays when there is no such row.
pub fn vertical(lines: &[Line], index: usize, delta: i32) -> usize {
    let Some(at) = line_of(lines, index) else {
        return index;
    };
    let Line::Row { emojis: here, .. } = &lines[at] else {
        return index;
    };
    let column = index - here.start;
    let rows: Vec<&Range<usize>> = lines.iter().filter_map(|line| if let Line::Row { emojis, .. } = line { Some(emojis) } else { None }).collect();
    let Some(row) = rows.iter().position(|row| *row == here) else {
        return index;
    };
    let target = row as i64 + delta as i64;
    match usize::try_from(target).ok().and_then(|target| rows.get(target)) {
        Some(row) => row.start + column.min(row.len() - 1),
        None => index,
    }
}

/// The emoji beside `index`, kept within the `len` shown.
pub fn sideways(index: usize, delta: i32, len: usize) -> usize {
    (index as i64 + delta as i64).clamp(0, len.saturating_sub(1) as i64) as usize
}

/// First line to show so the line `selected` fits in `height`, moving as little as possible from
/// `first`; the header right above a selected row comes along.
pub fn first_line(lines: &[Line], selected: usize, first: usize, height: f32) -> usize {
    let mut first = first.min(selected);
    if first == selected && selected > 0 && matches!(lines.get(selected - 1), Some(Line::Header(_))) {
        first -= 1;
    }
    while first < selected && lines[first..=selected].iter().map(Line::height).sum::<f32>() > height {
        first += 1;
    }
    first
}

/// `first` scrolled by `rows` lines, no further than where the last line is at the bottom.
pub fn scroll(lines: &[Line], first: usize, rows: i32, height: f32) -> usize {
    let last = (0..lines.len()).find(|&from| lines[from..].iter().map(Line::height).sum::<f32>() <= height).unwrap_or(lines.len().saturating_sub(1));
    (first as i64 + rows as i64).clamp(0, last as i64) as usize
}

/// Counts by emoji and the tone, from the state file's JSON.
pub fn parse_state(json: &serde_json::Value) -> (HashMap<String, u32>, usize) {
    let counts = json["counts"]
        .as_object()
        .map(|counts| counts.iter().filter_map(|(emoji, count)| Some((emoji.clone(), u32::try_from(count.as_u64()?).ok()?))).collect())
        .unwrap_or_default();
    let tone = json["tone"].as_u64().map_or(0, |tone| tone as usize);
    (counts, if tone < TONES.len() { tone } else { 0 })
}

pub fn state_json(counts: &HashMap<String, u32>, tone: usize) -> serde_json::Value {
    serde_json::json!({ "counts": counts, "tone": tone })
}

/// Copies `text`, waits for the launcher to close, gives the keyboard back to `window` (else
/// Hyprland focuses whatever is under the pointer), then types it there with wtype.
pub async fn paste(text: String, window: Option<String>) {
    crate::clipboard::copy(text.clone()).await;
    tokio::time::sleep(REFOCUS).await;
    if let Some(window) = window {
        let focused = tokio::task::spawn_blocking(move || crate::hyprland::focus_window(&window)).await.unwrap_or(false);
        if !focused {
            eprintln!("zehll: cannot give the focus back to the window for the emoji");
        }
        tokio::time::sleep(REFOCUS).await;
    }
    match Command::new("wtype").arg("--").arg(&text).status().await {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("zehll: wtype failed: {status}"),
        Err(err) => eprintln!("zehll: cannot run wtype: {err}"),
    }
}

fn path() -> PathBuf {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    state.join("zehll/emojis.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(s: &str) -> &'static Emoji {
        emojis::get(s).unwrap()
    }

    fn counts(picks: &[(&str, u32)]) -> HashMap<String, u32> {
        picks.iter().map(|(emoji, count)| (emoji.to_string(), *count)).collect()
    }

    fn row(range: Range<usize>) -> Line {
        Line::Row { emojis: range, counts: false }
    }

    #[test]
    fn an_empty_search_lists_every_group_under_its_header() {
        let (shown, lines) = layout("", &HashMap::new());
        assert_eq!(shown.len(), emojis::iter().count());
        assert_eq!(lines[0], Line::Header("SMILEYS & EMOTION"));
        assert_eq!(lines[1], row(0..6));
        assert_eq!(shown[0].as_str(), "😀");
        let headers: Vec<&str> = lines.iter().filter_map(|line| if let Line::Header(title) = line { Some(*title) } else { None }).collect();
        assert_eq!(headers.len(), 9);
        assert_eq!(headers.last(), Some(&"FLAGS"));
        // A group's last row may be short; the next group starts a new row.
        let smileys = Group::SmileysAndEmotion.emojis().count();
        let next = lines.iter().position(|line| *line == Line::Header("PEOPLE & BODY")).unwrap();
        assert_eq!(lines[next - 1], row(smileys - (smileys - 1) % COLUMNS - 1..smileys));
    }

    #[test]
    fn most_used_comes_first_with_counts() {
        let counts = counts(&[("🍌", 42), ("👍", 31), ("🙂", 7)]);
        let (shown, lines) = layout("", &counts);
        assert_eq!(lines[0], Line::Header("MOST USED"));
        assert_eq!(lines[1], Line::Row { emojis: 0..3, counts: true });
        assert_eq!(lines[2], Line::Header("SMILEYS & EMOTION"));
        assert_eq!(lines[3], row(3..9));
        assert_eq!(shown[..3].iter().map(|emoji| emoji.as_str()).collect::<Vec<_>>(), ["🍌", "👍", "🙂"]);
    }

    #[test]
    fn most_used_keeps_the_top_six_and_skips_unknown_emojis() {
        let counts = counts(&[("😀", 1), ("😃", 2), ("😄", 3), ("😁", 4), ("😆", 5), ("😅", 6), ("🤣", 7), ("not an emoji", 99)]);
        let most: Vec<&str> = most_used(&counts).iter().map(|emoji| emoji.as_str()).collect();
        assert_eq!(most, ["🤣", "😅", "😆", "😁", "😄", "😃"]);
    }

    #[test]
    fn a_search_flattens_to_ranked_matches() {
        let (shown, lines) = layout("banana", &HashMap::new());
        assert_eq!(shown[0].as_str(), "🍌");
        assert!(lines.iter().all(|line| matches!(line, Line::Row { counts: true, .. })));
        assert!(layout("zzqqxx", &HashMap::new()).0.is_empty());
    }

    #[test]
    fn a_search_matches_shortcodes_and_prefers_the_more_picked() {
        assert_eq!(search("+1", &HashMap::new())[0].as_str(), "👍");
        let laugh = search("laughing", &HashMap::new());
        assert!(laugh.iter().any(|emoji| emoji.as_str() == "😆"));
        assert!(laugh.iter().any(|emoji| emoji.as_str() == "🤣"));
        let joy = "😂";
        let picked = counts(&[(joy, 5)]);
        let first = search("face with tears", &picked)[0];
        assert_eq!(first.as_str(), joy);
    }

    #[test]
    fn keywords_are_read_from_the_table() {
        let table = parse_keywords("# comment\n⚡\thigh voltage,thunder\n☂\tumbrella,rain\n\n");
        assert_eq!(table.get("⚡"), Some(&vec!["high voltage", "thunder"]));
        assert_eq!(table.get("☂").map(Vec::len), Some(2));
        assert_eq!(table.len(), 2);
        // Every emoji emojilib knows is found, with or without its variation selector.
        assert!(keywords(get("⚡")).contains(&"thunder"));
        assert!(keywords(get("☂️")).contains(&"rain"));
        assert!(keywords(get("👍🏽")).contains(&"thumbsup"));
    }

    #[test]
    fn a_search_matches_keywords_below_names() {
        let thunder = search("thunder", &HashMap::new());
        assert!(thunder.iter().take(3).any(|emoji| emoji.as_str() == "⚡"), "{:?}", thunder.iter().take(5).map(|e| e.as_str()).collect::<Vec<_>>());
        // "lightning" is in a name (cloud with lightning), which beats ⚡'s keyword.
        let lightning = search("lightning", &HashMap::new());
        assert_eq!(lightning[0].as_str(), "🌩️");
        assert!(lightning.iter().any(|emoji| emoji.as_str() == "⚡"));
    }

    #[test]
    fn counts_and_tones_go_by_the_default_tone() {
        let mut counts = HashMap::new();
        bump(&mut counts, get("👍🏽"));
        bump(&mut counts, get("👍"));
        assert_eq!(counts.get("👍"), Some(&2));
        assert_eq!(count(&counts, get("👍🏿")), 2);
        assert_eq!(with_tone(get("👍"), 3).as_str(), "👍🏽");
        assert_eq!(with_tone(get("👍🏽"), 0).as_str(), "👍");
        assert_eq!(with_tone(get("🍌"), 3).as_str(), "🍌");
        // Toned glyphs the font lacks stay yellow, without the + mark.
        assert_eq!(with_tone(get("🫹"), 3).as_str(), "🫹");
        assert!(!has_tones(get("🫹")));
        assert!(has_tones(get("👍")));
        assert_eq!(cycle_tone(0, -1), 5);
        assert_eq!(cycle_tone(5, 1), 0);
        assert_eq!(cycle_tone(2, 1), 3);
    }

    #[test]
    fn rows_move_across_headers_keeping_the_column() {
        let lines = vec![Line::Header("A"), row(0..3), Line::Header("B"), row(3..9), row(9..11)];
        assert_eq!(line_of(&lines, 0), Some(1));
        assert_eq!(line_of(&lines, 10), Some(4));
        assert_eq!(line_of(&lines, 11), None);
        assert_eq!(vertical(&lines, 1, 1), 4);
        assert_eq!(vertical(&lines, 8, -1), 2);
        assert_eq!(vertical(&lines, 8, 1), 10);
        assert_eq!(vertical(&lines, 9, 1), 9);
        assert_eq!(vertical(&lines, 2, -1), 2);
        assert_eq!(sideways(0, -1, 11), 0);
        assert_eq!(sideways(2, 1, 11), 3);
        assert_eq!(sideways(10, 1, 11), 10);
    }

    #[test]
    fn the_view_follows_the_selection_with_its_header() {
        let lines = vec![Line::Header("A"), row(0..6), row(6..12), Line::Header("B"), row(12..18), row(18..24)];
        let rows = |n: usize| n as f32 * (TILE_HEIGHT + GAP);
        let header = HEADER_HEIGHT + GAP;
        // Header, row, row fit; the second header's row does not.
        let height = header + rows(2) + 1.0;
        assert_eq!(first_line(&lines, 1, 0, height), 0);
        assert_eq!(first_line(&lines, 4, 0, height), 2);
        assert_eq!(first_line(&lines, 5, 2, height), 3);
        // Back up to a section's first row: its header shows too.
        assert_eq!(first_line(&lines, 4, 5, height), 3);
        assert_eq!(first_line(&lines, 1, 4, height), 0);
        assert_eq!(scroll(&lines, 0, 1, height), 1);
        assert_eq!(scroll(&lines, 0, 99, height), 3);
        assert_eq!(scroll(&lines, 2, -5, height), 0);
    }

    #[test]
    fn the_state_round_trips() {
        let picks = counts(&[("🍌", 42), ("👍", 3)]);
        let (counts, tone) = parse_state(&state_json(&picks, 3));
        assert_eq!(counts, picks);
        assert_eq!(tone, 3);
        assert_eq!(parse_state(&serde_json::json!({ "tone": 9, "counts": { "🍌": "x" } })), (HashMap::new(), 0));
        assert_eq!(parse_state(&serde_json::Value::Null), (HashMap::new(), 0));
    }
}

