use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::core::action::{InputAction, MouseButton};
use crate::core::binding::KeyBinding;
use crate::core::key::VirtualKey;
use crate::core::modifiers::Modifiers;

/// Identificatore numerico univoco per una hotkey registrata nel sistema operativo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HotkeyId(pub u32);

/// Singolo tasto modificatore fisico del trigger (distinto per lato), per la gestione sicura
/// in stile remapper senza esporre tipi nativi di piattaforma.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhysicalModifierKey {
    LeftControl,
    RightControl,
    LeftAlt,
    RightAlt,
    LeftShift,
    RightShift,
    LeftWin,
    RightWin,
}

/// Errori specifici dello strato di piattaforma e interazione con l'OS.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PlatformError {
    #[error("La combinazione di tasti '{0}' è già registrata da un'altra applicazione")]
    HotkeyAlreadyRegistered(KeyBinding),

    #[error("Registrazione della hotkey fallita: {0}")]
    HotkeyRegistrationFailed(String),

    #[error(
        "Invio input incompleto o bloccato (inviati {inserted} su {expected} eventi richiesti)"
    )]
    InputSendFailed { expected: usize, inserted: usize },

    #[error("Timeout durante l'attesa del rilascio dei modificatori fisici dell'hotkey")]
    PhysicalModifiersTimeout,

    #[error("Esecuzione della sequenza di input annullata")]
    ExecutionCancelled,

    #[error("Creazione della finestra di ascolto messaggi fallita: {0}")]
    WindowCreationFailed(String),

    #[error("Un'altra istanza di RustBind è già in esecuzione nella sessione locale")]
    AlreadyRunning,

    #[error("Operazione tray bar fallita: {0}")]
    TrayError(String),

    #[error("Errore del sistema operativo: {0}")]
    OsError(String),
}

/// Gestore responsabile della registrazione e rimozione delle hotkey a livello di sistema operativo.
///
/// Rispetta la thread-affinity di Windows: l'implementazione può essere `!Send` ed è posseduta
/// unicamente dal thread del message loop Win32.
pub trait HotkeyManager {
    /// Registra una hotkey globale associandola a un identificatore univoco.
    fn register(&mut self, id: HotkeyId, binding: &KeyBinding) -> Result<(), PlatformError>;

    /// Rimuove la registrazione di una hotkey precedentemente registrata.
    fn unregister(&mut self, id: HotkeyId) -> Result<(), PlatformError>;

    /// Rimuove tutte le hotkey registrate.
    fn unregister_all(&mut self) -> Result<(), PlatformError>;
}

/// Esecutore safe per l'invio di eventi di input sintetico e ritardi interrompibili.
///
/// Progettato per essere condiviso (`Send + Sync`) ed eseguito su un thread dedicato,
/// permettendo l'annullamento immediato tramite flag atomico.
pub trait InputSender: Send + Sync {
    /// Esegue una singola azione di input controllando lo stato di annullamento.
    fn send_action(
        &self,
        action: &InputAction,
        cancel_flag: &AtomicBool,
    ) -> Result<(), PlatformError>;

    /// Attende che l'utente rilasci i modificatori fisici usati per innescare l'hotkey.
    fn wait_for_physical_modifiers_release(
        &self,
        modifiers: &Modifiers,
        cancel_flag: &AtomicBool,
        timeout: Duration,
    ) -> Result<(), PlatformError>;

    /// Rilascia temporaneamente i modificatori fisici del trigger attualmente premuti
    /// (stile macOS / remapper) e restituisce quelli effettivamente rilasciati.
    fn suppress_trigger_modifiers(
        &self,
        modifiers: &Modifiers,
        cancel_flag: &AtomicBool,
    ) -> Result<Vec<PhysicalModifierKey>, PlatformError>;

    /// Ripristina i modificatori rilasciati temporaneamente a fine sequenza.
    ///
    /// Se `force_restore` è `true` (remap rapido istantaneo senza `Delay`), ripristina
    /// tutti i modificatori soppressi all'avvio perché `SendInput(KEYUP)` azzera `GetAsyncKeyState`;
    /// altrimenti ripristina solo quelli rilevati ancora premuti o da auto-repeat.
    fn restore_trigger_modifiers(
        &self,
        released_modifiers: &[PhysicalModifierKey],
        trigger_modifiers: &Modifiers,
        cancel_flag: &AtomicBool,
        force_restore: bool,
    ) -> Result<(), PlatformError>;

