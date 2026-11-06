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
//! is the screen's backlight, set through logind (`Session.SetBrightness`); the keyboard
//! brightness keys do the same to the keyboard's backlight (task 3.14d). The media keys
//! (play, pause, next, previous, stop) go to the active MPRIS player ([`neue_dbus::mpris`])
//! and show nothing.
//!
//! The bar's volume extra (task 3.14c) shows the same default sink ([`Keys::volume_view`])
//! and its menu acts through the same PipeWire thread ([`Keys::volume_command`]).

use std::time::Instant;

use neue_audio::{Audio, Kind};
use neue_dbus::login::Login;
use neue_dbus::mpris::{Command, Mpris};
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

/// How long a call to a media player may take before it counts as hung.
const PLAYER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

/// The backlights there are, and a logind connection to set them with.
#[derive(Debug, Clone, Default)]
pub struct Lights {
    /// The screen's.
    screen: Option<Backlight>,
    /// The keyboard's.
    keyboard: Option<Backlight>,
    /// `None` without the system bus: no key can change a backlight then.
    login: Option<Login>,
}

/// What the OSD's timers and workers report.
#[derive(Debug, Clone)]
pub enum Event {
    /// The backlights and a logind connection to set them with.
    Backlights(Lights),
    /// The MPRIS client; `None` without a session bus.
    Media(Option<Mpris>),
    /// A media key's command ended: the player it went to, or why it failed.
    MediaSent(Result<Option<String>, String>),
    /// PipeWire's sinks and sources changed.
    Audio(neue_audio::State),
    /// A `SetBrightness` call ended; `true` for the keyboard's.
    Brightness {
        keyboard: bool,
        result: Result<(), String>,
    },
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
    lights: Lights,
    /// The raw brightness last asked for of the screen and of the keyboard: repeats step
    /// from it, since logind's write may not have reached sysfs yet. A new press reads
    /// sysfs again.
    brightness: Option<u32>,
    keyboard_brightness: Option<u32>,
    /// The media players' client; `None` until connected, or without a session bus.
    media: Option<Mpris>,
    osd: Option<Surface>,
    /// When the OSD starts fading, pushed back by every key.
    hide_at: Option<Instant>,
    /// Whether a hold timer is running (one at a time, however fast the keys repeat).
    holding: bool,
    /// Counts fades, so a stale fade's end doesn't close a surface shown again since.
    fades: u64,
}

