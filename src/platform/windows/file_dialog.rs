use std::path::PathBuf;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, GetOpenFileNameW, GetSaveFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR,
    OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::core::{PCWSTR, PWSTR, w};

const FILE_BUFFER_LEN: usize = 32768;

/// Filtro standard per i dialoghi Windows limitato ai file `.toml`.
const TOML_FILTER: &[u16] = &[
    b'F' as u16,
    b'i' as u16,
    b'l' as u16,
    b'e' as u16,
    b' ' as u16,
    b'T' as u16,
    b'O' as u16,
    b'M' as u16,
    b'L' as u16,
    b' ' as u16,
    b'(' as u16,
    b'*' as u16,
    b'.' as u16,
    b't' as u16,
    b'o' as u16,
    b'm' as u16,
    b'l' as u16,
    b')' as u16,
    0,
    b'*' as u16,
    b'.' as u16,
    b't' as u16,
    b'o' as u16,
    b'm' as u16,
    b'l' as u16,
    0,
    0,
];

/// Filtro standard per i dialoghi Windows limitato agli eseguibili `.exe`.
const EXE_FILTER: &[u16] = &[
    b'A' as u16,
    b'p' as u16,
    b'p' as u16,
    b's' as u16,
    b' ' as u16,
    b'(' as u16,
    b'*' as u16,
    b'.' as u16,
    b'e' as u16,
    b'x' as u16,
    b'e' as u16,
    b')' as u16,
    0,
    b'*' as u16,
    b'.' as u16,
    b'e' as u16,
    b'x' as u16,
    b'e' as u16,
    0,
    0,
];

fn extract_wide_string(buf: &[u16]) -> Option<PathBuf> {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    if len == 0 {
        return None;
    }
    Some(PathBuf::from(String::from_utf16_lossy(&buf[..len])))
}

/// Apre il selettore nativo Windows per scegliere manualmente un eseguibile `.exe`.
///
/// Restituisce `Ok(Some("nome_file.exe"))` oppure `Ok(None)` se annullato.
pub fn pick_exe_file() -> Result<Option<String>, String> {
    let mut file_buf = vec![0u16; FILE_BUFFER_LEN];
    let title = w!("Seleziona eseguibile applicazione (.exe)");
    let def_ext = w!("exe");

    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: HWND::default(),
        lpstrFilter: PCWSTR(EXE_FILTER.as_ptr()),
        lpstrFile: PWSTR(file_buf.as_mut_ptr()),
        nMaxFile: file_buf.len() as u32,
        lpstrTitle: title,
        lpstrDefExt: def_ext,
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };

    // SAFETY: `ofn` punta a buffer UTF-16 validi e `lStructSize` corrisponde alla dimensione
    // esatta di `OPENFILENAMEW` per tutta la durata della chiamata modale Win32.
    let ok = unsafe { GetOpenFileNameW(&mut ofn) }.as_bool();
    if ok {
        let name = extract_wide_string(&file_buf)
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()));
        return Ok(name.filter(|s| !s.trim().is_empty()));
    }

    // SAFETY: CommDlgExtendedError legge lo stato di errore dell'ultima chiamata Comdlg32 sul thread corrente.
    let err_code = unsafe { CommDlgExtendedError() };
    if err_code.0 == 0 {
        Ok(None)
    } else {
        Err(format!(
            "Finestra di selezione file fallita (codice 0x{:X})",
            err_code.0
        ))
    }
}

/// Apre il selettore nativo Windows per importare un file di configurazione `.toml`.
///
/// Restituisce `Ok(None)` se l'utente annulla la finestra di dialogo.
pub fn pick_toml_import_file() -> Result<Option<PathBuf>, String> {
    let mut file_buf = vec![0u16; FILE_BUFFER_LEN];
    let title = w!("Importa configurazione RustBind");
    let def_ext = w!("toml");

    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: HWND::default(),
        lpstrFilter: PCWSTR(TOML_FILTER.as_ptr()),
        lpstrFile: PWSTR(file_buf.as_mut_ptr()),
        nMaxFile: file_buf.len() as u32,
        lpstrTitle: title,
        lpstrDefExt: def_ext,
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };

    // SAFETY: `ofn` punta a buffer UTF-16 validi e `lStructSize` corrisponde alla dimensione
    // esatta di `OPENFILENAMEW` per tutta la durata della chiamata modale Win32.
    let ok = unsafe { GetOpenFileNameW(&mut ofn) }.as_bool();
    if ok {
        return Ok(extract_wide_string(&file_buf));
    }

    // SAFETY: CommDlgExtendedError legge lo stato di errore dell'ultima chiamata Comdlg32 sul thread corrente.
    let err_code = unsafe { CommDlgExtendedError() };
    if err_code.0 == 0 {
        Ok(None)
    } else {
        Err(format!(
            "Finestra di selezione file fallita (codice 0x{:X})",
            err_code.0
        ))
    }
}

/// Apre il selettore nativo Windows per esportare la configurazione corrente in un file `.toml`.
///
/// Restituisce `Ok(None)` se l'utente annulla la finestra di dialogo.
pub fn pick_toml_export_file(default_name: &str) -> Result<Option<PathBuf>, String> {
    let mut file_buf = vec![0u16; FILE_BUFFER_LEN];
    let wide_default: Vec<u16> = default_name.encode_utf16().collect();
    let copy_len = wide_default.len().min(FILE_BUFFER_LEN - 1);
    file_buf[..copy_len].copy_from_slice(&wide_default[..copy_len]);

    let title = w!("Esporta configurazione RustBind");
    let def_ext = w!("toml");

    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: HWND::default(),
        lpstrFilter: PCWSTR(TOML_FILTER.as_ptr()),
        lpstrFile: PWSTR(file_buf.as_mut_ptr()),
        nMaxFile: file_buf.len() as u32,
        lpstrTitle: title,
        lpstrDefExt: def_ext,
        Flags: OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };

    // SAFETY: `ofn` punta a buffer UTF-16 validi e `lStructSize` corrisponde alla dimensione
    // esatta di `OPENFILENAMEW` per tutta la durata della chiamata modale Win32.
    let ok = unsafe { GetSaveFileNameW(&mut ofn) }.as_bool();
    if ok {
        return Ok(extract_wide_string(&file_buf));
    }

    // SAFETY: CommDlgExtendedError legge lo stato di errore dell'ultima chiamata Comdlg32 sul thread corrente.
    let err_code = unsafe { CommDlgExtendedError() };
    if err_code.0 == 0 {
        Ok(None)
    } else {
        Err(format!(
            "Finestra di esportazione file fallita (codice 0x{:X})",
            err_code.0
        ))
    }
}
