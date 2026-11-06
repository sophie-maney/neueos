//! Searching menus (task 3.9, ADR 0082): every item of the menus being shown, flattened
//! with the path to it, and a query that finds some of them. Portable: the Linux-only
//! shell turns the app's menus into [`Entry`]s, the fallback menu makes its own.

use neue_kit::Shortcut;

/// The most hits the panel shows.
pub const MAX_HITS: usize = 8;

/// What the path's parts are joined with when shown.
pub const SEPARATOR: &str = " ▸ ";

/// One item that can be chosen, `target` saying what choosing it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry<T> {
    /// The menu titles and submenus down to the item, which is last (`File`, `Save`).
    pub path: Vec<String>,
    pub shortcut: Option<Shortcut>,
    pub target: T,
}

impl<T> Entry<T> {
    /// `File ▸ Save`, with the shortcut after it.
    pub fn text(&self) -> String {
        let path = self.path.join(SEPARATOR);
        self.shortcut
            .as_ref()
            .map_or(path.clone(), |keys| format!("{path}   {}", keys.label()))
    }

    fn label(&self) -> &str {
        self.path.last().map_or("", String::as_str)
    }
}

/// How well `entry` answers the words of `query`: lower is better, `None` is a miss. Every
/// word must be in the path; a label that starts with the first word beats one that
/// starts a word with it, and that beats the word being elsewhere in the label or only in
/// the menus above it.
fn rank<T>(entry: &Entry<T>, words: &[String]) -> Option<u8> {
    let label = entry.label().to_lowercase();
    let path = entry.path.join(SEPARATOR).to_lowercase();
    if !words.iter().all(|w| path.contains(w.as_str())) {
        return None;
    }
    let first = words.first()?;
    Some(if label.starts_with(first.as_str()) {
        0
    } else if label
        .split(|c: char| !c.is_alphanumeric())
        .any(|part| part.starts_with(first.as_str()))
    {
        1
    } else if label.contains(first.as_str()) {
        2
    } else {
        3
    })
}

/// The entries answering `query` (case does not matter, words may be in any order), best
/// first and at most [`MAX_HITS`]; in menu order among equals. Nothing for an empty query.
pub fn search<'a, T>(entries: &'a [Entry<T>], query: &str) -> Vec<&'a Entry<T>> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(u8, &Entry<T>)> = entries
        .iter()
        .filter_map(|e| Some((rank(e, &words)?, e)))
        .collect();
    // Stable: equals stay in menu order.
    hits.sort_by_key(|(rank, _)| *rank);
    hits.into_iter().map(|(_, e)| e).take(MAX_HITS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &[&str], target: u32) -> Entry<u32> {
        Entry {
            path: path.iter().map(|p| (*p).to_owned()).collect(),
            shortcut: None,
            target,
        }
    }

    fn menus() -> Vec<Entry<u32>> {
        vec![
            entry(&["File", "Save"], 1),
            entry(&["File", "Save As…"], 2),
            entry(&["Edit", "Select All"], 3),
            entry(&["View", "Show Saved Searches"], 4),
            entry(&["Pages", "Values"], 5),
        ]
    }

    fn found(query: &str) -> Vec<u32> {
        search(&menus(), query).iter().map(|e| e.target).collect()
    }

    #[test]
    fn it_finds_items_whatever_the_case() {
        assert_eq!(found("values"), [5]);
        assert_eq!(found("VALUES"), [5]);
        assert_eq!(found("nothing like it"), Vec::<u32>::new());
    }

    #[test]
    fn nothing_is_found_for_an_empty_query() {
        assert!(found("").is_empty());
        assert!(found("   ").is_empty());
    }

    #[test]
    fn labels_that_start_with_the_query_come_first() {
        // "Save" and "Save As…" start with it, "Show Saved Searches" only has a word that does.
        assert_eq!(found("sav"), [1, 2, 4]);
        assert_eq!(found("all"), [3]);
    }

    #[test]
    fn every_word_must_match_and_the_menu_counts() {
        assert_eq!(found("file save"), [1, 2]);
        assert_eq!(found("as file"), [2]);
        assert_eq!(found("edit save"), Vec::<u32>::new());
    }

    #[test]
    fn it_shows_at_most_a_few_hits() {
        let many: Vec<_> = (0..20).map(|i| entry(&["Go", "Item"], i)).collect();
        assert_eq!(search(&many, "item").len(), MAX_HITS);
    }

    #[test]
    fn an_entry_reads_as_its_path_and_shortcut() {
        let mut save = entry(&["File", "Save"], 0);
        assert_eq!(save.text(), "File ▸ Save");
        save.shortcut = Some(Shortcut::command("S"));
        assert!(save.text().starts_with("File ▸ Save   "));
        assert!(save.text().ends_with('S'));
    }
}
