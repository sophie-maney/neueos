//! Raw Wayland handles for wgpu. This is the only `unsafe` in the driver: wgpu can't take a
//! `wl_surface` from wayland-client without going through raw pointers.
#![allow(
    unsafe_code,
    reason = "wgpu needs raw wl_display/wl_surface pointers; each use has a SAFETY comment"
)]

use std::ptr::NonNull;

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, RawDisplayHandle, RawWindowHandle,
    WaylandDisplayHandle, WaylandWindowHandle,
};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Proxy};

/// The connection's `wl_display`, as a display handle for `wgpu::Instance`.
#[derive(Debug)]
pub(super) struct Display(pub(super) Connection);

impl Display {
    fn raw(&self) -> Result<RawDisplayHandle, HandleError> {
        let ptr =
            NonNull::new(self.0.backend().display_ptr().cast()).ok_or(HandleError::Unavailable)?;
        Ok(RawDisplayHandle::Wayland(WaylandDisplayHandle::new(ptr)))
    }
}

impl HasDisplayHandle for Display {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let raw = self.raw()?;
        // SAFETY: the pointer is the live `wl_display` owned by `self.0`, which outlives the
        // returned borrow.
        Ok(unsafe { DisplayHandle::borrow_raw(raw) })
    }
}

/// Creates a wgpu surface for `surface`.
///
/// The caller must drop the returned surface before destroying `surface` (the driver keeps
/// both in one struct, wgpu surface first, so field drop order guarantees it).
pub(super) fn create_surface(
    instance: &wgpu::Instance,
    conn: &Connection,
    surface: &WlSurface,
) -> Result<wgpu::Surface<'static>, super::Error> {
    let display = Display(conn.clone())
        .raw()
        .map_err(|e| super::Error::Gpu(e.to_string()))?;
    let ptr = NonNull::new(surface.id().as_ptr().cast())
        .ok_or_else(|| super::Error::Gpu("wl_surface has no C pointer".into()))?;
    let window = RawWindowHandle::Wayland(WaylandWindowHandle::new(ptr));
    // SAFETY: both pointers are live: the display belongs to `conn` (kept alive by the
    // driver for the whole run), and the surface outlives the wgpu surface (see above).
    unsafe {
        instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(display),
            raw_window_handle: window,
        })
    }
    .map_err(|e| super::Error::Gpu(e.to_string()))
}
