use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Gestore dei thread worker che detengono un `WakeHandle` verso la finestra del message loop.
///
/// Garantisce l'invariante di sicurezza Win32: prima di procedere alla distruzione della finestra
/// tramite `DestroyWindow`, tutti i thread con un `WakeHandle` devono essere terminati per evitare
/// la trasmissione di messaggi asincroni a una finestra non più esistente o a una finestra terza estranea
/// qualora il sistema operativo riallochi lo stesso identificatore `HWND` (riuso dell'handle).
#[derive(Debug, Default)]
pub struct WorkerThreads {
    handles: Vec<JoinHandle<()>>,
}

impl WorkerThreads {
    /// Inizializza un gestore di worker thread privo di thread registrati.
    pub fn new() -> Self {
        Self {
            handles: Vec::new(),
        }
    }

    /// Registra il `JoinHandle` di un thread worker precedentemente avviato.
    pub fn add(&mut self, handle: JoinHandle<()>) {
        self.handles.push(handle);
    }

    /// Avvia un nuovo thread worker e ne registra automaticamente il `JoinHandle`.
    pub fn spawn<F>(&mut self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let handle = std::thread::spawn(f);
        self.handles.push(handle);
    }

    /// Restituisce il numero di thread worker attualmente tracciati.
    pub fn len(&self) -> usize {
        self.handles.len()
    }

    /// Indica se non ci sono thread worker tracciati.
    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }

    /// Attende la terminazione di tutti i worker thread registrati entro il timeout complessivo specificato.
    ///
    /// Restituisce `true` se tutti i thread sono terminati con successo entro il timeout;
    /// restituisce `false` se almeno un thread è ancora attivo allo scadere del timeout.
    pub fn join_all(&mut self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;

        while let Some(handle) = self.handles.pop() {
            let now = Instant::now();
            let remaining = deadline.saturating_duration_since(now);

            if remaining.is_zero() {
                if !handle.is_finished() {
                    self.handles.push(handle);
                    return false;
                }
                let _ = handle.join();
                continue;
            }

            let step = Duration::from_millis(10);
            let mut finished = false;
            let step_deadline = Instant::now() + remaining;

            while Instant::now() < step_deadline {
                if handle.is_finished() {
                    finished = true;
                    break;
                }
                std::thread::sleep(step.min(remaining));
            }

            if finished || handle.is_finished() {
                let _ = handle.join();
            } else {
                self.handles.push(handle);
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_threads_successful_join() {
        let mut workers = WorkerThreads::new();
        assert!(workers.is_empty());

        workers.spawn(|| {
            std::thread::sleep(Duration::from_millis(10));
        });
        workers.spawn(|| {
            std::thread::sleep(Duration::from_millis(20));
        });

        assert_eq!(workers.len(), 2);
        let completed = workers.join_all(Duration::from_millis(500));
        assert!(completed, "Tutti i thread dovevano terminare entro 500ms");
        assert!(workers.is_empty());
    }

    #[test]
    fn test_worker_threads_timeout_preserves_running_handle() {
        let mut workers = WorkerThreads::new();

        // Avvia un thread che dorme ben oltre il timeout di attesa
        workers.spawn(|| {
            std::thread::sleep(Duration::from_millis(500));
        });

        // Timeout breve di 30ms: il join deve fallire restituendo false
        let completed = workers.join_all(Duration::from_millis(30));
        assert!(!completed, "Il join doveva scadere restituendo false");
        assert_eq!(
            workers.len(),
            1,
            "L'handle del thread non terminato deve rimanere tracciato"
        );

        // Attesa finale con timeout sufficiente per ripulire il thread del test
        let cleanup_ok = workers.join_all(Duration::from_secs(1));
        assert!(cleanup_ok);
    }
}
