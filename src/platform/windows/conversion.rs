use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, VIRTUAL_KEY, VK_0,
    VK_1, VK_2, VK_3, VK_4, VK_5, VK_6, VK_7, VK_8, VK_9, VK_A, VK_ADD, VK_B, VK_BACK, VK_C,
    VK_CAPITAL, VK_D, VK_DECIMAL, VK_DELETE, VK_DIVIDE, VK_DOWN, VK_E, VK_END, VK_ESCAPE, VK_F,
    VK_F1, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_F10, VK_F11, VK_F12, VK_F13,
    VK_F14, VK_F15, VK_F16, VK_F17, VK_F18, VK_F19, VK_F20, VK_F21, VK_F22, VK_F23, VK_F24, VK_G,
    VK_H, VK_HOME, VK_I, VK_INSERT, VK_J, VK_K, VK_L, VK_LEFT, VK_LWIN, VK_M, VK_MEDIA_NEXT_TRACK,
    VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_MEDIA_STOP, VK_MENU, VK_MULTIPLY, VK_N, VK_NEXT,
    VK_NUMLOCK, VK_NUMPAD0, VK_NUMPAD1, VK_NUMPAD2, VK_NUMPAD3, VK_NUMPAD4, VK_NUMPAD5, VK_NUMPAD6,
    VK_NUMPAD7, VK_NUMPAD8, VK_NUMPAD9, VK_O, VK_OEM_1, VK_OEM_2, VK_OEM_3, VK_OEM_4, VK_OEM_5,
    VK_OEM_6, VK_OEM_7, VK_OEM_COMMA, VK_OEM_MINUS, VK_OEM_PERIOD, VK_OEM_PLUS, VK_P, VK_PAUSE,
    VK_PRIOR, VK_Q, VK_R, VK_RETURN, VK_RIGHT, VK_S, VK_SCROLL, VK_SHIFT, VK_SNAPSHOT, VK_SPACE,
    VK_SUBTRACT, VK_T, VK_TAB, VK_U, VK_UP, VK_V, VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP,
    VK_W, VK_X, VK_Y, VK_Z,
};

use crate::core::key::VirtualKey;
use crate::core::modifiers::Modifiers;

