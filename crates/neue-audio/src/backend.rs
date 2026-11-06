//! The PipeWire thread: a main loop that binds the audio nodes and devices and the
//! `default` metadata, keeps a model of them, and applies [`Audio`]'s commands.
//!
//! Volume and mute are written as `pipewire-pulse` and `wpctl` write them: on the device's
//! active route when the node belongs to a card (so the hardware mixer moves and the
//! session manager saves it), on the node's `Props` otherwise (virtual and network
//! devices). They are read from the node's `Props`, which mirror the route.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Cursor;
use std::rc::Rc;
use std::time::Duration;

use pipewire as pw;
use pw::channel::Receiver;
use pw::device::{Device, DeviceListener};
use pw::metadata::{Metadata, MetadataListener};
use pw::node::{Node, NodeListener};
use pw::registry::{GlobalObject, RegistryRc};
use pw::spa::param::ParamType;
use pw::spa::pod::deserialize::PodDeserializer;
use pw::spa::pod::serialize::PodSerializer;
use pw::spa::pod::{Object, Pod, Property, Value, ValueArray};
use pw::spa::sys as spa;
use pw::spa::utils::dict::DictRef;
use pw::types::ObjectType;

use crate::model::{self, Endpoint, Kind, State};

/// How long to wait before connecting again after PipeWire went away or refused us.
const RETRY: Duration = Duration::from_secs(2);

/// What the PipeWire thread reports to.
type Notify = Rc<dyn Fn(State)>;

/// A change queued for the PipeWire thread.
enum Command {
    Volume { id: u32, volume: f64 },
    Mute { id: u32, muted: bool },
    Default { kind: Kind, name: String },
}

/// The handle on the PipeWire thread. Clone it freely; the thread runs until the process
/// ends.
#[derive(Clone)]
pub struct Audio {
    commands: pw::channel::Sender<Command>,
}

impl std::fmt::Debug for Audio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Audio").finish_non_exhaustive()
    }
}

impl Audio {
    /// Starts the PipeWire thread. `on_change` runs on that thread with the whole state
    /// whenever it changes, including an empty one when PipeWire goes away.
    ///
    /// # Errors
    /// When the thread can't be spawned.
    pub fn start(on_change: impl Fn(State) + Send + 'static) -> std::io::Result<Self> {
        let (commands, receiver) = pw::channel::channel();
        std::thread::Builder::new()
            .name("neue-audio".into())
            .spawn(move || run(receiver, Rc::new(on_change)))?;
        Ok(Self { commands })
    }

    /// Sets node `id`'s volume, on the cubic scale ([`crate::cubic`]); the balance between
    /// channels stays.
    pub fn set_volume(&self, id: u32, volume: f64) {
        self.send(Command::Volume { id, volume });
    }

    /// Mutes or unmutes node `id`.
    pub fn set_muted(&self, id: u32, muted: bool) {
        self.send(Command::Mute { id, muted });
    }

    /// Makes the node named `name` (`node.name`) the default sink or source. The session
    /// manager remembers the choice and moves streams to it.
    pub fn set_default(&self, kind: Kind, name: &str) {
        self.send(Command::Default {
            kind,
            name: name.to_owned(),
        });
    }

    fn send(&self, command: Command) {
        if self.commands.send(command).is_err() {
            tracing::warn!("the audio thread has ended");
        }
    }
}

/// The thread's body: connect, run until the connection ends, wait, and again.
fn run(mut receiver: Receiver<Command>, notify: Notify) {
    pw::init();
    let mut warned = false;
    loop {
        let (back, result) = connect(receiver, &notify);
        receiver = back;
        match result {
            Ok(()) => {
                tracing::warn!("lost PipeWire; reconnecting");
                warned = false;
                notify(State::default());
            }
            Err(error) if !warned => {
                tracing::warn!(%error, "can't connect to PipeWire; retrying every {RETRY:?}");
                warned = true;
            }
            Err(error) => tracing::debug!(%error, "can't connect to PipeWire"),
        }
        std::thread::sleep(RETRY);
    }
}

