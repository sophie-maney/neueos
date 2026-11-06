//! The volume and brightness keys and the on-screen display (task 3.14a).
//!
//! The compositor routes the keys to the shell (`neue_shell_v1`'s `system_key`); the driver
//! repeats the stepping ones. Each key changes a level ([`neue_shell::levels`]) and shows
//! it on the OSD: an overlay layer surface on the active output that lets the pointer
//! through. It is opened on the first key, fades out [`HOLD`] after the last one and is
//! closed once the fade is over, so the next key opens it on whichever output is active
//! then.
//!
//! The volume is a level kept here until the shell talks to PipeWire (task 3.14b). The
//! brightness is the screen's backlight, set through logind (`Session.SetBrightness`).

use std::time::Instant;

use neue_dbus::login::Login;
use neue_kit::desktop::{
    Anchor, ChordModifiers, LayerKeyboard, LayerSpec, Margin, Osd, OsdGlyph, ShellLayer, SystemKey,
    SystemKeyState,
};
use neue_kit::tokens::materials::osd::{BOTTOM_MARGIN, FADE, HOLD, RADIUS, SIZE};
use neue_kit::{Cx, Handle, WindowId};
use neue_shell::levels::{self, Backlight, Volume};

/// The OSD's layer-shell namespace.
const NAMESPACE: &str = "neue-osd";

/// What the OSD's timers and workers report.
#[derive(Debug, Clone)]
pub enum Event {
    /// The backlight and a logind connection to set it with; `None` without a backlight.
    Backlight(Option<(Backlight, Login)>),
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
    volume: Volume,
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
    /// Looks for the screen's backlight, and connects to logind if there is one.
    pub fn start<M: Send + 'static>(cx: &mut Cx<'_, M>, wrap: fn(Event) -> M) {
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
        if self.volume.apply(key, fine) {
            let glyph = if self.volume.muted {
                OsdGlyph::Muted
            } else {
                OsdGlyph::Volume
            };
            let level = self.volume.level;
            tracing::debug!(level, muted = self.volume.muted, "volume");
            self.show(cx, wrap, glyph, level);
            return;
        }
        let up = match key {
            SystemKey::BrightnessUp => true,
            SystemKey::BrightnessDown => false,
            // Keyboard backlights, the microphone and media keys: not yet.
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
