//! The launcher's Wallhaven page: wallhaven.cc's search API, its filters, thumbnails and
//! downloads. The page and its filters work like bnsplits/dotfiles' Wallhaven browser.

use std::path::{Path, PathBuf};

use iced::futures::{SinkExt, Stream};
use iced::stream;

pub const COLUMNS: usize = 3;
pub const ROWS: usize = 3;
const API: &str = "https://wallhaven.cc/api/v1/search";
/// Thumbnails downloaded at the same time.
const THUMB_BATCH: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sorting {
    DateAdded,
    Relevance,
    Random,
    Views,
    Favorites,
    Toplist,
}

impl Sorting {
    pub const ALL: [Sorting; 6] = [Sorting::DateAdded, Sorting::Relevance, Sorting::Random, Sorting::Views, Sorting::Favorites, Sorting::Toplist];

    /// The API's name, also kept in the state file.
    pub fn key(self) -> &'static str {
        match self {
            Sorting::DateAdded => "date_added",
            Sorting::Relevance => "relevance",
            Sorting::Random => "random",
            Sorting::Views => "views",
            Sorting::Favorites => "favorites",
            Sorting::Toplist => "toplist",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Sorting::DateAdded => "DATE ADDED",
            Sorting::Relevance => "RELEVANCE",
            Sorting::Random => "RANDOM",
            Sorting::Views => "VIEWS",
            Sorting::Favorites => "FAVORITES",
            Sorting::Toplist => "TOPLIST",
        }
    }

    pub fn next(self) -> Sorting {
        let at = Sorting::ALL.iter().position(|sorting| *sorting == self).unwrap_or(0);
        Sorting::ALL[(at + 1) % Sorting::ALL.len()]
    }
}

/// What the search asks for, kept in `~/.local/state/zehll/wallpapers.json` under `wallhaven`.
#[derive(Debug, Clone, PartialEq)]
pub struct Filters {
    /// General, Anime, People.
    pub categories: [bool; 3],
    /// SFW, Sketchy, NSFW. The last two need an API key.
    pub purity: [bool; 3],
    pub sorting: Sorting,
    pub descending: bool,
    pub apikey: Option<String>,
}

impl Default for Filters {
    fn default() -> Self {
        Self { categories: [true, true, false], purity: [true, false, false], sorting: Sorting::DateAdded, descending: true, apikey: None }
    }
}

