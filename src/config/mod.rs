pub mod error;
pub mod manager;
pub mod schema;

pub use error::ConfigError;
pub use manager::ConfigManager;
pub use schema::{Config, GeneralSettings, default_kill_switch};

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::core::MAX_ACTIONS_PER_SEQUENCE;
    use crate::core::action::InputAction;
    use crate::core::binding::KeyBinding;
    use crate::core::key::VirtualKey;
    use crate::core::rule::{Rule, Trigger};

    #[test]
    fn test_valid_config_roundtrip() {
        let toml_content = r#"
schema_version = 1

[settings]
kill_switch = "Ctrl+Alt+Shift+F12"
max_events_per_second = 100

[[rules]]
id = "paste_email"
name = "Incolla Email Personale"
enabled = true

[rules.trigger]
type = "hotkey"
binding = "Ctrl+Alt+E"

[[rules.actions]]
type = "send_text"
text = "utente@example.com"

[[rules.actions]]
type = "delay"
milliseconds = 50

[[rules.actions]]
type = "key_click"
key = "Tab"
"#;

        let config = ConfigManager::load_from_str(toml_content).unwrap();
        assert_eq!(config.schema_version, 1);
        assert_eq!(
            config.settings.kill_switch.to_string(),
            "Ctrl+Alt+Shift+F12"
        );
        assert_eq!(config.settings.max_events_per_second, 100);
        assert_eq!(config.rules.len(), 1);

        let rule = &config.rules[0];
        assert_eq!(rule.id, "paste_email");
        assert_eq!(rule.actions.len(), 3);

        // Verifica serializzazione e ricaricamento
        let serialized = ConfigManager::save_to_string(&config).unwrap();
        let reloaded = ConfigManager::load_from_str(&serialized).unwrap();
        assert_eq!(config, reloaded);
    }

    #[test]
    fn test_deny_unknown_fields() {
        let toml_with_unknown_field = r#"
schema_version = 1
some_unsupported_key = "malicious_or_deprecated"

[[rules]]
id = "test"
name = "Test"

[rules.trigger]
type = "hotkey"
binding = "Ctrl+A"

[[rules.actions]]
type = "key_click"
key = "B"
"#;

        let res = ConfigManager::load_from_str(toml_with_unknown_field);
        assert!(
            res.is_err(),
            "I campi sconosciuti devono essere categoricamente respinti all'avvio"
        );
    }

    #[test]
    fn test_unsupported_schema_version() {
        let toml_version_2 = r#"
schema_version = 2

[[rules]]
id = "test"
name = "Test"

[rules.trigger]
type = "hotkey"
binding = "Ctrl+A"

[[rules.actions]]
type = "key_click"
key = "B"
"#;

        let err = ConfigManager::load_from_str(toml_version_2).unwrap_err();
        match err {
            ConfigError::UnsupportedSchemaVersion { expected, found } => {
                assert_eq!(expected, 1);
                assert_eq!(found, 2);
            }
            other => panic!("Atteso UnsupportedSchemaVersion, ricevuto: {:?}", other),
        }

        let toml_zero = toml_version_2.replacen("schema_version = 2", "schema_version = 0", 1);
        assert_eq!(
            ConfigManager::load_from_str(&toml_zero).unwrap_err(),
            ConfigError::UnsupportedSchemaVersion {
                expected: 1,
                found: 0
            }
        );
    }

    #[test]
    fn test_mouse_actions_roundtrip_and_limits() {
        use crate::core::{MouseButton, MouseWheelDirection};

        let toml_v1 = r#"
schema_version = 1

[[rules]]
id = "mouse_macro"
name = "Macro Mouse"
enabled = true

[rules.trigger]
type = "hotkey"
binding = "Ctrl+Alt+M"

[[rules.actions]]
type = "mouse_move"
dx = 120
dy = -40

[[rules.actions]]
type = "mouse_click"
button = "left"

[[rules.actions]]
type = "mouse_down"
button = "x1"

[[rules.actions]]
type = "mouse_up"
button = "x1"

[[rules.actions]]
type = "mouse_scroll"
direction = "horizontal"
steps = -3
"#;

        let config = ConfigManager::load_from_str(toml_v1).unwrap();
        assert_eq!(config.schema_version, 1);
        assert_eq!(config.rules[0].actions.len(), 5);
        assert_eq!(
            config.rules[0].actions[0],
            InputAction::MouseMove { dx: 120, dy: -40 }
        );
        assert_eq!(
            config.rules[0].actions[1],
            InputAction::MouseClick {
                button: MouseButton::Left
            }
        );
        assert_eq!(
            config.rules[0].actions[4],
            InputAction::MouseScroll {
                direction: MouseWheelDirection::Horizontal,
                steps: -3
            }
        );

        let serialized = ConfigManager::save_to_string(&config).unwrap();
        let reloaded = ConfigManager::load_from_str(&serialized).unwrap();
        assert_eq!(config, reloaded);

        let mut bad_scroll = config.clone();
        bad_scroll.rules[0].actions = vec![InputAction::MouseScroll {
            direction: MouseWheelDirection::Vertical,
            steps: 0,
        }];
        assert!(matches!(
            ConfigManager::validate(&bad_scroll),
            Err(ConfigError::InvalidMouseScroll { .. })
        ));

        let mut bad_move = config;
        bad_move.rules[0].actions = vec![InputAction::MouseMove { dx: 0, dy: 0 }];
        assert!(matches!(
            ConfigManager::validate(&bad_move),
            Err(ConfigError::InvalidMouseMove { .. })
        ));
    }

    #[test]
    fn test_duplicate_rule_id_rejected() {
        let toml_duplicate = r#"
schema_version = 1

[[rules]]
id = "duplicate_id"
name = "Rule 1"

[rules.trigger]
type = "hotkey"
binding = "Ctrl+1"

[[rules.actions]]
type = "key_click"
key = "A"

[[rules]]
id = "duplicate_id"
name = "Rule 2"

[rules.trigger]
type = "hotkey"
binding = "Ctrl+2"

[[rules.actions]]
type = "key_click"
key = "B"
"#;

        let err = ConfigManager::load_from_str(toml_duplicate).unwrap_err();
        match err {
            ConfigError::DuplicateRuleId(id) => assert_eq!(id, "duplicate_id"),
            other => panic!("Atteso DuplicateRuleId, ricevuto: {:?}", other),
        }
    }

    #[test]
    fn test_empty_action_sequence_rejected() {
        let toml_empty = r#"
schema_version = 1

[[rules]]
id = "empty_rule"
name = "Empty"
actions = []

[rules.trigger]
type = "hotkey"
binding = "Ctrl+1"
"#;

        let err = ConfigManager::load_from_str(toml_empty).unwrap_err();
        match err {
            ConfigError::EmptyActionSequence(id) => assert_eq!(id, "empty_rule"),
            other => panic!("Atteso EmptyActionSequence, ricevuto: {:?}", other),
        }
    }

    #[test]
    fn test_max_actions_limit() {
        let mut actions = Vec::new();
        for _ in 0..=MAX_ACTIONS_PER_SEQUENCE {
            actions.push(InputAction::KeyClick {
                key: VirtualKey::KeyA,
            });
        }

        let config = Config {
            schema_version: 1,
            settings: GeneralSettings::default(),
            groups: Vec::new(),
            group_filters: Default::default(),
            rules: vec![Rule {
                id: "too_many_actions".to_string(),
                name: "Too Many".to_string(),
                groups: crate::core::rule::default_rule_groups(),
                enabled: true,
                wait_for_modifier_release: false,
                window_filter: None,
                trigger: Trigger::Hotkey {
                    binding: KeyBinding::from_str("Ctrl+1").unwrap(),
                },
                actions,
            }],
        };

        let err = ConfigManager::validate(&config).unwrap_err();
        match err {
            ConfigError::InvalidSequenceLength { count, max, .. } => {
                assert_eq!(count, 257);
                assert_eq!(max, 256);
            }
            other => panic!("Atteso InvalidSequenceLength, ricevuto: {:?}", other),
        }
    }

    #[test]
    fn test_send_text_length_limit() {
        let long_text = "a".repeat(1025);
        let config = Config {
            schema_version: 1,
            settings: GeneralSettings::default(),
            groups: Vec::new(),
            group_filters: Default::default(),
            rules: vec![Rule {
                id: "long_text_rule".to_string(),
                name: "Long Text".to_string(),
                groups: crate::core::rule::default_rule_groups(),
                enabled: true,
                wait_for_modifier_release: false,
                window_filter: None,
                trigger: Trigger::Hotkey {
                    binding: KeyBinding::from_str("Ctrl+1").unwrap(),
                },
                actions: vec![InputAction::SendText { text: long_text }],
            }],
        };

        let err = ConfigManager::validate(&config).unwrap_err();
        match err {
            ConfigError::TextTooLong { len, max, .. } => {
                assert_eq!(len, 1025);
                assert_eq!(max, 1024);
            }
            other => panic!("Atteso TextTooLong, ricevuto: {:?}", other),
        }
    }

    #[test]
    fn test_invalid_delay_limits() {
        // Zero delay non consentito
        let config_zero_delay = Config {
            schema_version: 1,
            settings: GeneralSettings::default(),
            groups: Vec::new(),
            group_filters: Default::default(),
            rules: vec![Rule {
                id: "zero_delay".to_string(),
                name: "Zero Delay".to_string(),
                groups: crate::core::rule::default_rule_groups(),
                enabled: true,
                wait_for_modifier_release: false,
                window_filter: None,
                trigger: Trigger::Hotkey {
                    binding: KeyBinding::from_str("Ctrl+1").unwrap(),
                },
                actions: vec![InputAction::Delay { milliseconds: 0 }],
            }],
        };
        assert!(ConfigManager::validate(&config_zero_delay).is_err());

        // Delay > 60_000 ms (1 minuto) non consentito
        let config_huge_delay = Config {
            schema_version: 1,
            settings: GeneralSettings::default(),
            groups: Vec::new(),
            group_filters: Default::default(),
            rules: vec![Rule {
                id: "huge_delay".to_string(),
                name: "Huge Delay".to_string(),
                groups: crate::core::rule::default_rule_groups(),
                enabled: true,
                wait_for_modifier_release: false,
                window_filter: None,
                trigger: Trigger::Hotkey {
                    binding: KeyBinding::from_str("Ctrl+1").unwrap(),
                },
                actions: vec![InputAction::Delay {
                    milliseconds: 60_001,
                }],
            }],
        };
        assert!(ConfigManager::validate(&config_huge_delay).is_err());
    }

    #[test]
    fn test_invalid_rate_limit() {
        let config_zero_rate = Config {
            schema_version: 1,
            settings: GeneralSettings {
                kill_switch: default_kill_switch(),
                resume_hotkey: None,
                max_events_per_second: 0,
                active_groups: Vec::new(),
            },
            groups: Vec::new(),
            group_filters: Default::default(),
            rules: Vec::new(),
        };
        assert!(ConfigManager::validate(&config_zero_rate).is_err());

        let config_excess_rate = Config {
            schema_version: 1,
            settings: GeneralSettings {
                kill_switch: default_kill_switch(),
                resume_hotkey: None,
                max_events_per_second: 101,
                active_groups: Vec::new(),
            },
            groups: Vec::new(),
            group_filters: Default::default(),
            rules: Vec::new(),
        };
        assert!(ConfigManager::validate(&config_excess_rate).is_err());
    }

    #[test]
    fn test_backup_corrupted_file() {
        let temp_dir = std::env::temp_dir().join("rustbind_test_backup");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let file_path = temp_dir.join("test_config.toml");
        std::fs::write(&file_path, "corrupted { invalid toml").unwrap();

        // 1. Il caricamento deve fallire con errore tipizzato (fail-closed)
        assert!(ConfigManager::load_from_file(&file_path).is_err());
        // 2. Il file originale NON deve essere toccato o rimosso
        assert!(
            file_path.exists(),
            "Il file originale non deve essere spostato né rimosso"
        );

        // 3. Creazione esplicita della copia di backup
        let backup_path = ConfigManager::backup_corrupted_file(&file_path).unwrap();
        assert!(
            file_path.exists(),
            "Il file originale deve rimanere integro"
        );
        assert!(
            backup_path.exists(),
            "La copia di backup deve essere stata creata"
        );
        assert!(
            backup_path
                .to_string_lossy()
                .contains("test_config.toml.bak."),
            "Il nome di backup deve contenere .bak.<timestamp>"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_privacy_unknown_field_does_not_leak_values_or_lines() {
        let toml_unknown = "schema_version = 1\nunknown_secret_field = \"SEGRETO123\"\n";
        let err = ConfigManager::load_from_str(toml_unknown).unwrap_err();
        let display = format!("{err}");
        let debug = format!("{err:?}");

        assert!(
            !display.contains("SEGRETO123"),
            "Display non deve contenere il valore del campo segreto: {display}"
        );
        assert!(
            !debug.contains("SEGRETO123"),
            "Debug non deve contenere il valore del campo segreto: {debug}"
        );
        assert!(
            !display.contains("unknown_secret_field"),
            "Display non deve esporre il nome del campo sconosciuto: {display}"
        );
        assert!(
            !display.contains(" |"),
            "Display non deve contenere estratti del file sorgente: {display}"
        );
        assert!(
            display.contains("campo sconosciuto alla riga 2 colonna 1"),
            "Display deve contenere indicazioni generiche di riga e colonna: {display}"
        );
    }

    #[test]
    fn test_privacy_syntax_error_does_not_leak_file_snippets() {
        let toml_syntax = "schema_version = 1\n[settings]\nsecret_token = \"SEGRETO123\n";
        let err = ConfigManager::load_from_str(toml_syntax).unwrap_err();
        let display = format!("{err}");
        let debug = format!("{err:?}");

        assert!(
            !display.contains("SEGRETO123"),
            "Display non deve contenere la stringa sentinella: {display}"
        );
        assert!(
            !debug.contains("SEGRETO123"),
            "Debug non deve contenere la stringa sentinella: {debug}"
        );
        assert!(
            !display.contains(" |"),
            "Display non deve contenere estratti del sorgente: {display}"
        );
        assert!(
            display.contains("errore di sintassi alla riga 3"),
            "Display deve contenere la segnalazione generica di sintassi con riga: {display}"
        );
    }

    #[test]
    fn test_privacy_out_of_bounds_value_does_not_leak_user_values() {
        // Rate limit con valore anomalo 9999
        let config_rate = Config {
            schema_version: 1,
            settings: GeneralSettings {
                kill_switch: default_kill_switch(),
                resume_hotkey: None,
                max_events_per_second: 9999,
                active_groups: Vec::new(),
            },
            groups: Vec::new(),
            group_filters: Default::default(),
            rules: Vec::new(),
        };
        let err_rate = ConfigManager::validate(&config_rate).unwrap_err();
        let display_rate = format!("{err_rate}");
        let debug_rate = format!("{err_rate:?}");

        assert!(
            !display_rate.contains("9999"),
            "Display non deve mostrare il valore utente inserito fuori limite: {display_rate}"
        );
        assert!(
            !debug_rate.contains("9999"),
            "Debug non deve mostrare il valore utente inserito fuori limite: {debug_rate}"
        );
        assert!(
            display_rate.contains("fuori dai limiti consentiti"),
            "Display deve usare messaggio generico fuori limiti: {display_rate}"
        );

        // Delay anomalo 999999
        let config_delay = Config {
            schema_version: 1,
            settings: GeneralSettings::default(),
            groups: Vec::new(),
            group_filters: Default::default(),
            rules: vec![Rule {
                id: "test_rule".to_string(),
                name: "Test".to_string(),
                groups: crate::core::rule::default_rule_groups(),
                enabled: true,
                wait_for_modifier_release: false,
                window_filter: None,
                trigger: Trigger::Hotkey {
                    binding: KeyBinding::from_str("Ctrl+1").unwrap(),
                },
                actions: vec![InputAction::Delay {
                    milliseconds: 999_999,
                }],
            }],
        };
        let err_delay = ConfigManager::validate(&config_delay).unwrap_err();
        let display_delay = format!("{err_delay}");
        let debug_delay = format!("{err_delay:?}");

        assert!(
            !display_delay.contains("999999"),
            "Display non deve mostrare il valore del ritardo: {display_delay}"
        );
        assert!(
            !debug_delay.contains("999999"),
            "Debug non deve mostrare il valore del ritardo: {debug_delay}"
        );
        assert!(
            display_delay.contains("fuori dai limiti consentiti"),
            "Display deve usare messaggio generico per il ritardo: {display_delay}"
        );
    }

    #[test]
    fn test_privacy_send_text_too_long_does_not_leak_payload() {
        let secret_payload = format!("SEGRETO123_{}", "A".repeat(1025));
        let config = Config {
            schema_version: 1,
            settings: GeneralSettings::default(),
            groups: Vec::new(),
            group_filters: Default::default(),
            rules: vec![Rule {
                id: "send_text_rule".to_string(),
                name: "Test".to_string(),
                groups: crate::core::rule::default_rule_groups(),
                enabled: true,
                wait_for_modifier_release: false,
                window_filter: None,
                trigger: Trigger::Hotkey {
                    binding: KeyBinding::from_str("Ctrl+1").unwrap(),
                },
                actions: vec![InputAction::SendText {
                    text: secret_payload,
                }],
            }],
        };

        let err = ConfigManager::validate(&config).unwrap_err();
        let display = format!("{err}");
        let debug = format!("{err:?}");

        assert!(
            !display.contains("SEGRETO123"),
            "Display non deve mai mostrare il testo dell'azione SendText: {display}"
        );
        assert!(
            !debug.contains("SEGRETO123"),
            "Debug non deve mai mostrare il testo dell'azione SendText: {debug}"
        );
        assert!(
            display.contains("supera la lunghezza massima"),
            "Display deve contenere il messaggio standard: {display}"
        );
    }

    #[test]
    fn test_privacy_state_error_and_tooltip() {
        let toml_unknown = "schema_version = 1\nsecret_token = \"SEGRETO123\"\n";
        let err = ConfigManager::load_from_str(toml_unknown).unwrap_err();

        // Messaggio di log / stato daemon
        let daemon_log_msg = format!("Configurazione non valida: {err}");
        assert!(
            !daemon_log_msg.contains("SEGRETO123"),
            "Il messaggio di log del daemon non deve contenere il segreto"
        );

        // Tooltip della tray icon
        let state = crate::engine::state::ServiceState::Error(daemon_log_msg);
        let tooltip = crate::platform::windows::tray::state_to_tooltip(&state);
        assert_eq!(tooltip, "RustBind - Error (No rules running)");
        assert!(!tooltip.contains("SEGRETO123"));
    }

    #[test]
    fn test_resolve_config_path() {
        let base_dir = std::path::Path::new(r"C:\Users\TestUser\AppData\Roaming");
        let resolved = ConfigManager::resolve_config_path(base_dir);
        assert_eq!(
            resolved,
            std::path::PathBuf::from(
                r"C:\Users\TestUser\AppData\Roaming\RustBind\configs\config.toml"
            )
        );
        assert_eq!(
            ConfigManager::resolve_active_config_path(base_dir),
            resolved
        );
    }

    fn make_sample_valid_config(max_events: u32) -> schema::Config {
        use std::str::FromStr;
        schema::Config {
            schema_version: 1,
            settings: schema::GeneralSettings {
                kill_switch: schema::default_kill_switch(),
                resume_hotkey: None,
                max_events_per_second: max_events,
                active_groups: Vec::new(),
            },
            groups: Vec::new(),
            group_filters: Default::default(),
            rules: vec![Rule {
                id: "sample".to_string(),
                name: "Sample Rule".to_string(),
                groups: crate::core::rule::default_rule_groups(),
                enabled: true,
                wait_for_modifier_release: false,
                window_filter: None,
                trigger: Trigger::Hotkey {
                    binding: KeyBinding::from_str("Ctrl+Alt+S").unwrap(),
                },
                actions: vec![InputAction::KeyClick {
                    key: VirtualKey::KeyA,
                }],
            }],
        }
    }

    #[test]
    fn test_save_to_file_safe_absent_file_creates_atomically() {
        let temp_dir =
            std::env::temp_dir().join(format!("rustbind_test_safe_absent_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let file_path = temp_dir.join("nested").join("config.toml");

        let cfg = make_sample_valid_config(80);
        let backup = ConfigManager::save_to_file_safe(&cfg, &file_path, false).unwrap();
        assert!(backup.is_none());
        assert!(file_path.exists());

        let reloaded = ConfigManager::load_from_file(&file_path).unwrap();
        assert_eq!(reloaded, cfg);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_to_file_safe_updates_existing_valid_file_without_backup() {
        let temp_dir =
            std::env::temp_dir().join(format!("rustbind_test_safe_update_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let file_path = temp_dir.join("config.toml");

        let initial = make_sample_valid_config(50);
        ConfigManager::save_to_file(&initial, &file_path).unwrap();

        let updated = make_sample_valid_config(90);
        let backup = ConfigManager::save_to_file_safe(&updated, &file_path, false).unwrap();
        assert!(backup.is_none());

        let reloaded = ConfigManager::load_from_file(&file_path).unwrap();
        assert_eq!(reloaded.settings.max_events_per_second, 90);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_to_file_safe_refuses_corrupted_file_when_flag_false() {
        let temp_dir =
            std::env::temp_dir().join(format!("rustbind_test_safe_refuse_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let file_path = temp_dir.join("config.toml");

        let original_corrupted = "schema_version = 1\nunknown_field = true\n";
        std::fs::write(&file_path, original_corrupted).unwrap();

        let cfg = make_sample_valid_config(60);
        let res = ConfigManager::save_to_file_safe(&cfg, &file_path, false);
        assert_eq!(res, Err(ConfigError::RefusedOverwriteInvalidFile));

        // Il contenuto originale non deve essere stato alterato
        let disk_content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(disk_content, original_corrupted);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_to_file_safe_backups_and_overwrites_corrupted_when_flag_true() {
        let temp_dir = std::env::temp_dir().join(format!(
            "rustbind_test_safe_backup_overwrite_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let file_path = temp_dir.join("config.toml");

        let original_corrupted = "corrupted { invalid_toml = ";
        std::fs::write(&file_path, original_corrupted).unwrap();

        let cfg = make_sample_valid_config(75);
        let backup_opt = ConfigManager::save_to_file_safe(&cfg, &file_path, true).unwrap();
        let backup_path = backup_opt.expect("Deve aver creato il file di backup");

        assert!(backup_path.exists());
        assert!(
            backup_path.to_string_lossy().contains("config.toml.bak."),
            "Il nome backup deve seguire il formato config.toml.bak.<timestamp>"
        );
        assert_eq!(
            std::fs::read_to_string(&backup_path).unwrap(),
            original_corrupted
        );

        let reloaded = ConfigManager::load_from_file(&file_path).unwrap();
        assert_eq!(reloaded, cfg);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_rule_group_and_wait_for_modifier_release_defaults_and_validation() {
        let toml_omitted = r#"
schema_version = 1

[settings]
kill_switch = "Ctrl+Alt+Shift+F12"
max_events_per_second = 100

[[rules]]
id = "r1"
name = "Regola 1"

[rules.trigger]
type = "hotkey"
binding = "Ctrl+1"

[[rules.actions]]
type = "delay"
milliseconds = 50
"#;
        let parsed_omitted = ConfigManager::load_from_str(toml_omitted).unwrap();
        assert!(parsed_omitted.settings.active_groups.is_empty());
        assert_eq!(parsed_omitted.rules[0].groups, vec!["default".to_string()]);
        assert!(!parsed_omitted.rules[0].wait_for_modifier_release);

        let toml_explicit = r#"
schema_version = 1

[settings]
kill_switch = "Ctrl+Alt+Shift+F12"
max_events_per_second = 100
active_groups = ["lavoro_1", "gaming-2"]

[[rules]]
id = "r1"
name = "Regola 1"
groups = ["lavoro_1"]
wait_for_modifier_release = true

[rules.trigger]
type = "hotkey"
binding = "Ctrl+1"

[[rules.actions]]
type = "delay"
milliseconds = 50
"#;
        let parsed_explicit = ConfigManager::load_from_str(toml_explicit).unwrap();
        assert_eq!(
            parsed_explicit.settings.active_groups,
            vec!["lavoro_1".to_string(), "gaming-2".to_string()]
        );
        assert_eq!(
            parsed_explicit.rules[0].groups,
            vec!["lavoro_1".to_string()]
        );
        assert!(parsed_explicit.rules[0].wait_for_modifier_release);

        // Gruppo vuoto o con caratteri non ammessi viene rifiutato senza esporre il valore utente
        for bad_group in ["", "   ", "SEGRETO gruppo!", "gruppo/1", "città"] {
            let mut bad_cfg = parsed_explicit.clone();
            bad_cfg.rules[0].groups = vec![bad_group.to_string()];
            let err = ConfigManager::validate(&bad_cfg).unwrap_err();
            assert_eq!(err, ConfigError::InvalidRuleGroup);
            assert!(!format!("{err}").contains("SEGRETO"));
            assert!(!format!("{err:?}").contains("SEGRETO"));
        }
    }

    #[test]
    fn test_unique_rule_name_and_resume_hotkey_validation() {
        let mut cfg = make_sample_valid_config(100);
        // Nome vuoto -> rifiutato
        cfg.rules[0].name = "   ".to_string();
        assert_eq!(
            ConfigManager::validate(&cfg),
            Err(ConfigError::EmptyRuleName)
        );

        // Nomi duplicati (case-insensitive) -> rifiutato in modo privacy-safe
        let mut dup_cfg = make_sample_valid_config(100);
        let mut second = dup_cfg.rules[0].clone();
        second.id = "sample_2".to_string();
        second.name = "sample rule".to_string();
        dup_cfg.rules.push(second);
        assert_eq!(
            ConfigManager::validate(&dup_cfg),
            Err(ConfigError::DuplicateRuleName)
        );

        // resume_hotkey == kill_switch -> rifiutato
        let mut ks_conflict = make_sample_valid_config(100);
        ks_conflict.settings.resume_hotkey = Some(ks_conflict.settings.kill_switch);
        assert_eq!(
            ConfigManager::validate(&ks_conflict),
            Err(ConfigError::ResumeHotkeyConflictsWithKillSwitch)
        );

        // resume_hotkey distinta dal kill-switch -> valida
        ks_conflict.settings.resume_hotkey =
            Some(KeyBinding::from_str("Ctrl+Alt+Shift+F11").unwrap());
        assert!(ConfigManager::validate(&ks_conflict).is_ok());

        // Multi-gruppo e Config.groups persistiti e validati
        ks_conflict.groups = vec!["lavoro".to_string(), "gaming".to_string()];
        ks_conflict.rules[0].set_groups(vec!["lavoro".to_string(), "gaming".to_string()]);
        let toml_out = ConfigManager::save_to_string(&ks_conflict).unwrap();
        let reloaded = ConfigManager::load_from_str(&toml_out).unwrap();
        assert_eq!(
            reloaded.rules[0].effective_groups(),
            vec!["lavoro".to_string(), "gaming".to_string()]
        );
        assert_eq!(
            reloaded.groups,
            vec!["lavoro".to_string(), "gaming".to_string()]
        );

        // Regola priva di gruppi: abilitata -> rifiutata; disattiva -> valida
        ks_conflict.rules[0].set_groups(Vec::new());
        ks_conflict.rules[0].enabled = true;
        assert_eq!(
            ConfigManager::validate(&ks_conflict),
            Err(ConfigError::EnabledRuleWithoutGroup)
        );
        ks_conflict.rules[0].enabled = false;
        assert!(ConfigManager::validate(&ks_conflict).is_ok());

        // Validazione WindowFilter contestuale + privacy messaggi di errore
        ks_conflict.rules[0].set_groups(vec!["default".to_string()]);
        ks_conflict.rules[0].enabled = true;
        ks_conflict.rules[0].window_filter = Some(crate::core::rule::WindowFilter {
            mode: crate::core::rule::WindowFilterMode::Include,
            process_name: None,
        });
        assert_eq!(
            ConfigManager::validate(&ks_conflict),
            Err(ConfigError::EmptyWindowFilter)
        );

        ks_conflict.rules[0].window_filter = Some(crate::core::rule::WindowFilter {
            mode: crate::core::rule::WindowFilterMode::Exclude,
            process_name: Some(r"C:\SecretPath\app.exe".to_string()),
        });
        let proc_err = ConfigManager::validate(&ks_conflict).unwrap_err();
        assert_eq!(proc_err, ConfigError::InvalidProcessNameFilter);
        assert!(!format!("{proc_err}").contains("SecretPath"));

        ks_conflict.rules[0].window_filter = Some(crate::core::rule::WindowFilter {
            mode: crate::core::rule::WindowFilterMode::Include,
            process_name: Some("code.exe, chrome.exe".to_string()),
        });
        let toml_ctx = ConfigManager::save_to_string(&ks_conflict).unwrap();
        let reloaded_ctx = ConfigManager::load_from_str(&toml_ctx).unwrap();
        assert_eq!(
            reloaded_ctx.rules[0].window_filter,
            ks_conflict.rules[0].window_filter
        );

        // Normalizzazione nome profilo e export atomico
        assert_eq!(
            ConfigManager::sanitize_profile_filename("../gaming profile.toml").unwrap(),
            "gaming_profile.toml"
        );
    }

    #[test]
    fn test_group_window_filters_validation_and_combination_algebra() {
        use crate::core::rule::{ForegroundWindow, WindowFilter, WindowFilterMode};

        let mut cfg = make_sample_valid_config(50);
        cfg.groups.push("coding".to_string());
        cfg.groups.push("browsers".to_string());
        cfg.group_filters.insert(
            "coding".to_string(),
            WindowFilter {
                mode: WindowFilterMode::Include,
                process_name: Some("code.exe, idea64.exe".to_string()),
            },
        );
        cfg.group_filters.insert(
            "browsers".to_string(),
            WindowFilter {
                mode: WindowFilterMode::Include,
                process_name: Some("chrome.exe, firefox.exe".to_string()),
            },
        );
        cfg.rules[0].set_groups(vec!["coding".to_string()]);
        cfg.rules[0].window_filter = None;

        // Roundtrip TOML + validazione
        let serialized = ConfigManager::save_to_string(&cfg).unwrap();
        let reloaded = ConfigManager::load_from_str(&serialized).unwrap();
        assert_eq!(reloaded.group_filters, cfg.group_filters);

        // Regola senza filtro eredita dal gruppo
        let code_fg = ForegroundWindow {
            process_name: Some("code.exe".to_string()),
        };
        let chrome_fg = ForegroundWindow {
            process_name: Some("chrome.exe".to_string()),
        };
        assert!(reloaded.is_rule_contextual(&reloaded.rules[0]));
        assert!(reloaded.rule_matches_context(&reloaded.rules[0], &code_fg));
        assert!(!reloaded.rule_matches_context(&reloaded.rules[0], &chrome_fg));

        // Regola in due gruppi -> OR tra gruppi + AND con eventuale filtro della regola
        let mut multi_rule = reloaded.rules[0].clone();
        multi_rule.set_groups(vec!["coding".to_string(), "browsers".to_string()]);
        multi_rule.window_filter = Some(WindowFilter {
            mode: WindowFilterMode::Exclude,
            process_name: Some("chrome.exe".to_string()),
        });
        assert!(reloaded.rule_matches_context(&multi_rule, &code_fg));
        assert!(!reloaded.rule_matches_context(&multi_rule, &chrome_fg));
        assert!(reloaded.rule_can_ever_match_context(&multi_rule));

        // Regola con filtro incompatibile con il proprio gruppo -> intersezione vuota
        let mut impossible_rule = reloaded.rules[0].clone();
        impossible_rule.set_groups(vec!["coding".to_string()]);
        impossible_rule.window_filter = Some(WindowFilter {
            mode: WindowFilterMode::Include,
            process_name: Some("chrome.exe".to_string()),
        });
        assert!(!reloaded.rule_can_ever_match_context(&impossible_rule));

        // Il gruppo "default" non accetta filtri contestuali
        let mut invalid_default = reloaded.clone();
        invalid_default.group_filters.insert(
            "default".to_string(),
            WindowFilter {
                mode: WindowFilterMode::Include,
                process_name: Some("notepad.exe".to_string()),
            },
        );
        assert_eq!(
            ConfigManager::validate(&invalid_default),
            Err(ConfigError::InvalidRuleGroup)
        );
    }
}
