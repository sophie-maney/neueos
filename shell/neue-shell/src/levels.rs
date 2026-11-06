//! What the volume and brightness keys do to a level (task 3.14a), and the screen's and
//! the keyboard's backlights in sysfs (task 3.14d). The process applies these and shows the result on the OSD.
//!
//! A key steps a level by one of the OSD's 16 steps, or by a quarter step with Shift+Alt
//! held (as on the Mac). Stepping lands on the grid: a level between two steps goes to
//! the next one in the key's direction.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use neue_kit::desktop::{ChordModifiers, SystemKey};
use neue_kit::tokens::materials::osd::STEPS;

/// Where the kernel lists screen backlights.
pub const BACKLIGHTS: &str = "/sys/class/backlight";
/// Where the kernel lists LEDs, among them keyboard backlights.
pub const LEDS: &str = "/sys/class/leds";

/// Grid points per unit level: 16 steps, or 64 quarter steps when `fine`.
fn grid(fine: bool) -> f64 {
    f64::from(STEPS) * if fine { 4.0 } else { 1.0 }
}

/// Whether `modifiers` ask for quarter steps (Shift+Alt).
pub fn fine(modifiers: ChordModifiers) -> bool {
    modifiers.shift && modifiers.alt
}

/// `level` (0 to 1) one step up or down, on the step grid, clamped to 0..=1.
pub fn step(level: f64, up: bool, fine: bool) -> f64 {
    let grid = grid(fine);
    // Off-grid levels (set elsewhere) snap to the next point in the key's direction; the
    // epsilon keeps a level already on the grid from counting as between two points.
    let at = level.clamp(0.0, 1.0) * grid;
    let next = if up {
        (at + 1e-6).floor() + 1.0
    } else {
        (at - 1e-6).ceil() - 1.0
    };
    (next / grid).clamp(0.0, 1.0)
}

/// The default sink's volume (on PipeWire's cubic scale, task 3.14b) and mute, as the
/// volume keys change them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Volume {
    /// 0 to 1; above 1 when something amplified it.
    pub level: f64,
    pub muted: bool,
}

impl Volume {
    /// Applies a volume key. Up and down unmute and step; Mute toggles. Up leaves an
    /// amplified level (above 1) alone rather than cut it to 1. Returns whether the key
    /// was a volume key.
    pub fn apply(&mut self, key: SystemKey, fine: bool) -> bool {
        match key {
            SystemKey::VolumeUp | SystemKey::VolumeDown => {
                self.muted = false;
                let up = key == SystemKey::VolumeUp;
                if !(up && self.level >= 1.0) {
                    self.level = step(self.level, up, fine);
                }
            }
            SystemKey::Mute => self.muted = !self.muted,
            _ => return false,
        }
        true
    }
}

/// A backlight in sysfs, of the screen or the keyboard: what `Session.SetBrightness` needs
/// to name it, and its range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backlight {
    /// The device's directory name (`intel_backlight`, `amdgpu_bl0`, `acpi_video0`;
    /// `tpacpi::kbd_backlight` for a keyboard).
    pub name: String,
    /// The highest raw brightness.
    pub max: u32,
    /// The logind subsystem it is in: `backlight` or `leds`.
    pub subsystem: &'static str,
    /// The lowest raw brightness a key steps down to: 1 for a screen (many panels turn off
    /// at 0, and the key that would bring them back is then hard to find in the dark), 0 for
    /// a keyboard, which is meant to go dark.
    floor: u32,
    dir: PathBuf,
}

