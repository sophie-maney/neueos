# 0079: Super as ⌘: the compositor sends third-party apps Control for Super

- **Status:** Accepted
- **Date:** 2026-11-06

## Context
PLAN.md Q1 was decided on 2026-11-05: Super is the command key (⌘), as on the Mac. neue-kit
apps can take Super themselves, but GTK, Qt, Firefox and Chromium use Ctrl for copy, paste,
quit and the rest, and have no setting to change it. So Super+C has to reach them as
Ctrl+C, except in terminals, where Ctrl+C interrupts and copying is Ctrl+Shift+C. Task 3.10
was split in three (3.10a this compositor part, 3.10b the kit's command key, 3.10c ⌘ in
third-party menus).

## Decision
1. **Rewrite the modifiers, not the keys.** The seat's xkb state and the key events stay
   as they are, so compositor bindings still see Super and run first (the input filter is
   unchanged). Only the `wl_keyboard.modifiers` events sent to a remapped client change:
   where the seat has Super (the real modifier `Mod4`, and xkbcommon's virtual `Super`
   where it has one), depressed, latched or locked, the client is sent Control. Clients
   resolve keys against the modifiers they are sent, so Super+C reads as Ctrl+C.
2. **Where:** the seat's keyboard focus type is a newtype, `KeyFocus(WlSurface)`
   (`command_key.rs`), whose `KeyboardTarget::modifiers` does the rewrite and delegates
   the rest to Smithay's `WlSurface` implementation. Every modifiers event goes through it:
   key presses, focus changes (`enter`), popup grabs. The masks are read from the keymap
   when it changes (Smithay holds the keyboard's lock inside `modifiers`, so they can't
   be read there); Control = bit 2 and Mod4 = bit 6, xkbcommon's fixed real-modifier
   order, until then. Smithay's fork is not touched.
3. **Who is remapped:** every client except
   - neue-kit apps: clients that bound `neue_window_manager_v1` (neue-platform binds it
     when an app starts, sandboxed or not) and the shell. They see Super and treat it as
     ⌘ themselves (3.10b). No new protocol: the binding is the signal.
   - apps whose app id is in `compositor.toml`'s `[super-as-command] exceptions`
     (compared ignoring case; the list replaces the default, which holds the common
     terminals). Popups count as their window's app; layer surfaces have no app id and are
     remapped unless their client is native.
   - a surface with a keyboard-shortcuts inhibitor (VM and remote-desktop viewers want the
     real Super).
   `[super-as-command] enabled = false` turns the remap off. Both settings apply on
   reload, from the next modifiers event.
4. **Known limits.** A remapped client still sees the Super key itself go down and up
   (with Control in its modifiers); apps that bind the bare Super key would notice.
   `neue_shell_v1.send_shortcut` presses real keys, so a chord with Super sent to a
   remapped app arrives as Ctrl like a typed one.

## Consequences
- Super+C/V/X/Z/A/Q/W/T/F and the rest work as on the Mac in third-party apps, and a
  compositor binding on Super still wins (Super+Q quits the session by default; a
  later task may move session actions to keep Super+Q for apps).
- Ctrl stays Ctrl: a third-party app still gets real Ctrl chords from the Ctrl keys.
- The shell does not yet know which apps are remapped; 3.10c adds that to `neue_shell_v1`
  so the menu bar can show their Ctrl shortcuts with ⌘.
- Super+arrow becomes Ctrl+arrow in remapped apps (word moves in GTK), not the Mac's
  line ends.
