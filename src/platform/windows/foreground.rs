use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::OsString;
use std::marker::PhantomData;
use std::os::windows::ffi::OsStringExt;
use std::path::Path;

use windows::Win32::Foundation::{ERROR_NO_MORE_ITEMS, HWND, LPARAM, WIN32_ERROR, WPARAM};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, REG_EXPAND_SZ, REG_SZ, REG_VALUE_TYPE,
    RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_SYSTEM_FOREGROUND, EnumWindows, GetForegroundWindow, GetWindowThreadProcessId,
    IsWindowVisible, PostMessageW, WINEVENT_OUTOFCONTEXT,
};
use windows::core::{BOOL, PCWSTR, PWSTR, w};

use crate::core::rule::ForegroundWindow;
use crate::platform::windows::handles::SafeHandle;
use crate::platform::windows::message_loop::{WM_WAKEUP, enqueue_foreground_changed_coalesced};

thread_local! {
    static WATCH_HWND: Cell<isize> = const { Cell::new(0) };
}

/// Voce deduplicata di applicazione installata o attiva per il selettore UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledAppEntry {
    pub display_name: Option<String>,
    pub process_name: String,
}

impl InstalledAppEntry {
    /// Restituisce un'etichetta leggibile (es. `"Brave (brave.exe)"` o `"notepad.exe"`).
    pub fn display_label(&self) -> String {
        match &self.display_name {
            Some(name) if !name.eq_ignore_ascii_case(&self.process_name) => {
                format!("{name} ({})", self.process_name)
            }
            _ => self.process_name.clone(),
        }
    }
}

struct SafeHkey(HKEY);

impl SafeHkey {
    fn raw(&self) -> HKEY {
        self.0
    }
}

impl Drop for SafeHkey {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: L'handle HKEY è stato aperto con RegOpenKeyExW e viene chiuso esattamente una volta.
            unsafe {
                let _ = RegCloseKey(self.0);
            }
        }
    }
}

unsafe extern "system" fn foreground_win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    if event != EVENT_SYSTEM_FOREGROUND || hwnd.is_invalid() || hwnd.0.is_null() {
        return;
    }

    let raw_watch = WATCH_HWND.with(|w| w.get());
    if raw_watch == 0 {
        return;
    }

    if enqueue_foreground_changed_coalesced() {
        let target_hwnd = HWND(raw_watch as *mut core::ffi::c_void);
        // SAFETY: Notifica asincrona WM_WAKEUP alla finestra del message loop proprietaria sullo stesso thread.
        unsafe {
            let _ = PostMessageW(Some(target_hwnd), WM_WAKEUP, WPARAM(0), LPARAM(0));
        }
    }
}

