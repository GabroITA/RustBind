use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::core::key::VirtualKey;
use crate::core::modifiers::Modifiers;

/// Rappresenta una combinazione di tasti completa (modificatori + tasto principale).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyBinding {
    pub modifiers: Modifiers,
    pub key: VirtualKey,
}

impl KeyBinding {
    pub const fn new(modifiers: Modifiers, key: VirtualKey) -> Self {
        Self { modifiers, key }
    }
}

impl fmt::Display for KeyBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers.is_empty() {
            write!(f, "{}", self.key)
        } else {
            write!(f, "{}+{}", self.modifiers, self.key)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseKeyBindingError(pub String);

impl fmt::Display for ParseKeyBindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Combinazione di tasti non valida: '{}'", self.0)
    }
}

impl std::error::Error for ParseKeyBindingError {}

impl FromStr for KeyBinding {
    type Err = ParseKeyBindingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(ParseKeyBindingError("Stringa vuota".to_string()));
        }

        let parts: Vec<&str> = trimmed.split('+').map(str::trim).collect();
        if parts.is_empty() {
            return Err(ParseKeyBindingError(trimmed.to_string()));
        }

        let mut modifiers = Modifiers::NONE;
        let mut key = None;

        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }
            let lower = part.to_lowercase();
            match lower.as_str() {
                "ctrl" | "control" => modifiers.ctrl = true,
                "alt" => modifiers.alt = true,
                "shift" => modifiers.shift = true,
                "win" | "windows" | "super" => modifiers.win = true,
                _ => {
                    // Non è un modificatore: deve essere il tasto principale
                    if i != parts.len() - 1 && parts.len() > 1 {
                        // I modificatori devono precedere il tasto principale
                        return Err(ParseKeyBindingError(format!(
                            "Il modificatore o tasto non valido '{}' precede il tasto finale",
                            part
                        )));
                    }
                    if key.is_some() {
                        return Err(ParseKeyBindingError(format!(
                            "Rilevati tasti multipli non modificatori in: '{}'",
                            trimmed
                        )));
                    }
                    let parsed_key = VirtualKey::from_str(part)
                        .map_err(|e| ParseKeyBindingError(e.to_string()))?;
                    key = Some(parsed_key);
                }
            }
        }

        let final_key = key.ok_or_else(|| {
            ParseKeyBindingError(format!(
                "Nessun tasto principale valido specificato nella combinazione '{}'",
                trimmed
            ))
        })?;

        Ok(Self {
            modifiers,
            key: final_key,
        })
    }
}

impl Serialize for KeyBinding {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for KeyBinding {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}
