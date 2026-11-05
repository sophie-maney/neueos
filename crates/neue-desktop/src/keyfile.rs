//! The key file format of desktop entries and `index.theme`: `[Group]` headers and
//! `Key[locale]=value` lines, `#` comments.

use std::env;

/// A locale for picking translated values: `lang_COUNTRY@MODIFIER` (the encoding is
/// dropped).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Locale {
    lang: String,
    country: Option<String>,
    modifier: Option<String>,
}

impl Locale {
    /// No locale: only the untranslated values match.
    pub fn none() -> Self {
        Self::default()
    }

    /// Parses `de_DE.UTF-8@euro`. `C`, `POSIX` and the empty string are no locale.
    pub fn parse(text: &str) -> Self {
        let text = text.trim();
        let (rest, modifier) = match text.split_once('@') {
            Some((rest, modifier)) => (rest, Some(modifier)),
            None => (text, None),
        };
        let rest = rest.split_once('.').map_or(rest, |(rest, _encoding)| rest);
        let (lang, country) = match rest.split_once('_') {
            Some((lang, country)) => (lang, Some(country)),
            None => (rest, None),
        };
        if lang.is_empty() || lang == "C" || lang == "POSIX" {
            return Self::none();
        }
        Self {
            lang: lang.to_owned(),
            country: country.filter(|c| !c.is_empty()).map(str::to_owned),
            modifier: modifier.filter(|m| !m.is_empty()).map(str::to_owned),
        }
    }

    /// From `LC_ALL`, `LC_MESSAGES` or `LANG`, whichever is set first.
    pub fn from_env() -> Self {
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|name| env::var(name).ok())
            .find(|v| !v.is_empty())
            .map_or_else(Self::none, |v| Self::parse(&v))
    }

    /// The keys' locale suffixes to try, best match first: `lang_COUNTRY@MODIFIER`,
    /// `lang_COUNTRY`, `lang@MODIFIER`, `lang`.
    fn candidates(&self) -> Vec<String> {
        if self.lang.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        if let (Some(country), Some(modifier)) = (&self.country, &self.modifier) {
            out.push(format!("{}_{country}@{modifier}", self.lang));
        }
        if let Some(country) = &self.country {
            out.push(format!("{}_{country}", self.lang));
        }
        if let Some(modifier) = &self.modifier {
            out.push(format!("{}@{modifier}", self.lang));
        }
        out.push(self.lang.clone());
        out
    }
}

/// One `[Group]` with its keys in file order.
#[derive(Debug, Default)]
pub(crate) struct Group {
    pub(crate) name: String,
    /// `(key, locale, raw value)`.
    entries: Vec<(String, Option<String>, String)>,
}