/// Converte un VirtualKey astratto nel codice nativo Win32 VIRTUAL_KEY.
pub fn to_win32_vk(key: VirtualKey) -> VIRTUAL_KEY {
    match key {
        VirtualKey::KeyA => VK_A,
        VirtualKey::KeyB => VK_B,
        VirtualKey::KeyC => VK_C,
        VirtualKey::KeyD => VK_D,
        VirtualKey::KeyE => VK_E,
        VirtualKey::KeyF => VK_F,
        VirtualKey::KeyG => VK_G,
        VirtualKey::KeyH => VK_H,
        VirtualKey::KeyI => VK_I,
        VirtualKey::KeyJ => VK_J,
        VirtualKey::KeyK => VK_K,
        VirtualKey::KeyL => VK_L,
        VirtualKey::KeyM => VK_M,
        VirtualKey::KeyN => VK_N,
        VirtualKey::KeyO => VK_O,
        VirtualKey::KeyP => VK_P,
        VirtualKey::KeyQ => VK_Q,
        VirtualKey::KeyR => VK_R,
        VirtualKey::KeyS => VK_S,
        VirtualKey::KeyT => VK_T,
        VirtualKey::KeyU => VK_U,
        VirtualKey::KeyV => VK_V,
        VirtualKey::KeyW => VK_W,
        VirtualKey::KeyX => VK_X,
        VirtualKey::KeyY => VK_Y,
        VirtualKey::KeyZ => VK_Z,

        VirtualKey::Num0 => VK_0,
        VirtualKey::Num1 => VK_1,
        VirtualKey::Num2 => VK_2,
        VirtualKey::Num3 => VK_3,
        VirtualKey::Num4 => VK_4,
        VirtualKey::Num5 => VK_5,
        VirtualKey::Num6 => VK_6,
        VirtualKey::Num7 => VK_7,
        VirtualKey::Num8 => VK_8,
        VirtualKey::Num9 => VK_9,

        VirtualKey::F1 => VK_F1,
        VirtualKey::F2 => VK_F2,
        VirtualKey::F3 => VK_F3,
        VirtualKey::F4 => VK_F4,
        VirtualKey::F5 => VK_F5,
        VirtualKey::F6 => VK_F6,
        VirtualKey::F7 => VK_F7,
        VirtualKey::F8 => VK_F8,
        VirtualKey::F9 => VK_F9,
        VirtualKey::F10 => VK_F10,
        VirtualKey::F11 => VK_F11,
        VirtualKey::F12 => VK_F12,
        VirtualKey::F13 => VK_F13,
        VirtualKey::F14 => VK_F14,
        VirtualKey::F15 => VK_F15,
        VirtualKey::F16 => VK_F16,
        VirtualKey::F17 => VK_F17,
        VirtualKey::F18 => VK_F18,
        VirtualKey::F19 => VK_F19,
        VirtualKey::F20 => VK_F20,
        VirtualKey::F21 => VK_F21,
        VirtualKey::F22 => VK_F22,
        VirtualKey::F23 => VK_F23,
        VirtualKey::F24 => VK_F24,

        VirtualKey::Shift => VK_SHIFT,
        VirtualKey::Control => windows::Win32::UI::Input::KeyboardAndMouse::VK_CONTROL,
        VirtualKey::Alt => VK_MENU,
        VirtualKey::Windows => VK_LWIN,

        VirtualKey::Enter => VK_RETURN,
        VirtualKey::Escape => VK_ESCAPE,
        VirtualKey::Space => VK_SPACE,
        VirtualKey::Tab => VK_TAB,
        VirtualKey::Backspace => VK_BACK,
        VirtualKey::Insert => VK_INSERT,
        VirtualKey::Delete => VK_DELETE,
        VirtualKey::Home => VK_HOME,
        VirtualKey::End => VK_END,
        VirtualKey::PageUp => VK_PRIOR,
        VirtualKey::PageDown => VK_NEXT,
        VirtualKey::Up => VK_UP,
        VirtualKey::Down => VK_DOWN,
        VirtualKey::Left => VK_LEFT,
        VirtualKey::Right => VK_RIGHT,

        VirtualKey::CapsLock => VK_CAPITAL,
        VirtualKey::ScrollLock => VK_SCROLL,
        VirtualKey::NumLock => VK_NUMLOCK,
        VirtualKey::PrintScreen => VK_SNAPSHOT,
        VirtualKey::Pause => VK_PAUSE,

        VirtualKey::Numpad0 => VK_NUMPAD0,
        VirtualKey::Numpad1 => VK_NUMPAD1,
        VirtualKey::Numpad2 => VK_NUMPAD2,
        VirtualKey::Numpad3 => VK_NUMPAD3,
        VirtualKey::Numpad4 => VK_NUMPAD4,
        VirtualKey::Numpad5 => VK_NUMPAD5,
        VirtualKey::Numpad6 => VK_NUMPAD6,
        VirtualKey::Numpad7 => VK_NUMPAD7,
        VirtualKey::Numpad8 => VK_NUMPAD8,
        VirtualKey::Numpad9 => VK_NUMPAD9,
        VirtualKey::NumpadAdd => VK_ADD,
        VirtualKey::NumpadSubtract => VK_SUBTRACT,
        VirtualKey::NumpadMultiply => VK_MULTIPLY,
        VirtualKey::NumpadDivide => VK_DIVIDE,
        VirtualKey::NumpadDecimal => VK_DECIMAL,
        VirtualKey::NumpadEnter => VK_RETURN,

        VirtualKey::Semicolon => VK_OEM_1,
        VirtualKey::Equal => VK_OEM_PLUS,
        VirtualKey::Comma => VK_OEM_COMMA,
        VirtualKey::Minus => VK_OEM_MINUS,
        VirtualKey::Period => VK_OEM_PERIOD,
        VirtualKey::Slash => VK_OEM_2,
        VirtualKey::Backquote => VK_OEM_3,
        VirtualKey::LeftBracket => VK_OEM_4,
        VirtualKey::Backslash => VK_OEM_5,
        VirtualKey::RightBracket => VK_OEM_6,
        VirtualKey::Quote => VK_OEM_7,

        VirtualKey::VolumeMute => VK_VOLUME_MUTE,
        VirtualKey::VolumeDown => VK_VOLUME_DOWN,
        VirtualKey::VolumeUp => VK_VOLUME_UP,
        VirtualKey::MediaNext => VK_MEDIA_NEXT_TRACK,
        VirtualKey::MediaPrev => VK_MEDIA_PREV_TRACK,
        VirtualKey::MediaStop => VK_MEDIA_STOP,
        VirtualKey::MediaPlayPause => VK_MEDIA_PLAY_PAUSE,
    }
}

