use std::marker::PhantomData;

use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
    NOTIFY_ICON_DATA_FLAGS, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIcon, CreatePopupMenu, DestroyMenu, GetCursorPos, HICON, MF_CHECKED,
    MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, PostMessageW, SetForegroundWindow,
    TPM_BOTTOMALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTALIGN, TrackPopupMenuEx, WM_CONTEXTMENU,
    WM_LBUTTONDBLCLK, WM_NULL,
};
use windows::core::PCWSTR;

use crate::config::manager::ConfigManager;
use crate::engine::state::ServiceState;
use crate::platform::traits::PlatformError;
use crate::platform::windows::handles::SafeHicon;
use crate::platform::windows::message_loop::WM_TRAY_CALLBACK;

const TRAY_ICON_UID: u32 = 100;

static TRAY_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Rimuove l'icona tray in modalità best-effort (es. durante panic hook o terminazione anomala).
pub fn remove_tray_icon_best_effort() {
    let raw = TRAY_HWND.load(std::sync::atomic::Ordering::SeqCst);
    if raw != 0 {
        let hwnd = HWND(raw as *mut std::ffi::c_void);
        let nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: TRAY_ICON_UID,
            ..Default::default()
        };
        // SAFETY: Invocazione documentata best-effort di Shell_NotifyIconW(NIM_DELETE) per ripulire
        // l'icona di notifica dalla barra di sistema in caso di panic o terminazione anomala.
        // NOTA DI THREAD-AFFINITY: Questa funzione può essere chiamata da un thread diverso da quello
        // del message loop proprietario dell'icona (ad esempio dal thread in cui si è verificato un panic).
        // Ciò è accettabile e sicuro poiché Shell_NotifyIconW opera a livello del sottosistema Shell
        // tramite messaggi verso la finestra della taskbar di sistema e non accede alla memoria interna
        // o alle strutture thread-local del message loop; inoltre, trattandosi di una pulizia best-effort
        // durante la terminazione del processo, l'eventuale mancato recapito viene gestito safe senza corruzione.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        }
        TRAY_HWND.store(0, std::sync::atomic::Ordering::SeqCst);
    }
}

pub const ID_TRAY_TOGGLE_PAUSE: u32 = 1001;
pub const ID_TRAY_CONFIG: u32 = 1002;
pub const ID_TRAY_RELOAD: u32 = 1003;
pub const ID_TRAY_EXIT: u32 = 1004;
pub const ID_TRAY_AUTOSTART: u32 = 1005;
pub const ID_TRAY_UPDATE: u32 = 1006;
pub const ID_TRAY_GROUP_BASE: u32 = 2000;

/// Evento di interazione dell'utente con la tray icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// Richiesta apertura menu contestuale (click destro del mouse o Shift+F10 / tasto Applicazione).
    ContextMenu,
    /// Doppio click sinistro del mouse (solo sospensione).
    DoubleClick,
}

/// Azione selezionata dall'utente nel menu contestuale della tray bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayMenuAction {
    TogglePause,
    ToggleGroup(String),
    OpenConfig,
    ReloadConfig,
    ToggleAutostart,
    OpenUpdate,
    Exit,
}

/// Interpreta in modo puro i codici dei messaggi Win32 associati alla tray icon.
///
/// Con `NOTIFYICON_VERSION_4`:
/// - `WM_CONTEXTMENU`: Apertura del menu contestuale (tasto destro del mouse, Shift+F10 o tasto Menu).
/// - `WM_LBUTTONDBLCLK`: Doppio click sinistro (solo sospensione).
/// - Ogni altro evento (ad es. `WM_RBUTTONUP`, `NIN_SELECT`, ecc.) restituisce `None`.
pub fn parse_tray_event(msg: u32, lparam: usize) -> Option<TrayEvent> {
    if msg != WM_TRAY_CALLBACK {
        return None;
    }

    let code = (lparam & 0xFFFF) as u32;
    match code {
        WM_CONTEXTMENU => Some(TrayEvent::ContextMenu),
        WM_LBUTTONDBLCLK => Some(TrayEvent::DoubleClick),
        _ => None,
    }
}

