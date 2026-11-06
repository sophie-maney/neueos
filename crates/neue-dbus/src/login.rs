//! The bits of systemd-logind (`org.freedesktop.login1`) the shell uses: locking this
//! session, restarting and shutting down for the logo menu (task 3.2b), and setting the
//! backlight for the brightness keys (task 3.14a). Logging out is the compositor quitting
//! (ADR 0027), not a logind call.
//!
//! Restart and Shut Down are `interactive`: polkit may ask for a password through the
//! session's agent (3.20) when other users are logged in.

use zbus::blocking::Connection;

/// logind's bus name.
pub const DESTINATION: &str = "org.freedesktop.login1";
/// The manager object.
pub const MANAGER_PATH: &str = "/org/freedesktop/login1";
/// The manager interface.
pub const MANAGER: &str = "org.freedesktop.login1.Manager";
/// The caller's own session (logind resolves `auto` from the caller's credentials).
pub const SESSION_PATH: &str = "/org/freedesktop/login1/session/auto";
/// The session interface.
pub const SESSION: &str = "org.freedesktop.login1.Session";

/// A logind client on the system bus.
#[derive(Debug, Clone)]
pub struct Login {
    conn: Connection,
}

impl Login {
    /// Connects to the system bus.
    pub fn system() -> Result<Self, crate::Error> {
        Ok(Self::new(Connection::system()?))
    }

    /// A client on `conn` (tests pass a peer-to-peer connection).
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Asks the session's lock screen to lock it (`Session.Lock`; logind sends the `Lock`
    /// signal, which the lock screen of task 3.18 answers).
    pub fn lock_session(&self) -> Result<(), crate::Error> {
        self.call(SESSION_PATH, SESSION, "Lock", &())
    }

    /// Restarts the machine (`Manager.Reboot`).
    pub fn reboot(&self) -> Result<(), crate::Error> {
        self.call(MANAGER_PATH, MANAGER, "Reboot", &(true,))
    }

    /// Shuts the machine down (`Manager.PowerOff`).
    pub fn power_off(&self) -> Result<(), crate::Error> {
        self.call(MANAGER_PATH, MANAGER, "PowerOff", &(true,))
    }

    /// Sets a backlight to `brightness` (raw, up to its `max_brightness`) through
    /// `Session.SetBrightness`, which needs no privileges for the active session's seat.
    /// `subsystem` is `backlight` (a screen) or `leds` (a keyboard); `name` is the device's
    /// directory in `/sys/class/<subsystem>`.
    pub fn set_brightness(
        &self,
        subsystem: &str,
        name: &str,
        brightness: u32,
    ) -> Result<(), crate::Error> {
        self.call(
            SESSION_PATH,
            SESSION,
            "SetBrightness",
            &(subsystem, name, brightness),
        )
    }

    fn call<B>(
        &self,
        path: &str,
        interface: &str,
        method: &str,
        body: &B,
    ) -> Result<(), crate::Error>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        self.conn
            .call_method(Some(DESTINATION), path, Some(interface), method, body)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    /// A stand-in for logind that records the calls it gets.
    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<Vec<String>>>);

    struct FakeManager(Fake);
    struct FakeSession(Fake);

    #[zbus::interface(name = "org.freedesktop.login1.Manager")]
    impl FakeManager {
        fn reboot(&self, interactive: bool) {
            self.0
                .0
                .lock()
                .unwrap()
                .push(format!("Reboot({interactive})"));
        }
        fn power_off(&self, interactive: bool) {
            self.0
                .0
                .lock()
                .unwrap()
                .push(format!("PowerOff({interactive})"));
        }
    }

    #[zbus::interface(name = "org.freedesktop.login1.Session")]
    impl FakeSession {
        fn lock(&self) {
            self.0.0.lock().unwrap().push("Lock".to_owned());
        }
        fn set_brightness(&self, subsystem: &str, name: &str, brightness: u32) {
            self.0
                .0
                .lock()
                .unwrap()
                .push(format!("SetBrightness({subsystem}, {name}, {brightness})"));
        }
    }

    #[test]
    fn calls_reach_logind() {
        let (server, client) = crate::testing::pair().unwrap();
        let fake = Fake::default();
        let objects = server.object_server();
        objects.at(MANAGER_PATH, FakeManager(fake.clone())).unwrap();
        objects.at(SESSION_PATH, FakeSession(fake.clone())).unwrap();

        let login = Login::new(client);
        login.lock_session().unwrap();
        login.reboot().unwrap();
        login.power_off().unwrap();
        login
            .set_brightness("backlight", "intel_backlight", 480)
            .unwrap();
        assert_eq!(
            *fake.0.lock().unwrap(),
            [
                "Lock",
                "Reboot(true)",
                "PowerOff(true)",
                "SetBrightness(backlight, intel_backlight, 480)"
            ]
        );
    }
}
