//! MPRIS media players (`org.mpris.MediaPlayer2.*` on the session bus) for the media keys
//! (task 3.14d).
//!
//! [`Mpris`] lists the players, picks the *active* one (the one playing, or else the one
//! that played last) and sends it a [`Command`]. "Played last" needs a memory, which a
//! thread started by [`Mpris::watch`] keeps: it notes every player whose `PlaybackStatus`
//! changes to `Playing`. Zbus proxies, not `mpris` or `playerctl` crates (D-Bus stays on
//! zbus, ADR 0005).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use zbus::MatchRule;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type as MessageType;
use zbus::zvariant::OwnedValue;

use crate::Error;

/// Every player's well-known name starts with this.
pub const NAME_PREFIX: &str = "org.mpris.MediaPlayer2.";
/// The object every player serves.
pub const PATH: &str = "/org/mpris/MediaPlayer2";
/// The interface with the transport controls and `PlaybackStatus`.
pub const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const BUS: &str = "org.freedesktop.DBus";
const BUS_PATH: &str = "/org/freedesktop/DBus";

/// How many players it remembers having played, most recent first.
const RECENT: usize = 8;

/// What a media key asks of a player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// The play key: starts a stopped or paused player, pauses a playing one.
    PlayPause,
    Pause,
    Next,
    Previous,
    Stop,
}

impl Command {
    /// The method on [`PLAYER`].
    pub const fn method(self) -> &'static str {
        match self {
            Self::PlayPause => "PlayPause",
            Self::Pause => "Pause",
            Self::Next => "Next",
            Self::Previous => "Previous",
            Self::Stop => "Stop",
        }
    }
}

/// A player's `PlaybackStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

impl PlaybackStatus {
    fn parse(status: &str) -> Option<Self> {
        match status {
            "Playing" => Some(Self::Playing),
            "Paused" => Some(Self::Paused),
            "Stopped" => Some(Self::Stopped),
            _ => None,
        }
    }
}

/// A player on the bus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Player {
    /// Its well-known name, `org.mpris.MediaPlayer2.<something>`.
    pub name: String,
    /// The unique name (`:1.42`) that owns [`name`](Self::name); what signals come from.
    pub owner: String,
    pub status: PlaybackStatus,
}

/// The player a media key goes to: a playing one if there is one, and among players of the
/// same kind the one in `recent` (unique names, most recent first) that played last. Players
/// `recent` doesn't know keep the order they came in, after the ones it does.
pub fn pick<'a>(players: &'a [Player], recent: &[String]) -> Option<&'a Player> {
    players
        .iter()
        .enumerate()
        .min_by_key(|(index, player)| {
            let seen = recent
                .iter()
                .position(|owner| *owner == player.owner)
                .unwrap_or(usize::MAX);
            (player.status != PlaybackStatus::Playing, seen, *index)
        })
        .map(|(_, player)| player)
}

/// Moves `owner` to the front of `recent`.
fn note_played(recent: &mut Vec<String>, owner: &str) {
    recent.retain(|o| o != owner);
    recent.insert(0, owner.to_owned());
    recent.truncate(RECENT);
}

/// An MPRIS client. Cloning shares the memory of who played last.
#[derive(Debug, Clone)]
pub struct Mpris {
    conn: Connection,
    recent: Arc<Mutex<Vec<String>>>,
}

impl Mpris {
    /// A client on `conn`: the session bus, or a peer-to-peer connection in tests.
    pub fn new(conn: Connection) -> Self {
        Self {
            conn,
            recent: Arc::default(),
        }
    }

    /// Connects to the session bus. Calls to a hung player fail after `timeout`.
    pub fn session(timeout: std::time::Duration) -> Result<Self, Error> {
        Ok(Self::new(crate::connect_session_client(timeout)?))
    }

    /// Every player on the bus now, in the bus's order. A player that doesn't answer is left
    /// out (it is hung, or went away while we asked).
    pub fn players(&self) -> Result<Vec<Player>, Error> {
        let names: Vec<String> = self
            .conn
            .call_method(Some(BUS), BUS_PATH, Some(BUS), "ListNames", &())?
            .body()
            .deserialize()?;
        let mut players = Vec::new();
        for name in names.into_iter().filter(|n| n.starts_with(NAME_PREFIX)) {
            match self.player(&name) {
                Ok(player) => players.push(player),
                Err(err) => tracing::debug!(%err, name, "MPRIS player doesn't answer"),
            }
        }
        Ok(players)
    }

