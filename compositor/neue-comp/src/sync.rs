//! Buffer synchronisation (task 1.14, ADR 0017): a commit is applied only once the GPU has
//! finished drawing its buffer.
//!
//! - **Explicit sync** (`linux-drm-syncobj-v1`): the client names an acquire point on a DRM
//!   timeline; the commit waits for it. Smithay signals the release point when the
//!   compositor lets go of the buffer. The global exists only on the tty backend, and only
//!   when the GPU supports `syncobj_eventfd` (without it the wait cannot be polled).
//! - **Implicit sync**: a dmabuf without an acquire point waits until its fences are
//!   readable.
//!
//! Both work through Smithay's commit blockers: a pre-commit hook adds a blocker and an
//! event source that clears it, so the event loop never blocks on the GPU.
//!
//! Adapted from Smithay's `anvil` (MIT): `anvil/src/shell/mod.rs` (`new_surface`).

use smithay::reexports::calloop::Interest;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::compositor::{
    BufferAssignment, CompositorHandler, SurfaceAttributes, add_blocker, add_pre_commit_hook,
    with_states,
};
use smithay::wayland::dmabuf::get_dmabuf;
use smithay::wayland::drm_syncobj::{DrmSyncobjCachedState, DrmSyncobjHandler, DrmSyncobjState};
use tracing::debug;

#[cfg(feature = "tty")]
use crate::state::Backend;
use crate::state::State;

impl DrmSyncobjHandler for State {
    fn drm_syncobj_state(&mut self) -> Option<&mut DrmSyncobjState> {
        #[cfg(feature = "tty")]
        if let Backend::Tty(tty) = &mut self.backend {
            return tty.syncobj_state.as_mut();
        }
        None
    }
}

/// Make commits of `surface` wait for their buffer (see the module docs). Called once per
/// surface, when it is created.
pub fn add_commit_blockers(surface: &WlSurface) {
    add_pre_commit_hook::<State, _>(surface, |state, _dh, surface| {
        let (acquire_point, dmabuf) = with_states(surface, |states| {
            let acquire_point = states
                .cached_state
                .get::<DrmSyncobjCachedState>()
                .pending()
                .acquire_point
                .clone();
            let dmabuf = match &states
                .cached_state
                .get::<SurfaceAttributes>()
                .pending()
                .buffer
            {
                Some(BufferAssignment::NewBuffer(buffer)) => get_dmabuf(buffer).cloned().ok(),
                _ => None,
            };
            (acquire_point, dmabuf)
        });
        // Only dmabufs carry GPU work; shm buffers are ready when attached.
        let Some(dmabuf) = dmabuf else { return };
        let Some(client) = surface.client() else {
            return;
        };

        if let Some(point) = acquire_point {
            match point.generate_blocker() {
                Ok((blocker, source)) => {
                    let client = client.clone();
                    let inserted = state.loop_handle.insert_source(source, move |_, _, state| {
                        let dh = state.display_handle.clone();
                        state
                            .client_compositor_state(&client)
                            .blocker_cleared(state, &dh);
                        Ok(())
                    });
                    if inserted.is_ok() {
                        add_blocker(surface, blocker);
                        return;
                    }
                }
                // Fall through to the implicit fences, which every driver also sets.
                Err(err) => debug!(%err, "no acquire point blocker"),
            }
        }

        if let Ok((blocker, source)) = dmabuf.generate_blocker(Interest::READ) {
            let inserted = state.loop_handle.insert_source(source, move |_, _, state| {
                let dh = state.display_handle.clone();
                state
                    .client_compositor_state(&client)
                    .blocker_cleared(state, &dh);
                Ok(())
            });
            if inserted.is_ok() {
                add_blocker(surface, blocker);
            }
        }
    });
}
