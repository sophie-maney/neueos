//! Helpers for tests (here and in neue-kit) without a bus daemon.

use std::os::unix::net::UnixStream;
use std::thread;

use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;

use crate::Error;

/// Two connected peer-to-peer connections: `(service, client)`. The service side is set up
/// as [`crate::connect_session`]'s is, but has no unique name (there's no bus), so it
/// can't be named in an appmenu address.
pub fn pair() -> Result<(Connection, Connection), Error> {
    let (a, b) = UnixStream::pair().map_err(zbus::Error::from)?;
    let guid = zbus::Guid::generate();
    // The handshake needs both ends at once.
    let server = thread::spawn(move || {
        crate::with_object_server(Builder::async_io_unix_stream(a).server(guid)?.p2p())?.build()
    });
    let client = Builder::async_io_unix_stream(b).p2p().build()?;
    let server = server
        .join()
        .map_err(|_| zbus::Error::Failure("the server handshake panicked".into()))??;
    Ok((server, client))
}
