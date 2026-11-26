use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::core::action::InputAction;
use crate::core::modifiers::Modifiers;
use crate::engine::circuit_breaker::{RateLimiter, count_action_events};
use crate::engine::clock::Clock;
use crate::platform::traits::{InputSender, PhysicalModifierKey, PlatformError};

/// Richiesta di esecuzione di una sequenza di azioni associata a una regola.
#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub rule_id: String,
    pub actions: Vec<InputAction>,
    pub trigger_modifiers: Modifiers,
    pub wait_for_modifier_release: bool,
}

/// Guard RAII per garantire il ripristino di `is_running` anche in caso di errore o panic.
struct RunningGuard(Arc<AtomicBool>);

impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Esecutore di sequenze su thread dedicato.
///
/// Garantisce:
/// - Esecuzione asincrona fuori dal message loop Win32.
/// - Singola sequenza attiva alla volta (i trigger concorrenti vengono rifiutati).
/// - Annullamento atomico e istantaneo tramite `cancel_flag`.
/// - Rilascio automatico esclusivo dei soli tasti premuti dal programma.
/// - Throttling tramite finestra mobile (`RateLimiter`).
pub struct SequenceExecutor {
    sender: Option<Sender<ExecutionRequest>>,
    cancel_flag: Arc<AtomicBool>,
    is_running: Arc<AtomicBool>,
    worker_handle: Option<JoinHandle<()>>,
}

impl SequenceExecutor {
    /// Crea e avvia il thread di esecuzione dedicato.
    pub fn spawn<I, C>(
        input_sender: I,
        clock: Arc<C>,
        max_events_per_second: usize,
        cancel_flag: Arc<AtomicBool>,
        is_running: Arc<AtomicBool>,
    ) -> Self
    where
        I: InputSender + 'static,
        C: Clock,
    {
        let (sender, receiver) = mpsc::channel::<ExecutionRequest>();

        let thread_cancel_flag = cancel_flag.clone();
        let thread_is_running = is_running.clone();

        let worker_handle = thread::spawn(move || {
            worker_loop(
                receiver,
                input_sender,
                clock,
                max_events_per_second,
                thread_cancel_flag,
                thread_is_running,
            );
        });

        Self {
            sender: Some(sender),
            cancel_flag,
            is_running,
            worker_handle: Some(worker_handle),
        }
    }

    /// Tenta di avviare una sequenza di input.
    ///
    /// Se un'altra sequenza è già in esecuzione, rifiuta la richiesta restituendo `false`.
    /// Se il thread accetta la richiesta, restituisce `true`.
    pub fn trigger(&self, request: ExecutionRequest) -> bool {
        // Garantisce atomicamente che solo una sequenza sia attiva alla volta
        if self
            .is_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            tracing::warn!(
                rule_id = %request.rule_id,
                "Trigger ignorato: una sequenza è già in corso di esecuzione"
            );
            return false;
        }

        // Resetta il flag di cancellazione prima di avviare la nuova sequenza
        self.cancel_flag.store(false, Ordering::SeqCst);

        if let Some(sender) = &self.sender {
            if let Err(err) = sender.send(request) {
                tracing::error!(
                    error = %err,
                    "Impossibile inviare la richiesta al worker thread di esecuzione"
                );
                self.is_running.store(false, Ordering::SeqCst);
                return false;
            }
            true
        } else {
            self.is_running.store(false, Ordering::SeqCst);
            false
        }
    }

    /// Restituisce se è attualmente in corso l'esecuzione di una sequenza.
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    /// Arresta il worker thread e attende il completamento pulito.
    pub fn stop(&mut self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
        // Chiude il canale per sbloccare receiver.recv()
        self.sender.take();
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for SequenceExecutor {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Loop del worker thread di esecuzione sequenze.
fn worker_loop<I, C>(
    receiver: Receiver<ExecutionRequest>,
    input_sender: I,
    clock: Arc<C>,
    max_events_per_second: usize,
    cancel_flag: Arc<AtomicBool>,
    is_running: Arc<AtomicBool>,
) where
    I: InputSender,
    C: Clock,
{
    let mut rate_limiter = RateLimiter::new(clock.clone(), max_events_per_second);

    while let Ok(request) = receiver.recv() {
        // Guard RAII per azzerare is_running a fine sequenza
        let _guard = RunningGuard(is_running.clone());

        let res = execute_sequence(
            &request,
            &input_sender,
            &mut rate_limiter,
            clock.as_ref(),
            &cancel_flag,
        );

        if let Err(err) = res {
            let _ = input_sender.release_pressed_keys();
            if err == PlatformError::ExecutionCancelled {
                tracing::warn!(
                    rule_id = %request.rule_id,
                    "Sequenza terminata anticipatamente per cancellazione (kill-switch)"
                );
            } else {
                tracing::error!(
                    rule_id = %request.rule_id,
                    error = %err,
                    "Errore durante l'esecuzione della sequenza"
                );
            }
        }
    }
}

/// Guard RAII per ripristinare i modificatori fisici temporaneamente soppressi a fine sequenza.
struct TriggerModifiersRestoreGuard<'a, I: InputSender> {
    input_sender: &'a I,
    released_modifiers: Vec<PhysicalModifierKey>,
    trigger_modifiers: Modifiers,
    cancel_flag: &'a AtomicBool,
}

impl<I: InputSender> Drop for TriggerModifiersRestoreGuard<'_, I> {
    fn drop(&mut self) {
        if !self.released_modifiers.is_empty() {
            let _ = self.input_sender.restore_trigger_modifiers(
                &self.released_modifiers,
                &self.trigger_modifiers,
                self.cancel_flag,
                false,
            );
        }
    }
}