    /// Rilascia tutti i tasti che il programma ha premuto (`KeyDown`) e non ancora rilasciato.
    fn release_pressed_keys(&self) -> Result<(), PlatformError>;
}

impl<T: InputSender + ?Sized> InputSender for Arc<T> {
    fn send_action(
        &self,
        action: &InputAction,
        cancel_flag: &AtomicBool,
    ) -> Result<(), PlatformError> {
        (**self).send_action(action, cancel_flag)
    }

    fn wait_for_physical_modifiers_release(
        &self,
        modifiers: &Modifiers,
        cancel_flag: &AtomicBool,
        timeout: Duration,
    ) -> Result<(), PlatformError> {
        (**self).wait_for_physical_modifiers_release(modifiers, cancel_flag, timeout)
    }

    fn suppress_trigger_modifiers(
        &self,
        modifiers: &Modifiers,
        cancel_flag: &AtomicBool,
    ) -> Result<Vec<PhysicalModifierKey>, PlatformError> {
        (**self).suppress_trigger_modifiers(modifiers, cancel_flag)
    }

    fn restore_trigger_modifiers(
        &self,
        released_modifiers: &[PhysicalModifierKey],
        trigger_modifiers: &Modifiers,
        cancel_flag: &AtomicBool,
        force_restore: bool,
    ) -> Result<(), PlatformError> {
        (**self).restore_trigger_modifiers(
            released_modifiers,
            trigger_modifiers,
            cancel_flag,
            force_restore,
        )
    }

    fn release_pressed_keys(&self) -> Result<(), PlatformError> {
        (**self).release_pressed_keys()
    }
}

/// Implementazione mock in-memory dei trait di piattaforma per test unitari e di integrazione.
#[derive(Default, Clone)]
pub struct MockPlatform {
    pub registered_hotkeys: Arc<Mutex<HashMap<HotkeyId, KeyBinding>>>,
    pub fail_on_binding: Arc<Mutex<Option<KeyBinding>>>,
    pub dispatched_actions: Arc<Mutex<Vec<InputAction>>>,
    pub currently_pressed: Arc<Mutex<HashSet<VirtualKey>>>,
    pub currently_pressed_mouse: Arc<Mutex<HashSet<MouseButton>>>,
    pub wait_for_modifiers_calls: Arc<AtomicUsize>,
    pub suppressed_modifiers_calls: Arc<AtomicUsize>,
    pub restored_modifiers_calls: Arc<AtomicUsize>,
    pub forced_restore_modifiers_calls: Arc<AtomicUsize>,
}

impl MockPlatform {
    pub fn new() -> Self {
        Self::default()
    }

    /// Configura il mock per simulare un conflitto su una specifica combinazione.
    pub fn set_conflicting_binding(&self, binding: KeyBinding) {
        *self
            .fail_on_binding
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(binding);
    }

