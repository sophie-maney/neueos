//! `wp_security_context_v1` (task 1.14, ADR 0017): sandboxes (Flatpak) hand their apps a
//! socket of their own, and clients connecting through it lose the privileged globals.
//!
//! A client carries its [`SecurityContext`] in [`ClientState`]; [`is_unrestricted`] is the
//! filter every privileged global is created with (see `State::new`).

use std::sync::Arc;

use smithay::reexports::wayland_server::Client;
use smithay::wayland::security_context::{
    SecurityContext, SecurityContextHandler, SecurityContextListenerSource,
};
use tracing::{debug, warn};

use crate::state::{ClientState, State};

/// Whether `client` may see privileged globals: it did not connect through a security
/// context. Clients of another display (never ours) are treated as restricted.
pub fn is_unrestricted(client: &Client) -> bool {
    client
        .get_data::<ClientState>()
        .is_some_and(|data| data.security_context.is_none())
}

impl SecurityContextHandler for State {
    fn context_created(&mut self, source: SecurityContextListenerSource, context: SecurityContext) {
        debug!(
            engine = ?context.sandbox_engine,
            app_id = ?context.app_id,
            "security context created"
        );
        // The source goes away by itself when the sandbox closes its end of `close_fd`.
        let result = self
            .loop_handle
            .insert_source(source, move |stream, _, state| {
                let data = ClientState {
                    security_context: Some(context.clone()),
                    ..ClientState::default()
                };
                if let Err(err) = state.display_handle.insert_client(stream, Arc::new(data)) {
                    warn!(%err, "failed to add a sandboxed client");
                }
            });
        if let Err(err) = result {
            warn!(err = %err.error, "failed to listen on a security context");
        }
    }
}