/// One connection: returns when PipeWire goes away, or fails to connect. Gives the
/// command receiver back either way.
fn connect(
    receiver: Receiver<Command>,
    notify: &Notify,
) -> (Receiver<Command>, Result<(), pw::Error>) {
    // Declared first, so dropped last: everything below borrows from the loop.
    let main_loop = match pw::main_loop::MainLoopRc::new(None) {
        Ok(main_loop) => main_loop,
        Err(error) => return (receiver, Err(error)),
    };
    let context = match pw::context::ContextRc::new(&main_loop, None) {
        Ok(context) => context,
        Err(error) => return (receiver, Err(error)),
    };
    let core = match context.connect_rc(None) {
        Ok(core) => core,
        Err(error) => return (receiver, Err(error)),
    };
    let registry = match core.get_registry_rc() {
        Ok(registry) => registry,
        Err(error) => return (receiver, Err(error)),
    };
    let session = Rc::new(RefCell::new(Session::new(Rc::clone(notify))));

    let quit = main_loop.downgrade();
    let core_listener = core
        .add_listener_local()
        .error(move |id, _seq, res, message| {
            if id == pw::core::PW_ID_CORE {
                tracing::warn!(res, message, "PipeWire connection error");
                if let Some(main_loop) = quit.upgrade() {
                    main_loop.quit();
                }
            } else {
                tracing::debug!(id, res, message, "PipeWire object error");
            }
        })
        .register();

    let (weak_registry, added, removed) = (
        registry.downgrade(),
        Rc::downgrade(&session),
        Rc::downgrade(&session),
    );
    let registry_listener = registry
        .add_listener_local()
        .global(move |global| {
            if let (Some(registry), Some(session)) = (weak_registry.upgrade(), added.upgrade()) {
                bind(&session, &registry, global);
            }
        })
        .global_remove(move |id| {
            if let Some(session) = removed.upgrade() {
                session.borrow_mut().remove(id);
            }
        })
        .register();

    let commands = Rc::downgrade(&session);
    let attached = receiver.attach(main_loop.loop_(), move |command| {
        if let Some(session) = commands.upgrade() {
            session.borrow().apply(command);
        }
    });

    tracing::info!("connected to PipeWire");
    session.borrow_mut().changed();
    main_loop.run();

    let receiver = attached.deattach();
    drop(registry_listener);
    drop(core_listener);
    // The proxies and their listeners go before the core they belong to.
    drop(session);
    (receiver, Ok(()))
}

/// A sink or source node.
struct NodeEntry {
    kind: Kind,
    proxy: Node,
    _listener: NodeListener,
    name: String,
    description: String,
    /// The card device (`device.id`) and its route's device index
    /// (`card.profile.device`), for nodes that belong to a card.
    route: Option<(u32, i32)>,
    /// Linear channel volumes, from `Props`; `None` until the first `Props` arrive (the
    /// node isn't listed until then).
    channels: Option<Vec<f32>>,
    muted: bool,
}

/// A card (`Audio/Device`) and its active routes.
struct DeviceEntry {
    proxy: Device,
    _listener: DeviceListener,
    /// Route index by card device index (`card.profile.device`).
    routes: HashMap<i32, i32>,
}

/// The `default` metadata.
struct DefaultsEntry {
    id: u32,
    proxy: Metadata,
    _listener: MetadataListener,
}

/// The model of one connection.
struct Session {
    notify: Notify,
    nodes: HashMap<u32, NodeEntry>,
    devices: HashMap<u32, DeviceEntry>,
    metadata: Option<DefaultsEntry>,
    /// The default sink's and source's `node.name`, from the metadata.
    default_sink: Option<String>,
    default_source: Option<String>,
    /// What was last reported, so only changes are.
    reported: Option<State>,
}