    /// Restituisce la lista delle azioni ricevute finora.
    pub fn get_dispatched_actions(&self) -> Vec<InputAction> {
        self.dispatched_actions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// Restituisce l'insieme dei tasti virtuali attualmente considerati premuti dal programma.
    pub fn get_pressed_keys(&self) -> HashSet<VirtualKey> {
        self.currently_pressed
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// Restituisce quante volte è stata invocata l'attesa di rilascio dei modificatori fisici.
    pub fn get_wait_for_modifiers_calls(&self) -> usize {
        self.wait_for_modifiers_calls.load(Ordering::SeqCst)
    }

    /// Restituisce quante volte sono stati temporaneamente soppressi i modificatori del trigger.
    pub fn get_suppressed_modifiers_calls(&self) -> usize {
        self.suppressed_modifiers_calls.load(Ordering::SeqCst)
    }

    /// Restituisce quante volte sono stati ripristinati i modificatori del trigger.
    pub fn get_restored_modifiers_calls(&self) -> usize {
        self.restored_modifiers_calls.load(Ordering::SeqCst)
    }

    /// Restituisce quante volte il ripristino è avvenuto in modalità remap istantaneo (`force_restore`).
    pub fn get_forced_restore_modifiers_calls(&self) -> usize {
        self.forced_restore_modifiers_calls.load(Ordering::SeqCst)
    }
}

impl HotkeyManager for MockPlatform {
    fn register(&mut self, id: HotkeyId, binding: &KeyBinding) -> Result<(), PlatformError> {
        if let Some(conflict) = self
            .fail_on_binding
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
        {
            if conflict == binding {
                return Err(PlatformError::HotkeyAlreadyRegistered(*binding));
            }
        }

        let mut hotkeys = self
            .registered_hotkeys
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if hotkeys.values().any(|b| b == binding) {
            return Err(PlatformError::HotkeyAlreadyRegistered(*binding));
        }

        hotkeys.insert(id, *binding);
        Ok(())
    }

    fn unregister(&mut self, id: HotkeyId) -> Result<(), PlatformError> {
        self.registered_hotkeys
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&id);
        Ok(())
    }

    fn unregister_all(&mut self) -> Result<(), PlatformError> {
        self.registered_hotkeys
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        Ok(())
    }
}

impl InputSender for MockPlatform {
    fn send_action(
        &self,
        action: &InputAction,
        cancel_flag: &AtomicBool,
    ) -> Result<(), PlatformError> {
        if cancel_flag.load(Ordering::Relaxed) {
            self.release_pressed_keys()?;
            return Err(PlatformError::ExecutionCancelled);
        }

        match action {
            InputAction::KeyDown { key } => {
                self.currently_pressed
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .insert(*key);
            }
            InputAction::KeyUp { key } => {
                self.currently_pressed
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(key);
            }
            InputAction::MouseDown { button } => {
                self.currently_pressed_mouse
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .insert(*button);
            }
            InputAction::MouseUp { button } => {
                self.currently_pressed_mouse
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(button);
            }
            InputAction::KeyClick { .. }
            | InputAction::SendText { .. }
            | InputAction::MouseClick { .. }
            | InputAction::MouseScroll { .. }
            | InputAction::MouseMove { .. } => {}
            InputAction::Delay { milliseconds } => {
                // Attesa interrompibile a passi di 10ms
                let deadline = Instant::now() + Duration::from_millis(*milliseconds);
                while Instant::now() < deadline {
                    if cancel_flag.load(Ordering::Relaxed) {
                        self.release_pressed_keys()?;
                        return Err(PlatformError::ExecutionCancelled);
                    }
                    std::thread::sleep(Duration::from_millis(5).min(deadline - Instant::now()));
                }
            }
        }

        self.dispatched_actions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(action.clone());
        Ok(())
    }

    fn wait_for_physical_modifiers_release(
        &self,
        _modifiers: &Modifiers,
        cancel_flag: &AtomicBool,
        _timeout: Duration,
    ) -> Result<(), PlatformError> {
        self.wait_for_modifiers_calls.fetch_add(1, Ordering::SeqCst);
        if cancel_flag.load(Ordering::Relaxed) {
            return Err(PlatformError::ExecutionCancelled);
        }
        Ok(())
    }

    fn suppress_trigger_modifiers(
        &self,
        modifiers: &Modifiers,
        cancel_flag: &AtomicBool,
    ) -> Result<Vec<PhysicalModifierKey>, PlatformError> {
        if cancel_flag.load(Ordering::Relaxed) {
            return Err(PlatformError::ExecutionCancelled);
        }
        let mut suppressed = Vec::new();
        if modifiers.ctrl {
            suppressed.push(PhysicalModifierKey::LeftControl);
        }
        if modifiers.alt {
            suppressed.push(PhysicalModifierKey::LeftAlt);
        }
        if modifiers.shift {
            suppressed.push(PhysicalModifierKey::LeftShift);
        }
        if modifiers.win {
            suppressed.push(PhysicalModifierKey::LeftWin);
        }
        if !suppressed.is_empty() {
            self.suppressed_modifiers_calls
                .fetch_add(1, Ordering::SeqCst);
        }
        Ok(suppressed)
    }

