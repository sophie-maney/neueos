//! neueOS Wayland compositor.
//!
//! Linux-only: the whole crate is gated, so on other hosts it compiles to an empty
//! library and stays a workspace member. This is a stub; the compositor core is
//! built in Phase 1.
#![cfg(target_os = "linux")]

/// Crate name, used by smoke tests and diagnostics.
pub const NAME: &str = "neue-comp";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_matches_crate() {
        assert_eq!(NAME, env!("CARGO_PKG_NAME"));
    }
}
