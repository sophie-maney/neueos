//! The notification server against a scripted client (task 3.15a's acceptance), over a
//! peer-to-peer connection so no bus daemon is needed: every method and signal,
//! replace-id and each close reason.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use neue_dbus::notifications::{
    CloseReason, INTERFACE, Notifications, PATH, Request, Timeout, Urgency,
};
use neue_dbus::testing::pair;
use zbus::MatchRule;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type as MessageType;
use zbus::zvariant::Value;

const WAIT: Duration = Duration::from_secs(5);

struct Setup {
    server: Notifications,
    client: Connection,
    requests: Receiver<Request>,
    signals: Receiver<(String, u32, String)>,
    // Kept so the server's connection stays open.
    _service: Connection,
}

fn setup() -> Setup {
    let (service, client) = pair().unwrap();
    let (tx, requests) = mpsc::channel();
    let server = Notifications::serve(&service, move |r| tx.send(r).unwrap()).unwrap();
    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .interface(INTERFACE)
        .unwrap()
        .build();
    let messages = MessageIterator::for_match_rule(rule, &client, Some(64)).unwrap();
    let (stx, signals) = mpsc::channel();
    std::thread::spawn(move || {
        for message in messages.flatten() {
            let member = message.header().member().unwrap().to_string();
            let body = message.body();
            let (id, arg) = if member == "NotificationClosed" {
                let (id, reason): (u32, u32) = body.deserialize().unwrap();
                (id, reason.to_string())
            } else {
                body.deserialize::<(u32, String)>().unwrap()
            };
            if stx.send((member, id, arg)).is_err() {
                return;
            }
        }
    });
    Setup {
        server,
        client,
        requests,
        signals,
        _service: service,
    }
}

impl Setup {
    fn call<B, R>(&self, method: &str, body: &B) -> R
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
        R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
    {
        self.client
            .call_method(None::<()>, PATH, Some(INTERFACE), method, body)
            .unwrap()
            .body()
            .deserialize()
            .unwrap()
    }

    fn notify(&self, replaces: u32, summary: &str, actions: &[&str], timeout: i32) -> u32 {
        let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
        hints.insert("urgency", Value::U8(2));
        hints.insert("desktop-entry", Value::from("org.example.Chat"));
        self.call(
            "Notify",
            &(
                "Chat",
                replaces,
                "chat-icon",
                summary,
                "Body text",
                actions.to_vec(),
                hints,
                timeout,
            ),
        )
    }

    fn request(&self) -> Request {
        self.requests.recv_timeout(WAIT).unwrap()
    }

    fn signal(&self) -> (String, u32, String) {
        self.signals.recv_timeout(WAIT).unwrap()
    }
}

fn closed(id: u32, reason: CloseReason) -> (String, u32, String) {
    ("NotificationClosed".into(), id, (reason as u32).to_string())
}

#[test]
fn capabilities_and_server_information() {
    let s = setup();
    let caps: Vec<String> = s.call("GetCapabilities", &());
    assert!(caps.contains(&"actions".to_owned()));
    assert!(caps.contains(&"body".to_owned()));
    let (name, vendor, _version, spec): (String, String, String, String) =
        s.call("GetServerInformation", &());
    assert_eq!(
        (name.as_str(), vendor.as_str(), spec.as_str()),
        ("neue-shell", "neueOS", "1.2")
    );
}

