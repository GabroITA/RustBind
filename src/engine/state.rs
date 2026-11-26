use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Stato operativo del daemon di RustBind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceState {
    /// Servizio attivo: hotkey registrate, esecuzione delle sequenze abilitata.
    Active,
    /// Servizio sospeso: hotkey temporaneamente disabilitate, nessuna sequenza viene avviata.
    Paused,
    /// Stato di errore critico (es. configurazione non valida o kill-switch occupato).
    /// Nessuna regola viene eseguita e l'icona della tray mostra lo stato di errore.
    Error(String),
}

impl ServiceState {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    pub fn is_paused(&self) -> bool {
        matches!(self, Self::Paused)
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }
}

impl fmt::Display for ServiceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => write!(f, "Attivo"),
            Self::Paused => write!(f, "In Pausa"),
            Self::Error(msg) => write!(f, "Errore: {}", msg),
        }
    }
}

/// Motivo categorico per cui una singola regola abilitata è stata saltata dal daemon in degradazione parziale.
///
/// Non espone mai payload testuali né nomi di combinazioni/valori di configurazione.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkippedRuleReason {
    /// Combinazione già occupata a livello di sistema operativo (Windows / altra app).
    OsAlreadyRegistered,
    /// Errore Win32 generico durante la chiamata `RegisterHotKey`.
    OsRegistrationFailed,
    /// Coincide con il Kill-Switch o con `resume_hotkey`.
    ConflictsWithSystemHotkey,
    /// Oscurata da una regola precedente con trigger e contesto sovrapposti.
    ShadowedByEarlierRule,
}

impl SkippedRuleReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::OsAlreadyRegistered => "os_already_registered",
            Self::OsRegistrationFailed => "os_registration_failed",
            Self::ConflictsWithSystemHotkey => "system_hotkey_conflict",
            Self::ShadowedByEarlierRule => "shadowed_rule",
        }
    }

    pub fn from_code(raw: &str) -> Option<Self> {
        match raw.trim() {
            "os_already_registered" => Some(Self::OsAlreadyRegistered),
            "os_registration_failed" => Some(Self::OsRegistrationFailed),
            "system_hotkey_conflict" => Some(Self::ConflictsWithSystemHotkey),
            "shadowed_rule" => Some(Self::ShadowedByEarlierRule),
            _ => None,
        }
    }
}

/// Singola regola saltata dal daemon durante la registrazione delle hotkey.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedRuleInfo {
    pub rule_id: String,
    pub reason: SkippedRuleReason,
}

/// Diagnostica runtime non sensibile scritta dal daemon (`%LOCALAPPDATA%\RustBind\daemon_runtime.txt`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DaemonRuntimeDiagnostics {
    pub pid: u32,
    pub resume_hotkey_failed: bool,
    pub skipped_rules: Vec<SkippedRuleInfo>,
}

/// Restituisce il percorso standard del file di diagnostica runtime del daemon (`%LOCALAPPDATA%\RustBind\daemon_runtime.txt`).
pub fn default_runtime_diagnostics_path() -> PathBuf {
    let logs_dir = crate::logging::default_log_directory();
    if let Some(parent) = logs_dir.parent() {
        parent.join("daemon_runtime.txt")
    } else {
        PathBuf::from("daemon_runtime.txt")
    }
}

/// Serializza in formato testuale minimale e sicuro la diagnostica runtime del daemon.
pub fn format_runtime_diagnostics(diag: &DaemonRuntimeDiagnostics) -> String {
    let mut lines = Vec::with_capacity(diag.skipped_rules.len() + 2);
    lines.push(format!("pid={}", diag.pid));
    lines.push(format!(
        "resume_hotkey_failed={}",
        if diag.resume_hotkey_failed { "1" } else { "0" }
    ));
    for entry in &diag.skipped_rules {
        lines.push(format!("skipped={}:{}", entry.rule_id, entry.reason.code()));
    }
    lines.join("\n") + "\n"
}

/// Effettua il parsing sicuro del file di diagnostica runtime del daemon.
pub fn parse_runtime_diagnostics(content: &str) -> Option<DaemonRuntimeDiagnostics> {
    let mut pid: Option<u32> = None;
    let mut resume_hotkey_failed = false;
    let mut skipped_rules = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        match key.trim() {
            "pid" => {
                if let Ok(parsed) = value.trim().parse::<u32>() {
                    pid = Some(parsed);
                }
            }
            "resume_hotkey_failed" => {
                resume_hotkey_failed =
                    value.trim() == "1" || value.trim().eq_ignore_ascii_case("true");
            }
            "skipped" => {
                if let Some((id_raw, reason_raw)) = value.split_once(':') {
                    let rule_id = id_raw.trim();
                    if !rule_id.is_empty()
                        && let Some(reason) = SkippedRuleReason::from_code(reason_raw)
                    {
                        skipped_rules.push(SkippedRuleInfo {
                            rule_id: rule_id.to_string(),
                            reason,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    Some(DaemonRuntimeDiagnostics {
        pid: pid?,
        resume_hotkey_failed,
        skipped_rules,
    })
}

/// Scrive atomicamente su disco la diagnostica runtime del daemon tramite file temporaneo e `rename`.
pub fn write_runtime_diagnostics(
    path: &Path,
    diag: &DaemonRuntimeDiagnostics,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let body = format_runtime_diagnostics(diag);
    let tmp_path = path.with_extension(format!("txt.tmp.{}", diag.pid));
    fs::write(&tmp_path, body)?;
    if let Err(err) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }
    Ok(())
}

/// Legge e decodifica il file di diagnostica runtime del daemon, se presente e valido.
pub fn read_runtime_diagnostics(path: &Path) -> Option<DaemonRuntimeDiagnostics> {
    let content = fs::read_to_string(path).ok()?;
    parse_runtime_diagnostics(&content)
}

/// Rimuove il file di diagnostica runtime alla chiusura pulita del daemon.
pub fn remove_runtime_diagnostics(path: &Path) {
    let _ = fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_diagnostics_roundtrip_and_atomic_write() {
        let diag = DaemonRuntimeDiagnostics {
            pid: 4242,
            resume_hotkey_failed: true,
            skipped_rules: vec![
                SkippedRuleInfo {
                    rule_id: "r_one".to_string(),
                    reason: SkippedRuleReason::OsAlreadyRegistered,
                },
                SkippedRuleInfo {
                    rule_id: "r_two".to_string(),
                    reason: SkippedRuleReason::ShadowedByEarlierRule,
                },
            ],
        };
        let dir = std::env::temp_dir().join(format!("rustbind_diag_test_{}", std::process::id()));
        let path = dir.join("daemon_runtime.txt");
        write_runtime_diagnostics(&path, &diag).expect("Scrittura diagnostica");
        let loaded = read_runtime_diagnostics(&path).expect("Lettura diagnostica");
        assert_eq!(loaded, diag);
        remove_runtime_diagnostics(&path);
        assert!(!path.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