/// Lingua usata dal daemon per tooltip e voci del menu tray (allineata a `preferences.toml`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrayLanguage {
    #[default]
    English,
    Italian,
    French,
    German,
    Spanish,
}

impl TrayLanguage {
    /// Converte il codice lingua persistito da `rustbind-ui` (`en`, `it`, `fr`, `de`, `es`).
    pub fn from_code(code: &str) -> Option<Self> {
        match code.trim().to_ascii_lowercase().as_str() {
            "en" => Some(Self::English),
            "it" => Some(Self::Italian),
            "fr" => Some(Self::French),
            "de" => Some(Self::German),
            "es" => Some(Self::Spanish),
            _ => None,
        }
    }
}

/// Estrae la lingua salvata dal contenuto TOML di `preferences.toml` (default: Inglese).
pub fn parse_language_from_preferences_toml(content: &str) -> TrayLanguage {
    let Ok(doc) = toml::from_str::<toml::Value>(content) else {
        return TrayLanguage::default();
    };
    doc.get("language")
        .and_then(toml::Value::as_str)
        .and_then(TrayLanguage::from_code)
        .unwrap_or_default()
}

/// Legge la lingua preferita dall'utente in `%APPDATA%\RustBind\preferences.toml`.
pub fn read_tray_language_from_app_data() -> TrayLanguage {
    let Ok(app_data_dir) = ConfigManager::default_app_data_dir() else {
        return TrayLanguage::default();
    };
    let prefs_path = app_data_dir.join("preferences.toml");
    let Ok(content) = std::fs::read_to_string(prefs_path) else {
        return TrayLanguage::default();
    };
    parse_language_from_preferences_toml(&content)
}

struct TrayMenuStrings {
    suspend: &'static str,
    resume: &'static str,
    paused_error: &'static str,
    active_groups: &'static str,
    configure: &'static str,
    reload_rules: &'static str,
    autostart: &'static str,
    update_available: &'static str,
    exit: &'static str,
}

fn tray_menu_strings(lang: TrayLanguage) -> TrayMenuStrings {
    match lang {
        TrayLanguage::English => TrayMenuStrings {
            suspend: "Suspend",
            resume: "Resume",
            paused_error: "Paused (Error)",
            active_groups: "Active groups",
            configure: "Configure...",
            reload_rules: "Reload Rules",
            autostart: "Start when Windows signs in",
            update_available: "Update available",
            exit: "Exit",
        },
        TrayLanguage::Italian => TrayMenuStrings {
            suspend: "Sospendi",
            resume: "Riprendi",
            paused_error: "Sospeso (In errore)",
            active_groups: "Gruppi attivi",
            configure: "Configura...",
            reload_rules: "Ricarica Regole",
            autostart: "Avvia all'accesso a Windows",
            update_available: "Aggiornamento disponibile",
            exit: "Esci",
        },
        TrayLanguage::French => TrayMenuStrings {
            suspend: "Suspendre",
            resume: "Reprendre",
            paused_error: "Suspendu (Erreur)",
            active_groups: "Groupes actifs",
            configure: "Configurer...",
            reload_rules: "Recharger les règles",
            autostart: "Démarrer avec Windows",
            update_available: "Mise à jour disponible",
            exit: "Quitter",
        },
        TrayLanguage::German => TrayMenuStrings {
            suspend: "Pausieren",
            resume: "Fortsetzen",
            paused_error: "Pausiert (Fehler)",
            active_groups: "Aktive Gruppen",
            configure: "Konfigurieren...",
            reload_rules: "Regeln neu laden",
            autostart: "Beim Windows-Start ausführen",
            update_available: "Update verfügbar",
            exit: "Beenden",
        },
        TrayLanguage::Spanish => TrayMenuStrings {
            suspend: "Suspender",
            resume: "Reanudar",
            paused_error: "Suspendido (Error)",
            active_groups: "Grupos activos",
            configure: "Configurar...",
            reload_rules: "Recargar reglas",
            autostart: "Iniciar con Windows",
            update_available: "Actualización disponible",
            exit: "Salir",
        },
    }
}