impl Session {
    fn new(notify: Notify) -> Self {
        Self {
            notify,
            nodes: HashMap::new(),
            devices: HashMap::new(),
            metadata: None,
            default_sink: None,
            default_source: None,
            reported: None,
        }
    }

    fn remove(&mut self, id: u32) {
        let removed = self.nodes.remove(&id).is_some() || self.devices.remove(&id).is_some();
        if self.metadata.as_ref().is_some_and(|m| m.id == id) {
            self.metadata = None;
            self.default_sink = None;
            self.default_source = None;
            self.changed();
        } else if removed {
            self.changed();
        }
    }

    fn node_info(&mut self, id: u32, props: Option<&DictRef>) {
        let (Some(node), Some(props)) = (self.nodes.get_mut(&id), props) else {
            return;
        };
        if let Some(name) = props.get("node.name") {
            name.clone_into(&mut node.name);
        }
        if let Some(description) = description(props) {
            description.clone_into(&mut node.description);
        }
        let device = props.get("device.id").and_then(|v| v.parse().ok());
        let card = props
            .get("card.profile.device")
            .and_then(|v| v.parse().ok());
        node.route = device.zip(card);
        self.changed();
    }

    fn node_props(&mut self, id: u32, pod: &Pod) {
        let Some(node) = self.nodes.get_mut(&id) else {
            return;
        };
        let Some(object) = object(pod) else {
            return;
        };
        let mut seen = false;
        for property in object.properties {
            match (property.key, property.value) {
                (spa::SPA_PROP_channelVolumes, Value::ValueArray(ValueArray::Float(volumes))) => {
                    node.channels = Some(volumes);
                    seen = true;
                }
                (spa::SPA_PROP_mute, Value::Bool(muted)) => {
                    node.muted = muted;
                    seen = true;
                }
                _ => {}
            }
        }
        // A node may send a second `Props` without volumes (its other parameters).
        if seen {
            self.changed();
        }
    }

    fn device_route(&mut self, id: u32, pod: &Pod) {
        let Some(device) = self.devices.get_mut(&id) else {
            return;
        };
        let Some(object) = object(pod) else {
            return;
        };
        let (mut index, mut card) = (None, None);
        for property in object.properties {
            match (property.key, property.value) {
                (spa::SPA_PARAM_ROUTE_index, Value::Int(i)) => index = Some(i),
                (spa::SPA_PARAM_ROUTE_device, Value::Int(d)) => card = Some(d),
                _ => {}
            }
        }
        if let (Some(index), Some(card)) = (index, card) {
            device.routes.insert(card, index);
        }
    }

    /// A `default` metadata property changed; `key: None` clears them all.
    fn default_changed(&mut self, key: Option<&str>, value: Option<&str>) {
        let named = value.and_then(model::parse_default);
        match key {
            None => {
                self.default_sink = None;
                self.default_source = None;
            }
            Some(key) if key == Kind::Sink.default_key() => self.default_sink = named,
            Some(key) if key == Kind::Source.default_key() => self.default_source = named,
            Some(_) => return,
        }
        self.changed();
    }

    /// Reports the state if it differs from the last report.
    fn changed(&mut self) {
        let state = self.state();
        if self.reported.as_ref() != Some(&state) {
            self.reported = Some(state.clone());
            (self.notify)(state);
        }
    }

    fn state(&self) -> State {
        let mut sinks = Vec::new();
        let mut sources = Vec::new();
        for (&id, node) in &self.nodes {
            let Some(channels) = &node.channels else {
                continue;
            };
            let endpoint = Endpoint {
                id,
                name: node.name.clone(),
                description: node.description.clone(),
                volume: model::level(channels),
                muted: node.muted,
            };
            match node.kind {
                Kind::Sink => sinks.push(endpoint),
                Kind::Source => sources.push(endpoint),
            }
        }
        sinks.sort_by_key(|e| e.id);
        sources.sort_by_key(|e| e.id);
        let find = |list: &[Endpoint], name: &Option<String>| {
            let name = name.as_deref()?;
            list.iter().find(|e| e.name == name).map(|e| e.id)
        };
        State {
            connected: true,
            default_sink: find(&sinks, &self.default_sink),
            default_source: find(&sources, &self.default_source),
            sinks,
            sources,
        }
    }

