use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, LPARAM, WIN32_ERROR, WPARAM};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ, REG_VALUE_TYPE, RegCloseKey,
    RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, SW_SHOWNORMAL, WM_CLOSE};
use windows::core::{PCWSTR, w};

use crate::platform::traits::PlatformError;
use crate::platform::windows::single_instance::SingleInstanceGuard;

/// Chiave di registro utente per l'avvio automatico all'accesso a Windows (nessun privilegio admin).
pub const AUTOSTART_RUN_SUBKEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");

/// Nome del valore nel registro `Run` associato a RustBind.
pub const AUTOSTART_VALUE_NAME: PCWSTR = w!("RustBind");

/// Wrapper RAII attorno a un handle di chiave di registro Windows (`HKEY`).
struct SafeHkey(HKEY);

impl SafeHkey {
    fn raw(&self) -> HKEY {
        self.0
    }
}

impl Drop for SafeHkey {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: L'handle HKEY è stato aperto con RegOpenKeyExW e viene chiuso esattamente
            // una volta al termine dello scope.
            unsafe {
                let _ = RegCloseKey(self.0);
            }
        }
    }
}

/// Converte una stringa Rust in buffer UTF-16 terminato da NUL per chiamate Win32.
fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Formatta il comando quotato per la voce `Run` del registro Windows.
///
/// Funzione pura per test unitari: racchiude il percorso tra virgolette e rifiuta
/// percorsi vuoti o contenenti caratteri di controllo/NUL o virgolette non sicure.
pub fn format_autostart_command(exe_path: &Path) -> Result<String, PlatformError> {
    let raw = exe_path.to_str().ok_or_else(|| {
        PlatformError::OsError("Percorso eseguibile non rappresentabile in UTF-8".into())
    })?;
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.contains('\0') || trimmed.contains('"') {
        return Err(PlatformError::OsError(
            "Percorso eseguibile non valido per la registrazione autostart".into(),
        ));
    }
    Ok(format!("\"{}\"", trimmed))
}

/// Risolve un eseguibile compagno nella stessa directory di `current_exe` (mai da `PATH` o shell).
pub fn resolve_sibling_exe_path(current_exe: &Path, exe_name: &str) -> Option<PathBuf> {
    let sibling = current_exe.parent()?.join(exe_name);
    sibling.is_file().then_some(sibling)
}

/// Determina l'eseguibile del daemon da registrare per l'avvio automatico.
///
/// Se invocato dal processo compagno `rustbind-ui.exe` e `rustbind.exe` esiste nella stessa
/// directory, seleziona `rustbind.exe` così che all'accesso a Windows parta il daemon residente.
pub fn resolve_autostart_exe_path(current_exe: &Path) -> PathBuf {
    let is_ui_exe = current_exe
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("rustbind-ui.exe"));

    if is_ui_exe {
        if let Some(daemon_sibling) = resolve_sibling_exe_path(current_exe, "rustbind.exe") {
            return daemon_sibling;
        }
    }

    current_exe.to_path_buf()
}

/// Avvia il daemon compagno (`rustbind.exe`) accanto all'eseguibile corrente se non è già attivo.
///
/// Verifica prima il Named Mutex del daemon (`Local\RustBind_Daemon_SingleInstance`) e risolve
/// il percorso esclusivamente dalla directory di `std::env::current_exe()`.
pub fn launch_daemon_if_not_running() -> bool {
    match SingleInstanceGuard::acquire() {
        Ok(guard) => drop(guard),
        Err(_) => return false,
    }

    let Ok(current_exe) = std::env::current_exe() else {
        return false;
    };
    let Some(daemon_path) = resolve_sibling_exe_path(&current_exe, "rustbind.exe") else {
        return false;
    };

    std::process::Command::new(&daemon_path).spawn().is_ok()
}

