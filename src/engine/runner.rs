use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::config::error::ConfigError;
use crate::config::schema::Config;
use crate::core::binding::KeyBinding;
use crate::core::key::VirtualKey;
use crate::core::modifiers::Modifiers;
use crate::core::rule::{ForegroundWindow, Rule, Trigger};
use crate::engine::clock::Clock;
use crate::engine::executor::{ExecutionRequest, SequenceExecutor};
use crate::engine::state::{ServiceState, SkippedRuleInfo, SkippedRuleReason};
use crate::platform::traits::{HotkeyId, HotkeyManager, InputSender, PlatformError};

/// Voce registrata per una hotkey utente (primaria della regola o variante con modificatori aggiuntivi pass-through).
struct RegisteredRuleHotkey {
    rule: Rule,
    is_primary: bool,
}

/// Indica se il tasto può produrre caratteri tramite `AltGr` (`Ctrl+Alt`) su layout europei/italiani.
fn is_altgr_sensitive_key(key: VirtualKey) -> bool {
    matches!(
        key,
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
    )
}

/// Genera le varianti con modificatori extra pass-through (es. `+Shift`, `+Ctrl`, `+Alt`) per
/// sommare la regola alle scorciatoie esistenti senza interferire con `Win` o `AltGr` (`Ctrl+Alt` su tasti testo).
fn passthrough_hotkey_variants(base: &KeyBinding) -> Vec<KeyBinding> {
    let m = base.modifiers;
    let key = base.key;
    let sensitive = is_altgr_sensitive_key(key);
    let mut variants = Vec::with_capacity(5);

    if !m.shift {
        variants.push(KeyBinding::new(Modifiers { shift: true, ..m }, key));
    }

    let allow_extra_ctrl = !m.ctrl && (!m.alt || !sensitive);
    if allow_extra_ctrl {
        variants.push(KeyBinding::new(Modifiers { ctrl: true, ..m }, key));
        if !m.shift {
            variants.push(KeyBinding::new(
                Modifiers {
                    ctrl: true,
                    shift: true,
                    ..m
                },
                key,
            ));
        }
    }

    let allow_extra_alt = !m.alt && (!m.ctrl || !sensitive);
    if allow_extra_alt {
        variants.push(KeyBinding::new(Modifiers { alt: true, ..m }, key));
        if !m.shift {
            variants.push(KeyBinding::new(
                Modifiers {
                    alt: true,
                    shift: true,
                    ..m
                },
                key,
            ));
        }
    }

    variants
}

/// Identificatore riservato e prioritario per l'hotkey di emergenza (kill-switch).
pub const KILL_SWITCH_HOTKEY_ID: HotkeyId = HotkeyId(1);

/// Identificatore riservato per l'hotkey opzionale di ripresa dalla pausa (`Paused -> Active`).
pub const RESUME_HOTKEY_ID: HotkeyId = HotkeyId(2);

/// Coordinatore del motore di esecuzione di RustBind.
///
/// Responsabilità:
/// - Gestione del ciclo di vita sul thread del message loop (possiede `HotkeyManager`, !Send).
/// - Applicazione rigorosa del comportamento **Fail-Closed**: se la configurazione è non valida
///   o se la registrazione del kill-switch fallisce all'avvio, il servizio entra in stato di
///   errore critico e **non registra né esegue alcuna regola**.
/// - Registrazione prioritaria del Kill-Switch di emergenza (`Ctrl+Alt+Shift+F12` di default).
/// - Annullamento atomico e istantaneo alla pressione del kill-switch senza passare dal canale.
/// - Rifiuto deterministico di trigger concorrenti (una sola sequenza attiva alla volta).
pub struct EngineRunner<H, C>
where
    H: HotkeyManager,
    C: Clock,
{
    hotkey_manager: H,
    clock: Arc<C>,
    executor: SequenceExecutor,
    state: Arc<Mutex<ServiceState>>,
    cancel_flag: Arc<AtomicBool>,
    rules_by_hotkey: HashMap<HotkeyId, RegisteredRuleHotkey>,
    kill_switch_id: HotkeyId,
    registered_resume_hotkey: Option<HotkeyId>,
    active_rules_count: Arc<AtomicUsize>,
    active_config: Option<Config>,
    system_claimed_bindings: HashSet<KeyBinding>,
    current_foreground: ForegroundWindow,
    active_contextual_rule_ids: Vec<String>,
    skipped_rules_by_id: HashMap<String, SkippedRuleReason>,
    resume_hotkey_failed: bool,
}

