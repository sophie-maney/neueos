//! Variable refresh rate per output (task 1.15c, ADR 0020).
//!
//! `[outputs."NAME"] adaptive-sync` asks for `off`, `on` or `fullscreen`. The tty backend
//! asks the connector what it can do ([`Support`]) when it lights it, and before every
//! render sets the CRTC's `VRR_ENABLED` to [`wanted`]. `fullscreen` needs a connector that
//! switches without a modeset (a modeset blanks the display for a second, not something to
//! do each time a game goes full screen); `on` accepts one modeset when it is applied.
//! The nested and headless backends have no VRR: their outputs report [`Support::None`].

use neue_config::compositor::AdaptiveSync;
use neue_ipc::{AdaptiveSyncInfo, AdaptiveSyncMode};

/// What a connector can do (Smithay's `VrrSupport`, without the DRM types).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Support {
    /// No VRR: the display, the driver or the backend cannot.
    #[default]
    None,
    /// VRR works, but switching it takes a modeset.
    NeedsModeset,
    /// VRR can be switched from one frame to the next.
    Seamless,
}

/// Whether an output with `support`, configured to `mode`, should run at a variable rate
/// now. `fullscreen`: a full-screen window covers the output and the session is unlocked.
// Only the tty backend switches VRR.
#[cfg_attr(not(feature = "tty"), allow(dead_code))]
pub fn wanted(mode: AdaptiveSync, support: Support, fullscreen: bool) -> bool {
    match (mode, support) {
        (AdaptiveSync::Off, _) | (_, Support::None) => false,
        (AdaptiveSync::On, _) => true,
        (AdaptiveSync::Fullscreen, Support::Seamless) => fullscreen,
        (AdaptiveSync::Fullscreen, Support::NeedsModeset) => false,
    }
}

/// Why `mode` cannot be honoured on a connector with `support`, if it cannot.
// Only the tty backend switches VRR.
#[cfg_attr(not(feature = "tty"), allow(dead_code))]
pub fn unmet(mode: AdaptiveSync, support: Support) -> Option<&'static str> {
    match (mode, support) {
        (AdaptiveSync::Off, _) => None,
        (_, Support::None) => Some("the display or driver does not support adaptive sync"),
        (AdaptiveSync::Fullscreen, Support::NeedsModeset) => Some(
            "the driver needs a modeset to switch adaptive sync; use `on` instead of `fullscreen`",
        ),
        _ => None,
    }
}

/// The `neuectl outputs` view of an output's adaptive sync.
pub fn info(mode: AdaptiveSync, support: Support, active: bool) -> AdaptiveSyncInfo {
    AdaptiveSyncInfo {
        mode: match mode {
            AdaptiveSync::Off => AdaptiveSyncMode::Off,
            AdaptiveSync::On => AdaptiveSyncMode::On,
            AdaptiveSync::Fullscreen => AdaptiveSyncMode::Fullscreen,
        },
        supported: support != Support::None,
        active,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_and_unsupported_are_never_variable() {
        for support in [Support::None, Support::NeedsModeset, Support::Seamless] {
            for fullscreen in [false, true] {
                assert!(!wanted(AdaptiveSync::Off, support, fullscreen));
            }
        }
        for mode in [AdaptiveSync::On, AdaptiveSync::Fullscreen] {
            assert!(!wanted(mode, Support::None, true));
        }
    }

    #[test]
    fn on_is_always_variable_where_supported() {
        for support in [Support::NeedsModeset, Support::Seamless] {
            assert!(wanted(AdaptiveSync::On, support, false));
            assert!(wanted(AdaptiveSync::On, support, true));
        }
    }

    #[test]
    fn fullscreen_follows_the_window_without_modesets() {
        assert!(wanted(AdaptiveSync::Fullscreen, Support::Seamless, true));
        assert!(!wanted(AdaptiveSync::Fullscreen, Support::Seamless, false));
        assert!(!wanted(
            AdaptiveSync::Fullscreen,
            Support::NeedsModeset,
            true
        ));
    }

    #[test]
    fn unmet_explains_only_what_cannot_be_done() {
        assert_eq!(unmet(AdaptiveSync::Off, Support::None), None);
        assert_eq!(unmet(AdaptiveSync::On, Support::NeedsModeset), None);
        assert_eq!(unmet(AdaptiveSync::Fullscreen, Support::Seamless), None);
        assert!(unmet(AdaptiveSync::On, Support::None).is_some());
        assert!(unmet(AdaptiveSync::Fullscreen, Support::NeedsModeset).is_some());
    }
}