/// Restituisce `true` se il tasto richiede il flag Win32 `KEYEVENTF_EXTENDEDKEY`.
pub fn is_extended_virtual_key(key: VirtualKey) -> bool {
    matches!(
        key,
        VirtualKey::Insert
            | VirtualKey::Delete
            | VirtualKey::Home
            | VirtualKey::End
            | VirtualKey::PageUp
            | VirtualKey::PageDown
            | VirtualKey::Up
            | VirtualKey::Down
            | VirtualKey::Left
            | VirtualKey::Right
            | VirtualKey::Windows
            | VirtualKey::PrintScreen
            | VirtualKey::NumLock
            | VirtualKey::NumpadDivide
            | VirtualKey::NumpadEnter
            | VirtualKey::VolumeMute
            | VirtualKey::VolumeDown
            | VirtualKey::VolumeUp
            | VirtualKey::MediaNext
            | VirtualKey::MediaPrev
            | VirtualKey::MediaStop
            | VirtualKey::MediaPlayPause
    )
}

/// Converte i Modifiers astratti nei flag HOT_KEY_MODIFIERS di Windows.
///
/// Include SEMPRE il flag MOD_NOREPEAT per evitare eventi multipli ripetuti
/// quando l'utente tiene premuta la combinazione.
pub fn to_win32_modifiers(modifiers: &Modifiers) -> HOT_KEY_MODIFIERS {
    let mut flags = MOD_NOREPEAT;

    if modifiers.alt {
        flags |= MOD_ALT;
    }
    if modifiers.ctrl {
        flags |= MOD_CONTROL;
    }
    if modifiers.shift {
        flags |= MOD_SHIFT;
    }
    if modifiers.win {
        flags |= MOD_WIN;
    }

    flags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_win32_vk_conversion() {
        assert_eq!(to_win32_vk(VirtualKey::F12), VK_F12);
        assert_eq!(to_win32_vk(VirtualKey::KeyA), VK_A);
        assert_eq!(to_win32_vk(VirtualKey::Enter), VK_RETURN);
        assert_eq!(to_win32_vk(VirtualKey::Space), VK_SPACE);
    }

    #[test]
    fn test_to_win32_modifiers_includes_norepeat() {
        let m = Modifiers {
            ctrl: true,
            alt: true,
            shift: false,
            win: false,
        };
        let win_mods = to_win32_modifiers(&m);
        assert_eq!(win_mods, MOD_NOREPEAT | MOD_CONTROL | MOD_ALT);
    }

    #[test]
    fn test_is_extended_virtual_key_classification() {
        for extended in [
            VirtualKey::Home,
            VirtualKey::End,
            VirtualKey::Left,
            VirtualKey::Right,
            VirtualKey::Up,
            VirtualKey::Down,
            VirtualKey::Insert,
            VirtualKey::Delete,
            VirtualKey::PageUp,
            VirtualKey::PageDown,
            VirtualKey::NumpadEnter,
            VirtualKey::NumpadDivide,
            VirtualKey::Windows,
        ] {
            assert!(
                is_extended_virtual_key(extended),
                "{:?} deve essere classificato come tasto esteso",
                extended
            );
        }

        for non_extended in [
            VirtualKey::Enter,
            VirtualKey::KeyA,
            VirtualKey::Space,
            VirtualKey::Tab,
            VirtualKey::Numpad1,
            VirtualKey::F12,
        ] {
            assert!(
                !is_extended_virtual_key(non_extended),
                "{:?} non deve essere classificato come tasto esteso",
                non_extended
            );
        }
    }
}
