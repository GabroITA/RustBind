use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Rappresentazione astratta dei tasti supportati, indipendente da Windows e da codici FFI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VirtualKey {
    // Lettere
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,

    // Numeri standard (fila superiore)
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,

    // Tasti funzione
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,

    // Modificatori generici come tasti autonomi
    Shift,
    Control,
    Alt,
    Windows,

    // Navigazione ed editing
    Enter,
    Escape,
    Space,
    Tab,
    Backspace,
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,

    // Blocchi e controlli
    CapsLock,
    ScrollLock,
    NumLock,
    PrintScreen,
    Pause,

    // Tastierino numerico
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadSubtract,
    NumpadMultiply,
    NumpadDivide,
    NumpadDecimal,
    NumpadEnter,

    // Simboli e punteggiatura
    Semicolon,
    Equal,
    Comma,
    Minus,
    Period,
    Slash,
    Backquote,
    LeftBracket,
    Backslash,
    RightBracket,
    Quote,

    // Controlli multimediali standard
    VolumeMute,
    VolumeDown,
    VolumeUp,
    MediaNext,
    MediaPrev,
    MediaStop,
    MediaPlayPause,
}

impl VirtualKey {
    /// Elenco completo di tutte le varianti di `VirtualKey`, ordinato per categoria
    /// (lettere, numeri, funzione, navigazione, tastierino, simboli, media, controlli, modificatori).
    pub const ALL: &'static [VirtualKey] = &[
        // Lettere
        Self::KeyA,
        Self::KeyB,
        Self::KeyC,
        Self::KeyD,
        Self::KeyE,
        Self::KeyF,
        Self::KeyG,
        Self::KeyH,
        Self::KeyI,
        Self::KeyJ,
        Self::KeyK,
        Self::KeyL,
        Self::KeyM,
        Self::KeyN,
        Self::KeyO,
        Self::KeyP,
        Self::KeyQ,
        Self::KeyR,
        Self::KeyS,
        Self::KeyT,
        Self::KeyU,
        Self::KeyV,
        Self::KeyW,
        Self::KeyX,
        Self::KeyY,
        Self::KeyZ,
        // Numeri
        Self::Num0,
        Self::Num1,
        Self::Num2,
        Self::Num3,
        Self::Num4,
        Self::Num5,
        Self::Num6,
        Self::Num7,
        Self::Num8,
        Self::Num9,
        // Funzione
        Self::F1,
        Self::F2,
        Self::F3,
        Self::F4,
        Self::F5,
        Self::F6,
        Self::F7,
        Self::F8,
        Self::F9,
        Self::F10,
        Self::F11,
        Self::F12,
        Self::F13,
        Self::F14,
        Self::F15,
        Self::F16,
        Self::F17,
        Self::F18,
        Self::F19,
        Self::F20,
        Self::F21,
        Self::F22,
        Self::F23,
        Self::F24,
        // Navigazione
        Self::Enter,
        Self::Escape,
        Self::Space,
        Self::Tab,
        Self::Backspace,
        Self::Insert,
        Self::Delete,
        Self::Home,
        Self::End,
        Self::PageUp,
        Self::PageDown,
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        // Tastierino numerico
        Self::Numpad0,
        Self::Numpad1,
        Self::Numpad2,
        Self::Numpad3,
        Self::Numpad4,
        Self::Numpad5,
        Self::Numpad6,
        Self::Numpad7,
        Self::Numpad8,
        Self::Numpad9,
        Self::NumpadAdd,
        Self::NumpadSubtract,
        Self::NumpadMultiply,
        Self::NumpadDivide,
        Self::NumpadDecimal,
        Self::NumpadEnter,
        // Simboli
        Self::Semicolon,
        Self::Equal,
        Self::Comma,
        Self::Minus,
        Self::Period,
        Self::Slash,
        Self::Backquote,
        Self::LeftBracket,
        Self::Backslash,
        Self::RightBracket,
        Self::Quote,
        // Media
        Self::VolumeMute,
        Self::VolumeDown,
        Self::VolumeUp,
        Self::MediaNext,
        Self::MediaPrev,
        Self::MediaStop,
        Self::MediaPlayPause,
        // Controlli
        Self::CapsLock,
        Self::ScrollLock,
        Self::NumLock,
        Self::PrintScreen,
        Self::Pause,
        // Modificatori
        Self::Shift,
        Self::Control,
        Self::Alt,
        Self::Windows,
    ];

    /// Restituisce il nome canonico del tasto in formato leggibile.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::KeyA => "A",
            Self::KeyB => "B",
            Self::KeyC => "C",
            Self::KeyD => "D",
            Self::KeyE => "E",
            Self::KeyF => "F",
            Self::KeyG => "G",
            Self::KeyH => "H",
            Self::KeyI => "I",
            Self::KeyJ => "J",
            Self::KeyK => "K",
            Self::KeyL => "L",
            Self::KeyM => "M",
            Self::KeyN => "N",
            Self::KeyO => "O",
            Self::KeyP => "P",
            Self::KeyQ => "Q",
            Self::KeyR => "R",
            Self::KeyS => "S",
            Self::KeyT => "T",
            Self::KeyU => "U",
            Self::KeyV => "V",
            Self::KeyW => "W",
            Self::KeyX => "X",
            Self::KeyY => "Y",
            Self::KeyZ => "Z",

            Self::Num0 => "0",
            Self::Num1 => "1",
            Self::Num2 => "2",
            Self::Num3 => "3",
            Self::Num4 => "4",
            Self::Num5 => "5",
            Self::Num6 => "6",
            Self::Num7 => "7",
            Self::Num8 => "8",
            Self::Num9 => "9",

            Self::F1 => "F1",
            Self::F2 => "F2",
            Self::F3 => "F3",
            Self::F4 => "F4",
            Self::F5 => "F5",
            Self::F6 => "F6",
            Self::F7 => "F7",
            Self::F8 => "F8",
            Self::F9 => "F9",
            Self::F10 => "F10",
            Self::F11 => "F11",
            Self::F12 => "F12",
            Self::F13 => "F13",
            Self::F14 => "F14",
            Self::F15 => "F15",
            Self::F16 => "F16",
            Self::F17 => "F17",
            Self::F18 => "F18",
            Self::F19 => "F19",
            Self::F20 => "F20",
            Self::F21 => "F21",
            Self::F22 => "F22",
            Self::F23 => "F23",
            Self::F24 => "F24",

            Self::Shift => "Shift",
            Self::Control => "Ctrl",
            Self::Alt => "Alt",
            Self::Windows => "Win",

            Self::Enter => "Enter",
            Self::Escape => "Escape",
            Self::Space => "Space",
            Self::Tab => "Tab",
            Self::Backspace => "Backspace",
            Self::Insert => "Insert",
            Self::Delete => "Delete",
            Self::Home => "Home",
            Self::End => "End",
            Self::PageUp => "PageUp",
            Self::PageDown => "PageDown",
            Self::Up => "Up",
            Self::Down => "Down",
            Self::Left => "Left",
            Self::Right => "Right",

            Self::CapsLock => "CapsLock",
            Self::ScrollLock => "ScrollLock",
            Self::NumLock => "NumLock",
            Self::PrintScreen => "PrintScreen",
            Self::Pause => "Pause",

            Self::Numpad0 => "Numpad0",
            Self::Numpad1 => "Numpad1",
            Self::Numpad2 => "Numpad2",
            Self::Numpad3 => "Numpad3",
            Self::Numpad4 => "Numpad4",
            Self::Numpad5 => "Numpad5",
            Self::Numpad6 => "Numpad6",
            Self::Numpad7 => "Numpad7",
            Self::Numpad8 => "Numpad8",
            Self::Numpad9 => "Numpad9",
            Self::NumpadAdd => "NumpadAdd",
            Self::NumpadSubtract => "NumpadSubtract",
            Self::NumpadMultiply => "NumpadMultiply",
            Self::NumpadDivide => "NumpadDivide",
            Self::NumpadDecimal => "NumpadDecimal",
            Self::NumpadEnter => "NumpadEnter",

            Self::Semicolon => ";",
            Self::Equal => "=",
            Self::Comma => ",",
            Self::Minus => "-",
            Self::Period => ".",
            Self::Slash => "/",
            Self::Backquote => "`",
            Self::LeftBracket => "[",
            Self::Backslash => "\\",
            Self::RightBracket => "]",
            Self::Quote => "'",

            Self::VolumeMute => "VolumeMute",
            Self::VolumeDown => "VolumeDown",
            Self::VolumeUp => "VolumeUp",
            Self::MediaNext => "MediaNext",
            Self::MediaPrev => "MediaPrev",
            Self::MediaStop => "MediaStop",
            Self::MediaPlayPause => "MediaPlayPause",
        }
    }
}

