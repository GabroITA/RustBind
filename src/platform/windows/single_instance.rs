use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow,
};
use windows::core::{PCWSTR, w};

use crate::platform::traits::PlatformError;
use crate::platform::windows::handles::SafeHandle;

/// Nome del Named Mutex per la sessione locale dell'utente (Daemon).
pub const SINGLE_INSTANCE_MUTEX_NAME: PCWSTR = w!("Local\\RustBind_Daemon_SingleInstance");

/// Nome del Named Mutex per la sessione locale dell'utente (UI di configurazione).
pub const UI_SINGLE_INSTANCE_MUTEX_NAME: PCWSTR = w!("Local\\RustBind_UI_SingleInstance");

/// Codice di uscita dedicato per il processo che rileva un'altra istanza già attiva.
pub const EXIT_CODE_ALREADY_RUNNING: i32 = 2;

/// Guard RAII per garantire l'esecuzione in istanza singola dei processi RustBind.
///
/// Trattiene il Named Mutex aperto e rilascia la risorsa automaticamente al rilascio (`Drop`).
#[derive(Debug)]
pub struct SingleInstanceGuard {
    handle: SafeHandle,
}

impl SingleInstanceGuard {
    /// Tenta di acquisire in modo esclusivo il Named Mutex del daemon nella sessione utente locale.
    ///
    /// Se il mutex esiste già (un'altra istanza è attiva), restituisce `Err(PlatformError::AlreadyRunning)`.
    pub fn acquire() -> Result<Self, PlatformError> {
        Self::acquire_named_wide(SINGLE_INSTANCE_MUTEX_NAME)
    }

    /// Tenta di acquisire in modo esclusivo il Named Mutex della UI di configurazione (`Local\RustBind_UI_SingleInstance`).
    pub fn acquire_ui() -> Result<Self, PlatformError> {
        Self::acquire_named_wide(UI_SINGLE_INSTANCE_MUTEX_NAME)
    }

    /// Tenta di acquisire in modo esclusivo un Named Mutex con nome arbitrario nella sessione corrente.
    pub fn acquire_named(mutex_name: &str) -> Result<Self, PlatformError> {
        let wide: Vec<u16> = mutex_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        Self::acquire_named_wide(PCWSTR(wide.as_ptr()))
    }

    fn acquire_named_wide(mutex_name: PCWSTR) -> Result<Self, PlatformError> {
        // SAFETY: Creazione del Named Mutex Win32 nella sessione utente corrente con nome UTF-16 NUL-terminated valido.
        let handle = unsafe {
            CreateMutexW(
                None, true, // Inizialmente posseduto
                mutex_name,
            )
        }
        .map_err(|e| PlatformError::OsError(format!("CreateMutexW fallito: {}", e)))?;

        // Avvolge subito in SafeHandle: se il mutex esiste già, il Drop di SafeHandle chiude l'handle
        // senza invocare ReleaseMutex (che agirebbe su un mutex appartenente all'altra istanza).
        let safe_handle = SafeHandle::new(handle);

        // SAFETY: Lettura codice errore Win32 dell'ultima CreateMutexW sullo stesso thread.
        let last_error = unsafe { GetLastError() };
        if last_error == ERROR_ALREADY_EXISTS {
            return Err(PlatformError::AlreadyRunning);
        }

        Ok(Self {
            handle: safe_handle,
        })
    }
}

/// Tenta in modalità best-effort di portare in primo piano una finestra esistente dato il suo titolo esatto.
pub fn focus_window_by_title(title: &str) -> bool {
    let wide_title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide_title` è una stringa UTF-16 NUL-terminated valida; FindWindowW cerca la finestra
    // top-level corrispondente al titolo esatto senza modificare memoria condivisa.
    let Ok(hwnd) = (unsafe { FindWindowW(PCWSTR::null(), PCWSTR(wide_title.as_ptr())) }) else {
        return false;
    };
    if hwnd.is_invalid() || hwnd.0 as usize == 0 {
        return false;
    }

    // SAFETY: `hwnd` è un handle di finestra individuato da FindWindowW; ShowWindow(SW_RESTORE) e
    // SetForegroundWindow operano in modalità best-effort sulla finestra target.
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd).as_bool()
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        // SAFETY: Rilascia la proprietà del mutex prima della chiusura dell'handle da parte di SafeHandle.
        unsafe {
            let _ = ReleaseMutex(self.handle.raw());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Richiede interazione con il sistema Windows reale"]
    fn test_single_instance_mutex_conflict_real() {
        let first = SingleInstanceGuard::acquire();
        if let Ok(_guard1) = first {
            // Un secondo tentativo con la prima istanza viva deve fallire con AlreadyRunning
            let second = SingleInstanceGuard::acquire();
            assert_eq!(second.err(), Some(PlatformError::AlreadyRunning));
        }
    }
}
