//! Plain-text clipboard for the winit driver: the macOS pasteboard, through `arboard`.

/// Reads and writes plain text. Without a pasteboard (never seen in practice), reads return
/// nothing and writes are dropped.
pub(super) struct Clipboard(Option<arboard::Clipboard>);

impl std::fmt::Debug for Clipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Clipboard").field(&self.0.is_some()).finish()
    }
}

impl Clipboard {
    pub(super) fn new() -> Self {
        Self(
            arboard::Clipboard::new()
                .inspect_err(|err| tracing::warn!(%err, "no system clipboard"))
                .ok(),
        )
    }

    /// The clipboard's text, if it holds any.
    pub(super) fn get(&mut self) -> Option<String> {
        self.0
            .as_mut()?
            .get_text()
            .inspect_err(|err| tracing::debug!(%err, "clipboard has no text"))
            .ok()
    }

    pub(super) fn set(&mut self, text: String) {
        if let Some(clipboard) = self.0.as_mut()
            && let Err(err) = clipboard.set_text(text)
        {
            tracing::warn!(%err, "can't write the clipboard");
        }
    }
}
