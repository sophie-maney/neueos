//! The volume and brightness keys and the on-screen display (task 3.14a).
//!
//! The compositor routes the keys to the shell (`neue_shell_v1`'s `system_key`); the driver
//! repeats the stepping ones. Each key changes a level ([`neue_shell::levels`]) and shows
//! it on the OSD: an overlay layer surface on the active output that lets the pointer
//! through. It is opened on the first key, fades out [`HOLD`] after the last one and is
//! closed once the fade is over, so the next key opens it on whichever output is active
//! then.
//!
//! The volume keys change the default sink's volume and mute, and the mic mute key the
//! default source's mute, through PipeWire ([`neue_audio`], task 3.14b). The brightness
//! is the screen's backlight, set through logind (`Session.SetBrightness`).
//!
//! The bar's volume extra (task 3.14c) shows the same default sink ([`Keys::volume_view`])
//! and its menu acts through the same PipeWire thread ([`Keys::volume_command`]).

use std::time::Instant;

use neue_audio::{Audio, Kind};
use neue_dbus::login::Login;
use neue_kit::desktop::{
    Anchor, ChordModifiers, LayerKeyboard, LayerSpec, Margin, Osd, OsdGlyph, ShellLayer, SystemKey,
    SystemKeyState,
};
use neue_kit::tokens::materials::osd::{BOTTOM_MARGIN, FADE, HOLD, RADIUS, SIZE};
use neue_kit::{Cx, Handle, WindowId};
use neue_shell::bar::{OutputView, VolumeCommand, VolumeView};
use neue_shell::levels::{self, Backlight, Volume};

/// The OSD's layer-shell namespace.
const NAMESPACE: &str = "neue-osd";

/// What the OSD's timers and workers report.
#[derive(Debug, Clone)]
pub enum Event {
    /// The backlight and a logind connection to set it with; `None` without a backlight.
    Backlight(Option<(Backlight, Login)>),
    /// PipeWire's sinks and sources changed.
    Audio(neue_audio::State),
    /// A `SetBrightness` call ended.
    Brightness(Result<(), String>),
    /// The hold timer fired.
    Hold,
    /// The fade that started with this generation should be over.
    Faded(u64),
}

/// The open OSD surface.
struct Surface {
    window: WindowId,
    osd: Handle<Osd>,
}

/// The keys' state and the OSD.
#[derive(Default)]
pub struct Keys {
    /// The PipeWire thread; `None` if it couldn't start.
    audio: Option<Audio>,
    /// What PipeWire last reported.
    sound: neue_audio::State,
    /// The default sink and the volume last asked for: repeats step from it, since
    /// PipeWire may not have answered yet. A new press reads the reported volume again.
    volume: Option<(u32, f64)>,
    backlight: Option<(Backlight, Login)>,
    /// The raw brightness last asked for: repeats step from it, since logind's write
    /// may not have reached sysfs yet. A new press reads sysfs again.
    brightness: Option<u32>,
    osd: Option<Surface>,
    /// When the OSD starts fading, pushed back by every key.
    hide_at: Option<Instant>,
    /// Whether a hold timer is running (one at a time, however fast the keys repeat).
    holding: bool,
    /// Counts fades, so a stale fade's end doesn't close a surface shown again since.
    fades: u64,
}