impl Filters {
    pub fn parse(json: &serde_json::Value) -> Self {
        let defaults = Self::default();
        let flags = |key: &str, fallback: [bool; 3]| {
            let text = json[key].as_str().unwrap_or("");
            let flags: Vec<bool> = text.chars().map(|c| c == '1').collect();
            match (text.len() == 3 && text.chars().all(|c| c == '0' || c == '1'), flags.as_slice()) {
                (true, &[a, b, c]) if a || b || c => [a, b, c],
                _ => fallback,
            }
        };
        let sorting = json["sorting"].as_str().and_then(|key| Sorting::ALL.into_iter().find(|sorting| sorting.key() == key));
        Self {
            categories: flags("categories", defaults.categories),
            purity: flags("purity", defaults.purity),
            sorting: sorting.unwrap_or(defaults.sorting),
            descending: json["order"].as_str() != Some("asc"),
            apikey: json["apikey"].as_str().map(str::trim).filter(|key| !key.is_empty()).map(str::to_owned),
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        let mut json = serde_json::json!({
            "categories": bits(self.categories),
            "purity": bits(self.purity),
            "sorting": self.sorting.key(),
            "order": if self.descending { "desc" } else { "asc" },
        });
        if let Some(key) = &self.apikey {
            json["apikey"] = key.as_str().into();
        }
        json
    }

    /// Toggles a category, keeping at least one on.
    pub fn toggle_category(&mut self, index: usize) {
        toggle_keeping_one(&mut self.categories, index);
    }

    /// Toggles a purity, keeping at least one on; Sketchy and NSFW need an API key.
    pub fn toggle_purity(&mut self, index: usize) {
        if index > 0 && self.apikey.is_none() {
            return;
        }
        toggle_keeping_one(&mut self.purity, index);
    }
}

fn toggle_keeping_one(flags: &mut [bool; 3], index: usize) {
    if index < flags.len() && (!flags[index] || flags.iter().filter(|on| **on).count() > 1) {
        flags[index] = !flags[index];
    }
}

fn bits(flags: [bool; 3]) -> String {
    flags.iter().map(|on| if *on { '1' } else { '0' }).collect()
}

/// Percent-encodes a query value.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (byte as char).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

impl Filters {
    /// Whether Sketchy and NSFW can be picked.
    pub fn has_key(&self) -> bool {
        self.apikey.is_some()
    }
}

/// One search result.
#[derive(Debug, Clone, PartialEq)]
pub struct Wallpaper {
    pub id: String,
    pub size: Option<(u32, u32)>,
    /// The full image.
    pub url: String,
    pub thumb: String,
}

impl Wallpaper {
    /// Its file name as wallhaven names it, `wallhaven-<id>.<ext>`.
    pub fn file_name(&self) -> String {
        let extension = self.url.rsplit('.').next().filter(|extension| extension.len() <= 4 && !extension.contains('/')).unwrap_or("jpg");
        format!("wallhaven-{}.{extension}", self.id)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Results {
    pub wallpapers: Vec<Wallpaper>,
    pub page: u32,
    pub last_page: u32,
}

/// The search URL for `query` on `page`.
pub fn search_url(query: &str, filters: &Filters, page: u32) -> String {
    let order = if filters.descending { "desc" } else { "asc" };
    let mut url = format!(
        "{API}?q={}&categories={}&purity={}&sorting={}&order={order}&page={page}",
        encode(query.trim()),
        bits(filters.categories),
        bits(filters.purity),
        filters.sorting.key()
    );
    if filters.sorting == Sorting::Toplist {
        url.push_str("&topRange=1M");
    }
    if let Some(key) = &filters.apikey {
        url.push_str(&format!("&apikey={}", encode(key)));
    }
    url
}

/// The API's answer, or its error message.
pub fn parse_results(json: &serde_json::Value) -> Result<Results, String> {
    let Some(data) = json["data"].as_array() else {
        return Err(json["error"].as_str().unwrap_or("Unexpected answer from wallhaven").to_owned());
    };
    let wallpapers = data
        .iter()
        .filter_map(|item| {
            let size = item["resolution"].as_str().and_then(|text| text.split_once('x')).and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)));
            Some(Wallpaper {
                id: item["id"].as_str()?.to_owned(),
                size,
                url: item["path"].as_str()?.to_owned(),
                thumb: item["thumbs"]["large"].as_str()?.to_owned(),
            })
        })
        .collect();
    let page = |key: &str| json["meta"][key].as_u64().unwrap_or(1) as u32;
    Ok(Results { wallpapers, page: page("current_page"), last_page: page("last_page") })
}

/// The page's state, kept in the launcher between openings.
#[derive(Debug, Default)]
pub struct Browser {
    pub filters: Filters,
    /// Results of every page loaded so far.
    pub results: Vec<Wallpaper>,
    /// Downloaded thumbnails by id; `None` when the download failed.
    pub thumbs: std::collections::HashMap<String, Option<PathBuf>>,
    pub page: u32,
    pub last_page: u32,
    /// A page is being fetched.
    pub loading: bool,
    pub error: Option<String>,
    pub first_row: usize,
    /// Latest search asked for; older answers arriving late are dropped.
    request: u64,
    /// Id of the wallpaper being downloaded to apply.
    pub applying: Option<String>,
    /// The search field holds the API key being typed, and this the search it replaced.
    pub editing_key: Option<String>,
}

impl Browser {
    /// Starts a new search: drops the results and returns the request's number.
    pub fn begin(&mut self) -> u64 {
        self.request += 1;
        self.results.clear();
        self.page = 0;
        self.last_page = 0;
        self.first_row = 0;
        self.loading = true;
        self.error = None;
        self.request
    }

    /// Starts loading the page after the last one, if there is one and nothing is loading.
    pub fn begin_more(&mut self) -> Option<(u64, u32)> {
        if self.loading || self.page == 0 || self.page >= self.last_page {
            return None;
        }
        self.request += 1;
        self.loading = true;
        Some((self.request, self.page + 1))
    }

