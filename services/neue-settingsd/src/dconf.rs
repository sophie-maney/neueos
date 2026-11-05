//! dconf keys derived from `appearance.toml` (`org.gnome.desktop.*`, read by GTK, libadwaita,
//! Qt's GNOME platform theme and xdg-desktop-portal-gtk), and the changeset that
//! `ca.desrt.dconf.Writer.Change` takes (ADR 0074 §5).
//!
//! A changeset is a GVariant of type `a{smv}` (path → new value, or nothing to reset) in
//! GVariant's own serialisation, little-endian, sent as `ay`. It is encoded here by hand for
//! the few value types we need; see GVariant's serialisation spec (framing offsets, padding)
//! and `dconf_changeset_serialise`.

use neue_config::AppearanceConfig;
use neue_config::appearance::{Accent, Contrast, Mode};

/// A dconf value of the types our keys use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// `b`
    Bool(bool),
    /// `i`
    Int32(i32),
    /// `s`
    Str(String),
}

/// The dconf keys for `appearance`.
pub fn keys(appearance: &AppearanceConfig) -> Vec<(&'static str, Value)> {
    let fonts = &appearance.fonts;
    let color_scheme = match appearance.mode {
        Mode::Light => "default",
        Mode::Dark => "prefer-dark",
    };
    // GNOME 47's accent names; graphite is closest to slate.
    let accent = match appearance.accent {
        Accent::Blue => "blue",
        Accent::Graphite => "slate",
    };
    let size = i32::try_from(appearance.cursor.size).unwrap_or(24);
    vec![
        (
            "/org/gnome/desktop/interface/color-scheme",
            Value::Str(color_scheme.into()),
        ),
        (
            "/org/gnome/desktop/interface/accent-color",
            Value::Str(accent.into()),
        ),
        (
            "/org/gnome/desktop/a11y/interface/high-contrast",
            Value::Bool(appearance.contrast == Contrast::Increased),
        ),
        (
            "/org/gnome/desktop/interface/enable-animations",
            Value::Bool(!appearance.reduced_motion),
        ),
        (
            "/org/gnome/desktop/interface/font-name",
            Value::Str(format!("{} {}", fonts.ui, fonts.ui_size)),
        ),
        (
            "/org/gnome/desktop/interface/monospace-font-name",
            Value::Str(format!("{} {}", fonts.monospace, fonts.monospace_size)),
        ),
        (
            "/org/gnome/desktop/interface/cursor-theme",
            Value::Str(appearance.cursor.theme.clone()),
        ),
        (
            "/org/gnome/desktop/interface/cursor-size",
            Value::Int32(size),
        ),
    ]
}

/// Encode `entries` (path, value or `None` to reset) as the `a{smv}` changeset.
pub fn changeset(entries: &[(&str, Option<&Value>)]) -> Vec<u8> {
    let elements: Vec<Vec<u8>> = entries
        .iter()
        .map(|(path, value)| dict_entry(path, *value))
        .collect();
    // An array of variable-size elements: each one aligned (dict entries holding a `v` align
    // to 8), then the end offset of every element.
    let mut body = Vec::new();
    let mut ends = Vec::with_capacity(elements.len());
    for element in elements {
        pad(&mut body, 8);
        body.extend_from_slice(&element);
        ends.push(body.len());
    }
    with_offsets(body, &ends)
}

/// `{smv}`: the key, padding to 8, the maybe-variant, then the key's end offset (the
/// framing offset of the one non-last variable-size member).
fn dict_entry(path: &str, value: Option<&Value>) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(path.as_bytes());
    body.push(0);
    let key_end = body.len();
    pad(&mut body, 8);
    if let Some(value) = value {
        // `mv` holding a value: the variant, then one zero byte (Just of a variable-size type).
        body.extend_from_slice(&variant(value));
        body.push(0);
    }
    with_offsets(body, &[key_end])
}

/// `v`: the value, a zero byte, the value's type string.
fn variant(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    let signature = match value {
        Value::Bool(b) => {
            out.push(u8::from(*b));
            "b"
        }
        Value::Int32(i) => {
            out.extend_from_slice(&i.to_le_bytes());
            "i"
        }
        Value::Str(s) => {
            out.extend_from_slice(s.as_bytes());
            out.push(0);
            "s"
        }
    };
    out.push(0);
    out.extend_from_slice(signature.as_bytes());
    out
}

fn pad(buf: &mut Vec<u8>, align: usize) {
    while !buf.len().is_multiple_of(align) {
        buf.push(0);
    }
}

/// `body` followed by `offsets`, each in the smallest size that can address the whole
/// container (offsets included), as GVariant's normal form requires.
fn with_offsets(mut body: Vec<u8>, offsets: &[usize]) -> Vec<u8> {
    if offsets.is_empty() {
        return body;
    }
    let size = [1usize, 2, 4, 8]
        .into_iter()
        .find(|&size| {
            let total = body.len() + size * offsets.len();
            size == 8 || total < (1usize << (8 * size))
        })
        .unwrap_or(8);
    for &offset in offsets {
        body.extend_from_slice(&offset.to_le_bytes()[..size]);
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_bool_entry() {
        // "/a/b\0", 3 bytes padding, <true> as Just: 01 00 'b' 00, then key end 5; the array
        // adds the element's end, 13.
        let blob = changeset(&[("/a/b", Some(&Value::Bool(true)))]);
        assert_eq!(
            blob,
            [b'/', b'a', b'/', b'b', 0, 0, 0, 0, 1, 0, b'b', 0, 5, 13]
        );
    }

    #[test]
    fn two_entries_are_aligned_and_framed() {
        let a = Value::Str("x".into());
        let b = Value::Int32(-1);
        let blob = changeset(&[("/k", Some(&a)), ("/j", Some(&b))]);
        // Entry 1: "/k\0" + 5 pad + "x\0" 00 's' + 00 (Just) + offset 3 = 14 bytes.
        // Pad to 16, entry 2: "/j\0" + 5 pad + ff ff ff ff 00 'i' + 00 + offset 3 = 16 bytes.
        let mut expected = vec![b'/', b'k', 0, 0, 0, 0, 0, 0, b'x', 0, 0, b's', 0, 3];
        expected.extend([0, 0]);
        expected.extend([
            b'/', b'j', 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff, 0, b'i', 0, 3,
        ]);
        expected.extend([14, 32]);
        assert_eq!(blob, expected);
    }

    #[test]
    fn a_reset_is_nothing() {
        assert_eq!(
            changeset(&[("/a", None)]),
            [b'/', b'a', 0, 0, 0, 0, 0, 0, 3, 9]
        );
    }

    #[test]
    fn large_containers_use_wider_offsets() {
        let long = Value::Str("x".repeat(300));
        let blob = changeset(&[("/a", Some(&long))]);
        // The entry is over 255 bytes, so its offset (3) and the array's take two bytes.
        let entry_end = blob.len() - 2;
        assert_eq!(&blob[entry_end - 2..entry_end], &[3, 0]);
        assert_eq!(
            usize::from(u16::from_le_bytes([blob[entry_end], blob[entry_end + 1]])),
            entry_end
        );
    }

    #[test]
    fn dark_mode_maps_to_prefer_dark() {
        let appearance = AppearanceConfig {
            mode: Mode::Dark,
            ..AppearanceConfig::default()
        };
        let keys = keys(&appearance);
        assert!(keys.contains(&(
            "/org/gnome/desktop/interface/color-scheme",
            Value::Str("prefer-dark".into())
        )));
        assert!(keys.contains(&("/org/gnome/desktop/interface/cursor-size", Value::Int32(24))));
    }
}