impl Keys {
    /// Starts the PipeWire thread, looks for the screen's backlight, and connects to
    /// logind if there is one.
    pub fn start<M: Send + 'static>(&mut self, cx: &mut Cx<'_, M>, wrap: fn(Event) -> M) {
        let sender = cx.sender();
        match Audio::start(move |state| {
            sender.send(wrap(Event::Audio(state)));
        }) {
            Ok(audio) => self.audio = Some(audio),
            Err(error) => tracing::warn!(%error, "no audio thread: the volume keys do nothing"),
        }
        cx.spawn(move |_| {
            let found = Backlight::find(levels::BACKLIGHTS.as_ref()).and_then(|backlight| {
                match Login::system() {
                    Ok(login) => Some((backlight, login)),
                    Err(error) => {
                        tracing::warn!(%error, "no system bus: the brightness keys do nothing");
                        None
                    }
                }
            });
            wrap(Event::Backlight(found))
        });
    }

    /// A system key from the compositor.
    pub fn key<M: Send + 'static>(
        &mut self,
        cx: &mut Cx<'_, M>,
        wrap: fn(Event) -> M,
        key: SystemKey,
        state: SystemKeyState,
        modifiers: ChordModifiers,
    ) {
        if state == SystemKeyState::Released {
            return;
        }
        let fine = levels::fine(modifiers);
        let up = match key {
            SystemKey::VolumeUp | SystemKey::VolumeDown | SystemKey::Mute => {
                self.volume_key(cx, wrap, key, state, fine);
                return;
            }
            SystemKey::MicMute => {
                self.mic_mute(cx, wrap);
                return;
            }
            SystemKey::BrightnessUp => true,
            SystemKey::BrightnessDown => false,
            // Keyboard backlights and media keys: not yet (task 3.14d).
            other => {
                tracing::debug!(key = ?other, "system key not handled");
                return;
            }
        };
        let Some((backlight, login)) = &self.backlight else {
            tracing::debug!("no backlight to change");
            return;
        };
        let now = match (state, self.brightness) {
            (SystemKeyState::Repeated, Some(raw)) => raw,
            _ => match backlight.read() {
                Ok(raw) => raw,
                Err(error) => {
                    tracing::warn!(%error, name = backlight.name, "can't read the backlight");
                    return;
                }
            },
        };
        let target = backlight.step(now, up, fine);
        let level = backlight.level(target);
        self.brightness = Some(target);
        if target != now {
            let (login, name) = (login.clone(), backlight.name.clone());
            cx.spawn(move |_| {
                wrap(Event::Brightness(
                    login
                        .set_brightness("backlight", &name, target)
                        .map_err(|e| e.to_string()),
                ))
            });
        }
        self.show(cx, wrap, OsdGlyph::Brightness, level);
    }

    /// Volume Up, Down or Mute: the default sink.
    fn volume_key<M: Send + 'static>(
        &mut self,
        cx: &mut Cx<'_, M>,
        wrap: fn(Event) -> M,
        key: SystemKey,
        state: SystemKeyState,
        fine: bool,
    ) {
        let (Some(audio), Some(sink)) = (&self.audio, self.sound.default_endpoint(Kind::Sink))
        else {
            tracing::debug!("no default sink");
            return;
        };
        let level = match (state, self.volume) {
            (SystemKeyState::Repeated, Some((id, level))) if id == sink.id => level,
            _ => sink.volume,
        };
        let mut volume = Volume {
            level,
            muted: sink.muted,
        };
        volume.apply(key, fine);
        if (volume.level - level).abs() > f64::EPSILON {
            audio.set_volume(sink.id, volume.level);
        }
        if volume.muted != sink.muted {
            audio.set_muted(sink.id, volume.muted);
        }
        tracing::debug!(
            sink = sink.name,
            level = volume.level,
            muted = volume.muted,
            "volume"
        );
        self.volume = Some((sink.id, volume.level));
        let glyph = if volume.muted {
            OsdGlyph::Muted
        } else {
            OsdGlyph::Volume
        };
        self.show(cx, wrap, glyph, volume.level.min(1.0));
    }

    /// Mic Mute: toggles the default source's mute.
    fn mic_mute<M: Send + 'static>(&mut self, cx: &mut Cx<'_, M>, wrap: fn(Event) -> M) {
        let (Some(audio), Some(source)) = (&self.audio, self.sound.default_endpoint(Kind::Source))
        else {
            tracing::debug!("no default source");
            return;
        };
        let muted = !source.muted;
        audio.set_muted(source.id, muted);
        tracing::debug!(source = source.name, muted, "microphone");
        let (glyph, level) = if muted {
            (OsdGlyph::MicrophoneMuted, 0.0)
        } else {
            (OsdGlyph::Microphone, source.volume.min(1.0))
        };
        self.show(cx, wrap, glyph, level);
    }

    /// The default sink as the volume extra shows it; `None` (hiding it) while PipeWire
    /// is away or has no default sink.
    pub fn volume_view(&self) -> Option<VolumeView> {
        volume_view(&self.sound)
    }

    /// Does what the volume menu chose. PipeWire's next report brings the new state.
    pub fn volume_command(&mut self, command: VolumeCommand) {
        let (Some(audio), Some(sink)) = (&self.audio, self.sound.default_endpoint(Kind::Sink))
        else {
            tracing::debug!(?command, "no default sink");
            return;
        };
        match command {
            VolumeCommand::SetLevel(level) => {
                audio.set_volume(sink.id, level);
                // Moving the slider up unmutes, as the volume keys do.
                if sink.muted && level > 0.0 {
                    audio.set_muted(sink.id, false);
                }
            }
            VolumeCommand::SetMuted(muted) => audio.set_muted(sink.id, muted),
            VolumeCommand::SetOutput(i) => match self.sound.sinks.get(i) {
                Some(output) => audio.set_default(Kind::Sink, &output.name),
                None => tracing::debug!(i, "no such output"),
            },
        }
    }

    /// A timer or worker reported.
    pub fn event<M: Send + 'static>(
        &mut self,
        cx: &mut Cx<'_, M>,
        wrap: fn(Event) -> M,
        event: Event,
    ) {
        match event {
            Event::Backlight(found) => {
                if let Some((backlight, _)) = &found {
                    tracing::info!(name = backlight.name, max = backlight.max, "backlight");
                }
                self.backlight = found;
            }
            Event::Audio(state) => {
                if state.connected != self.sound.connected {
                    tracing::info!(connected = state.connected, "PipeWire");
                }
                self.sound = state;
            }
            Event::Brightness(Ok(())) => {}
            Event::Brightness(Err(error)) => {
                tracing::warn!(%error, "setting the brightness failed");
                // Read sysfs again next time rather than trust the failed value.
                self.brightness = None;
            }
            Event::Hold => {
                self.holding = false;
                let Some(at) = self.hide_at else {
                    return;
                };
                let now = Instant::now();
                if now < at {
                    self.hold(cx, wrap, at - now);
                    return;
                }
                self.hide_at = None;
                if let Some(surface) = &self.osd {
                    surface.osd.hide(cx);
                    self.fades += 1;
                    let fade = self.fades;
                    cx.spawn(move |_| {
                        std::thread::sleep(FADE);
                        wrap(Event::Faded(fade))
                    });
                }
            }
            Event::Faded(fade) => {
                // Shown again since: hide_at is set and the surface stays.
                if fade == self.fades
                    && self.hide_at.is_none()
                    && let Some(surface) = self.osd.take()
                {
                    cx.close_window(surface.window);
                }
            }
        }
    }

    fn show<M: Send + 'static>(
        &mut self,
        cx: &mut Cx<'_, M>,
        wrap: fn(Event) -> M,
        glyph: OsdGlyph,
        level: f64,
    ) {
        if let Some(surface) = &self.osd {
            surface.osd.show(cx, glyph, level);
        } else {
            let mut osd = Handle::default();
            let root = Osd::new(glyph, level)
                .fade_in()
                .backdrop(cx.backdrop())
                .bind(&mut osd);
            #[expect(
                clippy::cast_possible_truncation,
                reason = "the OSD's 200 px side and margin"
            )]
            let (side, margin) = (SIZE as u32, BOTTOM_MARGIN as i32);
            let layer = LayerSpec {
                layer: ShellLayer::Overlay,
                margin: Margin {
                    bottom: margin,
                    ..Margin::default()
                },
                keyboard: LayerKeyboard::None,
                pointer: false,
                // The compositor puts it on the active output.
                output: None,
                ..LayerSpec::new(NAMESPACE, Anchor::BOTTOM)
            };
            let window = neue_kit::Window::new("Volume", (side, side), root)
                .layer(layer)
                .blur_behind_rounded(RADIUS);
            let window = cx.open_window(window);
            self.osd = Some(Surface { window, osd });
        }
        self.hide_at = Some(Instant::now() + HOLD);
        if !self.holding {
            self.hold(cx, wrap, HOLD);
        }
    }

    fn hold<M: Send + 'static>(
        &mut self,
        cx: &mut Cx<'_, M>,
        wrap: fn(Event) -> M,
        wait: std::time::Duration,
    ) {
        self.holding = true;
        cx.spawn(move |_| {
            std::thread::sleep(wait);
            wrap(Event::Hold)
        });
    }
}