impl Backlight {
    /// The backlight to drive among those in `class_dir` (normally [`BACKLIGHTS`]): a
    /// `firmware` one first, then `platform`, then `raw` (systemd's and GNOME's order),
    /// by name within a type. `None` when there is none (a desktop, a VM).
    pub fn find(class_dir: &Path) -> Option<Self> {
        let mut found: Vec<(u8, Self)> = fs::read_dir(class_dir)
            .ok()?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let dir = entry.path();
                let rank = match read_trimmed(&dir.join("type")).ok()?.as_str() {
                    "firmware" => 0,
                    "platform" => 1,
                    "raw" => 2,
                    _ => return None,
                };
                let max = read_trimmed(&dir.join("max_brightness"))
                    .ok()?
                    .parse()
                    .ok()?;
                if max == 0 {
                    return None;
                }
                let name = entry.file_name().into_string().ok()?;
                Some((
                    rank,
                    Self {
                        name,
                        max,
                        subsystem: "backlight",
                        floor: 1,
                        dir,
                    },
                ))
            })
            .collect();
        found.sort_by(|(a, x), (b, y)| a.cmp(b).then_with(|| x.name.cmp(&y.name)));
        found.into_iter().next().map(|(_, backlight)| backlight)
    }

    /// The keyboard backlight among the LEDs in `class_dir` (normally [`LEDS`]): the first,
    /// by name, called `<something>::kbd_backlight` (`tpacpi::kbd_backlight`,
    /// `asus::kbd_backlight`, `apple::kbd_backlight`) that has a range. `None` when there is
    /// none (most desktops, the VM).
    pub fn find_keyboard(class_dir: &Path) -> Option<Self> {
        let mut found: Vec<Self> = fs::read_dir(class_dir)
            .ok()?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                if !name.ends_with("::kbd_backlight") {
                    return None;
                }
                let dir = entry.path();
                let max = read_trimmed(&dir.join("max_brightness"))
                    .ok()?
                    .parse()
                    .ok()?;
                if max == 0 {
                    return None;
                }
                Some(Self {
                    name,
                    max,
                    subsystem: "leds",
                    floor: 0,
                    dir,
                })
            })
            .collect();
        found.sort_by(|a, b| a.name.cmp(&b.name));
        found.into_iter().next()
    }

    /// The current raw brightness.
    pub fn read(&self) -> io::Result<u32> {
        read_trimmed(&self.dir.join("brightness"))?
            .parse()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// `raw` as a level, 0 to 1.
    pub fn level(&self, raw: u32) -> f64 {
        f64::from(raw.min(self.max)) / f64::from(self.max)
    }

    /// The raw brightness a brightness key moves `raw` to. It changes by at least one
    /// unit while there is room, and stops at the backlight's floor going down (1 for a
    /// screen, 0 for a keyboard).
    pub fn step(&self, raw: u32, up: bool, fine: bool) -> u32 {
        let level = step(self.level(raw), up, fine);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "level is in 0..=1, so the product is in 0..=max"
        )]
        let mut target = (level * f64::from(self.max)).round() as u32;
        if up && target <= raw {
            target = raw.saturating_add(1);
        } else if !up && target >= raw {
            target = raw.saturating_sub(1);
        }
        target.clamp(self.floor, self.max)
    }
}