impl<H, C> EngineRunner<H, C>
where
    H: HotkeyManager,
    C: Clock,
{
    /// Inizializza il motore e tenta di registrare il kill-switch e le regole configurate.
    pub fn new<I>(
        hotkey_manager: H,
        input_sender: I,
        clock: Arc<C>,
        config_result: &Result<Config, ConfigError>,
    ) -> Self
    where
        I: InputSender + 'static,
    {
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let is_running = Arc::new(AtomicBool::new(false));
        let max_events = match config_result {
            Ok(c) => c.settings.max_events_per_second as usize,
            Err(_) => 100,
        };

        let executor = SequenceExecutor::spawn(
            input_sender,
            clock.clone(),
            max_events,
            cancel_flag.clone(),
            is_running,
        );

        let state = Arc::new(Mutex::new(ServiceState::Active));
        let active_rules_count = Arc::new(AtomicUsize::new(0));

        let mut runner = Self {
            hotkey_manager,
            clock,
            executor,
            state,
            cancel_flag,
            rules_by_hotkey: HashMap::new(),
            kill_switch_id: KILL_SWITCH_HOTKEY_ID,
            registered_resume_hotkey: None,
            active_rules_count,
            active_config: None,
            system_claimed_bindings: HashSet::new(),
            current_foreground: ForegroundWindow::default(),
            active_contextual_rule_ids: Vec::new(),
            skipped_rules_by_id: HashMap::new(),
            resume_hotkey_failed: false,
        };

        runner.apply_config(config_result);
        runner
    }

    /// Applica la configurazione rispettando fail-closed e registrazione prioritaria del kill-switch.
    pub fn apply_config(&mut self, config_result: &Result<Config, ConfigError>) {
        // Annulla immediatamente qualsiasi sequenza in corso e azzera registrazioni
        self.cancel_flag.store(true, Ordering::SeqCst);
        let _ = self.hotkey_manager.unregister_all();
        self.rules_by_hotkey.clear();
        self.registered_resume_hotkey = None;
        self.active_config = None;
        self.system_claimed_bindings.clear();
        self.active_contextual_rule_ids.clear();
        self.skipped_rules_by_id.clear();
        self.resume_hotkey_failed = false;

        match config_result {
            Err(err) => {
                let msg = format!("Configurazione non valida: {}", err);
                tracing::error!("{}", msg);
                *self.state.lock().unwrap_or_else(|p| p.into_inner()) = ServiceState::Error(msg);
                self.active_rules_count.store(0, Ordering::SeqCst);
                // FAIL-CLOSED: nessuna regola viene registrata né eseguita
            }
            Ok(config) => {
                // 1. Registra PRIMA il Kill-Switch di emergenza
                let kill_switch = &config.settings.kill_switch;
                if let Err(err) = self
                    .hotkey_manager
                    .register(self.kill_switch_id, kill_switch)
                {
                    let msg = format!("Registrazione kill-switch fallita: {}", err);
                    tracing::error!("{}", msg);
                    *self.state.lock().unwrap_or_else(|p| p.into_inner()) =
                        ServiceState::Error(msg);
                    self.active_rules_count.store(0, Ordering::SeqCst);
                    // Requisito di sicurezza fail-safe: se il kill-switch fallisce,
                    // NON registrare alcuna altra hotkey
                    return;
                }

                *self.state.lock().unwrap_or_else(|p| p.into_inner()) = ServiceState::Active;
                tracing::info!("Kill-switch registrato con successo: {}", kill_switch);

                let mut claimed_bindings: HashSet<KeyBinding> = HashSet::new();
                claimed_bindings.insert(*kill_switch);

                // 1b. Registra eventuale hotkey opzionale di ripresa (Paused -> Active)
                if let Some(resume_binding) = &config.settings.resume_hotkey {
                    if let Err(err) = self
                        .hotkey_manager
                        .register(RESUME_HOTKEY_ID, resume_binding)
                    {
                        self.resume_hotkey_failed = true;
                        tracing::warn!(
                            error = %err,
                            "Impossibile registrare l'hotkey di ripresa configurata"
                        );
                    } else {
                        self.registered_resume_hotkey = Some(RESUME_HOTKEY_ID);
                        claimed_bindings.insert(*resume_binding);
                        tracing::info!(
                            "Hotkey di ripresa registrata con successo: {}",
                            resume_binding
                        );
                    }
                }

                self.system_claimed_bindings = claimed_bindings;
                self.active_config = Some(config.clone());
                self.sync_user_rules_for_foreground(true);
            }
        }
    }

    fn rule_in_active_groups(config: &Config, rule: &Rule) -> bool {
        config.is_rule_in_active_groups(rule)
    }

    /// Indica se la configurazione attiva contiene almeno una regola abilitata con filtro contestuale
    /// (a livello di singola regola o tramite i gruppi attivi di appartenenza).
    pub fn has_contextual_rules(&self) -> bool {
        let Some(config) = &self.active_config else {
            return false;
        };
        if self.state().is_error() {
            return false;
        }
        config.rules.iter().any(|rule| {
            rule.enabled
                && Self::rule_in_active_groups(config, rule)
                && config.is_rule_contextual(rule)
        })
    }

    /// Aggiorna il contesto della finestra in primo piano risincronizzando le hotkey contestuali.
    pub fn on_foreground_changed(&mut self, fg: ForegroundWindow) {
        self.current_foreground = fg;
        if self.active_config.is_some() && !self.state().is_error() {
            self.sync_user_rules_for_foreground(false);
        }
    }

    /// Sincronizza dinamicamente le sole hotkey utente (ID >= 3) compatibili con il contesto attivo,
    /// preservando in ogni istante Kill-Switch e hotkey di ripresa senza interruzioni.
    fn sync_user_rules_for_foreground(&mut self, force: bool) {
        let Some(config) = self.active_config.clone() else {
            return;
        };

        let eligible_rules: Vec<Rule> = config
            .rules
            .iter()
            .filter(|rule| {
                rule.enabled && config.rule_matches_context(rule, &self.current_foreground)
            })
            .cloned()
            .collect();

        let desired_ids: Vec<String> = eligible_rules.iter().map(|r| r.id.clone()).collect();
        if !force && desired_ids == self.active_contextual_rule_ids {
            return;
        }

        // Deregistra solo le hotkey utente precedenti mantenendo sempre attivi kill-switch e resume_hotkey.
        let prev_user_ids: Vec<HotkeyId> = self.rules_by_hotkey.keys().copied().collect();
        for id in prev_user_ids {
            let _ = self.hotkey_manager.unregister(id);
        }
        self.rules_by_hotkey.clear();

        let mut claimed_bindings = self.system_claimed_bindings.clone();
        let mut next_id = 3u32;
        let mut registered_primary_rules: Vec<(Rule, KeyBinding)> = Vec::new();

        for rule in eligible_rules {
            let Trigger::Hotkey { binding } = rule.trigger;
            if binding == config.settings.kill_switch
                || config.settings.resume_hotkey == Some(binding)
            {
                self.skipped_rules_by_id.insert(
                    rule.id.clone(),
                    SkippedRuleReason::ConflictsWithSystemHotkey,
                );
                tracing::warn!(
                    rule_id = %rule.id,
                    "Regola saltata: coincide con il Kill-Switch o con l'hotkey di ripresa"
                );
                continue;
            }
            if claimed_bindings.contains(&binding) {
                self.skipped_rules_by_id
                    .insert(rule.id.clone(), SkippedRuleReason::ShadowedByEarlierRule);
                tracing::warn!(
                    rule_id = %rule.id,
                    "Regola saltata: combinazione già assegnata a una regola precedente"
                );
                continue;
            }

            let id = HotkeyId(next_id);
            next_id += 1;
            if let Err(err) = self.hotkey_manager.register(id, &binding) {
                let reason = match &err {
                    PlatformError::HotkeyAlreadyRegistered(_) => {
                        SkippedRuleReason::OsAlreadyRegistered
                    }
                    _ => SkippedRuleReason::OsRegistrationFailed,
                };
                self.skipped_rules_by_id.insert(rule.id.clone(), reason);
                tracing::warn!(
                    rule_id = %rule.id,
                    error = %err,
                    "Impossibile registrare la combinazione della regola per conflitto o errore di sistema"
                );
            } else {
                self.skipped_rules_by_id.remove(&rule.id);
                claimed_bindings.insert(binding);
                self.rules_by_hotkey.insert(
                    id,
                    RegisteredRuleHotkey {
                        rule: rule.clone(),
                        is_primary: true,
                    },
                );
                registered_primary_rules.push((rule, binding));
            }
        }

        for (rule, base_binding) in registered_primary_rules {
            for variant in passthrough_hotkey_variants(&base_binding) {
                if !claimed_bindings.insert(variant) {
                    continue;
                }
                let id = HotkeyId(next_id);
                next_id += 1;
                if let Err(err) = self.hotkey_manager.register(id, &variant) {
                    tracing::debug!(
                        rule_id = %rule.id,
                        error = %err,
                        "Variante pass-through non registrata per conflitto di sistema"
                    );
                } else {
                    self.rules_by_hotkey.insert(
                        id,
                        RegisteredRuleHotkey {
                            rule: rule.clone(),
                            is_primary: false,
                        },
                    );
                }
            }
        }

        self.active_contextual_rule_ids = desired_ids;
        let primary_count = self
            .rules_by_hotkey
            .values()
            .filter(|entry| entry.is_primary)
            .count();
        self.active_rules_count
            .store(primary_count, Ordering::SeqCst);
    }

    /// Transiziona da `Active` a `Paused` preservando rigidamente l'invariante fail-closed su `Error`.
    fn transition_to_paused_fail_closed(&self, source: &'static str) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        match &*state {
            ServiceState::Active => {
                *state = ServiceState::Paused;
                tracing::info!(source, "Servizio sospeso: stato Paused");
            }
            ServiceState::Paused => {
                tracing::debug!(source, "Richiesta sospensione con servizio già in Pausa");
            }
            ServiceState::Error(_) => {
                tracing::warn!(
                    source,
                    "Tentativo di uscita verso Paused ignorato: il servizio resta in stato Error (fail-closed)"
                );
            }
        }
    }

    /// Elabora un evento hotkey scatenato dal message loop Windows.
    pub fn handle_hotkey(&mut self, id: HotkeyId) {
        tracing::debug!(hotkey_id = id.0, "Elaborazione evento hotkey nel runner");

        // 1. Kill-Switch di emergenza: interruzione istantanea senza passare per il canale.
        // Imposta sempre cancel_flag = true e transiziona Active -> Paused, senza mai uscire da Error.
        if id == self.kill_switch_id {
            self.cancel_flag.store(true, Ordering::SeqCst);
            self.transition_to_paused_fail_closed("kill_switch");
            return;
        }

        // 1b. Hotkey opzionale di ripresa: transiziona Paused -> Active senza mai uscire da Error
        if Some(id) == self.registered_resume_hotkey {
            let current_state = self.state.lock().unwrap_or_else(|p| p.into_inner()).clone();
            match current_state {
                ServiceState::Paused => self.resume(),
                ServiceState::Active => {
                    tracing::debug!("Hotkey di ripresa ricevuta ma il servizio è già Attivo");
                }
                ServiceState::Error(_) => {
                    tracing::warn!(
                        "Tentativo di ripresa via hotkey ignorato: il servizio resta in stato Error (fail-closed)"
                    );
                }
            }
            return;
        }

        // 2. Verifica stato del servizio
        let current_state = self.state.lock().unwrap_or_else(|p| p.into_inner()).clone();
        if !current_state.is_active() {
            tracing::debug!(
                state = %current_state,
                "Hotkey ricevuta ma servizio non attivo, trigger ignorato"
            );
            return;
        }

        // 3. Dispatch della sequenza associata
        if let Some(entry) = self.rules_by_hotkey.get(&id) {
            let rule = &entry.rule;
            if !rule.matches_foreground(&self.current_foreground) {
                tracing::debug!(
                    rule_id = %rule.id,
                    "Trigger ignorato: contesto finestra attiva non più corrispondente"
                );
                return;
            }
            let trigger_modifiers = match &rule.trigger {
                Trigger::Hotkey { binding } => binding.modifiers,
            };

            let request = ExecutionRequest {
                rule_id: rule.id.clone(),
                actions: rule.actions.clone(),
                trigger_modifiers,
                wait_for_modifier_release: rule.wait_for_modifier_release,
            };

            let accepted = self.executor.trigger(request);
            if !accepted {
                tracing::warn!(
                    rule_id = %rule.id,
                    "Trigger scartato: un'altra sequenza è già in esecuzione"
                );
            }
        } else {
            tracing::warn!(
                hotkey_id = id.0,
                "Ricevuto identificatore hotkey sconosciuto"
            );
        }
    }

    /// Sospende temporaneamente l'esecuzione di qualsiasi regola.
    ///
    /// Imposta sempre `cancel_flag = true`; transiziona `Active -> Paused` e non esce mai da `Error`.
    pub fn pause(&mut self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
        self.transition_to_paused_fail_closed("pause");
    }

    /// Riprende l'esecuzione attiva se il servizio si trovava in Pausa.
    pub fn resume(&mut self) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if *state == ServiceState::Paused {
            *state = ServiceState::Active;
            tracing::info!("Servizio ripreso: stato Attivo");
        }
    }

    /// Restituisce lo stato attuale del servizio.
    pub fn state(&self) -> ServiceState {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Restituisce il numero di regole attualmente abilitate e registrate.
    pub fn active_rules_count(&self) -> usize {
        self.rules_by_hotkey
            .values()
            .filter(|entry| entry.is_primary)
            .count()
    }

    /// Restituisce un puntatore condiviso allo stato del servizio (`Arc<Mutex<ServiceState>>`).
    pub fn state_arc(&self) -> Arc<Mutex<ServiceState>> {
        self.state.clone()
    }

    /// Restituisce un puntatore condiviso al contatore atomico delle regole attive.
    pub fn active_rules_arc(&self) -> Arc<AtomicUsize> {
        self.active_rules_count.clone()
    }

    /// Restituisce il flag di cancellazione condiviso.
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel_flag.clone()
    }

    /// Restituisce se è attualmente in esecuzione una sequenza.
    pub fn is_running(&self) -> bool {
        self.executor.is_running()
    }

    /// Restituisce l'orologio utilizzato.
    pub fn clock(&self) -> Arc<C> {
        self.clock.clone()
    }

    /// Restituisce l'elenco ordinato delle regole abilitate saltate dal daemon per conflitto/errore.
    pub fn skipped_rules(&self) -> Vec<SkippedRuleInfo> {
        let mut out: Vec<SkippedRuleInfo> = self
            .skipped_rules_by_id
            .iter()
            .map(|(rule_id, reason)| SkippedRuleInfo {
                rule_id: rule_id.clone(),
                reason: *reason,
            })
            .collect();
        out.sort_by(|a, b| a.rule_id.cmp(&b.rule_id));
        out
    }

    /// Restituisce la configurazione attiva correntemente caricata dal daemon, se valida.
    pub fn active_config(&self) -> Option<&Config> {
        self.active_config.as_ref()
    }

    /// Indica se la registrazione dell'eventuale `resume_hotkey` è fallita in Windows.
    pub fn resume_hotkey_failed(&self) -> bool {
        self.resume_hotkey_failed
    }
}
