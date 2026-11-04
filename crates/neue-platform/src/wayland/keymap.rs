//! Key tables: evdev scancodes to `Code`, xkb keysyms to `NamedKey` and `Location`.
//!
//! Ported from winit 0.30's `platform_impl/linux/common/xkb/keymap.rs` (Apache-2.0 OR MIT,
//! compatible with GPL-3.0-or-later). winit's type names were renamed to `keyboard-types`'
//! (re-exported by Masonry): `Super`/`Hyper` keys are `Meta`, and space is a character.
//! Commented-out entries are kept as in winit.

use masonry_core::core::keyboard::{Code, Location, NamedKey};
use smithay_client_toolkit::seat::keyboard::Keysym;

/// The physical key for a Linux evdev scancode (`wl_keyboard.key`, without xkb's +8).
#[rustfmt::skip]
pub(super) fn code(scancode: u32) -> Code {
    match scancode {
        1 => Code::Escape,
        2 => Code::Digit1,
        3 => Code::Digit2,
        4 => Code::Digit3,
        5 => Code::Digit4,
        6 => Code::Digit5,
        7 => Code::Digit6,
        8 => Code::Digit7,
        9 => Code::Digit8,
        10 => Code::Digit9,
        11 => Code::Digit0,
        12 => Code::Minus,
        13 => Code::Equal,
        14 => Code::Backspace,
        15 => Code::Tab,
        16 => Code::KeyQ,
        17 => Code::KeyW,
        18 => Code::KeyE,
        19 => Code::KeyR,
        20 => Code::KeyT,
        21 => Code::KeyY,
        22 => Code::KeyU,
        23 => Code::KeyI,
        24 => Code::KeyO,
        25 => Code::KeyP,
        26 => Code::BracketLeft,
        27 => Code::BracketRight,
        28 => Code::Enter,
        29 => Code::ControlLeft,
        30 => Code::KeyA,
        31 => Code::KeyS,
        32 => Code::KeyD,
        33 => Code::KeyF,
        34 => Code::KeyG,
        35 => Code::KeyH,
        36 => Code::KeyJ,
        37 => Code::KeyK,
        38 => Code::KeyL,
        39 => Code::Semicolon,
        40 => Code::Quote,
        41 => Code::Backquote,
        42 => Code::ShiftLeft,
        43 => Code::Backslash,
        44 => Code::KeyZ,
        45 => Code::KeyX,
        46 => Code::KeyC,
        47 => Code::KeyV,
        48 => Code::KeyB,
        49 => Code::KeyN,
        50 => Code::KeyM,
        51 => Code::Comma,
        52 => Code::Period,
        53 => Code::Slash,
        54 => Code::ShiftRight,
        55 => Code::NumpadMultiply,
        56 => Code::AltLeft,
        57 => Code::Space,
        58 => Code::CapsLock,
        59 => Code::F1,
        60 => Code::F2,
        61 => Code::F3,
        62 => Code::F4,
        63 => Code::F5,
        64 => Code::F6,
        65 => Code::F7,
        66 => Code::F8,
        67 => Code::F9,
        68 => Code::F10,
        69 => Code::NumLock,
        70 => Code::ScrollLock,
        71 => Code::Numpad7,
        72 => Code::Numpad8,
        73 => Code::Numpad9,
        74 => Code::NumpadSubtract,
        75 => Code::Numpad4,
        76 => Code::Numpad5,
        77 => Code::Numpad6,
        78 => Code::NumpadAdd,
        79 => Code::Numpad1,
        80 => Code::Numpad2,
        81 => Code::Numpad3,
        82 => Code::Numpad0,
        83 => Code::NumpadDecimal,
        85 => Code::Lang5,
        86 => Code::IntlBackslash,
        87 => Code::F11,
        88 => Code::F12,
        89 => Code::IntlRo,
        90 => Code::Lang3,
        91 => Code::Lang4,
        92 => Code::Convert,
        93 => Code::KanaMode,
        94 => Code::NonConvert,
        // 95 => Code::KPJPCOMMA,
        96 => Code::NumpadEnter,
        97 => Code::ControlRight,
        98 => Code::NumpadDivide,
        99 => Code::PrintScreen,
        100 => Code::AltRight,
        // 101 => Code::LINEFEED,
        102 => Code::Home,
        103 => Code::ArrowUp,
        104 => Code::PageUp,
        105 => Code::ArrowLeft,
        106 => Code::ArrowRight,
        107 => Code::End,
        108 => Code::ArrowDown,
        109 => Code::PageDown,
        110 => Code::Insert,
        111 => Code::Delete,
        // 112 => Code::MACRO,
        113 => Code::AudioVolumeMute,
        114 => Code::AudioVolumeDown,
        115 => Code::AudioVolumeUp,
        // 116 => Code::POWER,
        117 => Code::NumpadEqual,
        // 118 => Code::KPPLUSMINUS,
        119 => Code::Pause,
        // 120 => Code::SCALE,
        121 => Code::NumpadComma,
        122 => Code::Lang1,
        123 => Code::Lang2,
        124 => Code::IntlYen,
        125 => Code::MetaLeft,
        126 => Code::MetaRight,
        127 => Code::ContextMenu,
        // 128 => Code::STOP,
        // 129 => Code::AGAIN,
        // 130 => Code::PROPS,
        // 131 => Code::UNDO,
        // 132 => Code::FRONT,
        // 133 => Code::COPY,
        // 134 => Code::OPEN,
        // 135 => Code::PASTE,
        // 136 => Code::FIND,
        // 137 => Code::CUT,
        // 138 => Code::HELP,
        // 139 => Code::MENU,
        // 140 => Code::CALC,
        // 141 => Code::SETUP,
        // 142 => Code::SLEEP,
        // 143 => Code::WAKEUP,
        // 144 => Code::FILE,
        // 145 => Code::SENDFILE,
        // 146 => Code::DELETEFILE,
        // 147 => Code::XFER,
        // 148 => Code::PROG1,
        // 149 => Code::PROG2,
        // 150 => Code::WWW,
        // 151 => Code::MSDOS,
        // 152 => Code::COFFEE,
        // 153 => Code::ROTATE_DISPLAY,
        // 154 => Code::CYCLEWINDOWS,
        // 155 => Code::MAIL,
        // 156 => Code::BOOKMARKS,
        // 157 => Code::COMPUTER,
        // 158 => Code::BACK,
        // 159 => Code::FORWARD,
        // 160 => Code::CLOSECD,
        // 161 => Code::EJECTCD,
        // 162 => Code::EJECTCLOSECD,
        163 => Code::MediaTrackNext,
        164 => Code::MediaPlayPause,
        165 => Code::MediaTrackPrevious,
        166 => Code::MediaStop,
        // 167 => Code::RECORD,
        // 168 => Code::REWIND,
        // 169 => Code::PHONE,
        // 170 => Code::ISO,
        // 171 => Code::CONFIG,
        // 172 => Code::HOMEPAGE,
        // 173 => Code::REFRESH,
        // 174 => Code::EXIT,
        // 175 => Code::MOVE,
        // 176 => Code::EDIT,
        // 177 => Code::SCROLLUP,
        // 178 => Code::SCROLLDOWN,
        // 179 => Code::KPLEFTPAREN,
        // 180 => Code::KPRIGHTPAREN,
        // 181 => Code::NEW,
        // 182 => Code::REDO,
        183 => Code::F13,
        184 => Code::F14,
        185 => Code::F15,
        186 => Code::F16,
        187 => Code::F17,
        188 => Code::F18,
        189 => Code::F19,
        190 => Code::F20,
        191 => Code::F21,
        192 => Code::F22,
        193 => Code::F23,
        194 => Code::F24,
        // 200 => Code::PLAYCD,
        // 201 => Code::PAUSECD,
        // 202 => Code::PROG3,
        // 203 => Code::PROG4,
        // 204 => Code::DASHBOARD,
        // 205 => Code::SUSPEND,
        // 206 => Code::CLOSE,
        // 207 => Code::PLAY,
        // 208 => Code::FASTFORWARD,
        // 209 => Code::BASSBOOST,
        // 210 => Code::PRINT,
        // 211 => Code::HP,
        // 212 => Code::CAMERA,
        // 213 => Code::SOUND,
        // 214 => Code::QUESTION,
        // 215 => Code::EMAIL,
        // 216 => Code::CHAT,
        // 217 => Code::SEARCH,
        // 218 => Code::CONNECT,
        // 219 => Code::FINANCE,
        // 220 => Code::SPORT,
        // 221 => Code::SHOP,
        // 222 => Code::ALTERASE,
        // 223 => Code::CANCEL,
        // 224 => Code::BRIGHTNESSDOW,
        // 225 => Code::BRIGHTNESSU,
        // 226 => Code::MEDIA,
        // 227 => Code::SWITCHVIDEOMODE,
        // 228 => Code::KBDILLUMTOGGLE,
        // 229 => Code::KBDILLUMDOWN,
        // 230 => Code::KBDILLUMUP,
        // 231 => Code::SEND,
        // 232 => Code::REPLY,
        // 233 => Code::FORWARDMAIL,
        // 234 => Code::SAVE,
        // 235 => Code::DOCUMENTS,
        // 236 => Code::BATTERY,
        // 237 => Code::BLUETOOTH,
        // 238 => Code::WLAN,
        // 239 => Code::UWB,
        // 241 => Code::VIDEO_NEXT,
        // 242 => Code::VIDEO_PREV,
        // 243 => Code::BRIGHTNESS_CYCLE,
        // 244 => Code::BRIGHTNESS_AUTO,
        // 245 => Code::DISPLAY_OFF,
        // 246 => Code::WWAN,
        // 247 => Code::RFKILL,
        // 248 => Code::KEY_MICMUTE,
        _ => Code::Unidentified,
    }
}

