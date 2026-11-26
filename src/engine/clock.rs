use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::platform::traits::PlatformError;

/// Astrazione per la gestione del tempo e delle attese interrompibili.
///
/// Permette l'utilizzo di un orologio di sistema reale (`SystemClock`) o simulato
/// e deterministico (`MockClock`) per test istantanei e privi di flaky delay.
pub trait Clock: Send + Sync + 'static {
    /// Restituisce l'istante temporale attuale.
    fn now(&self) -> Instant;

    /// Esegue un'attesa deterministica interrompibile tramite flag atomico.
    ///
    /// Se `cancel_flag` è impostato prima o durante l'attesa, restituisce
    /// immediatamente `Err(PlatformError::ExecutionCancelled)`.
    fn sleep_interruptible(
        &self,
        duration: Duration,
        cancel_flag: &AtomicBool,
    ) -> Result<(), PlatformError>;
}

/// Orologio basato sul tempo reale del sistema operativo Windows.
#[derive(Debug, Default)]
pub struct SystemClock;

impl SystemClock {
    pub fn new() -> Self {
        Self
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep_interruptible(
        &self,
        duration: Duration,
        cancel_flag: &AtomicBool,
    ) -> Result<(), PlatformError> {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            if cancel_flag.load(Ordering::Relaxed) {
                return Err(PlatformError::ExecutionCancelled);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            // Attesa frammentata a fette di 5 millisecondi per massima reattività al kill-switch
            std::thread::sleep(Duration::from_millis(5).min(remaining));
        }

        if cancel_flag.load(Ordering::Relaxed) {
            return Err(PlatformError::ExecutionCancelled);
        }

        Ok(())
    }
}

/// Orologio deterministico simulato per unit test veloci e privi di attese reali.
#[derive(Debug)]
pub struct MockClock {
    current: Mutex<Instant>,
    auto_advance_on_sleep: bool,
}

impl MockClock {
    pub fn new() -> Self {
        Self {
            current: Mutex::new(Instant::now()),
            auto_advance_on_sleep: true,
        }
    }

    pub fn with_start_time(start_time: Instant) -> Self {
        Self {
            current: Mutex::new(start_time),
            auto_advance_on_sleep: true,
        }
    }

    /// Configura se `sleep_interruptible` debba avanzare automaticamente il tempo simulato.
    pub fn set_auto_advance(&mut self, auto: bool) {
        self.auto_advance_on_sleep = auto;
    }

    /// Avanza manualmente il tempo simulato della durata indicata.
    pub fn advance(&self, duration: Duration) {
        let mut cur = self.current.lock().unwrap_or_else(|p| p.into_inner());
        *cur += duration;
    }
}

impl Default for MockClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for MockClock {
    fn now(&self) -> Instant {
        *self.current.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn sleep_interruptible(
        &self,
        duration: Duration,
        cancel_flag: &AtomicBool,
    ) -> Result<(), PlatformError> {
        if cancel_flag.load(Ordering::Relaxed) {
            return Err(PlatformError::ExecutionCancelled);
        }

        if self.auto_advance_on_sleep {
            self.advance(duration);
        }

        if cancel_flag.load(Ordering::Relaxed) {
            return Err(PlatformError::ExecutionCancelled);
        }

        Ok(())
    }
}
