use std::ffi::c_void;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use windows::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER,
    WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest,
    WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
    WinHttpSendRequest, WinHttpSetTimeouts,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, w};

use crate::platform::traits::PlatformError;
use crate::platform::windows::autostart::close_window_by_title_best_effort;
use crate::platform::windows::single_instance::SingleInstanceGuard;
use crate::updater::{
    GITHUB_LATEST_API_URL, GITHUB_RELEASES_LATEST_URL, ReleaseInfo, UpdateCheckOutcome,
    clear_update_cache, default_local_data_dir, default_update_cache_path, is_newer_version,
    parse_github_latest_release_json, parse_sha256_manifest_for_file, sha256_file,
    validate_allowed_https_url, write_update_cache,
};

const MAX_API_JSON_BYTES: usize = 1024 * 1024;
const MAX_SHA256_TXT_BYTES: usize = 64 * 1024;
const MAX_PACKAGE_ZIP_BYTES: usize = 64 * 1024 * 1024;

const REQUIRED_BINS: [&str; 3] = ["rustbind.exe", "rustbind-ui.exe", "rustbindctl.exe"];

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct SafeWinHttpHandle(*mut c_void);

impl SafeWinHttpHandle {
    fn new(raw: *mut c_void) -> Result<Self, String> {
        if raw.is_null() {
            Err("WinHTTP handle nullo".to_string())
        } else {
            Ok(Self(raw))
        }
    }

    fn raw(&self) -> *mut c_void {
        self.0
    }
}

impl Drop for SafeWinHttpHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: L'handle WinHTTP è stato aperto su questo thread ed è chiuso una volta sola al Drop.
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

/// Apre un URL HTTPS consentito (`github.com` / release) nel browser predefinito tramite `ShellExecuteW`.
pub fn open_https_url(url: &str) -> Result<(), PlatformError> {
    let _ = validate_allowed_https_url(url).map_err(PlatformError::OsError)?;
    let wide = to_wide_null(url.trim());
    // SAFETY: Invocazione documentata di ShellExecuteW con verbo "open" su URL HTTPS validato.
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
            "Apertura URL nel browser fallita (codice {})",
            hinst.0 as usize
        )));
    }
    Ok(())
}

struct HttpBytesResponse {
    status_code: u32,
    body: Vec<u8>,
}