/// The named key for `keysym`, if it has one. Keys that produce text return `None`.
#[rustfmt::skip]
pub(super) fn named_key(keysym: Keysym) -> Option<NamedKey> {
    Some(match keysym {
        // TTY function keys
        Keysym::BackSpace => NamedKey::Backspace,
        Keysym::Tab => NamedKey::Tab,
        // Keysym::Linefeed => NamedKey::Linefeed,
        Keysym::Clear => NamedKey::Clear,
        Keysym::Return => NamedKey::Enter,
        Keysym::Pause => NamedKey::Pause,
        Keysym::Scroll_Lock => NamedKey::ScrollLock,
        Keysym::Sys_Req => NamedKey::PrintScreen,
        Keysym::Escape => NamedKey::Escape,
        Keysym::Delete => NamedKey::Delete,

        // IME keys
        Keysym::Multi_key => NamedKey::Compose,
        Keysym::Codeinput => NamedKey::CodeInput,
        Keysym::SingleCandidate => NamedKey::SingleCandidate,
        Keysym::MultipleCandidate => NamedKey::AllCandidates,
        Keysym::PreviousCandidate => NamedKey::PreviousCandidate,

        // Japanese keys
        Keysym::Kanji => NamedKey::KanjiMode,
        Keysym::Muhenkan => NamedKey::NonConvert,
        Keysym::Henkan_Mode => NamedKey::Convert,
        Keysym::Romaji => NamedKey::Romaji,
        Keysym::Hiragana => NamedKey::Hiragana,
        Keysym::Hiragana_Katakana => NamedKey::HiraganaKatakana,
        Keysym::Zenkaku => NamedKey::Zenkaku,
        Keysym::Hankaku => NamedKey::Hankaku,
        Keysym::Zenkaku_Hankaku => NamedKey::ZenkakuHankaku,
        // Keysym::Touroku => NamedKey::Touroku,
        // Keysym::Massyo => NamedKey::Massyo,
        Keysym::Kana_Lock => NamedKey::KanaMode,
        Keysym::Kana_Shift => NamedKey::KanaMode,
        Keysym::Eisu_Shift => NamedKey::Alphanumeric,
        Keysym::Eisu_toggle => NamedKey::Alphanumeric,
        // NOTE: The next three items are aliases for values we've already mapped.
        // Keysym::Kanji_Bangou => NamedKey::CodeInput,
        // Keysym::Zen_Koho => NamedKey::AllCandidates,
        // Keysym::Mae_Koho => NamedKey::PreviousCandidate,

        // Cursor control & motion
        Keysym::Home => NamedKey::Home,
        Keysym::Left => NamedKey::ArrowLeft,
        Keysym::Up => NamedKey::ArrowUp,
        Keysym::Right => NamedKey::ArrowRight,
        Keysym::Down => NamedKey::ArrowDown,
        // Keysym::Prior => NamedKey::PageUp,
        Keysym::Page_Up => NamedKey::PageUp,
        // Keysym::Next => NamedKey::PageDown,
        Keysym::Page_Down => NamedKey::PageDown,
        Keysym::End => NamedKey::End,
        // Keysym::Begin => NamedKey::Begin,

        // Misc. functions
        Keysym::Select => NamedKey::Select,
        Keysym::Print => NamedKey::PrintScreen,
        Keysym::Execute => NamedKey::Execute,
        Keysym::Insert => NamedKey::Insert,
        Keysym::Undo => NamedKey::Undo,
        Keysym::Redo => NamedKey::Redo,
        Keysym::Menu => NamedKey::ContextMenu,
        Keysym::Find => NamedKey::Find,
        Keysym::Cancel => NamedKey::Cancel,
        Keysym::Help => NamedKey::Help,
        Keysym::Break => NamedKey::Pause,
        Keysym::Mode_switch => NamedKey::ModeChange,
        // Keysym::script_switch => NamedKey::ModeChange,
        Keysym::Num_Lock => NamedKey::NumLock,

        // Keypad keys
        // Keysym::KP_Space => return Key::Character(" "),
        Keysym::KP_Tab => NamedKey::Tab,
        Keysym::KP_Enter => NamedKey::Enter,
        Keysym::KP_F1 => NamedKey::F1,
        Keysym::KP_F2 => NamedKey::F2,
        Keysym::KP_F3 => NamedKey::F3,
        Keysym::KP_F4 => NamedKey::F4,
        Keysym::KP_Home => NamedKey::Home,
        Keysym::KP_Left => NamedKey::ArrowLeft,
        Keysym::KP_Up => NamedKey::ArrowUp,
        Keysym::KP_Right => NamedKey::ArrowRight,
        Keysym::KP_Down => NamedKey::ArrowDown,
        // Keysym::KP_Prior => NamedKey::PageUp,
        Keysym::KP_Page_Up => NamedKey::PageUp,
        // Keysym::KP_Next => NamedKey::PageDown,
        Keysym::KP_Page_Down => NamedKey::PageDown,
        Keysym::KP_End => NamedKey::End,
        // This is the key labeled "5" on the numpad when NumLock is off.
        // Keysym::KP_Begin => NamedKey::Begin,
        Keysym::KP_Insert => NamedKey::Insert,
        Keysym::KP_Delete => NamedKey::Delete,
        // Keysym::KP_Equal => NamedKey::Equal,
        // Keysym::KP_Multiply => NamedKey::Multiply,
        // Keysym::KP_Add => NamedKey::Add,
        // Keysym::KP_Separator => NamedKey::Separator,
        // Keysym::KP_Subtract => NamedKey::Subtract,
        // Keysym::KP_Decimal => NamedKey::Decimal,
        // Keysym::KP_Divide => NamedKey::Divide,

        // Keysym::KP_0 => return Key::Character("0"),
        // Keysym::KP_1 => return Key::Character("1"),
        // Keysym::KP_2 => return Key::Character("2"),
        // Keysym::KP_3 => return Key::Character("3"),
        // Keysym::KP_4 => return Key::Character("4"),
        // Keysym::KP_5 => return Key::Character("5"),
        // Keysym::KP_6 => return Key::Character("6"),
        // Keysym::KP_7 => return Key::Character("7"),
        // Keysym::KP_8 => return Key::Character("8"),
        // Keysym::KP_9 => return Key::Character("9"),

        // Function keys
        Keysym::F1 => NamedKey::F1,
        Keysym::F2 => NamedKey::F2,
        Keysym::F3 => NamedKey::F3,
        Keysym::F4 => NamedKey::F4,
        Keysym::F5 => NamedKey::F5,
        Keysym::F6 => NamedKey::F6,
        Keysym::F7 => NamedKey::F7,
        Keysym::F8 => NamedKey::F8,
        Keysym::F9 => NamedKey::F9,
        Keysym::F10 => NamedKey::F10,
        Keysym::F11 => NamedKey::F11,
        Keysym::F12 => NamedKey::F12,
        Keysym::F13 => NamedKey::F13,
        Keysym::F14 => NamedKey::F14,
        Keysym::F15 => NamedKey::F15,
        Keysym::F16 => NamedKey::F16,
        Keysym::F17 => NamedKey::F17,
        Keysym::F18 => NamedKey::F18,
        Keysym::F19 => NamedKey::F19,
        Keysym::F20 => NamedKey::F20,
        Keysym::F21 => NamedKey::F21,
        Keysym::F22 => NamedKey::F22,
        Keysym::F23 => NamedKey::F23,
        Keysym::F24 => NamedKey::F24,
        Keysym::F25 => NamedKey::F25,
        Keysym::F26 => NamedKey::F26,
        Keysym::F27 => NamedKey::F27,
        Keysym::F28 => NamedKey::F28,
        Keysym::F29 => NamedKey::F29,
        Keysym::F30 => NamedKey::F30,
        Keysym::F31 => NamedKey::F31,
        Keysym::F32 => NamedKey::F32,
        Keysym::F33 => NamedKey::F33,
        Keysym::F34 => NamedKey::F34,
        Keysym::F35 => NamedKey::F35,

        // Modifiers
        Keysym::Shift_L => NamedKey::Shift,
        Keysym::Shift_R => NamedKey::Shift,
        Keysym::Control_L => NamedKey::Control,
        Keysym::Control_R => NamedKey::Control,
        Keysym::Caps_Lock => NamedKey::CapsLock,
        // Keysym::Shift_Lock => NamedKey::ShiftLock,

        // Keysym::Meta_L => NamedKey::Meta,
        // Keysym::Meta_R => NamedKey::Meta,
        Keysym::Alt_L => NamedKey::Alt,
        Keysym::Alt_R => NamedKey::Alt,
        Keysym::Super_L => NamedKey::Meta,
        Keysym::Super_R => NamedKey::Meta,
        Keysym::Hyper_L => NamedKey::Meta,
        Keysym::Hyper_R => NamedKey::Meta,

        // XKB function and modifier keys
        // Keysym::ISO_Lock => NamedKey::IsoLock,
        // Keysym::ISO_Level2_Latch => NamedKey::IsoLevel2Latch,
        Keysym::ISO_Level3_Shift => NamedKey::AltGraph,
        Keysym::ISO_Level3_Latch => NamedKey::AltGraph,
        Keysym::ISO_Level3_Lock => NamedKey::AltGraph,
        // Keysym::ISO_Level5_Shift => NamedKey::IsoLevel5Shift,
        // Keysym::ISO_Level5_Latch => NamedKey::IsoLevel5Latch,
        // Keysym::ISO_Level5_Lock => NamedKey::IsoLevel5Lock,
        // Keysym::ISO_Group_Shift => NamedKey::IsoGroupShift,
        // Keysym::ISO_Group_Latch => NamedKey::IsoGroupLatch,
        // Keysym::ISO_Group_Lock => NamedKey::IsoGroupLock,
        Keysym::ISO_Next_Group => NamedKey::GroupNext,
        // Keysym::ISO_Next_Group_Lock => NamedKey::GroupNextLock,
        Keysym::ISO_Prev_Group => NamedKey::GroupPrevious,
        // Keysym::ISO_Prev_Group_Lock => NamedKey::GroupPreviousLock,
        Keysym::ISO_First_Group => NamedKey::GroupFirst,
        // Keysym::ISO_First_Group_Lock => NamedKey::GroupFirstLock,
        Keysym::ISO_Last_Group => NamedKey::GroupLast,
        // Keysym::ISO_Last_Group_Lock => NamedKey::GroupLastLock,
        Keysym::ISO_Left_Tab => NamedKey::Tab,
        // Keysym::ISO_Move_Line_Up => NamedKey::IsoMoveLineUp,
        // Keysym::ISO_Move_Line_Down => NamedKey::IsoMoveLineDown,
        // Keysym::ISO_Partial_Line_Up => NamedKey::IsoPartialLineUp,
        // Keysym::ISO_Partial_Line_Down => NamedKey::IsoPartialLineDown,
        // Keysym::ISO_Partial_Space_Left => NamedKey::IsoPartialSpaceLeft,
        // Keysym::ISO_Partial_Space_Right => NamedKey::IsoPartialSpaceRight,
        // Keysym::ISO_Set_Margin_Left => NamedKey::IsoSetMarginLeft,
        // Keysym::ISO_Set_Margin_Right => NamedKey::IsoSetMarginRight,
        // Keysym::ISO_Release_Margin_Left => NamedKey::IsoReleaseMarginLeft,
        // Keysym::ISO_Release_Margin_Right => NamedKey::IsoReleaseMarginRight,
        // Keysym::ISO_Release_Both_Margins => NamedKey::IsoReleaseBothMargins,
        // Keysym::ISO_Fast_Cursor_Left => NamedKey::IsoFastCursorLeft,
        // Keysym::ISO_Fast_Cursor_Right => NamedKey::IsoFastCursorRight,
        // Keysym::ISO_Fast_Cursor_Up => NamedKey::IsoFastCursorUp,
        // Keysym::ISO_Fast_Cursor_Down => NamedKey::IsoFastCursorDown,
        // Keysym::ISO_Continuous_Underline => NamedKey::IsoContinuousUnderline,
        // Keysym::ISO_Discontinuous_Underline => NamedKey::IsoDiscontinuousUnderline,
        // Keysym::ISO_Emphasize => NamedKey::IsoEmphasize,
        // Keysym::ISO_Center_Object => NamedKey::IsoCenterObject,
        Keysym::ISO_Enter => NamedKey::Enter,

        // dead_grave..dead_currency

        // dead_lowline..dead_longsolidusoverlay

        // dead_a..dead_capital_schwa

        // dead_greek

        // First_Virtual_Screen..Terminate_Server

        // AccessX_Enable..AudibleBell_Enable

        // Pointer_Left..Pointer_Drag5

        // Pointer_EnableKeys..Pointer_DfltBtnPrev

        // ch..C_H

        // 3270 terminal keys
        // Keysym::3270_Duplicate => NamedKey::Duplicate,
        // Keysym::3270_FieldMark => NamedKey::FieldMark,
        // Keysym::3270_Right2 => NamedKey::Right2,
        // Keysym::3270_Left2 => NamedKey::Left2,
        // Keysym::3270_BackTab => NamedKey::BackTab,
        Keysym::_3270_EraseEOF => NamedKey::EraseEof,
        // Keysym::3270_EraseInput => NamedKey::EraseInput,
        // Keysym::3270_Reset => NamedKey::Reset,
        // Keysym::3270_Quit => NamedKey::Quit,
        // Keysym::3270_PA1 => NamedKey::Pa1,
        // Keysym::3270_PA2 => NamedKey::Pa2,
        // Keysym::3270_PA3 => NamedKey::Pa3,
        // Keysym::3270_Test => NamedKey::Test,
        Keysym::_3270_Attn => NamedKey::Attn,
        // Keysym::3270_CursorBlink => NamedKey::CursorBlink,
        // Keysym::3270_AltCursor => NamedKey::AltCursor,
        // Keysym::3270_KeyClick => NamedKey::KeyClick,
        // Keysym::3270_Jump => NamedKey::Jump,
        // Keysym::3270_Ident => NamedKey::Ident,
        // Keysym::3270_Rule => NamedKey::Rule,
        // Keysym::3270_Copy => NamedKey::Copy,
        Keysym::_3270_Play => NamedKey::Play,
        // Keysym::3270_Setup => NamedKey::Setup,
        // Keysym::3270_Record => NamedKey::Record,
        // Keysym::3270_ChangeScreen => NamedKey::ChangeScreen,
        // Keysym::3270_DeleteWord => NamedKey::DeleteWord,
        Keysym::_3270_ExSelect => NamedKey::ExSel,
        Keysym::_3270_CursorSelect => NamedKey::CrSel,
        Keysym::_3270_PrintScreen => NamedKey::PrintScreen,
        Keysym::_3270_Enter => NamedKey::Enter,

        // space: winit names it; keyboard-types gives it as the character " ".
        // exclam..Sinh_kunddaliya

        // XFree86
        // Keysym::XF86_ModeLock => NamedKey::ModeLock,

        // XFree86 - Backlight controls
        Keysym::XF86_MonBrightnessUp => NamedKey::BrightnessUp,
        Keysym::XF86_MonBrightnessDown => NamedKey::BrightnessDown,
        // Keysym::XF86_KbdLightOnOff => NamedKey::LightOnOff,
        // Keysym::XF86_KbdBrightnessUp => NamedKey::KeyboardBrightnessUp,
        // Keysym::XF86_KbdBrightnessDown => NamedKey::KeyboardBrightnessDown,

        // XFree86 - "Internet"
        Keysym::XF86_Standby => NamedKey::Standby,
        Keysym::XF86_AudioLowerVolume => NamedKey::AudioVolumeDown,
        Keysym::XF86_AudioRaiseVolume => NamedKey::AudioVolumeUp,
        Keysym::XF86_AudioPlay => NamedKey::MediaPlay,
        Keysym::XF86_AudioStop => NamedKey::MediaStop,
        Keysym::XF86_AudioPrev => NamedKey::MediaTrackPrevious,
        Keysym::XF86_AudioNext => NamedKey::MediaTrackNext,
        Keysym::XF86_HomePage => NamedKey::BrowserHome,
        Keysym::XF86_Mail => NamedKey::LaunchMail,
        // Keysym::XF86_Start => NamedKey::Start,
        Keysym::XF86_Search => NamedKey::BrowserSearch,
        Keysym::XF86_AudioRecord => NamedKey::MediaRecord,

        // XFree86 - PDA
        Keysym::XF86_Calculator => NamedKey::LaunchApplication2,
        // Keysym::XF86_Memo => NamedKey::Memo,
        // Keysym::XF86_ToDoList => NamedKey::ToDoList,
        Keysym::XF86_Calendar => NamedKey::LaunchCalendar,
        Keysym::XF86_PowerDown => NamedKey::Power,
        // Keysym::XF86_ContrastAdjust => NamedKey::AdjustContrast,
        // Keysym::XF86_RockerUp => NamedKey::RockerUp,
        // Keysym::XF86_RockerDown => NamedKey::RockerDown,
        // Keysym::XF86_RockerEnter => NamedKey::RockerEnter,

        // XFree86 - More "Internet"
        Keysym::XF86_Back => NamedKey::BrowserBack,
        Keysym::XF86_Forward => NamedKey::BrowserForward,
        // Keysym::XF86_Stop => NamedKey::Stop,
        Keysym::XF86_Refresh => NamedKey::BrowserRefresh,
        Keysym::XF86_PowerOff => NamedKey::Power,
        Keysym::XF86_WakeUp => NamedKey::WakeUp,
        Keysym::XF86_Eject => NamedKey::Eject,
        Keysym::XF86_ScreenSaver => NamedKey::LaunchScreenSaver,
        Keysym::XF86_WWW => NamedKey::LaunchWebBrowser,
        Keysym::XF86_Sleep => NamedKey::Standby,
        Keysym::XF86_Favorites => NamedKey::BrowserFavorites,
        Keysym::XF86_AudioPause => NamedKey::MediaPause,
        // Keysym::XF86_AudioMedia => NamedKey::AudioMedia,
        Keysym::XF86_MyComputer => NamedKey::LaunchApplication1,
        // Keysym::XF86_VendorHome => NamedKey::VendorHome,
        // Keysym::XF86_LightBulb => NamedKey::LightBulb,
        // Keysym::XF86_Shop => NamedKey::BrowserShop,
        // Keysym::XF86_History => NamedKey::BrowserHistory,
        // Keysym::XF86_OpenURL => NamedKey::OpenUrl,
        // Keysym::XF86_AddFavorite => NamedKey::AddFavorite,
        // Keysym::XF86_HotLinks => NamedKey::HotLinks,
        // Keysym::XF86_BrightnessAdjust => NamedKey::BrightnessAdjust,
        // Keysym::XF86_Finance => NamedKey::BrowserFinance,
        // Keysym::XF86_Community => NamedKey::BrowserCommunity,
        Keysym::XF86_AudioRewind => NamedKey::MediaRewind,
        // Keysym::XF86_BackForward => Key::???,
        // XF86_Launch0..XF86_LaunchF

        // XF86_ApplicationLeft..XF86_CD
        Keysym::XF86_Calculater => NamedKey::LaunchApplication2, // Nice typo, libxkbcommon :)
        // XF86_Clear
        Keysym::XF86_Close => NamedKey::Close,
        Keysym::XF86_Copy => NamedKey::Copy,
        Keysym::XF86_Cut => NamedKey::Cut,
        // XF86_Display..XF86_Documents
        Keysym::XF86_Excel => NamedKey::LaunchSpreadsheet,
        // XF86_Explorer..XF86iTouch
        Keysym::XF86_LogOff => NamedKey::LogOff,
        // XF86_Market..XF86_MenuPB
        Keysym::XF86_MySites => NamedKey::BrowserFavorites,
        Keysym::XF86_New => NamedKey::New,
        // XF86_News..XF86_OfficeHome
        Keysym::XF86_Open => NamedKey::Open,
        // XF86_Option
        Keysym::XF86_Paste => NamedKey::Paste,
        Keysym::XF86_Phone => NamedKey::LaunchPhone,
        // XF86_Q
        Keysym::XF86_Reply => NamedKey::MailReply,
        Keysym::XF86_Reload => NamedKey::BrowserRefresh,
        // XF86_RotateWindows..XF86_RotationKB
        Keysym::XF86_Save => NamedKey::Save,
        // XF86_ScrollUp..XF86_ScrollClick
        Keysym::XF86_Send => NamedKey::MailSend,
        Keysym::XF86_Spell => NamedKey::SpellCheck,
        Keysym::XF86_SplitScreen => NamedKey::SplitScreenToggle,
        // XF86_Support..XF86_User2KB
        Keysym::XF86_Video => NamedKey::LaunchMediaPlayer,
        // XF86_WheelButton
        Keysym::XF86_Word => NamedKey::LaunchWordProcessor,
        // XF86_Xfer
        Keysym::XF86_ZoomIn => NamedKey::ZoomIn,
        Keysym::XF86_ZoomOut => NamedKey::ZoomOut,

        // XF86_Away..XF86_Messenger
        Keysym::XF86_WebCam => NamedKey::LaunchWebCam,
        Keysym::XF86_MailForward => NamedKey::MailForward,
        // XF86_Pictures
        Keysym::XF86_Music => NamedKey::LaunchMusicPlayer,

        // XF86_Battery..XF86_UWB
        Keysym::XF86_AudioForward => NamedKey::MediaFastForward,
        // XF86_AudioRepeat
        Keysym::XF86_AudioRandomPlay => NamedKey::RandomToggle,
        Keysym::XF86_Subtitle => NamedKey::Subtitle,
        Keysym::XF86_AudioCycleTrack => NamedKey::MediaAudioTrack,
        // XF86_CycleAngle..XF86_Blue
        Keysym::XF86_Suspend => NamedKey::Standby,
        Keysym::XF86_Hibernate => NamedKey::Hibernate,
        // XF86_TouchpadToggle..XF86_TouchpadOff
        Keysym::XF86_AudioMute => NamedKey::AudioVolumeMute,

        // XF86_Switch_VT_1..XF86_Switch_VT_12

        // XF86_Ungrab..XF86_ClearGrab
        Keysym::XF86_Next_VMode => NamedKey::VideoModeNext,
        // Keysym::XF86_Prev_VMode => NamedKey::VideoModePrevious,
        // XF86_LogWindowTree..XF86_LogGrabInfo

        // SunFA_Grave..SunFA_Cedilla

        // Keysym::SunF36 => NamedKey::F36 | NamedKey::F11,
        // Keysym::SunF37 => NamedKey::F37 | NamedKey::F12,

        // Keysym::SunSys_Req => NamedKey::PrintScreen,
        // The next couple of xkb (until SunStop) are already handled.
        // SunPrint_Screen..SunPageDown

        // SunUndo..SunFront
        Keysym::SUN_Copy => NamedKey::Copy,
        Keysym::SUN_Open => NamedKey::Open,
        Keysym::SUN_Paste => NamedKey::Paste,
        Keysym::SUN_Cut => NamedKey::Cut,

        // SunPowerSwitch
        Keysym::SUN_AudioLowerVolume => NamedKey::AudioVolumeDown,
        Keysym::SUN_AudioMute => NamedKey::AudioVolumeMute,
        Keysym::SUN_AudioRaiseVolume => NamedKey::AudioVolumeUp,
        // SUN_VideoDegauss
        Keysym::SUN_VideoLowerBrightness => NamedKey::BrightnessDown,
        Keysym::SUN_VideoRaiseBrightness => NamedKey::BrightnessUp,
        // SunPowerSwitchShift
        _ => return None,
    })
}