#[test]
fn notify_hands_on_the_parsed_notification_and_replaces_by_id() {
    let s = setup();
    let id = s.notify(0, "Hello", &["default", "", "reply", "Reply"], -1);
    assert_eq!(id, 1);
    let Request::Notify(n) = s.request() else {
        panic!("expected a notification")
    };
    assert_eq!((n.id, n.replaces), (1, false));
    assert_eq!((n.app_name.as_str(), n.summary.as_str()), ("Chat", "Hello"));
    assert_eq!(n.body, "Body text");
    assert_eq!(n.hints.urgency, Urgency::Critical);
    assert_eq!(n.hints.desktop_entry.as_deref(), Some("org.example.Chat"));
    assert_eq!(n.timeout, Timeout::Default);
    assert!(n.has_default_action());
    assert_eq!(
        n.buttons().collect::<Vec<_>>(),
        [&("reply".to_owned(), "Reply".to_owned())]
    );

    assert_eq!(s.notify(0, "Second", &[], 0), 2);
    let Request::Notify(n) = s.request() else {
        panic!("expected a notification")
    };
    assert_eq!((n.id, n.timeout), (2, Timeout::Never));

    // Replacing an open one keeps its id.
    assert_eq!(s.notify(1, "Hello again", &[], 3000), 1);
    let Request::Notify(n) = s.request() else {
        panic!("expected a notification")
    };
    assert_eq!((n.id, n.replaces), (1, true));
    assert_eq!(n.summary, "Hello again");
    assert_eq!(n.timeout, Timeout::After(Duration::from_millis(3000)));
}

#[test]
fn an_app_closing_its_notification_is_reason_3() {
    let s = setup();
    let id = s.notify(0, "Bye", &[], -1);
    s.request();
    let () = s.call("CloseNotification", &(id,));
    assert_eq!(s.signal(), closed(id, CloseReason::Closed));
    assert_eq!(s.request(), Request::Close(id));
    assert!(!s.server.is_open(id));
    // Closing it again (or an id never used) is quietly ignored.
    let () = s.call("CloseNotification", &(id,));
    let () = s.call("CloseNotification", &(77u32,));
    assert!(s.requests.recv_timeout(Duration::from_millis(200)).is_err());
    assert!(s.signals.recv_timeout(Duration::from_millis(200)).is_err());
}

#[test]
fn the_shell_closes_with_its_reasons() {
    let s = setup();
    for reason in [
        CloseReason::Expired,
        CloseReason::Dismissed,
        CloseReason::Undefined,
    ] {
        let id = s.notify(0, "x", &[], -1);
        s.request();
        assert!(s.server.close(id, reason).unwrap());
        assert_eq!(s.signal(), closed(id, reason));
        assert!(!s.server.close(id, reason).unwrap(), "already closed");
    }
    assert!(s.signals.recv_timeout(Duration::from_millis(200)).is_err());
}

#[test]
fn actions_send_the_token_first() {
    let s = setup();
    let id = s.notify(0, "Ping", &["default", "Open", "reply", "Reply"], -1);
    s.request();
    assert!(s.server.invoke(id, "default", Some("token-1")).unwrap());
    assert_eq!(s.signal(), ("ActivationToken".into(), id, "token-1".into()));
    assert_eq!(s.signal(), ("ActionInvoked".into(), id, "default".into()));
    // Without a token (the compositor gave none) only the action goes out.
    assert!(s.server.invoke(id, "reply", None).unwrap());
    assert_eq!(s.signal(), ("ActionInvoked".into(), id, "reply".into()));
    // An action leaves it open; the shell closes it as dismissed.
    assert!(s.server.is_open(id));
    assert!(s.server.close(id, CloseReason::Dismissed).unwrap());
    assert_eq!(s.signal(), closed(id, CloseReason::Dismissed));
    assert!(!s.server.invoke(id, "default", None).unwrap(), "closed");
    assert!(s.signals.recv_timeout(Duration::from_millis(200)).is_err());
}

#[test]
fn a_second_server_on_the_same_path_is_refused() {
    let s = setup();
    let (service, _client) = pair().unwrap();
    let first = Notifications::serve(&service, |_| {}).unwrap();
    assert!(Notifications::serve(&service, |_| {}).is_err());
    drop(first);
    assert!(
        Notifications::serve(&service, |_| {}).is_ok(),
        "freed on drop"
    );
    drop(s);
}