/// `state`'s default sink as the volume extra shows it, its sinks in PipeWire's order (the
/// menu's indices are looked up in `state.sinks`).
fn volume_view(state: &neue_audio::State) -> Option<VolumeView> {
    if !state.connected {
        return None;
    }
    let sink = state.default_endpoint(Kind::Sink)?;
    let outputs = state
        .sinks
        .iter()
        .map(|s| OutputView {
            name: s.description.clone(),
            default: s.id == sink.id,
        })
        .collect();
    Some(VolumeView {
        level: sink.volume,
        muted: sink.muted,
        outputs,
    })
}

#[cfg(test)]
mod tests {
    use neue_audio::{Endpoint, State};

    use super::*;

    fn sink(id: u32, name: &str, description: &str, volume: f64) -> Endpoint {
        Endpoint {
            id,
            name: name.to_owned(),
            description: description.to_owned(),
            volume,
            muted: false,
        }
    }

    #[test]
    fn the_volume_view_is_the_default_sink_and_lists_every_sink() {
        let mut state = State {
            connected: true,
            sinks: vec![
                sink(40, "alsa_output.hdmi", "HDMI", 0.2),
                sink(41, "alsa_output.analog", "Speakers", 0.56),
            ],
            default_sink: Some(41),
            ..State::default()
        };
        let view = volume_view(&state).expect("a default sink");
        assert!((view.level - 0.56).abs() < f64::EPSILON);
        let outputs: Vec<(&str, bool)> = view
            .outputs
            .iter()
            .map(|o| (o.name.as_str(), o.default))
            .collect();
        assert_eq!(outputs, [("HDMI", false), ("Speakers", true)]);
        state.default_sink = None;
        assert_eq!(volume_view(&state), None);
        state.default_sink = Some(41);
        state.connected = false;
        assert_eq!(volume_view(&state), None);
    }
}
