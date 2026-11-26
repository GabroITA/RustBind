use std::error::Error;

use crate::core::action::InputAction;

/// Trait astratto per il disaccoppiamento dell'esecuzione delle azioni.
///
/// Permette al motore logico (`engine`) di inviare azioni alla piattaforma reale (Win32 SendInput)
/// o a un'implementazione mock per i test, e predispone l'architettura per future estensioni.
pub trait ActionDispatcher {
    type Error: Error + Send + Sync + 'static;

    /// Esegue una singola azione di input o ritardo.
    fn dispatch(&mut self, action: &InputAction) -> Result<(), Self::Error>;
}
