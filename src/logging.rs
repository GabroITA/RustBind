use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tracing_subscriber::fmt::writer::MakeWriter;

/// Dimensione massima predefinita per il file di log: 5 MB.
pub const DEFAULT_MAX_LOG_SIZE_BYTES: u64 = 5 * 1024 * 1024;

/// Numero massimo di file di backup conservati dopo la rotazione.
pub const DEFAULT_MAX_BACKUP_FILES: usize = 2;

/// Determina il percorso della directory dei log in `%LOCALAPPDATA%\RustBind\logs`.
pub fn default_log_directory() -> PathBuf {
    if let Some(local_appdata) = std::env::var_os("LOCALAPPDATA") {
        PathBuf::from(local_appdata).join("RustBind").join("logs")
    } else if let Some(base_dirs) = directories::BaseDirs::new() {
        base_dirs.data_local_dir().join("RustBind").join("logs")
    } else {
        PathBuf::from("logs")
    }
}

/// Scrittore thread-safe con rotazione automatica basata sulla dimensione massima del file.
pub struct SizeLimitedLogWriter {
    inner: Mutex<LogWriterInner>,
}

struct LogWriterInner {
    file_path: PathBuf,
    current_file: File,
    current_size: u64,
    max_size_bytes: u64,
    max_backups: usize,
}

impl SizeLimitedLogWriter {
    /// Inizializza un nuovo scrittore aprendo o creando il file di log nel percorso specificato.
    pub fn new(file_path: PathBuf, max_size_bytes: u64, max_backups: usize) -> io::Result<Self> {
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let current_size = if file_path.exists() {
            fs::metadata(&file_path)?.len()
        } else {
            0
        };

        let current_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)?;

        Ok(Self {
            inner: Mutex::new(LogWriterInner {
                file_path,
                current_file,
                current_size,
                max_size_bytes,
                max_backups,
            }),
        })
    }

    /// Esegue la rotazione dei file di log se la dimensione massima è superata.
    fn rotate_if_needed(inner: &mut LogWriterInner, additional_bytes: usize) -> io::Result<()> {
        if inner.current_size + additional_bytes as u64 <= inner.max_size_bytes {
            return Ok(());
        }

        inner.current_file.flush()?;

        // Rimuove o scala i file di backup esistenti (es. .2 rimosso, .1 -> .2)
        for i in (1..=inner.max_backups).rev() {
            let src = backup_path(&inner.file_path, i);
            let dst = backup_path(&inner.file_path, i + 1);

            if i == inner.max_backups {
                if src.exists() {
                    let _ = fs::remove_file(&src);
                }
            } else if src.exists() {
                let _ = fs::rename(&src, &dst);
            }
        }

        // Il file corrente rustbind.log diventa rustbind.log.1
        let first_backup = backup_path(&inner.file_path, 1);
        if inner.file_path.exists() {
            let _ = fs::rename(&inner.file_path, &first_backup);
        }

        // Crea un nuovo file di log vuoto
        inner.current_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&inner.file_path)?;
        inner.current_size = 0;

        Ok(())
    }
}

fn backup_path(base_path: &Path, index: usize) -> PathBuf {
    PathBuf::from(format!("{}.{}", base_path.to_string_lossy(), index))
}

impl Write for SizeLimitedLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        Self::rotate_if_needed(&mut inner, buf.len())?;
        let written = inner.current_file.write(buf)?;
        inner.current_size += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.current_file.flush()
    }
}

/// Wrapper per implementare `Write` su riferimenti condivisi `&SizeLimitedLogWriter`.
pub struct SharedLogWriter<'a>(&'a SizeLimitedLogWriter);

impl<'a> Write for SharedLogWriter<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut inner = self.0.inner.lock().unwrap_or_else(|p| p.into_inner());
        SizeLimitedLogWriter::rotate_if_needed(&mut inner, buf.len())?;
        let written = inner.current_file.write(buf)?;
        inner.current_size += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut inner = self.0.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.current_file.flush()
    }
}

impl<'a> MakeWriter<'a> for SizeLimitedLogWriter {
    type Writer = SharedLogWriter<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        SharedLogWriter(self)
    }
}

/// Legge in modo sicuro e limitato le righe `WARN`/`ERROR` più recenti dalla coda del file di log.
///
/// Limita la lettura agli ultimi 64 KB del file ed espone righe già redatte dalla policy di logging.
pub fn read_recent_log_warnings(log_path: &Path, max_entries: usize) -> Vec<String> {
    const TAIL_BYTES: u64 = 64 * 1024;
    if max_entries == 0 {
        return Vec::new();
    }
    let Ok(mut file) = File::open(log_path) else {
        return Vec::new();
    };
    let Ok(meta) = file.metadata() else {
        return Vec::new();
    };
    let start = meta.len().saturating_sub(TAIL_BYTES);
    if start > 0 && file.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<&str> = text.lines().collect();
    if start > 0 && !lines.is_empty() {
        // Scarta la prima riga potenzialmente troncata al confine del seek.
        lines.remove(0);
    }
    let mut matched: Vec<String> = lines
        .into_iter()
        .filter(|line| line.contains(" WARN ") || line.contains(" ERROR "))
        .map(|line| line.trim().to_string())
        .collect();
    if matched.len() > max_entries {
        matched = matched.split_off(matched.len() - max_entries);
    }
    matched.reverse();
    matched
}