fn winhttp_get_bytes(
    url: &str,
    max_bytes: usize,
    accept_header: Option<&str>,
) -> Result<HttpBytesResponse, String> {
    let parsed = validate_allowed_https_url(url)?;
    let host_w = to_wide_null(&parsed.host);
    let path_w = to_wide_null(&parsed.path_and_query);
    let headers_opt_w: Option<Vec<u16>> =
        accept_header.map(|accept| format!("Accept: {accept}\r\n").encode_utf16().collect());

    // SAFETY: Apertura sessione WinHTTP nativa con proxy automatico di sistema Windows.
    let raw_session = unsafe {
        WinHttpOpen(
            w!("RustBind-Updater/1.0"),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    };
    let session = SafeWinHttpHandle::new(raw_session)
        .map_err(|_| "Inizializzazione sessione WinHTTP fallita".to_string())?;

    // SAFETY: Imposta timeout contenuti su sessione WinHTTP valida (resolve 5s, connect 6s, send 6s, recv 15s).
    unsafe {
        let _ = WinHttpSetTimeouts(session.raw(), 5_000, 6_000, 6_000, 15_000);
    }

    // SAFETY: Connessione HTTPS verso host validato sulla porta standard 443.
    let raw_connect = unsafe { WinHttpConnect(session.raw(), PCWSTR(host_w.as_ptr()), 443, 0) };
    let connect = SafeWinHttpHandle::new(raw_connect)
        .map_err(|_| format!("Connessione HTTPS a {} fallita", parsed.host))?;

    // SAFETY: Apre richiesta GET HTTPS (TLS) verso percorso validato.
    let raw_request = unsafe {
        WinHttpOpenRequest(
            connect.raw(),
            w!("GET"),
            PCWSTR(path_w.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        )
    };
    let request = SafeWinHttpHandle::new(raw_request)
        .map_err(|_| "Creazione richiesta HTTPS fallita".to_string())?;

    // SAFETY: Invio richiesta GET WinHTTP su handle valido.
    unsafe {
        WinHttpSendRequest(request.raw(), headers_opt_w.as_deref(), None, 0, 0, 0)
            .map_err(|e| format!("Richiesta di rete non riuscita: {e}"))?;
        WinHttpReceiveResponse(request.raw(), std::ptr::null_mut())
            .map_err(|e| format!("Risposta HTTP non ricevuta: {e}"))?;
    }

    let mut status_code: u32 = 0;
    let mut status_len: u32 = std::mem::size_of::<u32>() as u32;
    // SAFETY: Lettura codice HTTP numerico dalla risposta WinHTTP valida.
    unsafe {
        WinHttpQueryHeaders(
            request.raw(),
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status_code as *mut u32).cast()),
            &mut status_len,
            std::ptr::null_mut(),
        )
        .map_err(|e| format!("Lettura status code HTTP fallita: {e}"))?;
    }

    let mut body = Vec::new();
    loop {
        let mut available: u32 = 0;
        // SAFETY: Query dei byte disponibili sulla risposta WinHTTP valida.
        unsafe {
            WinHttpQueryDataAvailable(request.raw(), &mut available)
                .map_err(|e| format!("Lettura disponibilità dati HTTP fallita: {e}"))?;
        }
        if available == 0 {
            break;
        }
        let chunk_size = (available as usize).min(32 * 1024);
        if body.len().saturating_add(chunk_size) > max_bytes {
            return Err("Risposta HTTP oltre il limite massimo consentito".to_string());
        }
        let mut chunk = vec![0u8; chunk_size];
        let mut read_len: u32 = 0;
        // SAFETY: Buffer `chunk` valido e dimensionato esattamente per `chunk_size` byte.
        unsafe {
            WinHttpReadData(
                request.raw(),
                chunk.as_mut_ptr().cast(),
                chunk.len() as u32,
                &mut read_len,
            )
            .map_err(|e| format!("Lettura corpo risposta HTTP fallita: {e}"))?;
        }
        if read_len == 0 {
            break;
        }
        let n = (read_len as usize).min(chunk.len());
        if body.len().saturating_add(n) > max_bytes {
            return Err("Risposta HTTP oltre il limite massimo consentito".to_string());
        }
        body.extend_from_slice(&chunk[..n]);
    }

    Ok(HttpBytesResponse { status_code, body })
}

/// Interroga GitHub Releases (`GabroITA/RustBind`) e aggiorna la cache locale.
pub fn check_for_updates_blocking(current_version: &str) -> Result<UpdateCheckOutcome, String> {
    let resp = winhttp_get_bytes(
        GITHUB_LATEST_API_URL,
        MAX_API_JSON_BYTES,
        Some("application/vnd.github+json"),
    )?;

    if resp.status_code == 404 {
        if let Some(cache_path) = default_update_cache_path() {
            clear_update_cache(&cache_path);
        }
        return Ok(UpdateCheckOutcome::UpToDate {
            current_version: current_version.to_string(),
            latest_version: None,
            html_url: GITHUB_RELEASES_LATEST_URL.to_string(),
        });
    }

    if !(200..300).contains(&resp.status_code) {
        return Err(format!("GitHub API ha risposto HTTP {}", resp.status_code));
    }

    let text = String::from_utf8(resp.body)
        .map_err(|_| "Risposta GitHub API non in UTF-8 valido".to_string())?;
    let info = parse_github_latest_release_json(&text)?;

    if is_newer_version(current_version, &info.version) {
        if let Some(cache_path) = default_update_cache_path() {
            let _ = write_update_cache(&cache_path, current_version, &info);
        }
        Ok(UpdateCheckOutcome::UpdateAvailable(info))
    } else {
        if let Some(cache_path) = default_update_cache_path() {
            clear_update_cache(&cache_path);
        }
        Ok(UpdateCheckOutcome::UpToDate {
            current_version: current_version.to_string(),
            latest_version: Some(info.version),
            html_url: info.html_url,
        })
    }
}

