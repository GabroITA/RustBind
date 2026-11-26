pub mod action;
pub mod binding;
pub mod dispatcher;
pub mod key;
pub mod modifiers;
pub mod rule;

pub use action::{InputAction, MouseButton, MouseWheelDirection};
pub use binding::{KeyBinding, ParseKeyBindingError};
pub use dispatcher::ActionDispatcher;
pub use key::{ParseKeyError, VirtualKey};
pub use modifiers::{Modifiers, ParseModifierError};
pub use rule::{
    ForegroundWindow, MAX_WINDOW_FILTER_LEN, Rule, Trigger, WindowFilter, WindowFilterMode,
    default_rule_group, default_rule_groups,
};

// Costanti di sicurezza e limiti di sistema validati rigorosamente
pub const CURRENT_SCHEMA_VERSION: u32 = 1;
pub const MAX_ACTIONS_PER_SEQUENCE: usize = 256;
pub const MAX_SEND_TEXT_LENGTH: usize = 1024;
pub const MIN_DELAY_MS: u64 = 1;
pub const MAX_DELAY_MS: u64 = 60_000;
pub const MIN_EVENTS_PER_SECOND: u32 = 1;
pub const MAX_EVENTS_PER_SECOND: u32 = 100;
pub const MAX_MOUSE_SCROLL_STEPS: i32 = 50;
pub const MAX_MOUSE_MOVE_DELTA: i32 = 10_000;

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_virtual_key_parsing_and_display() {
        assert_eq!(VirtualKey::from_str("f12").unwrap(), VirtualKey::F12);
        assert_eq!(VirtualKey::from_str("F12").unwrap(), VirtualKey::F12);
        assert_eq!(VirtualKey::from_str("enter").unwrap(), VirtualKey::Enter);
        assert_eq!(VirtualKey::from_str("space").unwrap(), VirtualKey::Space);
        assert_eq!(VirtualKey::from_str("a").unwrap(), VirtualKey::KeyA);
        assert_eq!(VirtualKey::from_str("z").unwrap(), VirtualKey::KeyZ);
        assert_eq!(VirtualKey::from_str("0").unwrap(), VirtualKey::Num0);
        assert_eq!(VirtualKey::from_str("9").unwrap(), VirtualKey::Num9);
        assert_eq!(VirtualKey::F12.to_string(), "F12");
        assert_eq!(VirtualKey::Enter.to_string(), "Enter");

        assert!(VirtualKey::from_str("InvalidKeyName").is_err());
    }

    #[test]
    fn test_modifiers_combination_and_parsing() {
        let m = Modifiers::from_str("ctrl+alt").unwrap();
        assert!(m.ctrl);
        assert!(m.alt);
        assert!(!m.shift);
        assert!(!m.win);

        let m2 = Modifiers::from_str("Ctrl+Shift+Win").unwrap();
        assert!(m2.ctrl);
        assert!(!m2.alt);
        assert!(m2.shift);
        assert!(m2.win);

        let combined = m | m2;
        assert!(combined.ctrl);
        assert!(combined.alt);
        assert!(combined.shift);
        assert!(combined.win);

        assert!(Modifiers::from_str("ctrl+invalidmod").is_err());
    }

    #[test]
    fn test_key_binding_parsing() {
        let binding = KeyBinding::from_str("Ctrl+Alt+Shift+F12").unwrap();
        assert!(binding.modifiers.ctrl);
        assert!(binding.modifiers.alt);
        assert!(binding.modifiers.shift);
        assert!(!binding.modifiers.win);
        assert_eq!(binding.key, VirtualKey::F12);
        assert_eq!(binding.to_string(), "Ctrl+Alt+Shift+F12");

        let single = KeyBinding::from_str("Escape").unwrap();
        assert!(single.modifiers.is_empty());
        assert_eq!(single.key, VirtualKey::Escape);
        assert_eq!(single.to_string(), "Escape");

        // Error cases
        assert!(KeyBinding::from_str("").is_err());
        assert!(KeyBinding::from_str("Ctrl+").is_err());
        assert!(KeyBinding::from_str("Ctrl+Alt").is_err()); // Manca il tasto principale
        assert!(KeyBinding::from_str("F12+Ctrl").is_err()); // Modificatore dopo il tasto finale
    }

    #[test]
    fn test_send_text_debug_redaction() {
        let action = InputAction::SendText {
            text: "secret_password_123".to_string(),
        };
        let debug_str = format!("{:?}", action);
        assert!(
            !debug_str.contains("secret_password_123"),
            "Il payload sensibile non deve MAI comparire nell'output Debug"
        );
        assert!(
            debug_str.contains("<redacted, 19 chars>"),
            "L'output di debug deve indicare il numero di caratteri oscurati: {}",
            debug_str
        );
    }

    struct MockDispatcher {
        dispatched: Vec<InputAction>,
    }

    impl ActionDispatcher for MockDispatcher {
        type Error = std::convert::Infallible;

        fn dispatch(&mut self, action: &InputAction) -> Result<(), Self::Error> {
            self.dispatched.push(action.clone());
            Ok(())
        }
    }

    #[test]
    fn test_action_dispatcher_mock() {
        let mut dispatcher = MockDispatcher {
            dispatched: Vec::new(),
        };
        let action1 = InputAction::KeyClick {
            key: VirtualKey::KeyA,
        };
        let action2 = InputAction::Delay { milliseconds: 50 };

        dispatcher.dispatch(&action1).unwrap();
        dispatcher.dispatch(&action2).unwrap();

        assert_eq!(dispatcher.dispatched.len(), 2);
        assert_eq!(dispatcher.dispatched[0], action1);
        assert_eq!(dispatcher.dispatched[1], action2);
    }
}