    /// Keeps the answer to `request` and returns whether it was kept.
    pub fn accept(&mut self, request: u64, results: Result<Results, String>) -> bool {
        if request != self.request {
            return false;
        }
        self.loading = false;
        match results {
            Ok(results) => {
                self.results.extend(results.wallpapers);
                self.page = results.page;
                self.last_page = results.last_page;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        true
    }

    /// Whether the selection is close enough to the end to load the next page.
    pub fn near_end(&self, selected: usize) -> bool {
        self.page < self.last_page && selected + COLUMNS * ROWS >= self.results.len()
    }
}

fn cache_dir() -> PathBuf {
    let home = || PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".cache")).join("zehll/wallhaven")
}

/// Downloads `url` to `file` (through a partial file), unless it is there already.
async fn download(url: &str, file: &Path) -> Result<(), String> {
    if file.exists() {
        return Ok(());
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    }
    let partial = file.with_extension("part");
    let output = tokio::process::Command::new("curl")
        .args(["-fsSL", "--retry", "2", "--max-time", "120", "-o"])
        .arg(&partial)
        .arg(url)
        .output()
        .await
        .map_err(|err| format!("cannot run curl: {err}"))?;
    if !output.status.success() {
        let _ = std::fs::remove_file(&partial);
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    std::fs::rename(&partial, file).map_err(|err| err.to_string())
}

/// One page of results.
pub async fn search(url: String) -> Result<Results, String> {
    let output = tokio::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "20", &url])
        .output()
        .await
        .map_err(|err| format!("cannot run curl: {err}"))?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(match error.contains("401") {
            true => String::from("The API key was refused"),
            false if error.contains("429") => String::from("Too many requests, try again in a minute"),
            false => error,
        });
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|err| err.to_string())?;
    parse_results(&json)
}

/// Thumbnails of `wallpapers`, a few at a time, each sent with its id as soon as it is
/// downloaded (`None` when it failed).
pub fn thumbnails(wallpapers: Vec<Wallpaper>) -> impl Stream<Item = (String, Option<PathBuf>)> {
    stream::channel(THUMB_BATCH, async move |mut output| {
        for batch in wallpapers.chunks(THUMB_BATCH) {
            let fetched = batch.iter().cloned().map(|wallpaper| async move {
                let file = cache_dir().join("thumbs").join(format!("{}.jpg", wallpaper.id));
                let result = download(&wallpaper.thumb, &file).await;
                (wallpaper.id, result.ok().map(|()| file))
            });
            for result in iced::futures::future::join_all(fetched).await {
                if output.send(result).await.is_err() {
                    return;
                }
            }
        }
    })
}

