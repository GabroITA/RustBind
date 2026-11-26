use std::collections::HashMap;
use std::marker::PhantomData;

use windows::Win32::Foundation::{ERROR_HOTKEY_ALREADY_REGISTERED, HWND};
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey};

use crate::core::binding::KeyBinding;
use crate::platform::traits::{HotkeyId, HotkeyManager, PlatformError};
use crate::platform::windows::conversion::{to_win32_modifiers, to_win32_vk};

/// Gestore Win32 delle hotkey globali tramite `RegisterHotKey`.
///
/// Riceve e utilizza l'`HWND` della message-only window posseduta dal `MessageLoop`.
/// Rispetta la thread-affinity Win32: è `!Send` e `!Sync`.
pub struct WindowsHotkeyManager {
    hwnd: HWND,
    registered: HashMap<HotkeyId, KeyBinding>,
    _not_send_sync: PhantomData<*const ()>,
}

// Verifica a compile-time: WindowsHotkeyManager NON deve implementare Send né Sync (thread affinity).
static_assertions::assert_not_impl_any!(WindowsHotkeyManager: Send, Sync);

impl WindowsHotkeyManager {
    /// Crea una nuova istanza associata alla message-only window specificata.
    pub fn new(hwnd: HWND) -> Self {
        Self {
            hwnd,
            registered: HashMap::new(),
            _not_send_sync: PhantomData,
        }
    }

    /// Restituisce l'handle `HWND` della finestra a cui sono associate le hotkey.
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }
}

impl HotkeyManager for WindowsHotkeyManager {
    fn register(&mut self, id: HotkeyId, binding: &KeyBinding) -> Result<(), PlatformError> {
        let win_mods = to_win32_modifiers(&binding.modifiers);
        let win_vk = to_win32_vk(binding.key);

        // SAFETY: Chiamata ufficiale e documentata a RegisterHotKey.
        // Include il flag MOD_NOREPEAT e usa l'HWND dedicato della message loop window.
        let result =
            unsafe { RegisterHotKey(Some(self.hwnd), id.0 as i32, win_mods, win_vk.0 as u32) };

        if let Err(err) = result {
            if err.code() == ERROR_HOTKEY_ALREADY_REGISTERED.to_hresult() {
                return Err(PlatformError::HotkeyAlreadyRegistered(*binding));
            }
            return Err(PlatformError::HotkeyRegistrationFailed(format!(
                "Errore Win32 ({}) durante la registrazione di {}",
                err, binding
            )));
        }

        self.registered.insert(id, *binding);
        Ok(())
    }

    fn unregister(&mut self, id: HotkeyId) -> Result<(), PlatformError> {
        if self.registered.remove(&id).is_some() {
            // SAFETY: Deregistrazione documentata con UnregisterHotKey.
            unsafe {
                let _ = UnregisterHotKey(Some(self.hwnd), id.0 as i32);
            }
        }
        Ok(())
    }

    fn unregister_all(&mut self) -> Result<(), PlatformError> {
        for id in self.registered.keys() {
            // SAFETY: Deregistrazione pulita di ciascuna hotkey attiva.
            unsafe {
                let _ = UnregisterHotKey(Some(self.hwnd), id.0 as i32);
            }
        }
        self.registered.clear();
        Ok(())
    }
}

impl Drop for WindowsHotkeyManager {
    fn drop(&mut self) {
        // Garantisce che all'uscita o caduta del manager tutte le hotkey siano rilasciate
        let _ = self.unregister_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::key::VirtualKey;
    use crate::core::modifiers::Modifiers;
    use crate::platform::windows::message_loop::MessageLoop;

    // Test di sistema reale marcato #[ignore]: non tocca il sistema durante cargo test standard
    #[test]
    #[ignore = "Richiede interazione con il sistema Windows reale"]
    fn test_windows_register_and_unregister_hotkey_real() {
        let msg_loop = MessageLoop::new().expect("Creazione finestra message-only");
        let mut manager = WindowsHotkeyManager::new(msg_loop.hwnd());
        let binding = KeyBinding::new(
            Modifiers {
                ctrl: true,
                alt: true,
                shift: true,
                win: false,
            },
            VirtualKey::F12,
        );

        let id = HotkeyId(9999);
        let res = manager.register(id, &binding);
        if let Ok(()) = res {
            assert!(manager.unregister(id).is_ok());
        }
    }
}
