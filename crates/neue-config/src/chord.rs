//! Key chords such as `Super+Shift+Q`, the keys of the `[keybindings]` table.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Modifiers plus one key. The key is an xkb keysym name (`Q`, `Return`, `F5`, `space`);
/// the compositor resolves it. Parsing is case-insensitive for modifiers; [`Display`]
/// writes the canonical form (`Super+Ctrl+Alt+Shift+Key`), which is also the form
/// `[keybindings]` keys are rewritten to before layers merge.
///
/// [`Display`]: fmt::Display
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Chord {
    pub logo: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The key name; single ASCII letters are upper case.
    pub key: String,
}

/// A chord that does not parse.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChordError {
    #[error("empty key chord")]
    Empty,
    #[error("`{0}` is not a modifier (use Super, Ctrl, Alt or Shift)")]
    UnknownModifier(String),
    #[error("key chord `{0}` has no key after its modifiers")]
    NoKey(String),
    #[error("modifier `{0}` appears twice")]
    DuplicateModifier(String),
}

impl FromStr for Chord {
    type Err = ChordError;

    fn from_str(text: &str) -> Result<Self, ChordError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(ChordError::Empty);
        }
        let mut parts: Vec<&str> = text.split('+').map(str::trim).collect();
        // `Super++` is not supported: write the keysym name (`plus`).
        let key = parts.pop().filter(|k| !k.is_empty());
        let Some(key) = key else {
            return Err(ChordError::NoKey(text.to_owned()));
        };
        let mut chord = Self {
            logo: false,
            ctrl: false,
            alt: false,
            shift: false,
            key: if key.len() == 1 {
                key.to_ascii_uppercase()
            } else {
                key.to_owned()
            },
        };
        for part in parts {
            let flag = match part.to_ascii_lowercase().as_str() {
                "super" | "logo" | "mod4" => &mut chord.logo,
                "ctrl" | "control" => &mut chord.ctrl,
                "alt" | "mod1" => &mut chord.alt,
                "shift" => &mut chord.shift,
                _ => return Err(ChordError::UnknownModifier(part.to_owned())),
            };
            if std::mem::replace(flag, true) {
                return Err(ChordError::DuplicateModifier(part.to_owned()));
            }
        }
        Ok(chord)
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (set, name) in [
            (self.logo, "Super"),
            (self.ctrl, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
        ] {
            if set {
                write!(f, "{name}+")?;
            }
        }
        f.write_str(&self.key)
    }
}

impl Serialize for Chord {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Chord {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(text: &str) -> Chord {
        text.parse().unwrap()
    }

    #[test]
    fn parses_modifiers_in_any_order_and_case() {
        let c = chord("shift+SUPER+q");
        assert_eq!((c.logo, c.ctrl, c.alt, c.shift), (true, false, false, true));
        assert_eq!(c.key, "Q");
        assert_eq!(c.to_string(), "Super+Shift+Q");
        assert_eq!(
            chord("Mod4+Control+Return").to_string(),
            "Super+Ctrl+Return"
        );
        assert_eq!(chord("F5").to_string(), "F5");
    }

    #[test]
    fn spellings_of_one_chord_are_equal() {
        assert_eq!(chord("Super+Q"), chord("logo + q"));
    }

    #[test]
    fn rejects_bad_chords() {
        assert_eq!("".parse::<Chord>(), Err(ChordError::Empty));
        assert!(matches!(
            "Super+".parse::<Chord>(),
            Err(ChordError::NoKey(_))
        ));
        assert!(matches!(
            "Hyper+Q".parse::<Chord>(),
            Err(ChordError::UnknownModifier(_))
        ));
        assert!(matches!(
            "Super+Logo+Q".parse::<Chord>(),
            Err(ChordError::DuplicateModifier(_))
        ));
    }
}
