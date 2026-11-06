//! What the backend reports and the arithmetic behind it, without PipeWire.

use serde::{Deserialize, Serialize};

/// Which way audio goes through a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// An output (`Audio/Sink`): speakers, headphones.
    Sink,
    /// An input (`Audio/Source`, or a virtual one): a microphone.
    Source,
}

impl Kind {
    /// The kind of a node with this `media.class`, if it's an audio device we list.
    pub(crate) fn from_media_class(class: &str) -> Option<Self> {
        match class {
            "Audio/Sink" => Some(Self::Sink),
            "Audio/Source" | "Audio/Source/Virtual" => Some(Self::Source),
            _ => None,
        }
    }

    /// The `default` metadata's key for the default the session manager chose.
    pub(crate) fn default_key(self) -> &'static str {
        match self {
            Self::Sink => "default.audio.sink",
            Self::Source => "default.audio.source",
        }
    }

    /// The key that records the user's choice; the session manager follows it whenever
    /// that device is present (`wpctl set-default` writes it too).
    pub(crate) fn configured_key(self) -> &'static str {
        match self {
            Self::Sink => "default.configured.audio.sink",
            Self::Source => "default.configured.audio.source",
        }
    }
}

/// An audio sink or source.
#[derive(Debug, Clone, PartialEq)]
pub struct Endpoint {
    /// The PipeWire global id; valid until PipeWire restarts or the device goes away.
    pub id: u32,
    /// `node.name`, which the `default` metadata refers to.
    pub name: String,
    /// `node.description` (or the nick, or the name): what a menu shows.
    pub description: String,
    /// The loudest channel's volume on the cubic scale ([`cubic`]): 0 is silent, 1 is
    /// full; above 1 is amplified.
    pub volume: f64,
    pub muted: bool,
}

/// Everything the backend knows, sent whole on every change.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    /// Whether the thread is connected to PipeWire. Without it the lists are empty.
    pub connected: bool,
    /// The sinks, by id.
    pub sinks: Vec<Endpoint>,
    /// The sources, by id.
    pub sources: Vec<Endpoint>,
    /// The default sink's id, once the metadata names one that exists.
    pub default_sink: Option<u32>,
    /// The default source's id, likewise.
    pub default_source: Option<u32>,
}

impl State {
    /// The sinks or the sources.
    pub fn endpoints(&self, kind: Kind) -> &[Endpoint] {
        match kind {
            Kind::Sink => &self.sinks,
            Kind::Source => &self.sources,
        }
    }

    /// The default sink or source.
    pub fn default_endpoint(&self, kind: Kind) -> Option<&Endpoint> {
        let id = match kind {
            Kind::Sink => self.default_sink,
            Kind::Source => self.default_source,
        }?;
        self.endpoints(kind).iter().find(|e| e.id == id)
    }
}

/// A linear amplitude factor (what PipeWire stores in `channelVolumes`) on the cubic scale
/// `wpctl` shows and volume sliders use.
pub fn cubic(linear: f64) -> f64 {
    linear.max(0.0).cbrt()
}

/// The inverse of [`cubic`].
pub fn linear(cubic: f64) -> f64 {
    cubic.max(0.0).powi(3)
}

/// The cubic volume of a node with these channel volumes: the loudest channel's.
pub(crate) fn level(channels: &[f32]) -> f64 {
    let loudest = channels.iter().copied().fold(0.0_f32, f32::max);
    cubic(f64::from(loudest))
}

/// `channels` set to the cubic volume `target`, keeping their balance: the loudest
/// channel lands on `target` and the others keep their ratio to it. Silent channels (or
/// none known yet: `count` of them) all get `target`.
pub(crate) fn scaled(channels: &[f32], count: usize, target: f64) -> Vec<f32> {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "volumes are well inside f32's range; PipeWire stores them as f32"
    )]
    let target = linear(target) as f32;
    let loudest = channels.iter().copied().fold(0.0_f32, f32::max);
    if channels.is_empty() || loudest <= 0.0 {
        return vec![target; channels.len().max(count)];
    }
    channels.iter().map(|c| c / loudest * target).collect()
}

/// The JSON the `default` metadata holds for a device: `{ "name": "<node.name>" }`.
#[derive(Debug, Serialize, Deserialize)]
struct Named {
    name: String,
}

/// The node name in a `default` metadata value; `None` for a malformed one.
pub(crate) fn parse_default(json: &str) -> Option<String> {
    serde_json::from_str::<Named>(json).ok().map(|n| n.name)
}

/// The `default` metadata value naming `name`.
pub(crate) fn default_json(name: &str) -> String {
    serde_json::json!({ "name": name }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn cubic_matches_wpctl() {
        // `wpctl set-volume @DEFAULT_AUDIO_SINK@ 0.5` stores 0.125 in channelVolumes.
        assert!(close(linear(0.5), 0.125));
        assert!(close(cubic(0.125), 0.5));
        assert!(close(cubic(1.0), 1.0));
        assert!(close(cubic(-1.0), 0.0));
        assert!(close(linear(cubic(0.3)), 0.3));
    }

    #[test]
    fn level_is_the_loudest_channel() {
        assert!(close(level(&[0.125, 0.064]), 0.5));
        assert!(close(level(&[]), 0.0));
    }

    #[test]
    fn scaling_keeps_the_balance() {
        let out = scaled(&[0.125, 0.0625], 2, 1.0);
        assert!(close(f64::from(out[0]), 1.0));
        assert!(close(f64::from(out[1]), 0.5));
        assert_eq!(scaled(&[0.0, 0.0], 2, 0.5), vec![0.125, 0.125]);
        assert_eq!(scaled(&[], 2, 1.0), vec![1.0, 1.0]);
    }

    #[test]
    fn default_metadata_json() {
        assert_eq!(
            parse_default(r#"{ "name": "alsa_output.pci-0000_00_1f.3.analog-stereo" }"#).as_deref(),
            Some("alsa_output.pci-0000_00_1f.3.analog-stereo")
        );
        assert_eq!(parse_default("nonsense"), None);
        let json = default_json("a \"quoted\" sink");
        assert_eq!(parse_default(&json).as_deref(), Some("a \"quoted\" sink"));
    }

    #[test]
    fn the_default_endpoint_must_exist() {
        let sink = Endpoint {
            id: 31,
            name: "out".into(),
            description: "Out".into(),
            volume: 0.5,
            muted: false,
        };
        let mut state = State {
            connected: true,
            sinks: vec![sink.clone()],
            default_sink: Some(31),
            ..State::default()
        };
        assert_eq!(state.default_endpoint(Kind::Sink), Some(&sink));
        assert_eq!(state.default_endpoint(Kind::Source), None);
        state.default_sink = Some(40);
        assert_eq!(state.default_endpoint(Kind::Sink), None);
    }

    #[test]
    fn media_classes() {
        assert_eq!(Kind::from_media_class("Audio/Sink"), Some(Kind::Sink));
        assert_eq!(
            Kind::from_media_class("Audio/Source/Virtual"),
            Some(Kind::Source)
        );
        assert_eq!(Kind::from_media_class("Stream/Output/Audio"), None);
    }
}