/// Esegue deterministicamente la sequenza di azioni di una singola regola.
pub fn execute_sequence<I, C>(
    request: &ExecutionRequest,
    input_sender: &I,
    rate_limiter: &mut RateLimiter<C>,
    clock: &C,
    cancel_flag: &AtomicBool,
) -> Result<(), PlatformError>
where
    I: InputSender,
    C: Clock,
{
    let start_time = clock.now();
    tracing::info!(rule_id = %request.rule_id, "Inizio esecuzione sequenza");

    // 1. Gestione modificatori del trigger:
    // - Se abilitato `wait_for_modifier_release`: attende il rilascio fisico da parte dell'utente.
    // - Altrimenti (default macOS / remapper): rilascia temporaneamente i modificatori del trigger
    //   prima delle azioni e li ripristina al termine se ancora tenuti premuti fisicamente.
    let mut _restore_guard = TriggerModifiersRestoreGuard {
        input_sender,
        released_modifiers: Vec::new(),
        trigger_modifiers: request.trigger_modifiers,
        cancel_flag,
    };

    if request.trigger_modifiers.any() {
        if request.wait_for_modifier_release {
            if let Err(err) = input_sender.wait_for_physical_modifiers_release(
                &request.trigger_modifiers,
                cancel_flag,
                Duration::from_millis(500),
            ) {
                let _ = input_sender.release_pressed_keys();
                tracing::warn!(
                    rule_id = %request.rule_id,
                    error = %err,
                    "Attesa del rilascio modificatori fisici non riuscita o annullata"
                );
                return Err(err);
            }
        } else {
            _restore_guard.released_modifiers =
                input_sender.suppress_trigger_modifiers(&request.trigger_modifiers, cancel_flag)?;
        }
    }

    // 2. Esecuzione azione per azione
    for action in &request.actions {
        if cancel_flag.load(Ordering::Relaxed) {
            let _ = input_sender.release_pressed_keys();
            return Err(PlatformError::ExecutionCancelled);
        }

        match action {
            InputAction::Delay { milliseconds } => {
                clock.sleep_interruptible(Duration::from_millis(*milliseconds), cancel_flag)?;
            }
            InputAction::SendText { text } => {
                // Chunking a blocchi di max 50 caratteri (100 eventi di input) per
                // rate limiting e annullamento granulare
                let max_chunk_chars = 50;
                let chars: Vec<char> = text.chars().collect();
                for chunk in chars.chunks(max_chunk_chars) {
                    if cancel_flag.load(Ordering::Relaxed) {
                        let _ = input_sender.release_pressed_keys();
                        return Err(PlatformError::ExecutionCancelled);
                    }
                    let chunk_text: String = chunk.iter().collect();
                    let chunk_action = InputAction::SendText { text: chunk_text };
                    let evs = count_action_events(&chunk_action);
                    rate_limiter.throttle(evs, cancel_flag)?;
                    input_sender.send_action(&chunk_action, cancel_flag)?;
                }
            }
            other => {
                let evs = count_action_events(other);
                rate_limiter.throttle(evs, cancel_flag)?;
                input_sender.send_action(other, cancel_flag)?;
            }
        }
    }

    let elapsed = clock.now().saturating_duration_since(start_time);
    let has_delay = request
        .actions
        .iter()
        .any(|action| matches!(action, InputAction::Delay { .. }));
    let fast_remap = !has_delay && elapsed < Duration::from_millis(40);

    let released = std::mem::take(&mut _restore_guard.released_modifiers);
    input_sender.restore_trigger_modifiers(
        &released,
        &request.trigger_modifiers,
        cancel_flag,
        fast_remap,
    )?;

    tracing::info!(
        rule_id = %request.rule_id,
        duration_ms = elapsed.as_millis(),
        "Sequenza completata con successo"
    );

    Ok(())
}