    fn restore_trigger_modifiers(
        &self,
        released_modifiers: &[PhysicalModifierKey],
        _trigger_modifiers: &Modifiers,
        cancel_flag: &AtomicBool,
        force_restore: bool,
    ) -> Result<(), PlatformError> {
        if released_modifiers.is_empty() || cancel_flag.load(Ordering::Relaxed) {
            return Ok(());
        }
        self.restored_modifiers_calls.fetch_add(1, Ordering::SeqCst);
        if force_restore {
            self.forced_restore_modifiers_calls
                .fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }

    fn release_pressed_keys(&self) -> Result<(), PlatformError> {
        self.currently_pressed
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        self.currently_pressed_mouse
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::thread;

    use super::*;

    #[test]
    fn test_mock_platform_registration_and_conflict() {
        let mut platform = MockPlatform::new();
        let binding = KeyBinding::new(Modifiers::CTRL, VirtualKey::KeyA);

        // 1. Registrazione valida
        assert!(platform.register(HotkeyId(1), &binding).is_ok());

        // 2. Conflitto duplicato sulla stessa combinazione
        let err = platform.register(HotkeyId(2), &binding).unwrap_err();
        assert_eq!(err, PlatformError::HotkeyAlreadyRegistered(binding));

        // 3. Conflitto esterno simulato (es. altra app che possiede la combinazione)
        let other_binding = KeyBinding::new(Modifiers::ALT, VirtualKey::F4);
        platform.set_conflicting_binding(other_binding);
        let err2 = platform.register(HotkeyId(3), &other_binding).unwrap_err();
        assert_eq!(err2, PlatformError::HotkeyAlreadyRegistered(other_binding));

        // 4. Deregistrazione
        assert!(platform.unregister(HotkeyId(1)).is_ok());
        assert!(platform.register(HotkeyId(4), &binding).is_ok());
    }

    #[test]
    fn test_mock_platform_cancellation_and_key_release() {
        let platform = Arc::new(MockPlatform::new());
        let cancel_flag = Arc::new(AtomicBool::new(false));

        // 1. Simula pressione di un tasto dal programma
        platform
            .send_action(
                &InputAction::KeyDown {
                    key: VirtualKey::KeyA,
                },
                &cancel_flag,
            )
            .unwrap();
        assert!(platform.get_pressed_keys().contains(&VirtualKey::KeyA));

        // 2. Avvia un Delay lungo e annulla concorrentemente tramite flag atomico
        let platform_clone = platform.clone();
        let cancel_clone = cancel_flag.clone();
        let handle = thread::spawn(move || {
            platform_clone.send_action(
                &InputAction::Delay {
                    milliseconds: 10_000,
                },
                &cancel_clone,
            )
        });

        // Simula la pressione del kill-switch dopo 20ms
        thread::sleep(Duration::from_millis(20));
        cancel_flag.store(true, Ordering::Relaxed);

        let result = handle.join().unwrap();
        assert_eq!(result, Err(PlatformError::ExecutionCancelled));

        // 3. Verifica che all'annullamento i tasti premuti dal programma siano stati rilasciati
        assert!(
            platform.get_pressed_keys().is_empty(),
            "I tasti premuti dal programma devono essere rilasciati all'annullamento"
        );

        // 4. Verifica tracciamento e rilascio dei pulsanti mouse in stato Down
        let cancel_flag2 = Arc::new(AtomicBool::new(false));
        platform
            .send_action(
                &InputAction::MouseDown {
                    button: MouseButton::Left,
                },
                &cancel_flag2,
            )
            .unwrap();
        assert!(
            platform
                .currently_pressed_mouse
                .lock()
                .unwrap()
                .contains(&MouseButton::Left)
        );
        cancel_flag2.store(true, Ordering::Relaxed);
        let _ = platform.send_action(&InputAction::Delay { milliseconds: 10 }, &cancel_flag2);
        assert!(
            platform.currently_pressed_mouse.lock().unwrap().is_empty(),
            "I pulsanti mouse mantenuti dal programma devono essere rilasciati all'annullamento"
        );
    }
}