impl fmt::Display for VirtualKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseKeyError(pub String);

impl fmt::Display for ParseKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tasto sconosciuto o non valido: '{}'", self.0)
    }
}

impl std::error::Error for ParseKeyError {}

impl FromStr for VirtualKey {
    type Err = ParseKeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        let upper = trimmed.to_uppercase();

        match upper.as_str() {
            "A" => Ok(Self::KeyA),
            "B" => Ok(Self::KeyB),
            "C" => Ok(Self::KeyC),
            "D" => Ok(Self::KeyD),
            "E" => Ok(Self::KeyE),
            "F" => Ok(Self::KeyF),
            "G" => Ok(Self::KeyG),
            "H" => Ok(Self::KeyH),
            "I" => Ok(Self::KeyI),
            "J" => Ok(Self::KeyJ),
            "K" => Ok(Self::KeyK),
            "L" => Ok(Self::KeyL),
            "M" => Ok(Self::KeyM),
            "N" => Ok(Self::KeyN),
            "O" => Ok(Self::KeyO),
            "P" => Ok(Self::KeyP),
            "Q" => Ok(Self::KeyQ),
            "R" => Ok(Self::KeyR),
            "S" => Ok(Self::KeyS),
            "T" => Ok(Self::KeyT),
            "U" => Ok(Self::KeyU),
            "V" => Ok(Self::KeyV),
            "W" => Ok(Self::KeyW),
            "X" => Ok(Self::KeyX),
            "Y" => Ok(Self::KeyY),
            "Z" => Ok(Self::KeyZ),

            "0" => Ok(Self::Num0),
            "1" => Ok(Self::Num1),
            "2" => Ok(Self::Num2),
            "3" => Ok(Self::Num3),
            "4" => Ok(Self::Num4),
            "5" => Ok(Self::Num5),
            "6" => Ok(Self::Num6),
            "7" => Ok(Self::Num7),
            "8" => Ok(Self::Num8),
            "9" => Ok(Self::Num9),

            "F1" => Ok(Self::F1),
            "F2" => Ok(Self::F2),
            "F3" => Ok(Self::F3),
            "F4" => Ok(Self::F4),
            "F5" => Ok(Self::F5),
            "F6" => Ok(Self::F6),
            "F7" => Ok(Self::F7),
            "F8" => Ok(Self::F8),
            "F9" => Ok(Self::F9),
            "F10" => Ok(Self::F10),
            "F11" => Ok(Self::F11),
            "F12" => Ok(Self::F12),
            "F13" => Ok(Self::F13),
            "F14" => Ok(Self::F14),
            "F15" => Ok(Self::F15),
            "F16" => Ok(Self::F16),
            "F17" => Ok(Self::F17),
            "F18" => Ok(Self::F18),
            "F19" => Ok(Self::F19),
            "F20" => Ok(Self::F20),
            "F21" => Ok(Self::F21),
            "F22" => Ok(Self::F22),
            "F23" => Ok(Self::F23),
            "F24" => Ok(Self::F24),

            "SHIFT" => Ok(Self::Shift),
            "CTRL" | "CONTROL" => Ok(Self::Control),
            "ALT" => Ok(Self::Alt),
            "WIN" | "WINDOWS" | "SUPER" => Ok(Self::Windows),

            "ENTER" | "RETURN" => Ok(Self::Enter),
            "ESC" | "ESCAPE" => Ok(Self::Escape),
            "SPACE" | "SPACEBAR" => Ok(Self::Space),
            "TAB" => Ok(Self::Tab),
            "BACKSPACE" | "BKSP" => Ok(Self::Backspace),
            "INS" | "INSERT" => Ok(Self::Insert),
            "DEL" | "DELETE" => Ok(Self::Delete),
            "HOME" => Ok(Self::Home),
            "END" => Ok(Self::End),
            "PGUP" | "PAGEUP" => Ok(Self::PageUp),
            "PGDN" | "PAGEDOWN" => Ok(Self::PageDown),
            "UP" => Ok(Self::Up),
            "DOWN" => Ok(Self::Down),
            "LEFT" => Ok(Self::Left),
            "RIGHT" => Ok(Self::Right),

            "CAPSLOCK" | "CAPS" => Ok(Self::CapsLock),
            "SCROLLLOCK" => Ok(Self::ScrollLock),
            "NUMLOCK" => Ok(Self::NumLock),
            "PRINTSCREEN" | "PRTSC" | "PRTSCR" => Ok(Self::PrintScreen),
            "PAUSE" | "BREAK" => Ok(Self::Pause),

            "NUMPAD0" | "NUM0" => Ok(Self::Numpad0),
            "NUMPAD1" | "NUM1" => Ok(Self::Numpad1),
            "NUMPAD2" | "NUM2" => Ok(Self::Numpad2),
            "NUMPAD3" | "NUM3" => Ok(Self::Numpad3),
            "NUMPAD4" | "NUM4" => Ok(Self::Numpad4),
            "NUMPAD5" | "NUM5" => Ok(Self::Numpad5),
            "NUMPAD6" | "NUM6" => Ok(Self::Numpad6),
            "NUMPAD7" | "NUM7" => Ok(Self::Numpad7),
            "NUMPAD8" | "NUM8" => Ok(Self::Numpad8),
            "NUMPAD9" | "NUM9" => Ok(Self::Numpad9),
            "NUMPADADD" | "NUMADD" | "ADD" => Ok(Self::NumpadAdd),
            "NUMPADSUBTRACT" | "NUMSUB" | "SUBTRACT" => Ok(Self::NumpadSubtract),
            "NUMPADMULTIPLY" | "NUMMUL" | "MULTIPLY" => Ok(Self::NumpadMultiply),
            "NUMPADDIVIDE" | "NUMDIV" | "DIVIDE" => Ok(Self::NumpadDivide),
            "NUMPADDECIMAL" | "NUMDEC" | "DECIMAL" => Ok(Self::NumpadDecimal),
            "NUMPADENTER" => Ok(Self::NumpadEnter),

            ";" | "SEMICOLON" => Ok(Self::Semicolon),
            "=" | "EQUAL" => Ok(Self::Equal),
            "," | "COMMA" => Ok(Self::Comma),
            "-" | "MINUS" => Ok(Self::Minus),
            "." | "PERIOD" | "DOT" => Ok(Self::Period),
            "/" | "SLASH" => Ok(Self::Slash),
            "`" | "BACKQUOTE" | "GRAVE" => Ok(Self::Backquote),
            "[" | "LEFTBRACKET" => Ok(Self::LeftBracket),
            "\\" | "BACKSLASH" => Ok(Self::Backslash),
            "]" | "RIGHTBRACKET" => Ok(Self::RightBracket),
            "'" | "QUOTE" => Ok(Self::Quote),

            "VOLUMEMUTE" => Ok(Self::VolumeMute),
            "VOLUMEDOWN" => Ok(Self::VolumeDown),
            "VOLUMEUP" => Ok(Self::VolumeUp),
            "MEDIANEXT" => Ok(Self::MediaNext),
            "MEDIAPREV" => Ok(Self::MediaPrev),
            "MEDIASTOP" => Ok(Self::MediaStop),
            "MEDIAPLAYPAUSE" => Ok(Self::MediaPlayPause),

            _ => Err(ParseKeyError(s.to_string())),
        }
    }
}