impl Group {
    /// The untranslated value of `key`, raw (escapes not resolved). The first one wins.
    pub(crate) fn raw(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, locale, _)| locale.is_none() && k == key)
            .map(|(_, _, v)| v.as_str())
    }

    /// A string value with escapes resolved.
    pub(crate) fn string(&self, key: &str) -> Option<String> {
        self.raw(key).map(unescape)
    }

    /// A boolean: `true` or `false`; anything else is `None`.
    pub(crate) fn boolean(&self, key: &str) -> Option<bool> {
        match self.raw(key)?.trim() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }

    /// An integer, if it parses.
    pub(crate) fn int(&self, key: &str) -> Option<i64> {
        self.raw(key)?.trim().parse().ok()
    }

    /// A `;`-separated list (`\;` is a literal semicolon); empty items are dropped.
    pub(crate) fn list(&self, key: &str) -> Vec<String> {
        self.raw(key).map(split_list).unwrap_or_default()
    }

    /// A comma-separated list (`index.theme` uses commas, desktop entries semicolons).
    pub(crate) fn comma_list(&self, key: &str) -> Vec<String> {
        self.raw(key)
            .map(|v| {
                v.split(',')
                    .map(|i| i.trim().to_owned())
                    .filter(|i| !i.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The value of `key` for `locale`: the best translation, else the plain value.
    pub(crate) fn localized(&self, key: &str, locale: &Locale) -> Option<String> {
        self.localized_raw(key, locale).map(unescape)
    }

    /// Like [`Group::localized`], escapes not resolved (for lists).
    pub(crate) fn localized_raw(&self, key: &str, locale: &Locale) -> Option<&str> {
        for candidate in locale.candidates() {
            let found = self
                .entries
                .iter()
                .find(|(k, l, _)| k == key && l.as_deref() == Some(candidate.as_str()));
            if let Some((_, _, v)) = found {
                return Some(v);
            }
        }
        self.raw(key)
    }

    /// A list that may be translated (`Keywords`).
    pub(crate) fn localized_list(&self, key: &str, locale: &Locale) -> Vec<String> {
        self.localized_raw(key, locale)
            .map(split_list)
            .unwrap_or_default()
    }
}

/// A parsed file: its groups in file order.
#[derive(Debug, Default)]
pub(crate) struct KeyFile {
    pub(crate) groups: Vec<Group>,
}

impl KeyFile {
    /// Lenient: bad lines are skipped, a repeated group name is a second group (lookups use
    /// the first).
    pub(crate) fn parse(text: &str) -> Self {
        let mut file = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                file.groups.push(Group {
                    name: name.to_owned(),
                    entries: Vec::new(),
                });
                continue;
            }
            let (Some(group), Some((key, value))) = (file.groups.last_mut(), line.split_once('='))
            else {
                continue;
            };
            let key = key.trim_end();
            let (key, locale) = match key.split_once('[') {
                Some((key, rest)) => match rest.strip_suffix(']') {
                    Some(locale) => (key, Some(locale.to_owned())),
                    None => continue,
                },
                None => (key, None),
            };
            group
                .entries
                .push((key.to_owned(), locale, value.trim_start().to_owned()));
        }
        file
    }

    pub(crate) fn group(&self, name: &str) -> Option<&Group> {
        self.groups.iter().find(|g| g.name == name)
    }
}

/// Resolves `\s \n \t \r \\` (and `\;`, kept as `;`). Other backslashes stay.
fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(';') => out.push(';'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn split_list(raw: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(';') => current.push(';'),
                Some(other) => {
                    current.push('\\');
                    current.push(other);
                }
                None => current.push('\\'),
            },
            ';' => items.push(std::mem::take(&mut current)),
            c => current.push(c),
        }
    }
    items.push(current);
    items
        .into_iter()
        .map(|i| unescape(&i))
        .filter(|i| !i.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "# comment\n[Desktop Entry]\nName=Files\nName[de]=Dateien\nName[de_AT]=Dateien AT\n\
        Name[sr@latin]=Datoteke\nComment=Line\\nTwo\nCategories=A;B\\;C;;\nHidden=true\n[Other]\nX=1\n";

    #[test]
    fn locales_parse_and_fall_back() {
        let file = KeyFile::parse(TEXT);
        let g = file.group("Desktop Entry").unwrap();
        assert_eq!(g.localized("Name", &Locale::none()).unwrap(), "Files");
        assert_eq!(
            g.localized("Name", &Locale::parse("de_DE.UTF-8")).unwrap(),
            "Dateien"
        );
        assert_eq!(
            g.localized("Name", &Locale::parse("de_AT")).unwrap(),
            "Dateien AT"
        );
        assert_eq!(
            g.localized("Name", &Locale::parse("sr@latin")).unwrap(),
            "Datoteke"
        );
        assert_eq!(g.localized("Name", &Locale::parse("fr")).unwrap(), "Files");
        assert_eq!(Locale::parse("C"), Locale::none());
        assert_eq!(Locale::parse("POSIX.UTF-8"), Locale::none());
    }

    #[test]
    fn values_unescape_and_lists_split() {
        let file = KeyFile::parse(TEXT);
        let g = file.group("Desktop Entry").unwrap();
        assert_eq!(g.string("Comment").unwrap(), "Line\nTwo");
        assert_eq!(g.list("Categories"), ["A", "B;C"]);
        assert_eq!(g.boolean("Hidden"), Some(true));
        assert_eq!(g.boolean("Name"), None);
        assert_eq!(file.group("Other").unwrap().string("X").unwrap(), "1");
        assert!(file.group("Missing").is_none());
    }
}