    fn player(&self, name: &str) -> Result<Player, Error> {
        let owner: String = self
            .conn
            .call_method(Some(BUS), BUS_PATH, Some(BUS), "GetNameOwner", &(name,))?
            .body()
            .deserialize()?;
        let status: OwnedValue = self
            .conn
            .call_method(
                Some(name),
                PATH,
                Some(PROPERTIES),
                "Get",
                &(PLAYER, "PlaybackStatus"),
            )?
            .body()
            .deserialize()?;
        let status = String::try_from(status).map_err(zbus::Error::from)?;
        let status = PlaybackStatus::parse(&status)
            .ok_or_else(|| zbus::Error::Failure(format!("unknown PlaybackStatus {status:?}")))?;
        Ok(Player {
            name: name.to_owned(),
            owner,
            status,
        })
    }

    /// The player the media keys go to now (see [`pick`]); `None` when there is none.
    pub fn active(&self) -> Result<Option<Player>, Error> {
        let players = self.players()?;
        let mut recent = self.recent.lock().unwrap_or_else(PoisonError::into_inner);
        let picked = pick(&players, &recent).cloned();
        // A player that was already playing when it appeared sent no signal; it has played
        // now, as far as the next key press is concerned.
        if let Some(player) = picked
            .as_ref()
            .filter(|p| p.status == PlaybackStatus::Playing)
        {
            note_played(&mut recent, &player.owner);
        }
        Ok(picked)
    }

    /// Sends `command` to the active player and returns it; `None` (and nothing sent) when
    /// there is no player.
    pub fn send(&self, command: Command) -> Result<Option<Player>, Error> {
        let Some(player) = self.active()? else {
            return Ok(None);
        };
        self.conn.call_method(
            Some(player.name.as_str()),
            PATH,
            Some(PLAYER),
            command.method(),
            &(),
        )?;
        Ok(Some(player))
    }