fn to_wide_null(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Restituisce il testo del tooltip localizzato associato a ciascun stato di servizio.
pub fn state_to_tooltip_localized(state: &ServiceState, lang: TrayLanguage) -> &'static str {
    match (lang, state) {
        (TrayLanguage::English, ServiceState::Active) => "RustBind - Active",
        (TrayLanguage::English, ServiceState::Paused) => "RustBind - Paused",
        (TrayLanguage::English, ServiceState::Error(_)) => "RustBind - Error (No rules running)",
        (TrayLanguage::Italian, ServiceState::Active) => "RustBind - Attivo",
        (TrayLanguage::Italian, ServiceState::Paused) => "RustBind - Sospeso (In pausa)",
        (TrayLanguage::Italian, ServiceState::Error(_)) => {
            "RustBind - Errore (Nessuna regola in esecuzione)"
        }
        (TrayLanguage::French, ServiceState::Active) => "RustBind - Actif",
        (TrayLanguage::French, ServiceState::Paused) => "RustBind - Suspendu (En pause)",
        (TrayLanguage::French, ServiceState::Error(_)) => "RustBind - Erreur (Aucune règle active)",
        (TrayLanguage::German, ServiceState::Active) => "RustBind - Aktiv",
        (TrayLanguage::German, ServiceState::Paused) => "RustBind - Pausiert",
        (TrayLanguage::German, ServiceState::Error(_)) => "RustBind - Fehler (Keine Regeln aktiv)",
        (TrayLanguage::Spanish, ServiceState::Active) => "RustBind - Activo",
        (TrayLanguage::Spanish, ServiceState::Paused) => "RustBind - En pausa",
        (TrayLanguage::Spanish, ServiceState::Error(_)) => "RustBind - Error (Sin reglas activas)",
    }
}

/// Restituisce il testo del tooltip associato a ciascun stato di servizio (lingua predefinita).
pub fn state_to_tooltip(state: &ServiceState) -> &'static str {
    state_to_tooltip_localized(state, TrayLanguage::default())
}

/// Genera la maschera di bit per l'icona a 16x16 a runtime con forma geometrica specifica.
///
/// Restituisce la coppia `(and_mask, xor_mask)`:
/// - AND mask: 32 byte (1 bit/pixel, 0 = opaco, 1 = trasparente).
/// - XOR mask: 1024 byte (32 bpp BGRA a colori).
pub fn generate_icon_bitmaps(state: &ServiceState) -> ([u8; 32], [u8; 1024]) {
    let mut and_mask = [0xFFu8; 32]; // Di base tutto trasparente (bit = 1)
    let mut xor_mask = [0x00u8; 1024];

    for y in 0..16 {
        for x in 0..16 {
            let is_pixel_inside = match state {
                // 1. Stato Attivo: Cerchio pieno (disco)
                ServiceState::Active => {
                    let dx = x as f32 - 7.5;
                    let dy = y as f32 - 7.5;
                    (dx * dx + dy * dy) <= 30.25 // raggio ~5.5 px
                }
                // 2. Stato Pausa: Doppia barra verticale (simbolo di Pausa)
                ServiceState::Paused => {
                    (3..=12).contains(&y) && ((3..=6).contains(&x) || (9..=12).contains(&x))
                }
                // 3. Stato Errore: Croce diagonale (X)
                ServiceState::Error(_) => {
                    (2..=13).contains(&y)
                        && (2..=13).contains(&x)
                        && ((x as i32 - y as i32).abs() <= 1
                            || (x as i32 - (15 - y as i32)).abs() <= 1)
                }
            };

            if is_pixel_inside {
                // Imposta il bit dell'AND mask a 0 (opaco)
                let byte_idx = y * 2 + (x / 8);
                let bit_idx = 7 - (x % 8);
                and_mask[byte_idx] &= !(1 << bit_idx);

                // Colore del pixel (BGRA)
                let pixel_idx = (y * 16 + x) * 4;
                match state {
                    ServiceState::Active => {
                        // LimeGreen (B=50, G=205, R=50, A=255)
                        xor_mask[pixel_idx] = 50;
                        xor_mask[pixel_idx + 1] = 205;
                        xor_mask[pixel_idx + 2] = 50;
                        xor_mask[pixel_idx + 3] = 255;
                    }
                    ServiceState::Paused => {
                        // Orange (B=0, G=165, R=255, A=255)
                        xor_mask[pixel_idx] = 0;
                        xor_mask[pixel_idx + 1] = 165;
                        xor_mask[pixel_idx + 2] = 255;
                        xor_mask[pixel_idx + 3] = 255;
                    }
                    ServiceState::Error(_) => {
                        // Bright Red (B=30, G=30, R=240, A=255)
                        xor_mask[pixel_idx] = 30;
                        xor_mask[pixel_idx + 1] = 30;
                        xor_mask[pixel_idx + 2] = 240;
                        xor_mask[pixel_idx + 3] = 255;
                    }
                }
            }
        }
    }

    (and_mask, xor_mask)
}

