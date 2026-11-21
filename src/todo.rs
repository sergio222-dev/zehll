use std::path::PathBuf;

/// One task of ii's todo list (`~/.local/state/quickshell/user/todo.json`).
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub content: String,
    pub done: bool,
}

/// The item shown at `index` when the user acted on it; the file may have changed since,
/// so fall back to the first item with the same content.
fn position(items: &[Item], index: usize, content: &str) -> Option<usize> {
    match items.get(index) {
        Some(item) if item.content == content => Some(index),
        _ => items.iter().position(|item| item.content == content),
    }
}

pub fn toggle(items: &mut [Item], index: usize, content: &str) {
    if let Some(index) = position(items, index, content) {
        items[index].done = !items[index].done;
    }
}

pub fn remove(items: &mut Vec<Item>, index: usize, content: &str) {
    if let Some(index) = position(items, index, content) {
        items.remove(index);
    }
}

/// Reads the list from disk. ii may edit the same file, so every change starts from a fresh read.
pub fn load() -> Vec<Item> {
    std::fs::read_to_string(path()).map(|json| parse(&json)).unwrap_or_default()
}

pub fn save(items: &[Item]) {
    let path = path();
    let result = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, to_json(items)));
    if let Err(err) = result {
        eprintln!("zehll: cannot save {}: {err}", path.display());
    }
}

fn parse(json: &str) -> Vec<Item> {
    let items: Vec<serde_json::Value> = serde_json::from_str(json).unwrap_or_default();
    items
        .iter()
        .filter_map(|item| {
            let content = item["content"].as_str()?.to_owned();
            Some(Item { content, done: item["done"].as_bool().unwrap_or(false) })
        })
        .collect()
}

fn to_json(items: &[Item]) -> String {
    let items: Vec<serde_json::Value> =
        items.iter().map(|item| serde_json::json!({ "content": item.content, "done": item.done })).collect();
    serde_json::Value::Array(items).to_string()
}

fn path() -> PathBuf {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    state.join("quickshell/user/todo.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(content: &str, done: bool) -> Item {
        Item { content: content.to_owned(), done }
    }

    #[test]
    fn reads_and_writes_ii_format() {
        let items = parse(r#"[{"content":"buy milk","done":false},{"content":"ship bar","done":true}]"#);
        assert_eq!(items, vec![item("buy milk", false), item("ship bar", true)]);
        assert_eq!(to_json(&items), r#"[{"content":"buy milk","done":false},{"content":"ship bar","done":true}]"#);
        assert!(parse("not json").is_empty());
    }

    #[test]
    fn toggles_by_index_or_content() {
        let mut items = vec![item("a", false), item("b", false)];
        toggle(&mut items, 1, "b");
        assert!(items[1].done);
        // Another program inserted an item in front, so index 0 now holds "new".
        let mut items = vec![item("new", false), item("a", false)];
        toggle(&mut items, 0, "a");
        assert_eq!(items, vec![item("new", false), item("a", true)]);
    }

    #[test]
    fn removes_matching_item() {
        let mut items = vec![item("a", false), item("b", true)];
        remove(&mut items, 1, "b");
        assert_eq!(items, vec![item("a", false)]);
        remove(&mut items, 0, "missing");
        assert_eq!(items, vec![item("a", false)]);
    }
}