/// Downloads the full image, to the wallpaper folder when `keep`, else to Zehll's cache.
pub async fn fetch(wallpaper: Wallpaper, folder: PathBuf, keep: bool) -> Result<PathBuf, String> {
    let dir = if keep { folder } else { cache_dir().join("full") };
    let file = dir.join(wallpaper.file_name());
    download(&wallpaper.url, &file).await.map(|()| file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn filters_round_trip_and_default_to_general_anime_sfw() {
        let filters = Filters::default();
        assert_eq!(filters.categories, [true, true, false]);
        assert_eq!(filters.purity, [true, false, false]);
        assert_eq!(Filters::parse(&json!(null)), filters);
        let custom = Filters { categories: [false, true, true], purity: [true, true, false], sorting: Sorting::Toplist, descending: false, apikey: Some("k".into()) };
        assert_eq!(Filters::parse(&custom.to_json()), custom);
        assert_eq!(custom.to_json(), json!({ "categories": "011", "purity": "110", "sorting": "toplist", "order": "asc", "apikey": "k" }));
        // Unknown or malformed values fall back.
        assert_eq!(Filters::parse(&json!({ "categories": "000", "sorting": "nope" })), filters);
    }

    #[test]
    fn toggles_keep_one_on_and_need_a_key_for_sketchy_and_nsfw() {
        let mut filters = Filters::default();
        filters.toggle_category(0);
        assert_eq!(filters.categories, [false, true, false]);
        filters.toggle_category(1);
        assert_eq!(filters.categories, [false, true, false]);
        filters.toggle_purity(1);
        assert_eq!(filters.purity, [true, false, false]);
        filters.toggle_purity(0);
        assert_eq!(filters.purity, [true, false, false]);
        filters.apikey = Some("k".into());
        filters.toggle_purity(2);
        assert_eq!(filters.purity, [true, false, true]);
        filters.toggle_purity(0);
        assert_eq!(filters.purity, [false, false, true]);
    }

    #[test]
    fn the_search_url_carries_the_filters_and_the_key() {
        let mut filters = Filters::default();
        assert_eq!(
            search_url("blue sky", &filters, 2),
            "https://wallhaven.cc/api/v1/search?q=blue%20sky&categories=110&purity=100&sorting=date_added&order=desc&page=2"
        );
        filters.sorting = Sorting::Toplist;
        filters.apikey = Some(String::from("abc"));
        filters.descending = false;
        assert_eq!(
            search_url("", &filters, 1),
            "https://wallhaven.cc/api/v1/search?q=&categories=110&purity=100&sorting=toplist&order=asc&page=1&topRange=1M&apikey=abc"
        );
        assert!(search_url("a&b=c#", &Filters::default(), 1).contains("q=a%26b%3Dc%23&"));
    }

    #[test]
    fn results_are_read_from_the_api_answer() {
        let answer = json!({
            "data": [
                { "id": "d82v73", "resolution": "2501x1407", "path": "https://w.wallhaven.cc/full/d8/wallhaven-d82v73.png",
                  "thumbs": { "large": "https://th.wallhaven.cc/lg/d8/d82v73.jpg", "small": "s" } },
                { "id": "bad" }
            ],
            "meta": { "current_page": 2, "last_page": 9 }
        });
        let results = parse_results(&answer).unwrap();
        assert_eq!((results.page, results.last_page), (2, 9));
        assert_eq!(results.wallpapers.len(), 1);
        let wallpaper = &results.wallpapers[0];
        assert_eq!(wallpaper.size, Some((2501, 1407)));
        assert_eq!(wallpaper.thumb, "https://th.wallhaven.cc/lg/d8/d82v73.jpg");
        assert_eq!(wallpaper.file_name(), "wallhaven-d82v73.png");
        assert_eq!(parse_results(&json!({ "error": "Unauthorized" })), Err(String::from("Unauthorized")));
    }

    fn results(ids: &[&str], page: u32, last_page: u32) -> Results {
        let wallpapers = ids.iter().map(|id| Wallpaper { id: id.to_string(), size: None, url: format!("u/{id}.png"), thumb: format!("t/{id}.jpg") }).collect();
        Results { wallpapers, page, last_page }
    }

    #[test]
    fn pages_append_and_late_answers_are_dropped() {
        let mut browser = Browser::default();
        let first = browser.begin();
        assert!(browser.loading);
        assert_eq!(browser.begin_more(), None);
        assert!(browser.accept(first, Ok(results(&["a", "b"], 1, 3))));
        assert_eq!((browser.page, browser.last_page, browser.loading), (1, 3, false));
        let (more, page) = browser.begin_more().unwrap();
        assert_eq!(page, 2);
        // A new search makes the pending page stale.
        let second = browser.begin();
        assert!(browser.results.is_empty());
        assert!(!browser.accept(more, Ok(results(&["x"], 2, 3))));
        assert!(browser.accept(second, Ok(results(&["c"], 1, 1))));
        assert_eq!(browser.results.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(), ["c"]);
        // The last page loaded: nothing more.
        assert_eq!(browser.begin_more(), None);
        let third = browser.begin();
        assert!(browser.accept(third, Err(String::from("Unauthorized"))));
        assert_eq!(browser.error.as_deref(), Some("Unauthorized"));
        assert!(!browser.loading);
    }

    #[test]
    fn the_next_page_loads_when_the_selection_nears_the_end() {
        let mut browser = Browser::default();
        let request = browser.begin();
        browser.accept(request, Ok(results(&["a"; 24], 1, 5)));
        assert!(!browser.near_end(0));
        assert!(!browser.near_end(14));
        assert!(browser.near_end(15));
        browser.page = 5;
        assert!(!browser.near_end(23));
    }

    #[test]
    fn sorting_cycles() {
        assert_eq!(Sorting::DateAdded.next(), Sorting::Relevance);
        assert_eq!(Sorting::Toplist.next(), Sorting::DateAdded);
    }
}