    /// Starts remembering which players play: the ones playing now count as played just
    /// before the call, and a thread (named `mpris-signals`, ending with the connection)
    /// notes every later change to `Playing`.
    pub fn watch(&self) -> Result<(), Error> {
        let rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .interface(PROPERTIES)?
            .member("PropertiesChanged")?
            .path(PATH)?
            .build();
        let messages = MessageIterator::for_match_rule(rule, &self.conn, Some(32))?;
        // Subscribed first, so a player that starts in between is not missed.
        match self.players() {
            Ok(players) => {
                let mut recent = self.recent.lock().unwrap_or_else(PoisonError::into_inner);
                for player in players
                    .iter()
                    .filter(|p| p.status == PlaybackStatus::Playing)
                {
                    note_played(&mut recent, &player.owner);
                }
            }
            Err(err) => tracing::debug!(%err, "can't list MPRIS players yet"),
        }
        let recent = Arc::clone(&self.recent);
        std::thread::Builder::new()
            .name("mpris-signals".into())
            .spawn(move || {
                for message in messages {
                    let Ok(message) = message else { continue };
                    let Some(sender) = message.header().sender().map(ToString::to_string) else {
                        continue;
                    };
                    let body = message.body();
                    let Ok((interface, changed, _)) =
                        body.deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
                    else {
                        continue;
                    };
                    let playing = interface == PLAYER
                        && changed
                            .get("PlaybackStatus")
                            .and_then(|v| v.try_clone().ok())
                            .and_then(|v| String::try_from(v).ok())
                            .is_some_and(|s| s == "Playing");
                    if playing {
                        let mut recent = recent.lock().unwrap_or_else(PoisonError::into_inner);
                        note_played(&mut recent, &sender);
                    }
                }
            })
            .map_err(zbus::Error::from)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // test code

    use super::*;

    fn player(name: &str, owner: &str, status: PlaybackStatus) -> Player {
        Player {
            name: format!("{NAME_PREFIX}{name}"),
            owner: owner.to_owned(),
            status,
        }
    }

    fn names(players: &[Player], recent: &[&str]) -> Option<String> {
        let recent: Vec<String> = recent.iter().map(|s| (*s).to_owned()).collect();
        pick(players, &recent).map(|p| p.name.trim_start_matches(NAME_PREFIX).to_owned())
    }

    #[test]
    fn a_playing_player_wins_over_a_more_recent_paused_one() {
        let players = [
            player("a", ":1.1", PlaybackStatus::Paused),
            player("b", ":1.2", PlaybackStatus::Playing),
        ];
        assert_eq!(names(&players, &[":1.1"]).as_deref(), Some("b"));
    }

    #[test]
    fn among_players_of_a_kind_the_most_recent_wins() {
        let players = [
            player("a", ":1.1", PlaybackStatus::Paused),
            player("b", ":1.2", PlaybackStatus::Paused),
            player("c", ":1.3", PlaybackStatus::Playing),
            player("d", ":1.4", PlaybackStatus::Playing),
        ];
        assert_eq!(
            names(&players[..2], &[":1.2", ":1.1"]).as_deref(),
            Some("b")
        );
        assert_eq!(
            names(&players[..2], &[":1.1", ":1.2"]).as_deref(),
            Some("a")
        );
        assert_eq!(names(&players, &[":1.4", ":1.3"]).as_deref(), Some("d"));
        // Unknown to the memory: the bus's order, after the known ones.
        assert_eq!(names(&players[..2], &[]).as_deref(), Some("a"));
        assert_eq!(names(&players[..2], &[":1.2"]).as_deref(), Some("b"));
    }

    #[test]
    fn no_players_is_none() {
        assert_eq!(names(&[], &[":1.1"]), None);
    }

    #[test]
    fn the_memory_is_most_recent_first_and_bounded() {
        let mut recent = Vec::new();
        for i in 0..12 {
            note_played(&mut recent, &format!(":1.{i}"));
        }
        assert_eq!(recent.len(), RECENT);
        assert_eq!(recent[0], ":1.11");
        note_played(&mut recent, ":1.8");
        assert_eq!(&recent[..2], [":1.8", ":1.11"]);
        assert_eq!(recent.len(), RECENT);
    }

    /// A stand-in for the bus daemon's name service.
    struct FakeBus(Vec<(String, String)>);

    #[zbus::interface(name = "org.freedesktop.DBus")]
    impl FakeBus {
        fn list_names(&self) -> Vec<String> {
            let mut names = vec!["org.freedesktop.DBus".to_owned(), ":1.7".to_owned()];
            names.extend(self.0.iter().map(|(name, _)| name.clone()));
            names
        }
        fn get_name_owner(&self, name: &str) -> zbus::fdo::Result<String> {
            self.0
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, owner)| owner.clone())
                .ok_or_else(|| zbus::fdo::Error::NameHasNoOwner(name.to_owned()))
        }
    }

    /// A stand-in for a player: reports a status and records the calls it gets.
    struct FakePlayer {
        status: &'static str,
        calls: Arc<Mutex<Vec<&'static str>>>,
    }

    #[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
    impl FakePlayer {
        #[zbus(property)]
        fn playback_status(&self) -> String {
            self.status.to_owned()
        }
        fn play_pause(&self) {
            self.calls.lock().unwrap().push("PlayPause");
        }
        fn pause(&self) {
            self.calls.lock().unwrap().push("Pause");
        }
        fn next(&self) {
            self.calls.lock().unwrap().push("Next");
        }
        fn previous(&self) {
            self.calls.lock().unwrap().push("Previous");
        }
        fn stop(&self) {
            self.calls.lock().unwrap().push("Stop");
        }
    }

    /// The bus lists one mock player (and a name that isn't one); `(server, client,
    /// calls)`.
    fn mock(status: &'static str) -> (Connection, Mpris, Arc<Mutex<Vec<&'static str>>>) {
        let (server, client) = crate::testing::pair().unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let bus = FakeBus(vec![
            ("org.mpris.MediaPlayer2.mock".to_owned(), ":1.42".to_owned()),
            ("org.example.Other".to_owned(), ":1.43".to_owned()),
        ]);
        let player = FakePlayer {
            status,
            calls: Arc::clone(&calls),
        };
        {
            let objects = server.object_server();
            objects.at(BUS_PATH, bus).unwrap();
            objects.at(PATH, player).unwrap();
        }
        (server, Mpris::new(client), calls)
    }

    #[test]
    fn players_are_listed_with_their_status() {
        let (_server, mpris, _) = mock("Paused");
        assert_eq!(
            mpris.players().unwrap(),
            [Player {
                name: "org.mpris.MediaPlayer2.mock".to_owned(),
                owner: ":1.42".to_owned(),
                status: PlaybackStatus::Paused,
            }]
        );
    }

    #[test]
    fn keys_reach_the_active_player() {
        let (_server, mpris, calls) = mock("Playing");
        for command in [
            Command::PlayPause,
            Command::Pause,
            Command::Next,
            Command::Previous,
            Command::Stop,
        ] {
            let player = mpris.send(command).unwrap().expect("a player");
            assert_eq!(player.name, "org.mpris.MediaPlayer2.mock");
        }
        assert_eq!(
            *calls.lock().unwrap(),
            ["PlayPause", "Pause", "Next", "Previous", "Stop"]
        );
    }

    #[test]
    fn a_player_found_playing_is_remembered_as_the_last_to_play() {
        let (_server, mpris, _) = mock("Playing");
        assert!(mpris.recent.lock().unwrap().is_empty());
        mpris.active().unwrap().expect("a player");
        assert_eq!(*mpris.recent.lock().unwrap(), [":1.42"]);
        // A paused one is not.
        let (_server, mpris, _) = mock("Paused");
        mpris.active().unwrap().expect("a player");
        assert!(mpris.recent.lock().unwrap().is_empty());
    }

    #[test]
    fn without_players_nothing_is_sent() {
        let (server, client) = crate::testing::pair().unwrap();
        server
            .object_server()
            .at(BUS_PATH, FakeBus(Vec::new()))
            .unwrap();
        let mpris = Mpris::new(client);
        assert_eq!(mpris.send(Command::PlayPause).unwrap(), None);
    }
}