/// Inizializza il sottosistema di logging di `tracing` su file in `%LOCALAPPDATA%\RustBind\logs`.
/// Se `debug` è abilitato (flag `--debug-log`), imposta il livello massimo a DEBUG; altrimenti INFO.
pub fn init_file_logging(debug: bool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let log_dir = default_log_directory();
    let log_file = log_dir.join("rustbind.log");

    let writer = SizeLimitedLogWriter::new(
        log_file,
        DEFAULT_MAX_LOG_SIZE_BYTES,
        DEFAULT_MAX_BACKUP_FILES,
    )?;

    let max_level = if debug {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    tracing_subscriber::fmt()
        .with_max_level(max_level)
        .with_writer(writer)
        .with_ansi(false)
        .with_target(false)
        .try_init()?;

    Ok(())
}

/// Estrae unicamente la posizione sorgente (file:riga) e il tipo di errore dal panico,
/// garantendo in modo assoluto che il payload (messaggio o valore del panico) NON venga mai estratto o formattato.
#[allow(deprecated)]
pub fn format_panic_location_and_type(info: &std::panic::PanicInfo<'_>) -> (String, &'static str) {
    let location = match info.location() {
        Some(loc) => format!("{}:{}", loc.file(), loc.line()),
        None => "posizione_sconosciuta".to_string(),
    };

    let error_type = if info.payload().is::<&str>() || info.payload().is::<String>() {
        "String/&str"
    } else {
        "Box<dyn Any>"
    };

    (location, error_type)
}

/// Gestore centralizzato dell'evento di panico.
///
/// 1. Registra su tracing/stderr solo la posizione (file:riga) e il tipo generico di errore (MAI il payload).
/// 2. Esegue una rimozione best-effort dell'icona tray (`NIM_DELETE`) per non lasciare icone orfane.
/// 3. Se `exit_process` è true, termina il processo con codice 101.
#[allow(deprecated)]
pub fn handle_panic_event(info: &std::panic::PanicInfo<'_>, exit_process: bool) {
    let (location, error_type) = format_panic_location_and_type(info);

    tracing::error!(
        location = %location,
        error_type = %error_type,
        "Panic imprevisto nel daemon RustBind"
    );

    eprintln!(
        "RustBind Daemon Panic a {} [tipo errore: {}]",
        location, error_type
    );

    // Pulizia best-effort dell'icona di notifica sulla barra delle applicazioni di Windows
    crate::platform::windows::tray::remove_tray_icon_best_effort();

    if exit_process {
        std::process::exit(101);
    }
}

/// Installa il panic hook sicuro conforme alla policy di privacy di RustBind.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        handle_panic_event(info, true);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_writer_rotation() {
        let temp_dir =
            std::env::temp_dir().join(format!("rustbind_log_test_{}", std::process::id()));
        let log_file = temp_dir.join("test.log");

        // Soglia piccolissima di 100 byte e max 2 backup
        let mut writer = SizeLimitedLogWriter::new(log_file.clone(), 100, 2).unwrap();

        // Scrive 60 byte
        let payload1 = b"012345678901234567890123456789012345678901234567890123456789";
        writer.write_all(payload1).unwrap();
        writer.flush().unwrap();
        assert!(log_file.exists());
        assert_eq!(fs::metadata(&log_file).unwrap().len(), 60);

        // Scrive altri 60 byte (totale 120 > 100): deve innescare la rotazione
        writer.write_all(payload1).unwrap();
        writer.flush().unwrap();

        let backup1 = backup_path(&log_file, 1);
        assert!(
            backup1.exists(),
            "Il backup .1 deve esistere dopo la rotazione"
        );
        assert_eq!(fs::metadata(&backup1).unwrap().len(), 60);
        assert_eq!(fs::metadata(&log_file).unwrap().len(), 60);

        // Pulizia
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_panic_hook_does_not_leak_payload() {
        let prev_hook = std::panic::take_hook();
        let captured_log = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let captured_log_clone = captured_log.clone();

        std::panic::set_hook(Box::new(move |info| {
            let (loc, err_type) = format_panic_location_and_type(info);
            let msg = format!("PANIC: loc={loc}, type={err_type}");
            *captured_log_clone.lock().unwrap() = msg;
            // Non usciamo dal processo durante il test
        }));

        let result = std::panic::catch_unwind(|| {
            panic!("SEGRETO123");
        });
        assert!(result.is_err());

        // Ripristina l'hook originale
        std::panic::set_hook(prev_hook);

        let log_content = captured_log.lock().unwrap().clone();
        assert!(
            !log_content.contains("SEGRETO123"),
            "Il log del panic non deve contenere il payload segreto: {log_content}"
        );
        assert!(
            log_content.contains("logging.rs"),
            "Il log del panic deve contenere la posizione del file: {log_content}"
        );
        assert!(
            log_content.contains("type=String/&str"),
            "Il log del panic deve contenere il tipo di errore: {log_content}"
        );
    }
}