/// Osservatore Win32 out-of-context per i cambi di finestra in primo piano.
///
/// Utilizza esclusivamente `SetWinEventHook(EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT)`
/// (nessuna DLL injection né hook tastiera/input) e rispetta la thread-affinity Win32 (`!Send` e `!Sync`).
pub struct ForegroundWatcher {
    fg_hook: HWINEVENTHOOK,
    _not_send_sync: PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(ForegroundWatcher: Send, Sync);

impl ForegroundWatcher {
    /// Installa l'hook out-of-context associandolo alla finestra del message loop corrente.
    pub fn new(wake_hwnd: HWND) -> Self {
        WATCH_HWND.with(|w| w.set(wake_hwnd.0 as isize));

        // SAFETY: Hook WinEvent out-of-context ristretto al solo EVENT_SYSTEM_FOREGROUND sul thread del message loop.
        let fg_hook = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(foreground_win_event_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };
        if fg_hook.is_invalid() {
            tracing::warn!("Installazione hook EVENT_SYSTEM_FOREGROUND fallita");
        }

        Self {
            fg_hook,
            _not_send_sync: PhantomData,
        }
    }
}

impl Drop for ForegroundWatcher {
    fn drop(&mut self) {
        WATCH_HWND.with(|w| w.set(0));
        if !self.fg_hook.is_invalid() {
            // SAFETY: Rimozione dell'hook out-of-context registrato sullo stesso thread.
            unsafe {
                let _ = UnhookWinEvent(self.fg_hook);
            }
        }
    }
}

fn process_name_from_hwnd(hwnd: HWND) -> Option<String> {
    let mut pid: u32 = 0;
    // SAFETY: Estrae il PID proprietario dell'handle finestra.
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    if pid == 0 {
        return None;
    }

    // SAFETY: Apre il processo con il solo permesso minimo PROCESS_QUERY_LIMITED_INFORMATION
    // (esclude lettura/scrittura memoria di altri processi) gestito tramite wrapper RAII SafeHandle.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let safe_handle = SafeHandle::new(handle);

    let mut buf = vec![0u16; 1024];
    let mut size = buf.len() as u32;
    // SAFETY: QueryFullProcessImageNameW riceve buffer valido e puntatore alla dimensione disponibile.
    unsafe {
        QueryFullProcessImageNameW(
            safe_handle.raw(),
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        )
        .ok()?;
    }

    if size == 0 {
        return None;
    }
    let full_path = OsString::from_wide(&buf[..size as usize]);
    let file_name = Path::new(&full_path).file_name()?;
    let text = file_name.to_string_lossy().trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

/// Rileva il solo nome eseguibile del processo della finestra attualmente in primo piano.
pub fn query_foreground_window() -> ForegroundWindow {
    // SAFETY: Lettura ufficiale dell'HWND della finestra attualmente in primo piano.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() || hwnd.0.is_null() {
        return ForegroundWindow::default();
    }

    ForegroundWindow {
        process_name: process_name_from_hwnd(hwnd),
    }
}

fn is_noise_or_self_exe(exe_name: &str) -> bool {
    let low = exe_name.trim().to_ascii_lowercase();
    if low.is_empty() || !low.ends_with(".exe") {
        return true;
    }
    low.starts_with("unins")
        || low.starts_with("uninstall")
        || low.starts_with("setup")
        || low.starts_with("update")
        || low == "install.exe"
        || low == "msiexec.exe"
        || low.contains("webview2")
        || low == "rustbind-ui.exe"
        || low == "rustbind.exe"
        || low == "rustbindctl.exe"
}

fn extract_exe_filename_from_icon_or_path(raw: &str) -> Option<String> {
    let before_comma = raw.split(',').next()?.trim().trim_matches('"').trim();
    if before_comma.is_empty() {
        return None;
    }
    let lower = before_comma.to_ascii_lowercase();
    let exe_end = lower.find(".exe")? + 4;
    let exe_slice = before_comma[..exe_end].trim_matches('"').trim();
    let file_name = Path::new(exe_slice)
        .file_name()?
        .to_string_lossy()
        .trim()
        .to_string();
    if is_noise_or_self_exe(&file_name) {
        None
    } else {
        Some(file_name)
    }
}

fn read_reg_string(hkey: HKEY, value_name: PCWSTR) -> Option<String> {
    let mut reg_type = REG_VALUE_TYPE::default();
    let mut byte_len = 0u32;
    // SAFETY: Prima chiamata di misura su HKEY aperto.
    let measure_err = unsafe {
        RegQueryValueExW(
            hkey,
            value_name,
            None,
            Some(&mut reg_type),
            None,
            Some(&mut byte_len),
        )
    };
    if measure_err != WIN32_ERROR(0)
        || (reg_type != REG_SZ && reg_type != REG_EXPAND_SZ)
        || !(2..=8192).contains(&byte_len)
    {
        return None;
    }

    let word_len = (byte_len as usize).div_ceil(2);
    let mut buf = vec![0u16; word_len];
    let mut actual_bytes = byte_len;
    // SAFETY: Buffer dimensione almeno `byte_len` byte.
    let read_err = unsafe {
        RegQueryValueExW(
            hkey,
            value_name,
            None,
            Some(&mut reg_type),
            Some(buf.as_mut_ptr() as *mut u8),
            Some(&mut actual_bytes),
        )
    };
    if read_err != WIN32_ERROR(0) {
        return None;
    }
    let used_words = (actual_bytes as usize / 2).min(buf.len());
    let nul_pos = buf[..used_words]
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(used_words);
    let text = String::from_utf16_lossy(&buf[..nul_pos]).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn insert_dedup_app(
    map: &mut HashMap<String, InstalledAppEntry>,
    process_name: String,
    display_name: Option<String>,
) {
    if is_noise_or_self_exe(&process_name) {
        return;
    }
    let key = process_name.to_ascii_lowercase();
    let clean_display = display_name.and_then(|d| {
        let t = d.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
    });
    match map.get_mut(&key) {
        Some(existing) => {
            if existing.display_name.is_none() && clean_display.is_some() {
                existing.display_name = clean_display;
            }
        }
        None => {
            map.insert(
                key,
                InstalledAppEntry {
                    display_name: clean_display,
                    process_name,
                },
            );
        }
    }
}

fn collect_app_paths_from_root(
    root: HKEY,
    subkey: PCWSTR,
    map: &mut HashMap<String, InstalledAppEntry>,
) {
    let mut hkey = HKEY::default();
    // SAFETY: Apertura in sola lettura di `App Paths`.
    let open_err = unsafe { RegOpenKeyExW(root, subkey, Some(0), KEY_READ, &mut hkey) };
    if open_err != WIN32_ERROR(0) {
        return;
    }
    let safe_root = SafeHkey(hkey);

    let mut idx = 0u32;
    let mut name_buf = vec![0u16; 512];
    loop {
        let mut name_len = name_buf.len() as u32;
        // SAFETY: Enumerazione chiavi figlie di `App Paths`.
        let err = unsafe {
            RegEnumKeyExW(
                safe_root.raw(),
                idx,
                Some(PWSTR(name_buf.as_mut_ptr())),
                &mut name_len,
                None,
                None,
                None,
                None,
            )
        };
        if err == ERROR_NO_MORE_ITEMS || err != WIN32_ERROR(0) {
            break;
        }
        idx += 1;

        let exe_name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
        insert_dedup_app(map, exe_name, None);
    }
}

fn collect_uninstall_apps_from_root(
    root: HKEY,
    subkey: PCWSTR,
    map: &mut HashMap<String, InstalledAppEntry>,
) {
    let mut hkey = HKEY::default();
    // SAFETY: Apertura in sola lettura della chiave `Uninstall`.
    let open_err = unsafe { RegOpenKeyExW(root, subkey, Some(0), KEY_READ, &mut hkey) };
    if open_err != WIN32_ERROR(0) {
        return;
    }
    let safe_root = SafeHkey(hkey);

    let mut idx = 0u32;
    let mut name_buf = vec![0u16; 512];
    loop {
        let mut name_len = name_buf.len() as u32;
        // SAFETY: Enumerazione sottochiavi `Uninstall`.
        let err = unsafe {
            RegEnumKeyExW(
                safe_root.raw(),
                idx,
                Some(PWSTR(name_buf.as_mut_ptr())),
                &mut name_len,
                None,
                None,
                None,
                None,
            )
        };
        if err == ERROR_NO_MORE_ITEMS {
            break;
        }
        idx += 1;
        if err != WIN32_ERROR(0) {
            continue;
        }

        let mut child_wide = name_buf[..name_len as usize].to_vec();
        child_wide.push(0);
        let mut child_hkey = HKEY::default();
        // SAFETY: Apertura in sola lettura della singola sottochiave applicativa enumerata.
        let child_err = unsafe {
            RegOpenKeyExW(
                safe_root.raw(),
                PCWSTR(child_wide.as_ptr()),
                Some(0),
                KEY_READ,
                &mut child_hkey,
            )
        };
        if child_err != WIN32_ERROR(0) {
            continue;
        }
        let safe_child = SafeHkey(child_hkey);

        let Some(display_icon) = read_reg_string(safe_child.raw(), w!("DisplayIcon")) else {
            continue;
        };
        let Some(exe_name) = extract_exe_filename_from_icon_or_path(&display_icon) else {
            continue;
        };
        let display_name = read_reg_string(safe_child.raw(), w!("DisplayName"));
        insert_dedup_app(map, exe_name, display_name);
    }
}

unsafe extern "system" fn enum_visible_processes_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    if hwnd.is_invalid() || hwnd.0.is_null() {
        return BOOL(1);
    }
    // SAFETY: Verifica visibilità finestra tramite API ufficiale User32.
    if !unsafe { IsWindowVisible(hwnd).as_bool() } {
        return BOOL(1);
    }
    let Some(process_name) = process_name_from_hwnd(hwnd) else {
        return BOOL(1);
    };

    // SAFETY: `lparam` punta all'`HashMap` allocata sullo stack di `list_installed_applications`.
    let map = unsafe { &mut *(lparam.0 as *mut HashMap<String, InstalledAppEntry>) };
    insert_dedup_app(map, process_name, None);
    BOOL(1)
}

/// Restituisce l'elenco unico e deduplicato delle applicazioni installate (Registro Windows)
/// e dei processi desktop attivi, ordinato alfabeticamente per il selettore stile pannello NVIDIA.
pub fn list_installed_applications() -> Vec<InstalledAppEntry> {
    let mut map: HashMap<String, InstalledAppEntry> = HashMap::new();

    collect_app_paths_from_root(
        HKEY_LOCAL_MACHINE,
        w!(r"Software\Microsoft\Windows\CurrentVersion\App Paths"),
        &mut map,
    );
    collect_app_paths_from_root(
        HKEY_CURRENT_USER,
        w!(r"Software\Microsoft\Windows\CurrentVersion\App Paths"),
        &mut map,
    );

    collect_uninstall_apps_from_root(
        HKEY_LOCAL_MACHINE,
        w!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall"),
        &mut map,
    );
    collect_uninstall_apps_from_root(
        HKEY_LOCAL_MACHINE,
        w!(r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
        &mut map,
    );
    collect_uninstall_apps_from_root(
        HKEY_CURRENT_USER,
        w!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall"),
        &mut map,
    );

    // Integra eventuali app/giochi in esecuzione portatili non presenti nel Registro,
    // mantenendo una sola voce univoca per ciascun eseguibile.
    let ptr = (&mut map as *mut HashMap<String, InstalledAppEntry>) as isize;
    // SAFETY: EnumWindows invoca la callback sincrona sullo stack corrente.
    unsafe {
        let _ = EnumWindows(Some(enum_visible_processes_proc), LPARAM(ptr));
    }

    let mut entries: Vec<InstalledAppEntry> = map.into_values().collect();
    entries.sort_by(|a, b| {
        a.display_label()
            .to_lowercase()
            .cmp(&b.display_label().to_lowercase())
    });
    entries
}