    fn apply(&self, command: Command) {
        match command {
            Command::Volume { id, volume } => {
                let Some(node) = self.nodes.get(&id) else {
                    tracing::debug!(id, "no such audio node");
                    return;
                };
                let Some(channels) = node.channels.as_deref().filter(|c| !c.is_empty()) else {
                    tracing::debug!(id, "the node's channels aren't known yet");
                    return;
                };
                let volumes = model::scaled(channels, channels.len(), volume);
                let volumes = Property::new(
                    spa::SPA_PROP_channelVolumes,
                    Value::ValueArray(ValueArray::Float(volumes)),
                );
                self.write(node, vec![volumes]);
            }
            Command::Mute { id, muted } => {
                let Some(node) = self.nodes.get(&id) else {
                    tracing::debug!(id, "no such audio node");
                    return;
                };
                self.write(
                    node,
                    vec![Property::new(spa::SPA_PROP_mute, Value::Bool(muted))],
                );
            }
            Command::Default { kind, name } => {
                let Some(metadata) = &self.metadata else {
                    tracing::warn!("no default metadata: is a session manager running?");
                    return;
                };
                metadata.proxy.set_property(
                    0,
                    kind.configured_key(),
                    Some("Spa:String:JSON"),
                    Some(&model::default_json(&name)),
                );
            }
        }
    }

    /// Writes `props` on the node's active route, or on the node without one.
    fn write(&self, node: &NodeEntry, props: Vec<Property>) {
        let route = node.route.and_then(|(device, card)| {
            let device = self.devices.get(&device)?;
            Some((device, card, *device.routes.get(&card)?))
        });
        if let Some((device, card, index)) = route {
            let route = Object {
                type_: spa::SPA_TYPE_OBJECT_ParamRoute,
                id: spa::SPA_PARAM_Route,
                properties: vec![
                    Property::new(spa::SPA_PARAM_ROUTE_index, Value::Int(index)),
                    Property::new(spa::SPA_PARAM_ROUTE_device, Value::Int(card)),
                    Property::new(
                        spa::SPA_PARAM_ROUTE_props,
                        Value::Object(Object {
                            type_: spa::SPA_TYPE_OBJECT_Props,
                            id: spa::SPA_PARAM_Route,
                            properties: props,
                        }),
                    ),
                    // Let the session manager remember it for this port.
                    Property::new(spa::SPA_PARAM_ROUTE_save, Value::Bool(true)),
                ],
            };
            if let Some(bytes) = serialize(route)
                && let Some(pod) = Pod::from_bytes(&bytes)
            {
                device.proxy.set_param(ParamType::Route, 0, pod);
            }
        } else {
            let props = Object {
                type_: spa::SPA_TYPE_OBJECT_Props,
                id: spa::SPA_PARAM_Props,
                properties: props,
            };
            if let Some(bytes) = serialize(props)
                && let Some(pod) = Pod::from_bytes(&bytes)
            {
                node.proxy.set_param(ParamType::Props, 0, pod);
            }
        }
    }
}