impl Serialize for VirtualKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for VirtualKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn test_all_virtual_keys_roundtrip_and_completeness() {
        let mut seen = HashSet::new();
        for key in VirtualKey::ALL {
            // Verifica che non ci siano duplicati in ALL
            assert!(
                seen.insert(*key),
                "Variante duplicata in VirtualKey::ALL: {:?}",
                key
            );
            // Verifica roundtrip canonico as_str -> from_str
            assert_eq!(
                VirtualKey::from_str(key.as_str()),
                Ok(*key),
                "Roundtrip fallito per VirtualKey::{:?} ('{}')",
                key,
                key.as_str()
            );
        }

        // Match esaustivo: garantisce a compile-time che nessuna variante manchi da VirtualKey::ALL
        assert_eq!(VirtualKey::ALL.len(), 118);
        for key in VirtualKey::ALL {
            match key {
                VirtualKey::KeyA
                | VirtualKey::KeyB
                | VirtualKey::KeyC
                | VirtualKey::KeyD
                | VirtualKey::KeyE
                | VirtualKey::KeyF
                | VirtualKey::KeyG
                | VirtualKey::KeyH
                | VirtualKey::KeyI
                | VirtualKey::KeyJ
                | VirtualKey::KeyK
                | VirtualKey::KeyL
                | VirtualKey::KeyM
                | VirtualKey::KeyN
                | VirtualKey::KeyO
                | VirtualKey::KeyP
                | VirtualKey::KeyQ
                | VirtualKey::KeyR
                | VirtualKey::KeyS
                | VirtualKey::KeyT
                | VirtualKey::KeyU
                | VirtualKey::KeyV
                | VirtualKey::KeyW
                | VirtualKey::KeyX
                | VirtualKey::KeyY
                | VirtualKey::KeyZ
                | VirtualKey::Num0
                | VirtualKey::Num1
                | VirtualKey::Num2
                | VirtualKey::Num3
                | VirtualKey::Num4
                | VirtualKey::Num5
                | VirtualKey::Num6
                | VirtualKey::Num7
                | VirtualKey::Num8
                | VirtualKey::Num9
                | VirtualKey::F1
                | VirtualKey::F2
                | VirtualKey::F3
                | VirtualKey::F4
                | VirtualKey::F5
                | VirtualKey::F6
                | VirtualKey::F7
                | VirtualKey::F8
                | VirtualKey::F9
                | VirtualKey::F10
                | VirtualKey::F11
                | VirtualKey::F12
                | VirtualKey::F13
                | VirtualKey::F14
                | VirtualKey::F15
                | VirtualKey::F16
                | VirtualKey::F17
                | VirtualKey::F18
                | VirtualKey::F19
                | VirtualKey::F20
                | VirtualKey::F21
                | VirtualKey::F22
                | VirtualKey::F23
                | VirtualKey::F24
                | VirtualKey::Shift
                | VirtualKey::Control
                | VirtualKey::Alt
                | VirtualKey::Windows
                | VirtualKey::Enter
                | VirtualKey::Escape
                | VirtualKey::Space
                | VirtualKey::Tab
                | VirtualKey::Backspace
                | VirtualKey::Insert
                | VirtualKey::Delete
                | VirtualKey::Home
                | VirtualKey::End
                | VirtualKey::PageUp
                | VirtualKey::PageDown
                | VirtualKey::Up
                | VirtualKey::Down
                | VirtualKey::Left
                | VirtualKey::Right
                | VirtualKey::CapsLock
                | VirtualKey::ScrollLock
                | VirtualKey::NumLock
                | VirtualKey::PrintScreen
                | VirtualKey::Pause
                | VirtualKey::Numpad0
                | VirtualKey::Numpad1
                | VirtualKey::Numpad2
                | VirtualKey::Numpad3
                | VirtualKey::Numpad4
                | VirtualKey::Numpad5
                | VirtualKey::Numpad6
                | VirtualKey::Numpad7
                | VirtualKey::Numpad8
                | VirtualKey::Numpad9
                | VirtualKey::NumpadAdd
                | VirtualKey::NumpadSubtract
                | VirtualKey::NumpadMultiply
                | VirtualKey::NumpadDivide
                | VirtualKey::NumpadDecimal
                | VirtualKey::NumpadEnter
                | VirtualKey::Semicolon
                | VirtualKey::Equal
                | VirtualKey::Comma
                | VirtualKey::Minus
                | VirtualKey::Period
                | VirtualKey::Slash
                | VirtualKey::Backquote
                | VirtualKey::LeftBracket
                | VirtualKey::Backslash
                | VirtualKey::RightBracket
                | VirtualKey::Quote
                | VirtualKey::VolumeMute
                | VirtualKey::VolumeDown
                | VirtualKey::VolumeUp
                | VirtualKey::MediaNext
                | VirtualKey::MediaPrev
                | VirtualKey::MediaStop
                | VirtualKey::MediaPlayPause => {}
            }
        }
    }
}
