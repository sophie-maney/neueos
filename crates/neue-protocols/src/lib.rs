//! Wayland protocols that are not in `wayland-protocols`, generated from the XML in
//! `protocols/` (see `docs/protocols/`).
//!
//! * [`gtk_shell`]: GTK's `gtk_shell1` and `gtk_surface1` (ADR 0005, ADR 0025). GTK apps
//!   announce their `org.gtk.Menus` address through it.
//!
//! Enable `server` for the compositor and `client` for scripted test clients.

// The generated code is not ours to lint.
#![allow(
    clippy::all,
    missing_docs,
    missing_debug_implementations,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unused_imports,
    unused_variables,
    dead_code
)]

/// `gtk_shell1` v5 (`protocols/gtk-shell.xml`).
pub mod gtk_shell {
    #[cfg(feature = "client")]
    pub mod client {
        //! Client-side API (tests).
        use wayland_client;
        use wayland_client::protocol::*;

        pub mod __interfaces {
            use wayland_client::protocol::__interfaces::*;
            wayland_scanner::generate_interfaces!("./protocols/gtk-shell.xml");
        }
        use self::__interfaces::*;

        wayland_scanner::generate_client_code!("./protocols/gtk-shell.xml");
    }

    #[cfg(feature = "server")]
    pub mod server {
        //! Server-side API (the compositor).
        use wayland_server;
        use wayland_server::protocol::*;

        pub mod __interfaces {
            use wayland_server::protocol::__interfaces::*;
            wayland_scanner::generate_interfaces!("./protocols/gtk-shell.xml");
        }
        use self::__interfaces::*;

        wayland_scanner::generate_server_code!("./protocols/gtk-shell.xml");
    }
}
