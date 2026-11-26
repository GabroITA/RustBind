pub mod circuit_breaker;
pub mod clock;
pub mod executor;
pub mod runner;
pub mod state;

pub use circuit_breaker::{RateLimiter, count_action_events};
pub use clock::{Clock, MockClock, SystemClock};
pub use executor::{ExecutionRequest, SequenceExecutor, execute_sequence};
pub use runner::{EngineRunner, KILL_SWITCH_HOTKEY_ID, RESUME_HOTKEY_ID};
pub use state::ServiceState;

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::time::Duration;

    use super::*;
    use crate::config::error::ConfigError;
    use crate::config::schema::{Config, GeneralSettings};
    use crate::core::action::InputAction;
    use crate::core::binding::KeyBinding;
    use crate::core::key::VirtualKey;
    use crate::core::rule::{Rule, Trigger, default_rule_groups};
    use crate::platform::traits::{HotkeyId, MockPlatform};

    fn make_test_config() -> Config {
        Config {
            schema_version: 1,
            settings: GeneralSettings::default(),
            groups: Vec::new(),
            group_filters: Default::default(),
            rules: vec![
                Rule {
                    id: "rule_1".to_string(),
                    name: "Rule One".to_string(),
                    groups: default_rule_groups(),
                    enabled: true,
                    wait_for_modifier_release: false,
                    window_filter: None,
                    trigger: Trigger::Hotkey {
                        binding: KeyBinding::from_str("Ctrl+1").unwrap(),
                    },
                    actions: vec![InputAction::KeyClick {
                        key: VirtualKey::KeyA,
                    }],
                },
                Rule {
                    id: "rule_2".to_string(),
                    name: "Rule Two".to_string(),
                    groups: default_rule_groups(),
                    enabled: true,
                    wait_for_modifier_release: false,
                    window_filter: None,
                    trigger: Trigger::Hotkey {
                        binding: KeyBinding::from_str("Ctrl+2").unwrap(),
                    },
                    actions: vec![InputAction::KeyClick {
                        key: VirtualKey::KeyB,
                    }],
                },
            ],
        }
    }

    #[test]
    fn test_startup_with_absent_config_starts_active_with_zero_rules() {
        let platform = MockPlatform::new();
        let clock = Arc::new(MockClock::new());

        // File assente: il daemon genera una configurazione vuota con 0 regole
        // senza scrivere nulla su disco.
        let empty_config = Config::default();

        let runner =
            EngineRunner::new(platform.clone(), platform.clone(), clock, &Ok(empty_config));

        // Lo stato deve essere Attivo, 0 regole utente attive, ma il kill-switch deve essere registrato
        assert_eq!(runner.state(), ServiceState::Active);
        assert_eq!(runner.active_rules_count(), 0);
        let registered = platform
            .registered_hotkeys
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        // Solo il kill-switch registrato (ID 1)
        assert_eq!(registered.len(), 1);
        assert!(registered.contains_key(&KILL_SWITCH_HOTKEY_ID));
    }

    #[test]
    fn test_startup_with_invalid_config_fail_closed() {
        let platform = MockPlatform::new();
        let clock = Arc::new(MockClock::new());
        let invalid_config_result: Result<Config, ConfigError> =
            Err(ConfigError::DuplicateRuleId("test_dup".to_string()));

        let runner = EngineRunner::new(
            platform.clone(),
            platform.clone(),
            clock,
            &invalid_config_result,
        );

        // FAIL-CLOSED: Lo stato deve essere Error e nessuna hotkey deve essere registrata
        assert!(runner.state().is_error());
        assert_eq!(runner.active_rules_count(), 0);
        let registered = platform
            .registered_hotkeys
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        assert!(
            registered.is_empty(),
            "Nessuna hotkey deve essere registrata se la configurazione è non valida"
        );
    }

    #[test]
    fn test_startup_with_busy_kill_switch_blocks_all_rules() {
        let platform = MockPlatform::new();
        let clock = Arc::new(MockClock::new());
        let config = make_test_config();

        // Simula il kill-switch già occupato da un'altra applicazione
        platform.set_conflicting_binding(config.settings.kill_switch);

        let runner = EngineRunner::new(platform.clone(), platform.clone(), clock, &Ok(config));

        // Requisito di sicurezza fail-safe: se il kill-switch non è registrabile,
        // stato Error e NESSUNA regola configurata viene registrata
        assert!(runner.state().is_error());
        assert_eq!(runner.active_rules_count(), 0);

        let registered = platform
            .registered_hotkeys
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        assert!(
            registered.is_empty(),
            "Nessuna regola deve essere registrata se il kill-switch fallisce all'avvio"
        );
    }

    #[test]
    fn test_ipc_suspend_when_kill_switch_busy_preserves_error_state() {
        use crate::platform::windows::ipc_server::{
            IPC_SUSPEND_REJECTED_ERROR_STATE, apply_ipc_suspend,
        };

        let platform = MockPlatform::new();
        let clock = Arc::new(MockClock::new());
        let config = make_test_config();

        // 1. Simula kill-switch occupato da un'altra applicazione
        platform.set_conflicting_binding(config.settings.kill_switch);

        let mut runner = EngineRunner::new(platform.clone(), platform.clone(), clock, &Ok(config));

        // Verifica stato iniziale di errore fail-closed
        assert!(runner.state().is_error());
        assert_eq!(runner.active_rules_count(), 0);

        // 2. Arriva una richiesta IPC di Suspend
        let (next_state, outcome) = apply_ipc_suspend(&runner.state());

        // L'esito deve essere 0x02 (rifiutato perché in stato Errore)
        assert_eq!(outcome, IPC_SUSPEND_REJECTED_ERROR_STATE);
        assert!(next_state.is_error());

        // 3. Il gestore in main.rs ri-verifica lo stato: non invoca pause()
        if outcome == 0x01 && runner.state().is_active() {
            runner.pause();
        }

        // Lo stato deve rimanere rigidamente Errore e nessuna regola deve essere attiva
        assert!(
            runner.state().is_error(),
            "Lo stato deve rimanere Errore dopo tentativo di Suspend IPC"
        );
        assert_eq!(runner.active_rules_count(), 0);
    }

    #[test]
    fn test_kill_switch_sequence_cancel_flag_paused_tray_update() {
        use crate::platform::windows::tray::state_to_tooltip;

        let platform = MockPlatform::new();
        let clock = Arc::new(SystemClock::new());

        let mut config = make_test_config();
        config.rules = vec![Rule {
            id: "slow_action".to_string(),
            name: "Slow Action".to_string(),
            groups: default_rule_groups(),
            enabled: true,
            wait_for_modifier_release: false,
            window_filter: None,
            trigger: Trigger::Hotkey {
                binding: KeyBinding::from_str("Ctrl+1").unwrap(),
            },
            actions: vec![
                InputAction::KeyDown {
                    key: VirtualKey::KeyB,
                },
                InputAction::Delay {
                    milliseconds: 3_000,
                },
                InputAction::KeyUp {
                    key: VirtualKey::KeyB,
                },
            ],
        }];

        let mut runner = EngineRunner::new(platform.clone(), platform.clone(), clock, &Ok(config));
        assert_eq!(runner.state(), ServiceState::Active);
        assert_eq!(state_to_tooltip(&runner.state()), "RustBind - Active");

        // 1. Innesca l'hotkey della regola (ID 3, dopo kill-switch ID 1 e resume riservato ID 2)
        runner.handle_hotkey(HotkeyId(3));
        thread::sleep(Duration::from_millis(30));
        assert!(runner.is_running());
        assert!(!runner.cancel_flag().load(Ordering::SeqCst));

        // 2. Preme il Kill-Switch (ID 1)
        runner.handle_hotkey(KILL_SWITCH_HOTKEY_ID);

        // 3. Verifica flag atomico di annullamento immediato
        assert!(
            runner.cancel_flag().load(Ordering::SeqCst),
            "Il flag atomico di cancellazione deve essere impostato a true"
        );

        // 4. Verifica stato Paused
        assert_eq!(
            runner.state(),
            ServiceState::Paused,
            "Lo stato del servizio deve passare immediatamente a Paused"
        );

        // 5. Verifica aggiornamento della tray (tooltip e stato)
        let tooltip = state_to_tooltip(&runner.state());
        assert_eq!(
            tooltip, "RustBind - Paused",
            "Il tooltip della tray icon deve riflettere lo stato Sospeso (In pausa)"
        );

        // 6. Attende completamento e verifica rilascio pulito dei tasti premuti
        thread::sleep(Duration::from_millis(40));
        assert!(platform.get_pressed_keys().is_empty());
    }

    #[test]
    fn test_kill_switch_pressed_during_sequence() {
        let platform = MockPlatform::new();
        let clock = Arc::new(SystemClock::new());

        let mut config = make_test_config();
        config.rules = vec![Rule {
            id: "long_seq".to_string(),
            name: "Long Sequence".to_string(),
            groups: default_rule_groups(),
            enabled: true,
            wait_for_modifier_release: false,
            window_filter: None,
            trigger: Trigger::Hotkey {
                binding: KeyBinding::from_str("Ctrl+1").unwrap(),
            },
            actions: vec![
                InputAction::KeyDown {
                    key: VirtualKey::KeyA,
                },
                InputAction::Delay {
                    milliseconds: 5_000,
                },
                InputAction::KeyUp {
                    key: VirtualKey::KeyA,
                },
            ],
        }];

        let mut runner = EngineRunner::new(platform.clone(), platform.clone(), clock, &Ok(config));
        assert!(runner.state().is_active());

        // Innesca la sequenza (ID 3 corrisponde alla prima regola utente)
        runner.handle_hotkey(HotkeyId(3));

        // Attende brevemente che il thread di esecuzione inizi e prema KeyA
        thread::sleep(Duration::from_millis(30));
        assert!(
            platform.get_pressed_keys().contains(&VirtualKey::KeyA),
            "KeyA deve risultare premuto dal programma all'inizio della sequenza"
        );

        // Preme il Kill-Switch di emergenza (ID 1)
        runner.handle_hotkey(KILL_SWITCH_HOTKEY_ID);

        // Verifica stato immediato: il daemon passa in Pausa
        assert_eq!(runner.state(), ServiceState::Paused);
        assert!(runner.cancel_flag().load(Ordering::SeqCst));

        // Attende la terminazione del worker e il cleanup
        thread::sleep(Duration::from_millis(40));

        // Verifica che i tasti del programma siano stati rilasciati all'annullamento
        assert!(
            platform.get_pressed_keys().is_empty(),
            "Tutti i tasti premuti dal programma devono essere rilasciati all'attivazione del kill-switch"
        );
    }

    #[test]
    fn test_second_hotkey_ignored_while_first_running() {
        let platform = MockPlatform::new();
        let clock = Arc::new(SystemClock::new());

        let mut config = make_test_config();
        config.rules = vec![
            Rule {
                id: "first_rule".to_string(),
                name: "First".to_string(),
                groups: default_rule_groups(),
                enabled: true,
                wait_for_modifier_release: false,
                window_filter: None,
                trigger: Trigger::Hotkey {
                    binding: KeyBinding::from_str("Ctrl+1").unwrap(),
                },
                actions: vec![InputAction::Delay { milliseconds: 100 }],
            },
            Rule {
                id: "second_rule".to_string(),
                name: "Second".to_string(),
                groups: default_rule_groups(),
                enabled: true,
                wait_for_modifier_release: false,
                window_filter: None,
                trigger: Trigger::Hotkey {
                    binding: KeyBinding::from_str("Ctrl+2").unwrap(),
                },
                actions: vec![InputAction::KeyClick {
                    key: VirtualKey::KeyZ,
                }],
            },
        ];

        let mut runner = EngineRunner::new(platform.clone(), platform.clone(), clock, &Ok(config));

        // Avvia la prima sequenza
        runner.handle_hotkey(HotkeyId(3));
        thread::sleep(Duration::from_millis(20));
        assert!(
            runner.is_running(),
            "La prima sequenza deve essere in corso"
        );

        // Tenta di avviare la seconda sequenza mentre la prima è attiva
        runner.handle_hotkey(HotkeyId(4));

        // Attende che la prima finisca
        thread::sleep(Duration::from_millis(150));
        assert!(!runner.is_running(), "La sequenza deve essere terminata");

        // La seconda sequenza deve essere stata ignorata: nessun click di KeyZ deve essere registrato
        let dispatched = platform.get_dispatched_actions();
        let has_key_z = dispatched
            .iter()
            .any(|a| matches!(a, InputAction::KeyClick { key } if *key == VirtualKey::KeyZ));
        assert!(
            !has_key_z,
            "La seconda sequenza deve essere stata ignorata poiché la prima era già in esecuzione"
        );
    }

    #[test]
    fn test_rate_limiter_throttles_without_dropping_events() {
        let clock = Arc::new(MockClock::new());
        let start_time = clock.now();
        let cancel_flag = AtomicBool::new(false);

        // Limite a 10 eventi al secondo
        let mut limiter = RateLimiter::new(clock.clone(), 10);

        // Invia esattamente 10 eventi: deve passare immediatamente senza avanzamento temporale
        limiter.throttle(10, &cancel_flag).unwrap();
        assert_eq!(
            clock.now().duration_since(start_time),
            Duration::ZERO,
            "10 eventi entro il limite non devono causare attesa"
        );

        // Invia altri 2 eventi: superando la soglia dei 10 ev/s, deve rallentare e avanzare
        // deterministicamente di almeno 1 secondo (finestra mobile)
        limiter.throttle(2, &cancel_flag).unwrap();
        let elapsed = clock.now().duration_since(start_time);
        assert!(
            elapsed >= Duration::from_secs(1),
            "Il rate limiter deve aver atteso che i vecchi eventi uscissero dalla finestra mobile (elapsed: {:?})",
            elapsed
        );

        // Nessun evento perso: la storia attuale contiene i 2 eventi appena registrati
        assert_eq!(limiter.current_event_count(), 2);
    }

    #[test]
    fn test_pause_and_resume_lifecycle() {
        let platform = MockPlatform::new();
        let clock = Arc::new(MockClock::new());
        let config = make_test_config();

        let mut runner = EngineRunner::new(platform.clone(), platform.clone(), clock, &Ok(config));
        assert!(runner.state().is_active());

        // Mette in Pausa
        runner.pause();
        assert_eq!(runner.state(), ServiceState::Paused);

        // In Pausa, i trigger vengono ignorati
        runner.handle_hotkey(HotkeyId(3));
        assert!(
            platform.get_dispatched_actions().is_empty(),
            "Nessuna azione deve essere inviata mentre il servizio è in Pausa"
        );

        // Riprende il servizio
        runner.resume();
        assert_eq!(runner.state(), ServiceState::Active);

        // Ora il trigger viene accettato
        runner.handle_hotkey(HotkeyId(3));
        thread::sleep(Duration::from_millis(50));
        assert!(
            !platform.get_dispatched_actions().is_empty(),
            "Le azioni devono essere eseguite dopo la ripresa del servizio"
        );
    }

    #[test]
    fn test_reload_config_from_error_to_active() {
        let platform = MockPlatform::new();
        let clock = Arc::new(MockClock::new());
        let invalid_config = Err(ConfigError::DuplicateRuleId("dup".to_string()));

        // Inizializza con config non valida: entra in stato Error
        let mut runner =
            EngineRunner::new(platform.clone(), platform.clone(), clock, &invalid_config);
        assert!(runner.state().is_error());
        assert_eq!(runner.active_rules_count(), 0);

        // Ricarica con una configurazione valida: deve passare ad Active e registrare le regole
        let valid_config = make_test_config();
        runner.apply_config(&Ok(valid_config));

        assert_eq!(runner.state(), ServiceState::Active);
        assert_eq!(runner.active_rules_count(), 2);
        let registered = platform
            .registered_hotkeys
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        // 1 kill-switch + 2 regole primarie + varianti pass-through (es. Ctrl+Shift+1 / Ctrl+Shift+2)
        assert!(registered.contains_key(&KILL_SWITCH_HOTKEY_ID));
        assert!(
            registered
                .values()
                .any(|b| *b == KeyBinding::from_str("Ctrl+1").unwrap())
        );
        assert!(
            registered
                .values()
                .any(|b| *b == KeyBinding::from_str("Ctrl+2").unwrap())
        );
        assert!(
            registered
                .values()
                .any(|b| *b == KeyBinding::from_str("Ctrl+Shift+1").unwrap())
        );
        assert!(
            registered
                .values()
                .any(|b| *b == KeyBinding::from_str("Ctrl+Shift+2").unwrap())
        );
    }

    #[test]
    fn test_reload_config_with_invalid_becomes_error_and_unregisters_all() {
        let platform = MockPlatform::new();
        let clock = Arc::new(MockClock::new());
        let valid_config = make_test_config();

        // Inizializza in stato Active
        let mut runner =
            EngineRunner::new(platform.clone(), platform.clone(), clock, &Ok(valid_config));
        assert_eq!(runner.state(), ServiceState::Active);
        assert_eq!(runner.active_rules_count(), 2);

        // Ricarica con una configurazione non valida: deve passare in Error e deregistrare tutto
        let invalid_config = Err(ConfigError::DuplicateRuleId(
            "invalid_on_reload".to_string(),
        ));
        runner.apply_config(&invalid_config);

        assert!(runner.state().is_error());
        assert_eq!(runner.active_rules_count(), 0);
        let registered = platform
            .registered_hotkeys
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        assert!(
            registered.is_empty(),
            "Tutte le hotkey devono essere deregistrate al fallimento del reload"
        );
    }

    #[test]
    fn test_pause_in_error_state_preserves_error_and_sets_cancel_flag() {
        let platform = MockPlatform::new();
        let clock = Arc::new(MockClock::new());
        let invalid_config: Result<Config, ConfigError> =
            Err(ConfigError::DuplicateRuleId("dup_err".to_string()));

        let mut runner = EngineRunner::new(platform.clone(), platform, clock, &invalid_config);
        assert!(runner.state().is_error());

        // Resetta il cancel_flag per verificare che pause() lo imposti comunque a true
        runner.cancel_flag().store(false, Ordering::SeqCst);

        runner.pause();

        assert!(
            runner.cancel_flag().load(Ordering::SeqCst),
            "pause() deve sempre impostare cancel_flag a true"
        );
        assert!(
            runner.state().is_error(),
            "pause() non deve mai far uscire da ServiceState::Error"
        );
    }

    #[test]
    fn test_kill_switch_in_error_state_preserves_error_and_sets_cancel_flag() {
        let platform = MockPlatform::new();
        let clock = Arc::new(MockClock::new());
        let invalid_config: Result<Config, ConfigError> =
            Err(ConfigError::DuplicateRuleId("dup_ks".to_string()));

        let mut runner = EngineRunner::new(platform.clone(), platform, clock, &invalid_config);
        assert!(runner.state().is_error());

        runner.cancel_flag().store(false, Ordering::SeqCst);
        runner.handle_hotkey(KILL_SWITCH_HOTKEY_ID);

        assert!(
            runner.cancel_flag().load(Ordering::SeqCst),
            "Il kill-switch deve sempre impostare cancel_flag a true"
        );
        assert!(
            runner.state().is_error(),
            "Il kill-switch non deve mai far uscire da ServiceState::Error"
        );
    }

    #[test]
    fn test_single_rule_conflict_partial_degradation_keeps_other_rules_active() {
        let platform = MockPlatform::new();
        let clock = Arc::new(SystemClock::new());
        let config = make_test_config();

        // Conflitto sulla sola regola 1 (Ctrl+1); kill-switch e regola 2 (Ctrl+2) liberi
        let rule1_binding = KeyBinding::from_str("Ctrl+1").unwrap();
        platform.set_conflicting_binding(rule1_binding);

        let mut runner = EngineRunner::new(platform.clone(), platform.clone(), clock, &Ok(config));

        assert_eq!(runner.state(), ServiceState::Active);
        assert_eq!(runner.active_rules_count(), 1);

        // La regola 1 (ID 3) è stata saltata; la regola 2 (ID 4) deve essere eseguibile
        runner.handle_hotkey(HotkeyId(4));
        thread::sleep(Duration::from_millis(50));

        let dispatched = platform.get_dispatched_actions();
        assert!(
            dispatched
                .iter()
                .any(|a| matches!(a, InputAction::KeyClick { key } if *key == VirtualKey::KeyB)),
            "La regola 2 non in conflitto deve restare attiva ed eseguibile"
        );
    }

    #[test]
    fn test_immediate_execution_skips_modifier_release_wait_by_default() {
        let platform = MockPlatform::new();
        let clock = Arc::new(SystemClock::new());
        let config = make_test_config();
        assert!(!config.rules[0].wait_for_modifier_release);

        let mut runner = EngineRunner::new(platform.clone(), platform.clone(), clock, &Ok(config));
        runner.handle_hotkey(HotkeyId(3));
        thread::sleep(Duration::from_millis(50));

        assert_eq!(
            platform.get_wait_for_modifiers_calls(),
            0,
            "Con rule.wait_for_modifier_release=false la sequenza deve partire subito senza attendere il rilascio"
        );
        assert_eq!(
            platform.get_suppressed_modifiers_calls(),
            1,
            "In modalità default (remapper/macOS) i modificatori del trigger devono essere temporaneamente soppressi"
        );
        assert_eq!(
            platform.get_restored_modifiers_calls(),
            1,
            "Al termine della sequenza i modificatori temporaneamente soppressi vanno ripristinati se ancora premuti"
        );
        assert_eq!(
            platform.get_forced_restore_modifiers_calls(),
            1,
            "Un remap istantaneo senza Delay deve ripristinare subito i modificatori soppressi per supportare trigger ripetuti a modificatore mantenuto"
        );
        assert_eq!(platform.get_dispatched_actions().len(), 1);
    }

    #[test]
    fn test_sequence_with_delay_does_not_force_restore_modifiers() {
        let platform = MockPlatform::new();
        let clock = Arc::new(SystemClock::new());
        let mut config = make_test_config();
        config.rules[0].actions = vec![
            InputAction::Delay { milliseconds: 5 },
            InputAction::KeyClick {
                key: VirtualKey::End,
            },
        ];

        let mut runner = EngineRunner::new(platform.clone(), platform.clone(), clock, &Ok(config));
        runner.handle_hotkey(HotkeyId(3));
        thread::sleep(Duration::from_millis(60));

        assert_eq!(platform.get_suppressed_modifiers_calls(), 1);
        assert_eq!(platform.get_restored_modifiers_calls(), 1);
        assert_eq!(
            platform.get_forced_restore_modifiers_calls(),
            0,
            "Le sequenze con Delay non devono forzare il ripristino dei modificatori"
        );
    }

    #[test]
    fn test_wait_for_modifier_release_per_rule_when_enabled() {
        let platform = MockPlatform::new();
        let clock = Arc::new(SystemClock::new());
        let mut config = make_test_config();
        // Solo la seconda regola abilita wait_for_modifier_release; la prima usa modalità remapper
        config.rules[0].wait_for_modifier_release = false;
        config.rules[1].wait_for_modifier_release = true;

        let mut runner = EngineRunner::new(platform.clone(), platform.clone(), clock, &Ok(config));

        runner.handle_hotkey(HotkeyId(3));
        thread::sleep(Duration::from_millis(50));
        assert_eq!(platform.get_wait_for_modifiers_calls(), 0);
        assert_eq!(platform.get_suppressed_modifiers_calls(), 1);
        assert_eq!(platform.get_restored_modifiers_calls(), 1);

        runner.handle_hotkey(HotkeyId(4));
        thread::sleep(Duration::from_millis(50));
        assert_eq!(
            platform.get_wait_for_modifiers_calls(),
            1,
            "Con rule.wait_for_modifier_release=true sulla singola regola deve attendere il rilascio dei modificatori"
        );
        assert_eq!(
            platform.get_suppressed_modifiers_calls(),
            1,
            "Con wait_for_modifier_release=true non deve inviare soppressione sintetica dei modificatori"
        );
        assert_eq!(platform.get_restored_modifiers_calls(), 1);
        assert_eq!(platform.get_dispatched_actions().len(), 2);
    }

    #[test]
    fn test_active_groups_filtering_at_startup_and_empty_means_all() {
        let platform = MockPlatform::new();
        let clock = Arc::new(SystemClock::new());

        let mut config = make_test_config();
        config.rules[0].set_groups(vec!["lavoro".to_string()]);
        config.rules[1].set_groups(vec!["gaming".to_string()]);

        // Filtro vuoto -> registra entrambe le regole abilitate
        let mut runner = EngineRunner::new(
            platform.clone(),
            platform.clone(),
            clock,
            &Ok(config.clone()),
        );
        assert_eq!(runner.active_rules_count(), 2);

        // Filtro su solo "gaming" -> registra solo il kill-switch e la regola del gruppo gaming
        config.settings.active_groups = vec!["gaming".to_string()];
        runner.apply_config(&Ok(config));
        assert_eq!(runner.active_rules_count(), 1);

        let registered = platform
            .registered_hotkeys
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        assert!(registered.contains_key(&KILL_SWITCH_HOTKEY_ID));
        assert!(
            !registered
                .values()
                .any(|b| *b == KeyBinding::from_str("Ctrl+1").unwrap()),
            "La regola del gruppo 'lavoro' non deve essere registrata"
        );
        assert!(
            registered
                .values()
                .any(|b| *b == KeyBinding::from_str("Ctrl+2").unwrap()),
            "La regola del gruppo 'gaming' deve essere registrata"
        );
    }

    #[test]
    fn test_shared_rule_across_multiple_active_groups_registers_once_without_conflict() {
        let platform = MockPlatform::new();
        let clock = Arc::new(SystemClock::new());

        let mut config = make_test_config();
        config.rules.truncate(1);
        config.rules[0].set_groups(vec!["lavoro".to_string(), "gaming".to_string()]);
        config.settings.active_groups = vec!["lavoro".to_string(), "gaming".to_string()];

        let mut runner = EngineRunner::new(platform.clone(), platform.clone(), clock, &Ok(config));
        assert_eq!(runner.state(), ServiceState::Active);
        assert_eq!(
            runner.active_rules_count(),
            1,
            "Una regola presente in più gruppi attivi deve essere registrata esattamente una volta"
        );

        runner.handle_hotkey(HotkeyId(3));
        thread::sleep(Duration::from_millis(50));
        assert_eq!(platform.get_dispatched_actions().len(), 1);
    }

    #[test]
    fn test_resume_hotkey_resumes_paused_and_stays_fail_closed_on_error() {
        let platform = MockPlatform::new();
        let clock = Arc::new(MockClock::new());

        let mut config = make_test_config();
        config.settings.resume_hotkey = Some(KeyBinding::from_str("Ctrl+Alt+Shift+F11").unwrap());

        let mut runner = EngineRunner::new(platform.clone(), platform, clock, &Ok(config));
        assert_eq!(runner.state(), ServiceState::Active);

        runner.pause();
        assert_eq!(runner.state(), ServiceState::Paused);

        // Premendo l'hotkey di ripresa passa da Paused ad Active
        runner.handle_hotkey(RESUME_HOTKEY_ID);
        assert_eq!(runner.state(), ServiceState::Active);

        // Con configurazione non valida (Error), anche se forzato l'ID non esce da Error
        let invalid: Result<Config, ConfigError> = Err(ConfigError::DuplicateRuleName);
        runner.apply_config(&invalid);
        assert!(runner.state().is_error());
        runner.handle_hotkey(RESUME_HOTKEY_ID);
        assert!(runner.state().is_error());
    }

    #[test]
    fn test_groupless_rule_never_registered_even_when_all_groups_active() {
        let platform = MockPlatform::new();
        let clock = Arc::new(MockClock::new());

        let mut config = make_test_config();
        config.rules.truncate(1);
        config.rules[0].set_groups(Vec::new());
        config.rules[0].enabled = true;
        config.settings.active_groups.clear();

        let runner = EngineRunner::new(platform.clone(), platform, clock, &Ok(config));
        assert_eq!(runner.state(), ServiceState::Active);
        assert_eq!(runner.active_rules_count(), 0);
    }

    #[test]
    fn test_passthrough_modifier_variants_and_explicit_rule_precedence() {
        let platform = MockPlatform::new();
        let clock = Arc::new(SystemClock::new());

        let mut config = make_test_config();
        // Regola 1: Alt+Right -> End (deve registrare anche Alt+Shift+Right, Ctrl+Alt+Right, ecc.)
        config.rules[0].trigger = Trigger::Hotkey {
            binding: KeyBinding::from_str("Alt+Right").unwrap(),
        };
        config.rules[0].actions = vec![InputAction::KeyClick {
            key: VirtualKey::End,
        }];
        // Regola 2: esplicita su Alt+Shift+Right -> KeyA (deve avere priorità sulla variante pass-through della Regola 1)
        config.rules[1].trigger = Trigger::Hotkey {
            binding: KeyBinding::from_str("Alt+Shift+Right").unwrap(),
        };
        config.rules[1].actions = vec![InputAction::KeyClick {
            key: VirtualKey::KeyA,
        }];
        // Regola 3: Alt+KeyE -> KeyB (tasto sensibile a AltGr: NON deve registrare Ctrl+Alt+KeyE)
        config.rules.push(Rule {
            id: "rule_alt_e".to_string(),
            name: "Alt E".to_string(),
            groups: default_rule_groups(),
            enabled: true,
            wait_for_modifier_release: false,
            window_filter: None,
            trigger: Trigger::Hotkey {
                binding: KeyBinding::from_str("Alt+E").unwrap(),
            },
            actions: vec![InputAction::KeyClick {
                key: VirtualKey::KeyB,
            }],
        });

        let mut runner = EngineRunner::new(platform.clone(), platform.clone(), clock, &Ok(config));
        assert_eq!(runner.active_rules_count(), 3);

        let alt_shift_right_id = {
            let registered = platform
                .registered_hotkeys
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            let values: Vec<_> = registered.values().copied().collect();

            // Deve esistere la variante Ctrl+Alt+Right e Ctrl+Alt+Shift+Right per Regola 1
            assert!(values.contains(&KeyBinding::from_str("Ctrl+Alt+Right").unwrap()));
            assert!(values.contains(&KeyBinding::from_str("Ctrl+Alt+Shift+Right").unwrap()));
            // Deve esistere Alt+Shift+E per Regola 3, ma NON Ctrl+Alt+E (protezione AltGr)
            assert!(values.contains(&KeyBinding::from_str("Alt+Shift+E").unwrap()));
            assert!(!values.contains(&KeyBinding::from_str("Ctrl+Alt+E").unwrap()));

            registered
                .iter()
                .find_map(|(id, b)| {
                    (*b == KeyBinding::from_str("Alt+Shift+Right").unwrap()).then_some(*id)
                })
                .unwrap()
        };

        // L'attivazione di Alt+Shift+Right deve eseguire la Regola 2 esplicita (KeyA), non Regola 1 (End)
        runner.handle_hotkey(alt_shift_right_id);
        thread::sleep(Duration::from_millis(50));
        assert_eq!(
            platform.get_dispatched_actions(),
            vec![InputAction::KeyClick {
                key: VirtualKey::KeyA
            }]
        );
    }

    #[test]
    fn test_contextual_rules_swap_hotkeys_by_foreground_and_keep_kill_switch() {
        use crate::core::rule::{ForegroundWindow, WindowFilter, WindowFilterMode};

        let platform = MockPlatform::new();
        let clock = Arc::new(SystemClock::new());
        let mut config = make_test_config();
        // Due regole abilitate sullo STESSO trigger Alt+Left per app diverse:
        // Regola 1 -> solo code.exe; Regola 2 -> solo chrome.exe
        config.rules[0].id = "vscode_home".to_string();
        config.rules[0].trigger = Trigger::Hotkey {
            binding: KeyBinding::from_str("Alt+Left").unwrap(),
        };
        config.rules[0].actions = vec![InputAction::KeyClick {
            key: VirtualKey::Home,
        }];
        config.rules[0].window_filter = Some(WindowFilter {
            mode: WindowFilterMode::Include,
            process_name: Some("code.exe".to_string()),
        });

        config.rules[1].id = "chrome_back".to_string();
        config.rules[1].trigger = Trigger::Hotkey {
            binding: KeyBinding::from_str("Alt+Left").unwrap(),
        };
        config.rules[1].actions = vec![InputAction::KeyClick {
            key: VirtualKey::Escape,
        }];
        config.rules[1].window_filter = Some(WindowFilter {
            mode: WindowFilterMode::Include,
            process_name: Some("chrome.exe".to_string()),
        });

        // Regola 3 globale con esclusione su valorant.exe
        config.rules.push(Rule {
            id: "global_except_game".to_string(),
            name: "Global Except Game".to_string(),
            groups: default_rule_groups(),
            enabled: true,
            wait_for_modifier_release: false,
            window_filter: Some(WindowFilter {
                mode: WindowFilterMode::Exclude,
                process_name: Some("valorant.exe".to_string()),
            }),
            trigger: Trigger::Hotkey {
                binding: KeyBinding::from_str("Ctrl+3").unwrap(),
            },
            actions: vec![InputAction::KeyClick {
                key: VirtualKey::KeyC,
            }],
        });

        let mut runner = EngineRunner::new(platform.clone(), platform.clone(), clock, &Ok(config));
        assert!(runner.has_contextual_rules());

        // Notepad -> nessuna delle due regole Alt+Left è registrata (pass-through nativo Windows!), globale attiva
        runner.on_foreground_changed(ForegroundWindow {
            process_name: Some("notepad.exe".to_string()),
        });
        assert_eq!(runner.active_rules_count(), 1);
        {
            let reg = platform
                .registered_hotkeys
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            assert!(reg.contains_key(&KILL_SWITCH_HOTKEY_ID));
            assert!(
                !reg.values()
                    .any(|b| *b == KeyBinding::from_str("Alt+Left").unwrap())
            );
            assert!(
                reg.values()
                    .any(|b| *b == KeyBinding::from_str("Ctrl+3").unwrap())
            );
        }

        // VS Code -> regola vscode_home registrata (Home) + globale
        runner.on_foreground_changed(ForegroundWindow {
            process_name: Some("Code.exe".to_string()),
        });
        assert_eq!(runner.active_rules_count(), 2);
        let alt_left_id = {
            let reg = platform
                .registered_hotkeys
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            reg.iter()
                .find_map(|(id, b)| {
                    (*b == KeyBinding::from_str("Alt+Left").unwrap()).then_some(*id)
                })
                .unwrap()
        };
        runner.handle_hotkey(alt_left_id);
        thread::sleep(Duration::from_millis(40));
        assert_eq!(
            platform.get_dispatched_actions().last().cloned(),
            Some(InputAction::KeyClick {
                key: VirtualKey::Home
            })
        );

        // Chrome -> scatta chrome_back (Escape) senza conflitto di registrazione
        runner.on_foreground_changed(ForegroundWindow {
            process_name: Some("chrome.exe".to_string()),
        });
        let chrome_alt_left_id = {
            let reg = platform
                .registered_hotkeys
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            reg.iter()
                .find_map(|(id, b)| {
                    (*b == KeyBinding::from_str("Alt+Left").unwrap()).then_some(*id)
                })
                .unwrap()
        };
        runner.handle_hotkey(chrome_alt_left_id);
        thread::sleep(Duration::from_millis(40));
        assert_eq!(
            platform.get_dispatched_actions().last().cloned(),
            Some(InputAction::KeyClick {
                key: VirtualKey::Escape
            })
        );

        // Valorant -> tutte le regole escluse (0), ma il kill-switch resta SEMPRE registrato
        runner.on_foreground_changed(ForegroundWindow {
            process_name: Some("VALORANT".to_string()),
        });
        assert_eq!(runner.active_rules_count(), 0);
        assert!(
            platform
                .registered_hotkeys
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .contains_key(&KILL_SWITCH_HOTKEY_ID)
        );
    }

    #[test]
    fn test_runner_records_skipped_rules_and_reasons_for_diagnostics() {
        use crate::engine::state::{SkippedRuleInfo, SkippedRuleReason};

        let platform = MockPlatform::new();
        let clock = Arc::new(SystemClock::new());
        platform.set_conflicting_binding(KeyBinding::from_str("Ctrl+9").unwrap());

        let mut config = make_test_config();
        // Regola 1 -> conflitto OS esterno
        config.rules[0].id = "os_busy".to_string();
        config.rules[0].trigger = Trigger::Hotkey {
            binding: KeyBinding::from_str("Ctrl+9").unwrap(),
        };
        // Regola 2 -> coincide con kill_switch
        config.rules[1].id = "ks_clash".to_string();
        config.rules[1].trigger = Trigger::Hotkey {
            binding: config.settings.kill_switch,
        };
        // Regola 3 (valida) e Regola 4 (duplicato della 3 -> shadowed)
        config.rules.push(Rule {
            id: "valid_primary".to_string(),
            name: "Valid Primary".to_string(),
            groups: default_rule_groups(),
            enabled: true,
            wait_for_modifier_release: false,
            window_filter: None,
            trigger: Trigger::Hotkey {
                binding: KeyBinding::from_str("Ctrl+5").unwrap(),
            },
            actions: vec![InputAction::KeyClick {
                key: VirtualKey::KeyA,
            }],
        });
        config.rules.push(Rule {
            id: "shadowed_dup".to_string(),
            name: "Shadowed Dup".to_string(),
            groups: default_rule_groups(),
            enabled: true,
            wait_for_modifier_release: false,
            window_filter: None,
            trigger: Trigger::Hotkey {
                binding: KeyBinding::from_str("Ctrl+5").unwrap(),
            },
            actions: vec![InputAction::KeyClick {
                key: VirtualKey::KeyB,
            }],
        });

        let runner = EngineRunner::new(platform.clone(), platform, clock, &Ok(config));
        assert_eq!(runner.active_rules_count(), 1);
        assert_eq!(
            runner.skipped_rules(),
            vec![
                SkippedRuleInfo {
                    rule_id: "ks_clash".to_string(),
                    reason: SkippedRuleReason::ConflictsWithSystemHotkey,
                },
                SkippedRuleInfo {
                    rule_id: "os_busy".to_string(),
                    reason: SkippedRuleReason::OsAlreadyRegistered,
                },
                SkippedRuleInfo {
                    rule_id: "shadowed_dup".to_string(),
                    reason: SkippedRuleReason::ShadowedByEarlierRule,
                },
            ]
        );
    }

    #[test]
    fn test_group_context_filter_with_rule_refinement() {
        let platform = MockPlatform::new();
        let clock = Arc::new(SystemClock::new());
        let mut config = make_test_config();
        config.rules.truncate(1);
        config.groups = vec!["coding".to_string()];
        config.group_filters.insert(
            "coding".to_string(),
            crate::core::rule::WindowFilter {
                mode: crate::core::rule::WindowFilterMode::Include,
                process_name: Some("code.exe, idea64.exe".to_string()),
            },
        );
        config.rules[0].set_groups(vec!["coding".to_string()]);
        config.rules[0].window_filter = Some(crate::core::rule::WindowFilter {
            mode: crate::core::rule::WindowFilterMode::Exclude,
            process_name: Some("idea64.exe".to_string()),
        });

        let mut runner = EngineRunner::new(platform.clone(), platform, clock, &Ok(config));
        assert!(runner.has_contextual_rules());
        // Finestra iniziale vuota -> non corrisponde a code.exe
        assert_eq!(runner.active_rules_count(), 0);

        // Passa a code.exe -> gruppo include code.exe E regola non lo esclude -> registrata
        runner.on_foreground_changed(crate::core::rule::ForegroundWindow {
            process_name: Some("code.exe".to_string()),
        });
        assert_eq!(runner.active_rules_count(), 1);

        // Passa a idea64.exe -> gruppo include idea64.exe MA regola lo esclude -> disattivata
        runner.on_foreground_changed(crate::core::rule::ForegroundWindow {
            process_name: Some("idea64.exe".to_string()),
        });
        assert_eq!(runner.active_rules_count(), 0);
    }
}
