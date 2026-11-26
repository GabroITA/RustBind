use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, HANDLE, MAX_PATH, S_FALSE, S_OK};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize, IPersistFile,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    QueryFullProcessImageNameW, TerminateProcess,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::core::{Interface, PCWSTR, PWSTR};

struct SafeHandle(HANDLE);

impl Drop for SafeHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: l'handle appartiene esclusivamente a questo wrapper RAII ed e' valido fino al Drop.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

struct ComGuard {
    should_uninit: bool,
}

impl ComGuard {
    fn init() -> Result<Self, String> {
        // SAFETY: inizializza COM STA sul thread corrente di xtask per creare il collegamento ShellLink.
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if hr == S_OK || hr == S_FALSE {
            Ok(Self {
                should_uninit: true,
            })
        } else {
            Err(format!("CoInitializeEx fallito: {hr:?}"))
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.should_uninit {
            // SAFETY: bilancia esclusivamente un CoInitializeEx riuscito sullo stesso thread.
            unsafe {
                CoUninitialize();
            }
        }
    }
}

fn to_wide_null(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn to_wide_str_null(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Crea (o aggiorna) un file `.lnk` di Windows usando le interfacce COM ufficiali `IShellLinkW` e `IPersistFile`.
pub fn create_windows_shortcut(
    shortcut_path: &Path,
    target_exe: &Path,
    working_dir: &Path,
    description: &str,
) -> Result<(), String> {
    if let Some(parent) = shortcut_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Creazione directory shortcut fallita: {e}"))?;
    }

    let _com = ComGuard::init()?;

    // SAFETY: COM e' inizializzato sullo stesso thread via ComGuard; ShellLink implementa IShellLinkW.
    let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
        .map_err(|e| format!("CoCreateInstance(ShellLink) fallito: {e}"))?;

    let target_w = to_wide_null(target_exe);
    let workdir_w = to_wide_null(working_dir);
    let desc_w = to_wide_str_null(description);
    let shortcut_w = to_wide_null(shortcut_path);

    // SAFETY: i buffer wide terminano tutti con '\0' e restano validi per tutta la durata delle chiamate COM.
    unsafe {
        link.SetPath(PCWSTR(target_w.as_ptr()))
            .map_err(|e| format!("IShellLinkW::SetPath fallito: {e}"))?;
        link.SetWorkingDirectory(PCWSTR(workdir_w.as_ptr()))
            .map_err(|e| format!("IShellLinkW::SetWorkingDirectory fallito: {e}"))?;
        link.SetDescription(PCWSTR(desc_w.as_ptr()))
            .map_err(|e| format!("IShellLinkW::SetDescription fallito: {e}"))?;
        link.SetIconLocation(PCWSTR(target_w.as_ptr()), 0)
            .map_err(|e| format!("IShellLinkW::SetIconLocation fallito: {e}"))?;

        let persist: IPersistFile = link
            .cast()
            .map_err(|e| format!("Cast a IPersistFile fallito: {e}"))?;
        persist
            .Save(PCWSTR(shortcut_w.as_ptr()), true)
            .map_err(|e| format!("IPersistFile::Save fallito: {e}"))?;
    }

    Ok(())
}

/// Verifica se `rustbind.exe` o `rustbind-ui.exe` sono in esecuzione da `target_dir`;
/// con `force = true` ne richiede la terminazione prima di sovrascrivere o rimuovere i binari.
pub fn ensure_binaries_not_running(target_dir: &Path, force: bool) -> Result<(), String> {
    let normalized_dir =
        std::fs::canonicalize(target_dir).unwrap_or_else(|_| target_dir.to_path_buf());
    let mut matching_pids: Vec<u32> = Vec::new();

    // SAFETY: CreateToolhelp32Snapshot crea uno snapshot sola-lettura dei processi di sistema chiuso via SafeHandle.
    let raw_snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .map_err(|e| format!("CreateToolhelp32Snapshot fallito: {e}"))?;
    let snapshot = SafeHandle(raw_snapshot);

    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };

    // SAFETY: entry.dwSize e' inizializzato e snapshot e' un handle valido chiuso al Drop.
    let mut has_entry = unsafe { Process32FirstW(snapshot.0, &mut entry).is_ok() };
    while has_entry {
        let name_end = entry
            .szExeFile
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(entry.szExeFile.len());
        let exe_name = OsString::from_wide(&entry.szExeFile[..name_end])
            .to_string_lossy()
            .to_ascii_lowercase();

        if exe_name == "rustbind.exe" || exe_name == "rustbind-ui.exe" {
            // SAFETY: accesso in sola query al percorso del processo per verificare se appartiene a target_dir.
            if let Ok(raw_proc) = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION,
                    false,
                    entry.th32ProcessID,
                )
            } {
                let proc_handle = SafeHandle(raw_proc);
                let mut buf = vec![0u16; MAX_PATH as usize * 4];
                let mut len = buf.len() as u32;
                // SAFETY: buf e' allocato con capienza len caratteri wide.
                if unsafe {
                    QueryFullProcessImageNameW(
                        proc_handle.0,
                        PROCESS_NAME_WIN32,
                        PWSTR(buf.as_mut_ptr()),
                        &mut len,
                    )
                    .is_ok()
                } {
                    let full_path = PathBuf::from(OsString::from_wide(&buf[..len as usize]));
                    if let Some(parent) = full_path.parent() {
                        let parent_norm =
                            std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
                        if parent_norm
                            .to_string_lossy()
                            .eq_ignore_ascii_case(&normalized_dir.to_string_lossy())
                        {
                            matching_pids.push(entry.th32ProcessID);
                        }
                    }
                }
            }
        }

        // SAFETY: iterazione successiva sullo snapshot dei processi valido.
        has_entry = unsafe { Process32NextW(snapshot.0, &mut entry).is_ok() };
    }

    if matching_pids.is_empty() {
        return Ok(());
    }

    if !force {
        return Err(format!(
            "RustBind e' attualmente in esecuzione da '{}'. Chiudilo dalla tray (o usa --force) prima di proseguire.",
            target_dir.display()
        ));
    }

    for pid in matching_pids {
        // SAFETY: l'utente ha richiesto esplicitamente --force per chiudere il processo in target_dir.
        if let Ok(raw_proc) = unsafe { OpenProcess(PROCESS_TERMINATE, false, pid) } {
            let proc_handle = SafeHandle(raw_proc);
            // SAFETY: TerminateProcess termina il processo utente identificato prima dell'aggiornamento file.
            let _ = unsafe { TerminateProcess(proc_handle.0, 0) };
        }
    }
    std::thread::sleep(Duration::from_millis(300));
    Ok(())
}