fn read_trimmed(path: &Path) -> io::Result<String> {
    Ok(fs::read_to_string(path)?.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn steps_land_on_the_grid() {
        assert!(close(step(0.5, true, false), 9.0 / 16.0));
        assert!(close(step(0.5, false, false), 7.0 / 16.0));
        // Between two steps: the next one in the key's direction.
        assert!(close(step(0.53, true, false), 9.0 / 16.0));
        assert!(close(step(0.53, false, false), 8.0 / 16.0));
        assert!(close(step(0.5, true, true), 33.0 / 64.0));
        assert!(close(step(1.0, true, false), 1.0));
        assert!(close(step(0.0, false, false), 0.0));
    }

    #[test]
    fn volume_keys_step_and_unmute_and_mute_toggles() {
        let mut volume = Volume {
            level: 0.5,
            muted: false,
        };
        assert!(volume.apply(SystemKey::Mute, false));
        assert!(volume.muted);
        assert!(volume.apply(SystemKey::VolumeDown, false));
        assert!(!volume.muted);
        assert!(close(volume.level, 7.0 / 16.0));
        assert!(!volume.apply(SystemKey::BrightnessUp, false));
    }

    #[test]
    fn up_keeps_an_amplified_volume() {
        let mut volume = Volume {
            level: 1.5,
            muted: true,
        };
        assert!(volume.apply(SystemKey::VolumeUp, false));
        assert!(close(volume.level, 1.5));
        assert!(!volume.muted);
        assert!(volume.apply(SystemKey::VolumeDown, false));
        assert!(close(volume.level, 15.0 / 16.0));
    }

    fn fake(dir: &Path, name: &str, kind: &str, max: u32, now: u32) {
        let device = dir.join(name);
        fs::create_dir(&device).unwrap();
        fs::write(device.join("type"), format!("{kind}\n")).unwrap();
        fs::write(device.join("max_brightness"), format!("{max}\n")).unwrap();
        fs::write(device.join("brightness"), format!("{now}\n")).unwrap();
    }

    #[test]
    fn the_firmware_backlight_wins_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        fake(dir.path(), "intel_backlight", "raw", 96_000, 48_000);
        fake(dir.path(), "acpi_video0", "firmware", 100, 40);
        let backlight = Backlight::find(dir.path()).unwrap();
        assert_eq!(backlight.name, "acpi_video0");
        assert_eq!(backlight.max, 100);
        assert_eq!(backlight.read().unwrap(), 40);
        assert!(close(backlight.level(40), 0.4));
    }

    #[test]
    fn no_backlight_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Backlight::find(dir.path()), None);
        assert_eq!(Backlight::find(&dir.path().join("missing")), None);
        // A device with no range isn't one.
        fake(dir.path(), "broken", "raw", 0, 0);
        assert_eq!(Backlight::find(dir.path()), None);
    }

    #[test]
    fn raw_steps_move_and_never_turn_the_panel_off() {
        let dir = tempfile::tempdir().unwrap();
        fake(dir.path(), "intel_backlight", "raw", 96_000, 48_000);
        let b = Backlight::find(dir.path()).unwrap();
        assert_eq!(b.step(48_000, true, false), 54_000);
        assert_eq!(b.step(48_000, false, false), 42_000);
        assert_eq!(b.step(6_000, false, false), 1);
        assert_eq!(b.step(96_000, true, false), 96_000);
        // A coarse range still moves by one.
        let small = Backlight { max: 7, ..b };
        assert_eq!(small.step(3, true, true), 4);
        assert_eq!(small.step(1, false, false), 1);
    }

    #[test]
    fn the_keyboard_backlight_is_found_by_name_and_can_go_dark() {
        let dir = tempfile::tempdir().unwrap();
        fake(dir.path(), "input3::capslock", "raw", 1, 0);
        fake(dir.path(), "tpacpi::kbd_backlight", "raw", 2, 1);
        fake(dir.path(), "asus::kbd_backlight", "raw", 3, 0);
        let k = Backlight::find_keyboard(dir.path()).unwrap();
        assert_eq!(
            (k.name.as_str(), k.max, k.subsystem),
            ("asus::kbd_backlight", 3, "leds")
        );
        assert_eq!(k.read().unwrap(), 0);
        // Few steps: each key press moves one, and down reaches 0 (off).
        assert_eq!(k.step(0, true, false), 1);
        assert_eq!(k.step(1, true, false), 2);
        assert_eq!(k.step(3, true, false), 3);
        assert_eq!(k.step(2, false, false), 1);
        assert_eq!(k.step(1, false, false), 0);
        assert_eq!(k.step(0, false, false), 0);
        assert!(close(k.level(k.step(0, true, false)), 1.0 / 3.0));
    }

    #[test]
    fn no_keyboard_backlight_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Backlight::find_keyboard(dir.path()), None);
        assert_eq!(Backlight::find_keyboard(&dir.path().join("missing")), None);
        fake(dir.path(), "input3::capslock", "raw", 1, 0);
        fake(dir.path(), "broken::kbd_backlight", "raw", 0, 0);
        assert_eq!(Backlight::find_keyboard(dir.path()), None);
        // The screen's lookup doesn't pick it up either.
        assert_eq!(
            Backlight::find(dir.path()).map(|b| b.subsystem),
            Some("backlight")
        );
    }
}