/// Where on the keyboard `keysym` sits (left and right modifiers, the numeric keypad).
#[rustfmt::skip]
pub(super) fn location(keysym: Keysym) -> Location {
    match keysym {
        Keysym::Shift_L
        | Keysym::Control_L
        | Keysym::Meta_L
        | Keysym::Alt_L
        | Keysym::Super_L
        | Keysym::Hyper_L => Location::Left,
        Keysym::Shift_R
        | Keysym::Control_R
        | Keysym::Meta_R
        | Keysym::Alt_R
        | Keysym::Super_R
        | Keysym::Hyper_R => Location::Right,
        Keysym::KP_0
        | Keysym::KP_1
        | Keysym::KP_2
        | Keysym::KP_3
        | Keysym::KP_4
        | Keysym::KP_5
        | Keysym::KP_6
        | Keysym::KP_7
        | Keysym::KP_8
        | Keysym::KP_9
        | Keysym::KP_Space
        | Keysym::KP_Tab
        | Keysym::KP_Enter
        | Keysym::KP_F1
        | Keysym::KP_F2
        | Keysym::KP_F3
        | Keysym::KP_F4
        | Keysym::KP_Home
        | Keysym::KP_Left
        | Keysym::KP_Up
        | Keysym::KP_Right
        | Keysym::KP_Down
        | Keysym::KP_Page_Up
        | Keysym::KP_Page_Down
        | Keysym::KP_End
        | Keysym::KP_Begin
        | Keysym::KP_Insert
        | Keysym::KP_Delete
        | Keysym::KP_Equal
        | Keysym::KP_Multiply
        | Keysym::KP_Add
        | Keysym::KP_Separator
        | Keysym::KP_Subtract
        | Keysym::KP_Decimal
        | Keysym::KP_Divide => Location::Numpad,
        _ => Location::Standard,
    }
}