/// Crea a runtime un `SafeHicon` a 32 bpp a partire dalla maschera di bit della forma.
pub fn create_runtime_icon(state: &ServiceState) -> Result<SafeHicon, PlatformError> {
    let (and_mask, xor_mask) = generate_icon_bitmaps(state);

    // SAFETY: Chiamata ufficiale a `CreateIcon` con array validi di dimensioni 16x16 a 32 bpp.
    let hicon = unsafe {
        CreateIcon(
            None,
            16,
            16,
            1,  // 1 plane
            32, // 32 bits per pixel (BGRA)
            and_mask.as_ptr(),
            xor_mask.as_ptr(),
        )
    }
    .map_err(|e| PlatformError::TrayError(format!("CreateIcon fallito: {}", e)))?;

    Ok(SafeHicon::new(hicon))
}

/// Gestore Win32 della Tray Icon nella barra delle applicazioni (`Shell_NotifyIconW`).
///
/// Responsabilità:
/// - Mantenere l'icona costantemente visibile nella barra (nessuna opzione per nasconderla).
/// - Riflettere lo stato reale con forma geometrica diversa (accessibilità) e tooltip informativo.
/// - Ricreare l'icona su ricezione di `TaskbarCreated` in caso di riavvio di Windows Explorer.
/// - Gestire il menu contestuale nativo.
pub struct TrayManager {
    hwnd: HWND,
    icon_active: SafeHicon,
    icon_paused: SafeHicon,
    icon_error: SafeHicon,
    icon_added: bool,
    language: TrayLanguage,
    _not_send: PhantomData<*const ()>,
}

// Verifica che TrayManager sia !Send e !Sync per l'affinità con il message loop.
static_assertions::assert_not_impl_any!(TrayManager: Send, Sync);

impl TrayManager {
    /// Inizializza il gestore generando a runtime le tre icone geometriche.
    pub fn new(hwnd: HWND) -> Result<Self, PlatformError> {
        let icon_active = create_runtime_icon(&ServiceState::Active)?;
        let icon_paused = create_runtime_icon(&ServiceState::Paused)?;
        let icon_error = create_runtime_icon(&ServiceState::Error(String::new()))?;

        Ok(Self {
            hwnd,
            icon_active,
            icon_paused,
            icon_error,
            icon_added: false,
            language: read_tray_language_from_app_data(),
            _not_send: PhantomData,
        })
    }

    fn select_hicon(&self, state: &ServiceState) -> HICON {
        match state {
            ServiceState::Active => self.icon_active.raw(),
            ServiceState::Paused => self.icon_paused.raw(),
            ServiceState::Error(_) => self.icon_error.raw(),
        }
    }

    fn create_nid(&self, state: &ServiceState, flags: NOTIFY_ICON_DATA_FLAGS) -> NOTIFYICONDATAW {
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: TRAY_ICON_UID,
            uFlags: flags,
            uCallbackMessage: WM_TRAY_CALLBACK,
            hIcon: self.select_hicon(state),
            szTip: [0; 128],
            ..Default::default()
        };