impl Keys {
    /// Starts the PipeWire thread, looks for the backlights (and connects to logind if there
    /// is one), and connects to the session bus for the media keys.
    pub fn start<M: Send + 'static>(&mut self, cx: &mut Cx<'_, M>, wrap: fn(Event) -> M) {
        let sender = cx.sender();
        match Audio::start(move |state| {
            sender.send(wrap(Event::Audio(state)));
        }) {
            Ok(audio) => self.audio = Some(audio),
            Err(error) => tracing::warn!(%error, "no audio thread: the volume keys do nothing"),
        }
        cx.spawn(move |_| {
            let screen = Backlight::find(levels::BACKLIGHTS.as_ref());
            let keyboard = Backlight::find_keyboard(levels::LEDS.as_ref());
            let login = if screen.is_some() || keyboard.is_some() {
                Login::system()
                    .inspect_err(|error| {
                        tracing::warn!(%error, "no system bus: the brightness keys do nothing");
                    })
                    .ok()
            } else {
                None
            };
            wrap(Event::Backlights(Lights {
                screen,
                keyboard,
                login,
            }))
        });
        cx.spawn(move |_| {
            let media = Mpris::session(PLAYER_TIMEOUT)
                .and_then(|mpris| mpris.watch().map(|()| mpris))
                .inspect_err(|error| {
                    tracing::warn!(%error, "no session bus: the media keys do nothing");
                })
                .ok();
            wrap(Event::Media(media))
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
        match key {
            SystemKey::VolumeUp | SystemKey::VolumeDown | SystemKey::Mute => {
                self.volume_key(cx, wrap, key, state, fine);
            }
            SystemKey::MicMute => self.mic_mute(cx, wrap),
            SystemKey::BrightnessUp | SystemKey::BrightnessDown => {
                let up = key == SystemKey::BrightnessUp;
                self.backlight_key(cx, wrap, false, up, state, fine);
            }
            SystemKey::KeyboardBrightnessUp | SystemKey::KeyboardBrightnessDown => {
                let up = key == SystemKey::KeyboardBrightnessUp;
                self.backlight_key(cx, wrap, true, up, state, fine);
            }
            SystemKey::Play => self.media_key(cx, wrap, Command::PlayPause),
            SystemKey::Pause => self.media_key(cx, wrap, Command::Pause),
            SystemKey::Next => self.media_key(cx, wrap, Command::Next),
            SystemKey::Previous => self.media_key(cx, wrap, Command::Previous),
            SystemKey::Stop => self.media_key(cx, wrap, Command::Stop),
            SystemKey::Eject => tracing::debug!("system key not handled: Eject"),
        }
    }

    /// A media key: `command` to the active player, off the event loop (the bus may be
    /// slow, and a hung player makes the call wait out its timeout).
    fn media_key<M: Send + 'static>(
        &mut self,
        cx: &mut Cx<'_, M>,
        wrap: fn(Event) -> M,
        command: Command,
    ) {
        let Some(mpris) = self.media.clone() else {
            tracing::debug!(?command, "no media client");
            return;
        };
        cx.spawn(move |_| {
            wrap(Event::MediaSent(
                mpris
                    .send(command)
                    .map(|player| player.map(|p| p.name))
                    .map_err(|e| e.to_string()),
            ))
        });
    }

    /// A brightness key for the screen's backlight, or the keyboard's when `keyboard`.
    fn backlight_key<M: Send + 'static>(
        &mut self,
        cx: &mut Cx<'_, M>,
        wrap: fn(Event) -> M,
        keyboard: bool,
        up: bool,
        state: SystemKeyState,
        fine: bool,
    ) {
        let (Some(backlight), Some(login)) = (
            if keyboard {
                &self.lights.keyboard
            } else {
                &self.lights.screen
            },
            &self.lights.login,
        ) else {
            tracing::debug!(keyboard, "no backlight to change");
            return;
        };
        let asked = if keyboard {
            &mut self.keyboard_brightness
        } else {
            &mut self.brightness
        };
        let now = match (state, *asked) {
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
        *asked = Some(target);
        if target != now {
            let (login, name, subsystem) =
                (login.clone(), backlight.name.clone(), backlight.subsystem);
            cx.spawn(move |_| {
                wrap(Event::Brightness {
                    keyboard,
                    result: login
                        .set_brightness(subsystem, &name, target)
                        .map_err(|e| e.to_string()),
                })
            });
        }
        let glyph = if keyboard {
            OsdGlyph::Keyboard
        } else {
            OsdGlyph::Brightness
        };
        self.show(cx, wrap, glyph, level);
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
            Event::Backlights(lights) => {
                for backlight in [&lights.screen, &lights.keyboard].into_iter().flatten() {
                    tracing::info!(
                        name = backlight.name,
                        max = backlight.max,
                        "{}",
                        backlight.subsystem
                    );
                }
                self.lights = lights;
            }
            Event::Media(media) => self.media = media,
            Event::MediaSent(Ok(Some(player))) => tracing::debug!(player, "media key sent"),
            Event::MediaSent(Ok(None)) => tracing::debug!("no media player"),
            Event::MediaSent(Err(error)) => tracing::warn!(%error, "a media key failed"),
            Event::Audio(state) => {
                if state.connected != self.sound.connected {
                    tracing::info!(connected = state.connected, "PipeWire");
                }
                self.sound = state;
            }
            Event::Brightness { result: Ok(()), .. } => {}
            Event::Brightness {
                keyboard,
                result: Err(error),
            } => {
                tracing::warn!(%error, keyboard, "setting the brightness failed");
                // Read sysfs again next time rather than trust the failed value.
                if keyboard {
                    self.keyboard_brightness = None;
                } else {
                    self.brightness = None;
                }
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