/// Scarica il pacchetto portatile dell'ultima release, verifica SHA-256, estrae lo stage e avvia l'updater.
pub fn download_stage_and_launch_updater(release: &ReleaseInfo) -> Result<(), String> {
    let zip_url = release.zip_url.as_deref().ok_or_else(|| {
        "Pacchetto Windows x64 (.zip) non trovato nella release GitHub".to_string()
    })?;

    let current_exe = std::env::current_exe()
        .map_err(|e| format!("Impossibile determinare il percorso dell'eseguibile: {e}"))?;
    let install_dir = current_exe
        .parent()
        .ok_or_else(|| "Directory di installazione non valida".to_string())?
        .to_path_buf();

    let local_root = default_local_data_dir()
        .ok_or_else(|| "Impossibile risolvere %LOCALAPPDATA%\\RustBind".to_string())?;
    let work_dir = local_root
        .join("updates")
        .join(format!("v{}", release.version));
    if work_dir.exists() {
        let _ = fs::remove_dir_all(&work_dir);
    }
    fs::create_dir_all(&work_dir).map_err(|e| format!("Creazione cartella update fallita: {e}"))?;

    let zip_filename = format!("rustbind-{}-windows-x64.zip", release.version);
    let zip_path = work_dir.join(&zip_filename);
    let zip_resp = winhttp_get_bytes(zip_url, MAX_PACKAGE_ZIP_BYTES, None)?;
    if !(200..300).contains(&zip_resp.status_code) {
        return Err(format!(
            "Download pacchetto fallito (HTTP {})",
            zip_resp.status_code
        ));
    }
    {
        let mut f = File::create(&zip_path).map_err(|e| e.to_string())?;
        f.write_all(&zip_resp.body).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
    }

    let manifest_text = if let Some(sha_url) = release.sha256_url.as_deref() {
        let sha_resp = winhttp_get_bytes(sha_url, MAX_SHA256_TXT_BYTES, None)?;
        if !(200..300).contains(&sha_resp.status_code) {
            return Err(format!(
                "Download checksum SHA-256 fallito (HTTP {})",
                sha_resp.status_code
            ));
        }
        Some(
            String::from_utf8(sha_resp.body)
                .map_err(|_| "Manifest SHA-256 non valido".to_string())?,
        )
    } else {
        None
    };

    if let Some(manifest) = &manifest_text
        && let Some(expected_zip_hash) = parse_sha256_manifest_for_file(manifest, &zip_filename)
    {
        let actual = sha256_file(&zip_path)?;
        if actual != expected_zip_hash {
            return Err("Verifica integrita' SHA-256 dello zip fallita".to_string());
        }
    }

    let staged_dir = work_dir.join("staged");
    fs::create_dir_all(&staged_dir).map_err(|e| e.to_string())?;
    let status = Command::new("tar.exe")
        .arg("-x")
        .arg("-f")
        .arg(&zip_path)
        .arg("-C")
        .arg(&staged_dir)
        .status()
        .map_err(|e| format!("Estrazione archivio fallita (`tar.exe`): {e}"))?;
    if !status.success() {
        return Err(format!("Estrazione archivio terminata con stato {status}"));
    }

    for bin in REQUIRED_BINS {
        let bin_path = staged_dir.join(bin);
        if !bin_path.is_file() {
            return Err(format!(
                "Eseguibile '{bin}' mancante nel pacchetto scaricato"
            ));
        }
        if let Some(manifest) = &manifest_text
            && let Some(expected_bin_hash) = parse_sha256_manifest_for_file(manifest, bin)
        {
            let actual = sha256_file(&bin_path)?;
            if actual != expected_bin_hash {
                return Err(format!("Verifica integrita' SHA-256 fallita per {bin}"));
            }
        }
    }

    let was_daemon_running = match SingleInstanceGuard::acquire() {
        Ok(guard) => {
            drop(guard);
            false
        }
        Err(_) => true,
    };

    let staged_ui = staged_dir.join("rustbind-ui.exe");
    let restart_arg = if was_daemon_running {
        "--restart-daemon=1"
    } else {
        "--restart-daemon=0"
    };
    Command::new(&staged_ui)
        .arg("--apply-update")
        .arg(&install_dir)
        .arg(restart_arg)
        .spawn()
        .map_err(|e| format!("Avvio installazione aggiornamento fallito: {e}"))?;

    Ok(())
}