        let tooltip = state_to_tooltip_localized(state, self.language);
        let units: Vec<u16> = tooltip.encode_utf16().take(127).collect();
        for (i, &u) in units.iter().enumerate() {
            nid.szTip[i] = u;
        }
        nid.szTip[units.len()] = 0;

        nid
    }

    /// Registra l'icona nella barra di sistema (`NIM_ADD`) e configura `NOTIFYICON_VERSION_4`.
    pub fn add_icon(&mut self, state: &ServiceState) -> Result<(), PlatformError> {
        self.language = read_tray_language_from_app_data();
        let nid = self.create_nid(state, NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP);

        // SAFETY: Registrazione documentata con Shell_NotifyIconW.
        let success = unsafe { Shell_NotifyIconW(NIM_ADD, &nid) };
        if !success.as_bool() {
            return Err(PlatformError::TrayError(
                "Shell_NotifyIconW(NIM_ADD) fallito".to_string(),
            ));
        }

        // SAFETY: Imposta la versione NOTIFYICON_VERSION_4 per abilitare il routing moderno
        // dei messaggi (WM_CONTEXTMENU) e la gestione standard della tray bar.
        let mut version_nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: TRAY_ICON_UID,
            ..Default::default()
        };
        version_nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        let version_ok = unsafe { Shell_NotifyIconW(NIM_SETVERSION, &version_nid) };
        if !version_ok.as_bool() {
            tracing::warn!("Shell_NotifyIconW(NIM_SETVERSION, NOTIFYICON_VERSION_4) non riuscito");
        }

        self.icon_added = true;
        TRAY_HWND.store(self.hwnd.0 as isize, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    /// Aggiorna l'icona e il tooltip per riflettere un cambio di stato del servizio (`NIM_MODIFY`).
    pub fn update_state(&mut self, state: &ServiceState) -> Result<(), PlatformError> {
        self.language = read_tray_language_from_app_data();
        if !self.icon_added {
            return self.add_icon(state);
        }

        let nid = self.create_nid(state, NIF_ICON | NIF_TIP | NIF_SHOWTIP);

        // SAFETY: Aggiornamento documentato con Shell_NotifyIconW.
        let success = unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
        if !success.as_bool() {
            return Err(PlatformError::TrayError(
                "Shell_NotifyIconW(NIM_MODIFY) fallito".to_string(),
            ));
        }

        Ok(())
    }

    /// Rimuove l'icona dalla barra di sistema (`NIM_DELETE`).
    pub fn remove_icon(&mut self) -> Result<(), PlatformError> {
        if !self.icon_added {
            return Ok(());
        }

        let nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: TRAY_ICON_UID,
            ..Default::default()
        };

        // SAFETY: Rimozione documentata dell'icona dalla tray bar.
        let success = unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
        self.icon_added = false;
        TRAY_HWND.store(0, std::sync::atomic::Ordering::SeqCst);

        if !success.as_bool() {
            return Err(PlatformError::TrayError(
                "Shell_NotifyIconW(NIM_DELETE) fallito".to_string(),
            ));
        }

        Ok(())
    }

    /// Ripristina l'icona quando Explorer invia il messaggio broadcast `TaskbarCreated`.
    pub fn on_taskbar_created(&mut self, state: &ServiceState) -> Result<(), PlatformError> {
        self.icon_added = false;
        self.add_icon(state)
    }

    /// Mostra il menu contestuale nativo Win32 e restituisce l'azione selezionata.
    pub fn show_context_menu(
        &mut self,
        state: &ServiceState,
        autostart_enabled: bool,
        update_available: bool,
        group_items: &[(String, bool)],
    ) -> Option<TrayMenuAction> {
        let latest_lang = read_tray_language_from_app_data();
        if latest_lang != self.language {
            self.language = latest_lang;
            let _ = self.update_state(state);
        }
        let strings = tray_menu_strings(self.language);

        // SAFETY: Creazione del menu contestuale nativo.
        let menu = unsafe { CreatePopupMenu() }.ok()?;

        let toggle_w = to_wide_null(match state {
            ServiceState::Active => strings.suspend,
            ServiceState::Paused => strings.resume,
            ServiceState::Error(_) => strings.paused_error,
        });
        let groups_w = to_wide_null(strings.active_groups);
        let config_w = to_wide_null(strings.configure);
        let reload_w = to_wide_null(strings.reload_rules);
        let autostart_w = to_wide_null(strings.autostart);
        let update_w = to_wide_null(strings.update_available);
        let exit_w = to_wide_null(strings.exit);

        let autostart_flags = MF_STRING
            | if autostart_enabled {
                MF_CHECKED
            } else {
                MF_UNCHECKED
            };

        let visible_groups: Vec<&(String, bool)> = group_items.iter().take(1000).collect();

        unsafe {
            // Voce 1: Sospendi / Riprendi
            let _ = AppendMenuW(
                menu,
                MF_STRING,
                ID_TRAY_TOGGLE_PAUSE as usize,
                PCWSTR(toggle_w.as_ptr()),
            );

            // Voce 2: Sottomenu Gruppi attivi (disabilitato se in Error / nessun gruppo disponibile)
            if visible_groups.is_empty() {
                let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, PCWSTR(groups_w.as_ptr()));
            } else if let Ok(sub_menu) = CreatePopupMenu() {
                for (idx, (group_name, active)) in visible_groups.iter().enumerate() {
                    let group_w = to_wide_null(group_name);
                    let flags = MF_STRING | if *active { MF_CHECKED } else { MF_UNCHECKED };
                    let _ = AppendMenuW(
                        sub_menu,
                        flags,
                        (ID_TRAY_GROUP_BASE as usize) + idx,
                        PCWSTR(group_w.as_ptr()),
                    );
                }
                if AppendMenuW(
                    menu,
                    MF_STRING | MF_POPUP,
                    sub_menu.0 as usize,
                    PCWSTR(groups_w.as_ptr()),
                )
                .is_err()
                {
                    let _ = DestroyMenu(sub_menu);
                }
            }

            // Voce 3: Configura... (avvia il compagno UI separato rustbind-ui.exe)
            let _ = AppendMenuW(
                menu,
                MF_STRING,
                ID_TRAY_CONFIG as usize,
                PCWSTR(config_w.as_ptr()),
            );

            // Voce 4: Ricarica Regole
            let _ = AppendMenuW(
                menu,
                MF_STRING,
                ID_TRAY_RELOAD as usize,
                PCWSTR(reload_w.as_ptr()),
            );

            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);

            // Voce 5: Avvio automatico utente (HKCU\...\Run)
            let _ = AppendMenuW(
                menu,
                autostart_flags,
                ID_TRAY_AUTOSTART as usize,
                PCWSTR(autostart_w.as_ptr()),
            );

            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);

            // Voce opzionale sopra ad Esci: Aggiornamento disponibile
            if update_available {
                let _ = AppendMenuW(
                    menu,
                    MF_STRING,
                    ID_TRAY_UPDATE as usize,
                    PCWSTR(update_w.as_ptr()),
                );
            }

            // Voce finale: Esci
            let _ = AppendMenuW(
                menu,
                MF_STRING,
                ID_TRAY_EXIT as usize,
                PCWSTR(exit_w.as_ptr()),
            );

            // Obbligatorio per Windows affinché il menu si chiuda cliccando altrove.
            // SAFETY: SetForegroundWindow porta il focus sulla finestra proprietaria del menu.
            let fg_ok = SetForegroundWindow(self.hwnd);
            if !fg_ok.as_bool() {
                tracing::warn!("SetForegroundWindow per il menu tray ha restituito false");
            }

            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);

            let cmd = TrackPopupMenuEx(
                menu,
                (TPM_RETURNCMD | TPM_NONOTIFY | TPM_BOTTOMALIGN | TPM_RIGHTALIGN).0,
                pt.x,
                pt.y,
                self.hwnd,
                None,
            );

            // SAFETY: Risolve il bug documentato di Windows (MSDN KB135788): dopo TrackPopupMenuEx
            // è necessario inviare un messaggio WM_NULL alla finestra per rilasciare correttamente il focus.
            if let Err(err) = PostMessageW(Some(self.hwnd), WM_NULL, WPARAM(0), LPARAM(0)) {
                tracing::warn!(error = %err, "PostMessageW(WM_NULL) fallito dopo TrackPopupMenuEx");
            }

            let _ = DestroyMenu(menu);

            let cmd_id = cmd.0 as u32;
            match cmd_id {
                ID_TRAY_TOGGLE_PAUSE => Some(TrayMenuAction::TogglePause),
                ID_TRAY_CONFIG => Some(TrayMenuAction::OpenConfig),
                ID_TRAY_RELOAD => Some(TrayMenuAction::ReloadConfig),
                ID_TRAY_AUTOSTART => Some(TrayMenuAction::ToggleAutostart),
                ID_TRAY_UPDATE => Some(TrayMenuAction::OpenUpdate),
                ID_TRAY_EXIT => Some(TrayMenuAction::Exit),
                id if id >= ID_TRAY_GROUP_BASE => visible_groups
                    .get((id - ID_TRAY_GROUP_BASE) as usize)
                    .map(|(name, _)| TrayMenuAction::ToggleGroup(name.clone())),
                _ => None,
            }
        }
    }
}