/// Verifica se l'avvio automatico utente di RustBind (`HKCU\...\Run\RustBind`) è abilitato.
pub fn is_autostart_enabled() -> Result<bool, PlatformError> {
    let mut hkey = HKEY::default();
    // SAFETY: Apertura in sola lettura della chiave standard HKCU\...\Run con output su `hkey`.
    let open_err = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            AUTOSTART_RUN_SUBKEY,
            Some(0),
            KEY_READ,
            &mut hkey,
        )
    };
    if open_err == ERROR_FILE_NOT_FOUND {
        return Ok(false);
    }
    if open_err != WIN32_ERROR(0) {
        return Err(PlatformError::OsError(format!(
            "RegOpenKeyExW (KEY_READ) fallito: {:?}",
            open_err
        )));
    }
    let safe_key = SafeHkey(hkey);

    let mut reg_type = REG_VALUE_TYPE::default();
    let mut byte_len = 0u32;
    // SAFETY: Prima chiamata di misura per verificare l'esistenza del valore "RustBind" su chiave valida.
    let query_err = unsafe {
        RegQueryValueExW(
            safe_key.raw(),
            AUTOSTART_VALUE_NAME,
            None,
            Some(&mut reg_type),
            None,
            Some(&mut byte_len),
        )
    };
    if query_err == ERROR_FILE_NOT_FOUND {
        return Ok(false);
    }
    if query_err != WIN32_ERROR(0) {
        return Err(PlatformError::OsError(format!(
            "RegQueryValueExW fallito: {:?}",
            query_err
        )));
    }

    Ok(reg_type == REG_SZ && byte_len > 2)
}

/// Abilita o disabilita l'avvio automatico utente di RustBind in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
pub fn set_autostart_enabled(enabled: bool) -> Result<(), PlatformError> {
    let mut hkey = HKEY::default();
    // SAFETY: Apertura con permesso utente standard `KEY_SET_VALUE` su `HKCU\...\Run` (nessun privilegio admin).
    let open_err = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            AUTOSTART_RUN_SUBKEY,
            Some(0),
            KEY_SET_VALUE,
            &mut hkey,
        )
    };
    if !enabled && open_err == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    if open_err != WIN32_ERROR(0) {
        return Err(PlatformError::OsError(format!(
            "RegOpenKeyExW (KEY_SET_VALUE) fallito: {:?}",
            open_err
        )));
    }
    let safe_key = SafeHkey(hkey);

    if enabled {
        let current_exe = std::env::current_exe()
            .map_err(|e| PlatformError::OsError(format!("current_exe() non disponibile: {}", e)))?;
        let target_exe = resolve_autostart_exe_path(&current_exe);
        let command = format_autostart_command(&target_exe)?;
        let wide = to_wide_null(&command);
        let bytes: Vec<u8> = wide.iter().flat_map(|u| u.to_le_bytes()).collect();

        // SAFETY: Scrittura valore `REG_SZ` terminato da NUL UTF-16 su handle di registro valido aperto con KEY_SET_VALUE.
        let set_err = unsafe {
            RegSetValueExW(
                safe_key.raw(),
                AUTOSTART_VALUE_NAME,
                Some(0),
                REG_SZ,
                Some(&bytes),
            )
        };
        if set_err != WIN32_ERROR(0) {
            return Err(PlatformError::OsError(format!(
                "RegSetValueExW fallito: {:?}",
                set_err
            )));
        }
    } else {
        // SAFETY: Rimozione del valore "RustBind" dalla chiave Run dell'utente corrente.
        let del_err = unsafe { RegDeleteValueW(safe_key.raw(), AUTOSTART_VALUE_NAME) };
        if del_err != WIN32_ERROR(0) && del_err != ERROR_FILE_NOT_FOUND {
            return Err(PlatformError::OsError(format!(
                "RegDeleteValueW fallito: {:?}",
                del_err
            )));
        }
    }

    Ok(())
}

/// Apre una cartella locale esistente (o la crea se mancante) in Esplora File tramite API ufficiale `ShellExecuteW`.
///
/// Verifica che il percorso sia valido e privo di caratteri NUL; non invoca mai shell testuali.
pub fn open_directory_in_explorer(dir: &Path) -> Result<(), PlatformError> {
    let raw = dir
        .to_str()
        .ok_or_else(|| PlatformError::OsError("Percorso cartella non valido (UTF-8)".into()))?
        .trim();
    if raw.is_empty() || raw.contains('\0') {
        return Err(PlatformError::OsError(
            "Percorso cartella vuoto o non valido".into(),
        ));
    }
    if !dir.exists() {
        std::fs::create_dir_all(dir)
            .map_err(|e| PlatformError::OsError(format!("Creazione cartella fallita: {e}")))?;
    }
    if !dir.is_dir() {
        return Err(PlatformError::OsError(
            "Il percorso indicato non è una directory".into(),
        ));
    }

    let wide = to_wide_null(raw);
    // SAFETY: Invocazione documentata di ShellExecuteW con verbo "open" su percorso directory esistente.
    let hinst = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if (hinst.0 as usize) <= 32 {
        return Err(PlatformError::OsError(format!(
            "Apertura cartella in Explorer fallita (codice {})",
            hinst.0 as usize
        )));
    }
    Ok(())
}

