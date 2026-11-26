use std::marker::PhantomData;

use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, DestroyWindow, HICON};

/// Wrapper RAII attorno a un handle di finestra HWND.
///
/// Rispetta rigorosamente la thread affinity Win32: una finestra appartiene
/// esclusivamente al thread che l'ha creata. `SafeHwnd` è esplicitamente `!Send` e `!Sync`.
/// Garantisce la distruzione della finestra al rilascio (`Drop`) sul medesimo thread.
#[derive(Debug)]
pub struct SafeHwnd {
    raw: HWND,
    _not_send_sync: PhantomData<*const ()>,
}

impl SafeHwnd {
    pub fn new(hwnd: HWND) -> Self {
        Self {
            raw: hwnd,
            _not_send_sync: PhantomData,
        }
    }

    pub fn raw(&self) -> HWND {
        self.raw
    }

    /// Disarma il wrapper RAII evitando la distruzione della finestra con `DestroyWindow`.
    /// Restituisce l'handle `HWND` grezzo sottostante.
    pub fn leak(mut self) -> HWND {
        let hwnd = self.raw;
        self.raw = HWND::default();
        hwnd
    }
}

// Verifica a compile-time: SafeHwnd NON deve implementare Send né Sync.
static_assertions::assert_not_impl_any!(SafeHwnd: Send, Sync);

impl Drop for SafeHwnd {
    fn drop(&mut self) {
        if !self.raw.is_invalid() && self.raw.0 as usize != 0 {
            // SAFETY: Invocazione documentata di DestroyWindow sull'handle valido posseduto,
            // eseguita sul thread proprietario del message loop che l'ha istanziata.
            unsafe {
                if let Err(err) = DestroyWindow(self.raw) {
                    tracing::warn!(error = %err, "DestroyWindow fallito durante il drop di SafeHwnd");
                }
            }
        }
    }
}

/// Wrapper RAII attorno a un handle grafico HICON.
///
/// Garantisce la distruzione dell'icona tramite `DestroyIcon` al rilascio (`Drop`).
/// Come per le finestre, rimane vincolato al thread grafico del message loop.
#[derive(Debug)]
pub struct SafeHicon {
    raw: HICON,
    _not_send_sync: PhantomData<*const ()>,
}

impl SafeHicon {
    pub fn new(hicon: HICON) -> Self {
        Self {
            raw: hicon,
            _not_send_sync: PhantomData,
        }
    }

    pub fn raw(&self) -> HICON {
        self.raw
    }
}

// Verifica a compile-time: SafeHicon NON deve implementare Send né Sync.
static_assertions::assert_not_impl_any!(SafeHicon: Send, Sync);

impl Drop for SafeHicon {
    fn drop(&mut self) {
        if !self.raw.is_invalid() && self.raw.0 as usize != 0 {
            // SAFETY: Invocazione documentata di DestroyIcon sull'handle HICON valido posseduto.
            unsafe {
                let _ = DestroyIcon(self.raw);
            }
        }
    }
}

/// Wrapper RAII attorno a un handle di sistema generico Win32 (HANDLE).
///
/// Garantisce la chiusura dell'handle tramite `CloseHandle` al rilascio (`Drop`).
#[derive(Debug)]
pub struct SafeHandle {
    raw: HANDLE,
}

// SAFETY: Il tipo SafeHandle racchiude ESCLUSIVAMENTE handle di oggetti kernel Win32 (quali eventi,
// mutex, token o named pipe) e MAI handle grafici o di finestre (HWND, HICON) che possiedono thread-affinity.
// Gli oggetti di sincronizzazione kernel (dispatcher objects) sono thread-safe a livello di sistema operativo
// e possono essere segnalati, attesi, scambiati o chiusi in sicurezza da qualsiasi thread del medesimo processo.
unsafe impl Send for SafeHandle {}
unsafe impl Sync for SafeHandle {}

impl SafeHandle {
    pub fn new(handle: HANDLE) -> Self {
        Self { raw: handle }
    }

    pub fn raw(&self) -> HANDLE {
        self.raw
    }
}

impl Drop for SafeHandle {
    fn drop(&mut self) {
        if !self.raw.is_invalid() && self.raw.0 as usize != 0 {
            // SAFETY: Invocazione documentata di CloseHandle sull'handle di sistema aperto.
            unsafe {
                let _ = CloseHandle(self.raw);
            }
        }
    }
}