impl Drop for TrayManager {
    fn drop(&mut self) {
        let _ = self.remove_icon();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::Shell::NIN_SELECT;
    use windows::Win32::UI::WindowsAndMessaging::WM_RBUTTONUP;

    #[test]
    fn test_parse_tray_event_different_msg_returns_none() {
        assert_eq!(parse_tray_event(WM_NULL, WM_CONTEXTMENU as usize), None);
        assert_eq!(parse_tray_event(0, 0), None);
    }

    #[test]
    fn test_parse_tray_event_context_menu() {
        // Puro WM_CONTEXTMENU (versione 4)
        assert_eq!(
            parse_tray_event(WM_TRAY_CALLBACK, WM_CONTEXTMENU as usize),
            Some(TrayEvent::ContextMenu)
        );
        // Con uID (100) nei 16 bit alti (HIWORD)
        let lparam_with_uid = ((TRAY_ICON_UID as usize) << 16) | (WM_CONTEXTMENU as usize);
        assert_eq!(
            parse_tray_event(WM_TRAY_CALLBACK, lparam_with_uid),
            Some(TrayEvent::ContextMenu)
        );
    }

    #[test]
    fn test_parse_tray_event_legacy_rbuttonup_ignored() {
        // WM_RBUTTONUP deve essere ignorato per evitare che click destro accodi due volte l'apertura del menu
        assert_eq!(
            parse_tray_event(WM_TRAY_CALLBACK, WM_RBUTTONUP as usize),
            None
        );
    }

    #[test]
    fn test_parse_tray_event_double_click() {
        assert_eq!(
            parse_tray_event(WM_TRAY_CALLBACK, WM_LBUTTONDBLCLK as usize),
            Some(TrayEvent::DoubleClick)
        );
    }

    #[test]
    fn test_parse_tray_event_non_context_menu_codes() {
        // WM_CONTEXTMENU apre il menu contestuale
        assert_eq!(
            parse_tray_event(WM_TRAY_CALLBACK, WM_CONTEXTMENU as usize),
            Some(TrayEvent::ContextMenu)
        );
        // WM_LBUTTONDBLCLK genera DoubleClick come previsto dalla logica attuale
        assert_eq!(
            parse_tray_event(WM_TRAY_CALLBACK, WM_LBUTTONDBLCLK as usize),
            Some(TrayEvent::DoubleClick)
        );
        // Codici non gestiti restituiscono None (click singolo NIN_SELECT, WM_RBUTTONUP, codice 0x0401, arbitrario 0x1234)
        assert_eq!(
            parse_tray_event(WM_TRAY_CALLBACK, NIN_SELECT as usize),
            None
        );
        assert_eq!(
            parse_tray_event(WM_TRAY_CALLBACK, WM_RBUTTONUP as usize),
            None
        );
        assert_eq!(parse_tray_event(WM_TRAY_CALLBACK, 0x0401), None);
        assert_eq!(parse_tray_event(WM_TRAY_CALLBACK, 0x1234), None);
    }

    #[test]
    fn test_parse_tray_event_unknown_code_returns_none() {
        assert_eq!(parse_tray_event(WM_TRAY_CALLBACK, 0xFFFF), None);
    }

    #[test]
    fn test_state_to_tooltip_mapping() {
        assert_eq!(state_to_tooltip(&ServiceState::Active), "RustBind - Active");
        assert_eq!(state_to_tooltip(&ServiceState::Paused), "RustBind - Paused");
        assert_eq!(
            state_to_tooltip(&ServiceState::Error("config err".to_string())),
            "RustBind - Error (No rules running)"
        );

        assert_eq!(
            state_to_tooltip_localized(&ServiceState::Active, TrayLanguage::Italian),
            "RustBind - Attivo"
        );
        assert_eq!(
            state_to_tooltip_localized(&ServiceState::Paused, TrayLanguage::Italian),
            "RustBind - Sospeso (In pausa)"
        );
        assert_eq!(
            state_to_tooltip_localized(
                &ServiceState::Error("config err".to_string()),
                TrayLanguage::Italian
            ),
            "RustBind - Errore (Nessuna regola in esecuzione)"
        );
    }

    #[test]
    fn test_parse_language_from_preferences_toml_and_menu_strings() {
        assert_eq!(
            parse_language_from_preferences_toml(""),
            TrayLanguage::English
        );
        assert_eq!(
            parse_language_from_preferences_toml(
                "theme = \"rustbind_classic\"\nlanguage = \"it\"\ntrue_black = false\n"
            ),
            TrayLanguage::Italian
        );
        assert_eq!(
            parse_language_from_preferences_toml("language = \"de\"\n"),
            TrayLanguage::German
        );
        assert_eq!(
            parse_language_from_preferences_toml("language = \"xx\"\n"),
            TrayLanguage::English
        );
        assert_eq!(
            parse_language_from_preferences_toml("not valid toml ===="),
            TrayLanguage::English
        );

        for lang in [
            TrayLanguage::English,
            TrayLanguage::Italian,
            TrayLanguage::French,
            TrayLanguage::German,
            TrayLanguage::Spanish,
        ] {
            let m = tray_menu_strings(lang);
            assert!(!m.suspend.is_empty());
            assert!(!m.configure.is_empty());
            assert!(!m.exit.is_empty());
        }
    }

    #[test]
    fn test_icon_bitmaps_geometric_shapes_are_distinct() {
        let (and_active, xor_active) = generate_icon_bitmaps(&ServiceState::Active);
        let (and_paused, xor_paused) = generate_icon_bitmaps(&ServiceState::Paused);
        let (and_error, xor_error) = generate_icon_bitmaps(&ServiceState::Error(String::new()));

        // Le maschere geometriche devono essere diverse tra tutti e tre gli stati
        assert_ne!(and_active, and_paused);
        assert_ne!(and_active, and_error);
        assert_ne!(and_paused, and_error);

        assert_ne!(xor_active, xor_paused);
        assert_ne!(xor_active, xor_error);
        assert_ne!(xor_paused, xor_error);
    }

    #[test]
    #[ignore = "Richiede interazione con il sistema Windows reale"]
    fn test_runtime_icon_creation_real() {
        let icon_active = create_runtime_icon(&ServiceState::Active);
        assert!(icon_active.is_ok(), "Creazione icona Active riuscita");

        let icon_paused = create_runtime_icon(&ServiceState::Paused);
        assert!(icon_paused.is_ok(), "Creazione icona Paused riuscita");

        let icon_error = create_runtime_icon(&ServiceState::Error(String::new()));
        assert!(icon_error.is_ok(), "Creazione icona Error riuscita");
    }
}
