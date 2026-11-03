//! Procedural, vector-based materials and painting primitives.
//!
//! Portable crate: must build and pass tests on macOS and Linux.
//! This is a stub; the proof of concept (linen, Aqua button) lands in task 0.6.

/// Crate name, used by smoke tests and diagnostics.
pub const NAME: &str = "neue-paint";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_matches_crate() {
        assert_eq!(NAME, env!("CARGO_PKG_NAME"));
    }
}
