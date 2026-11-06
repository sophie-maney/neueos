//! neueOS's desktop shell (PLAN.md §3.3, task 3.2b, ADR 0068): one process with a layer
//! surface per output and one set of state, started and restarted by the compositor
//! (`neue-comp --shell`, ADR 0066).
//!
//! This library holds what is portable, so the gallery and the goldens can use it on
//! macOS: the menu bar's pieces ([`bar`]), the clock's text ([`clock`]), the window list
//! behind the active app's name ([`windows`]), the About panel ([`about`]), the fallback
//! menu ([`fallback`]), menu search ([`search`], [`help`]) and what the volume and
//! brightness keys do ([`levels`]), the open notifications' clocks
//! ([`notifications`]) and the dock's tiles ([`dock`]). The process
//! itself (`src/shell.rs`) is Linux-only.

pub mod about;
pub mod bar;
pub mod clock;
pub mod dock;
pub mod fallback;
pub mod help;
pub mod levels;
pub mod notifications;
pub mod picture;
pub mod search;
pub mod windows;
