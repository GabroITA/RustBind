use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Modificatori da tastiera combinabili (Ctrl, Alt, Shift, Win).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
        win: false,
    };

    pub const CTRL: Self = Self {
        ctrl: true,
        alt: false,
        shift: false,
        win: false,
    };

    pub const ALT: Self = Self {
        ctrl: false,
        alt: true,
        shift: false,
        win: false,
    };

    pub const SHIFT: Self = Self {
        ctrl: false,
        alt: false,
        shift: true,
        win: false,
    };

    pub const WIN: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
        win: true,
    };

    pub const fn new(ctrl: bool, alt: bool, shift: bool, win: bool) -> Self {
        Self {
            ctrl,
            alt,
            shift,
            win,
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.ctrl && !self.alt && !self.shift && !self.win
    }

    pub fn any(&self) -> bool {
        !self.is_empty()
    }

    /// Combina due insiemi di modificatori tramite OR logico.
    pub fn combine(self, other: Self) -> Self {
        Self {
            ctrl: self.ctrl || other.ctrl,
            alt: self.alt || other.alt,
            shift: self.shift || other.shift,
            win: self.win || other.win,
        }
    }
}

impl std::ops::BitOr for Modifiers {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.combine(rhs)
    }
}

impl fmt::Display for Modifiers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.win {
            parts.push("Win");
        }
        write!(f, "{}", parts.join("+"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseModifierError(pub String);

impl fmt::Display for ParseModifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Modificatore sconosciuto: '{}'", self.0)
    }
}

impl std::error::Error for ParseModifierError {}

impl FromStr for Modifiers {
    type Err = ParseModifierError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut mods = Self::NONE;
        for part in s.split('+') {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            match trimmed.to_lowercase().as_str() {
                "ctrl" | "control" => mods.ctrl = true,
                "alt" => mods.alt = true,
                "shift" => mods.shift = true,
                "win" | "windows" | "super" => mods.win = true,
                _ => return Err(ParseModifierError(trimmed.to_string())),
            }
        }
        Ok(mods)
    }
}

impl Serialize for Modifiers {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Modifiers {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}
