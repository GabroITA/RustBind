use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::core::action::InputAction;
use crate::engine::clock::Clock;
use crate::platform::traits::PlatformError;

/// Calcola il numero di eventi di input elementari generati da una singola `InputAction`.
pub fn count_action_events(action: &InputAction) -> usize {
    match action {
        InputAction::KeyClick { .. } => 2, // KeyDown + KeyUp
        InputAction::KeyDown { .. } => 1,
        InputAction::KeyUp { .. } => 1,
        InputAction::SendText { text } => text.encode_utf16().count() * 2, // UTF-16 code units (down + up)
        InputAction::Delay { .. } => 0,
        InputAction::MouseClick { .. } => 2, // MouseDown + MouseUp
        InputAction::MouseDown { .. } => 1,
        InputAction::MouseUp { .. } => 1,
        InputAction::MouseScroll { .. } => 1,
        InputAction::MouseMove { .. } => 1,
    }
}

/// Circuit breaker per la limitazione della velocità di invio di input sintetico (Rate Limiter).
///
/// Funziona con una finestra temporale scorrevole (moving window) di 1 secondo esatto.
/// Non scarta mai gli eventi in eccesso: quando la velocità massima consentita viene superata,
/// rallenta deterministicamente l'invio calcolando il tempo necessario affinché i vecchi eventi
/// escano dalla finestra, effettuando un'attesa interrompibile (`sleep_interruptible`).
#[derive(Debug)]
pub struct RateLimiter<C: Clock> {
    clock: Arc<C>,
    max_events_per_second: usize,
    history: VecDeque<Instant>,
}

impl<C: Clock> RateLimiter<C> {
    /// Inizializza il rate limiter con l'orologio specificato e la soglia massima di eventi/sec.
    pub fn new(clock: Arc<C>, max_events_per_second: usize) -> Self {
        Self {
            clock,
            max_events_per_second,
            history: VecDeque::with_capacity(max_events_per_second * 2),
        }
    }

    /// Rimuove dalla cronologia gli eventi più vecchi di 1 secondo rispetto a `now`.
    fn purge_expired(&mut self, now: Instant) {
        let window_duration = Duration::from_secs(1);
        while let Some(&timestamp) = self.history.front() {
            if now.saturating_duration_since(timestamp) >= window_duration {
                self.history.pop_front();
            } else {
                break;
            }
        }
    }

    /// Restituisce il numero di eventi attualmente registrati nell'ultimo secondo.
    pub fn current_event_count(&mut self) -> usize {
        let now = self.clock.now();
        self.purge_expired(now);
        self.history.len()
    }

    /// Azzera la cronologia degli eventi (es. al cambio configurazione o ripristino).
    pub fn reset(&mut self) {
        self.history.clear();
    }

    /// Rallenta l'invio se l'aggiunta di `event_count` eventi supera la soglia di frequenza.
    ///
    /// Attende deterministicamente tramite `Clock::sleep_interruptible`. Se `cancel_flag`
    /// è impostato, l'attesa viene interrotta istantaneamente.
    pub fn throttle(
        &mut self,
        event_count: usize,
        cancel_flag: &AtomicBool,
    ) -> Result<(), PlatformError> {
        if event_count == 0 {
            return Ok(());
        }

        let window_duration = Duration::from_secs(1);

        loop {
            if cancel_flag.load(Ordering::Relaxed) {
                return Err(PlatformError::ExecutionCancelled);
            }

            let now = self.clock.now();
            self.purge_expired(now);

            // Se c'è spazio sufficiente nella finestra mobile da 1 secondo, registra ed esci
            if self.history.len() + event_count <= self.max_events_per_second {
                let now = self.clock.now();
                for _ in 0..event_count {
                    self.history.push_back(now);
                }
                return Ok(());
            }

            // Calcola quanti eventi devono uscire dalla finestra
            let excess =
                (self.history.len() + event_count).saturating_sub(self.max_events_per_second);

            // Calcola l'istante di scadenza del timestamp bersaglio
            let wait_duration =
                if let Some(&target_time) = self.history.get(excess.saturating_sub(1)) {
                    let expiry = target_time + window_duration;
                    let now = self.clock.now();
                    if expiry > now {
                        expiry - now
                    } else {
                        Duration::from_millis(1)
                    }
                } else if let Some(&first) = self.history.front() {
                    let expiry = first + window_duration;
                    let now = self.clock.now();
                    if expiry > now {
                        expiry - now
                    } else {
                        Duration::from_millis(1)
                    }
                } else {
                    Duration::from_secs(1)
                };

            // Esegue l'attesa calcolata interrompibile
            self.clock.sleep_interruptible(wait_duration, cancel_flag)?;
        }
    }
}