/// Binds a new global if it is one we follow: an audio sink or source, a card, or the
/// `default` metadata.
fn bind(session: &Rc<RefCell<Session>>, registry: &RegistryRc, global: &GlobalObject<&DictRef>) {
    let id = global.id;
    let prop = |key: &str| global.props.and_then(|p| p.get(key));
    match global.type_ {
        ObjectType::Node => {
            let Some(kind) = prop("media.class").and_then(Kind::from_media_class) else {
                return;
            };
            let proxy: Node = match registry.bind(global) {
                Ok(proxy) => proxy,
                Err(error) => {
                    tracing::warn!(%error, id, "can't bind an audio node");
                    return;
                }
            };
            let (info, param) = (Rc::downgrade(session), Rc::downgrade(session));
            let listener = proxy
                .add_listener_local()
                .info(move |node| {
                    if let Some(session) = info.upgrade() {
                        session.borrow_mut().node_info(id, node.props());
                    }
                })
                .param(move |_seq, kind, _index, _next, pod| {
                    if kind == ParamType::Props
                        && let Some(pod) = pod
                        && let Some(session) = param.upgrade()
                    {
                        session.borrow_mut().node_props(id, pod);
                    }
                })
                .register();
            proxy.subscribe_params(&[ParamType::Props]);
            let name = prop("node.name").unwrap_or_default().to_owned();
            let description = global
                .props
                .and_then(description)
                .unwrap_or(&name)
                .to_owned();
            session.borrow_mut().nodes.insert(
                id,
                NodeEntry {
                    kind,
                    proxy,
                    _listener: listener,
                    name,
                    description,
                    route: None,
                    channels: None,
                    muted: false,
                },
            );
        }
        ObjectType::Device if prop("media.class") == Some("Audio/Device") => {
            let proxy: Device = match registry.bind(global) {
                Ok(proxy) => proxy,
                Err(error) => {
                    tracing::warn!(%error, id, "can't bind an audio device");
                    return;
                }
            };
            let param = Rc::downgrade(session);
            let listener = proxy
                .add_listener_local()
                .param(move |_seq, kind, _index, _next, pod| {
                    if kind == ParamType::Route
                        && let Some(pod) = pod
                        && let Some(session) = param.upgrade()
                    {
                        session.borrow_mut().device_route(id, pod);
                    }
                })
                .register();
            proxy.subscribe_params(&[ParamType::Route]);
            session.borrow_mut().devices.insert(
                id,
                DeviceEntry {
                    proxy,
                    _listener: listener,
                    routes: HashMap::new(),
                },
            );
        }
        ObjectType::Metadata if prop("metadata.name") == Some("default") => {
            let proxy: Metadata = match registry.bind(global) {
                Ok(proxy) => proxy,
                Err(error) => {
                    tracing::warn!(%error, id, "can't bind the default metadata");
                    return;
                }
            };
            let property = Rc::downgrade(session);
            let listener = proxy
                .add_listener_local()
                .property(move |subject, key, _type, value| {
                    if subject == pw::core::PW_ID_CORE
                        && let Some(session) = property.upgrade()
                    {
                        session.borrow_mut().default_changed(key, value);
                    }
                    0
                })
                .register();
            session.borrow_mut().metadata = Some(DefaultsEntry {
                id,
                proxy,
                _listener: listener,
            });
        }
        _ => {}
    }
}

/// What a menu calls a node: its description, nick or name.
fn description(props: &DictRef) -> Option<&str> {
    props
        .get("node.description")
        .or_else(|| props.get("node.nick"))
        .filter(|s| !s.is_empty())
}

/// A param pod as an object, if it is one.
fn object(pod: &Pod) -> Option<Object> {
    match PodDeserializer::deserialize_any_from(pod.as_bytes()) {
        Ok((_, Value::Object(object))) => Some(object),
        Ok(_) => None,
        Err(error) => {
            tracing::debug!(?error, "unreadable PipeWire param");
            None
        }
    }
}

/// An object as pod bytes.
fn serialize(object: Object) -> Option<Vec<u8>> {
    match PodSerializer::serialize(Cursor::new(Vec::new()), &Value::Object(object)) {
        Ok((cursor, _)) => Some(cursor.into_inner()),
        Err(error) => {
            tracing::warn!(?error, "can't build a PipeWire param");
            None
        }
    }
}
