use std::fmt;

use serde::{Deserialize, Serialize};

use crate::core::key::VirtualKey;

/// Pulsanti standard e laterali del mouse supportati da Win32 `SendInput` (`INPUT_MOUSE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

impl MouseButton {
    pub const ALL: &'static [Self] = &[Self::Left, Self::Right, Self::Middle, Self::X1, Self::X2];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Middle => "Middle",
            Self::X1 => "X1",
            Self::X2 => "X2",
        }
    }
}

/// Direzione dello scorrimento rotella tramite `SendInput` (`MOUSEEVENTF_WHEEL` / `MOUSEEVENTF_HWHEEL`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseWheelDirection {
    #[default]
    Vertical,
    Horizontal,
}

impl MouseWheelDirection {
    pub const ALL: &'static [Self] = &[Self::Vertical, Self::Horizontal];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vertical => "Vertical",
            Self::Horizontal => "Horizontal",
        }
    }
}

/// Singola azione elementare di input o ritardo eseguibile dal motore.
///
/// In conformità alle direttive di sicurezza e privacy:
/// - Esclusa qualsiasi azione di avvio processi (`LaunchProcess`).
/// - Le azioni mouse utilizzano esclusivamente `INPUT_MOUSE` via Win32 `SendInput`.
/// - L'implementazione di `Debug` oscura rigorosamente il contenuto testuale di `SendText`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputAction {
    /// Pressione e rilascio immediato di un tasto virtuale.
    KeyClick { key: VirtualKey },
    /// Pressione (down) di un tasto virtuale.
    KeyDown { key: VirtualKey },
    /// Rilascio (up) di un tasto virtuale.
    KeyUp { key: VirtualKey },
    /// Digitazione di testo Unicode sintetico.
    SendText { text: String },
    /// Pausa deterministica di durata specificata in millisecondi.
    Delay { milliseconds: u64 },
    /// Pressione e rilascio immediato di un pulsante del mouse.
    MouseClick { button: MouseButton },
    /// Pressione mantenuta (down) di un pulsante del mouse.
    MouseDown { button: MouseButton },
    /// Rilascio (up) di un pulsante del mouse.
    MouseUp { button: MouseButton },
    /// Scorrimento della rotella del mouse espresso in tacche (`steps`, ±120 unità Win32 per tacca).
    MouseScroll {
        #[serde(default)]
        direction: MouseWheelDirection,
        steps: i32,
    },
    /// Spostamento relativo del puntatore in pixel (`dx`, `dy`) tramite `MOUSEEVENTF_MOVE`.
    MouseMove { dx: i32, dy: i32 },
}

impl fmt::Debug for InputAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KeyClick { key } => f.debug_struct("KeyClick").field("key", key).finish(),
            Self::KeyDown { key } => f.debug_struct("KeyDown").field("key", key).finish(),
            Self::KeyUp { key } => f.debug_struct("KeyUp").field("key", key).finish(),
            Self::SendText { text } => f
                .debug_struct("SendText")
                .field(
                    "text",
                    &format_args!("<redacted, {} chars>", text.chars().count()),
                )
                .finish(),
            Self::Delay { milliseconds } => f
                .debug_struct("Delay")
                .field("milliseconds", milliseconds)
                .finish(),
            Self::MouseClick { button } => f
                .debug_struct("MouseClick")
                .field("button", button)
                .finish(),
            Self::MouseDown { button } => {
                f.debug_struct("MouseDown").field("button", button).finish()
            }
            Self::MouseUp { button } => f.debug_struct("MouseUp").field("button", button).finish(),
            Self::MouseScroll { direction, steps } => f
                .debug_struct("MouseScroll")
                .field("direction", direction)
                .field("steps", steps)
                .finish(),
            Self::MouseMove { dx, dy } => f
                .debug_struct("MouseMove")
                .field("dx", dx)
                .field("dy", dy)
                .finish(),
        }
    }
}