/// Tenta in modalità best-effort di chiudere in modo pulito una finestra top-level individuata dal titolo esatto.
///
/// Invia `WM_CLOSE` tramite `PostMessageW` senza ricorrere a terminazioni forzate (`taskkill` o shell).
pub fn close_window_by_title_best_effort(title: &str) -> bool {
    if title.is_empty() {
        return false;
    }

    let wide_title = to_wide_null(title);
    // SAFETY: `wide_title` è una stringa UTF-16 terminata da NUL valida per tutta la chiamata FindWindowW.
    let Ok(hwnd) = (unsafe { FindWindowW(PCWSTR::null(), PCWSTR(wide_title.as_ptr())) }) else {
        return false;
    };
    if hwnd.is_invalid() || hwnd.0 as usize == 0 {
        return false;
    }

    // SAFETY: Invio asincrono best-effort di `WM_CLOSE` alla finestra individuata da `FindWindowW`
    // per consentirne la chiusura ordinata.
    unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)).is_ok() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_autostart_command_quotes_and_rejects_invalid() {
        let valid = format_autostart_command(Path::new(r"C:\Program Files\RustBind\rustbind.exe"))
            .expect("Percorso valido");
        assert_eq!(valid, r#""C:\Program Files\RustBind\rustbind.exe""#);

        assert!(format_autostart_command(Path::new("")).is_err());
        assert!(format_autostart_command(Path::new("   ")).is_err());
        assert!(format_autostart_command(Path::new("bad\"quote.exe")).is_err());
    }

    #[test]
    fn test_resolve_autostart_exe_path_daemon_and_non_ui() {
        let daemon = Path::new(r"C:\App\rustbind.exe");
        assert_eq!(resolve_autostart_exe_path(daemon), daemon);

        // Se rustbind.exe non esiste accanto a un path fittizio, mantiene il path fornito
        let temp_ui = std::env::temp_dir()
            .join("rustbind_nonexistent_test_dir")
            .join("rustbind-ui.exe");
        assert_eq!(resolve_autostart_exe_path(&temp_ui), temp_ui);
    }

    #[test]
    fn test_resolve_autostart_exe_path_uses_daemon_sibling_when_ui() {
        let dir =
            std::env::temp_dir().join(format!("rustbind_autostart_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let daemon = dir.join("rustbind.exe");
        let ui = dir.join("rustbind-ui.exe");
        std::fs::write(&daemon, b"").unwrap();
        std::fs::write(&ui, b"").unwrap();

        assert_eq!(resolve_autostart_exe_path(&ui), daemon);
        assert_eq!(
            resolve_sibling_exe_path(&ui, "rustbind.exe"),
            Some(daemon.clone())
        );
        assert_eq!(
            resolve_sibling_exe_path(&daemon, "rustbind-ui.exe"),
            Some(ui.clone())
        );
        assert_eq!(resolve_sibling_exe_path(&ui, "missing.exe"), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_close_window_empty_title_returns_false() {
        assert!(!close_window_by_title_best_effort(""));
    }

    #[test]
    #[ignore = "Modifica il registro utente reale HKCU\\...\\Run"]
    fn test_registry_autostart_roundtrip_real() {
        let initially_enabled = is_autostart_enabled().expect("Lettura stato autostart");
        set_autostart_enabled(true).expect("Abilitazione autostart");
        assert!(is_autostart_enabled().expect("Verifica post-abilitazione"));
        set_autostart_enabled(initially_enabled).expect("Ripristino stato iniziale");
    }

    #[test]
    #[ignore = "Interroga FindWindowW sul desktop Windows reale"]
    fn test_close_nonexistent_window_real() {
        assert!(!close_window_by_title_best_effort(
            "RustBind_NonExistent_Window_Test_999999"
        ));
    }
}