/// Se `rustbind-ui.exe` è stato avviato in modalità `--apply-update`, applica l'aggiornamento e rilancia l'app.
pub fn apply_staged_update_from_cli_if_requested() -> Option<i32> {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let idx = args.iter().position(|a| a == "--apply-update")?;
    let install_dir = args.get(idx + 1).map(PathBuf::from)?;
    let restart_daemon = args.iter().any(|a| a == "--restart-daemon=1");

    if run_staged_apply(&install_dir, restart_daemon).is_ok() {
        Some(0)
    } else {
        Some(1)
    }
}

fn wait_until_mutex_free(
    acquire: fn() -> Result<SingleInstanceGuard, PlatformError>,
    max_wait: Duration,
) -> bool {
    let deadline = Instant::now() + max_wait;
    while Instant::now() < deadline {
        if let Ok(guard) = acquire() {
            drop(guard);
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn copy_binary_with_retry(src: &Path, dst: &Path) -> Result<(), String> {
    for attempt in 0..15 {
        match fs::copy(src, dst) {
            Ok(_) => return Ok(()),
            Err(err) if attempt < 14 => {
                if attempt == 7 {
                    let backup = dst.with_extension("exe.old");
                    let _ = fs::remove_file(&backup);
                    let _ = fs::rename(dst, &backup);
                }
                let _ = err;
                std::thread::sleep(Duration::from_millis(150));
            }
            Err(err) => return Err(err.to_string()),
        }
    }
    Ok(())
}

fn run_staged_apply(install_dir: &Path, restart_daemon: bool) -> Result<(), String> {
    if !install_dir.is_dir() {
        return Err("Directory di installazione assente".to_string());
    }
    let staged_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let staged_dir = staged_exe
        .parent()
        .ok_or_else(|| "Directory stage non valida".to_string())?;

    for bin in REQUIRED_BINS {
        if !staged_dir.join(bin).is_file() {
            return Err(format!("Binario staged mancante: {bin}"));
        }
    }

    let _ = wait_until_mutex_free(SingleInstanceGuard::acquire_ui, Duration::from_secs(6));

    if !wait_until_mutex_free(SingleInstanceGuard::acquire, Duration::from_millis(200)) {
        let _ = close_window_by_title_best_effort("RustBind_MessageLoop");
        let freed = wait_until_mutex_free(SingleInstanceGuard::acquire, Duration::from_secs(4));
        if !freed {
            let _ = close_window_by_title_best_effort("RustBind_MessageLoop");
            let _ = wait_until_mutex_free(SingleInstanceGuard::acquire, Duration::from_secs(2));
        }
    }

    for bin in REQUIRED_BINS {
        copy_binary_with_retry(&staged_dir.join(bin), &install_dir.join(bin))?;
    }
    let readme_src = staged_dir.join("README.txt");
    if readme_src.is_file() {
        let _ = fs::copy(readme_src, install_dir.join("README.txt"));
    }

    if let Some(cache_path) = default_update_cache_path() {
        clear_update_cache(&cache_path);
    }

    if restart_daemon {
        let _ = Command::new(install_dir.join("rustbind.exe")).spawn();
        std::thread::sleep(Duration::from_millis(200));
    }
    let _ = Command::new(install_dir.join("rustbind-ui.exe")).spawn();
    Ok(())
}

/// Rimuove in modalità best-effort eventuali file `.exe.old` o staging di aggiornamenti precedenti.
pub fn cleanup_stale_update_artifacts() {
    if let Ok(current_exe) = std::env::current_exe()
        && let Some(dir) = current_exe.parent()
    {
        for bin in REQUIRED_BINS {
            let old = dir.join(bin).with_extension("exe.old");
            if old.exists() {
                let _ = fs::remove_file(old);
            }
        }
    }
    if let Some(local_root) = default_local_data_dir() {
        let updates_dir = local_root.join("updates");
        if updates_dir.is_dir() {
            let _ = fs::remove_dir_all(updates_dir);
        }
    }
}
