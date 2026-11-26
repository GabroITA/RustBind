use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use directories::BaseDirs;

use crate::config::error::ConfigError;
use crate::config::schema::Config;
use crate::core::action::InputAction;
use crate::core::{
    CURRENT_SCHEMA_VERSION, MAX_ACTIONS_PER_SEQUENCE, MAX_DELAY_MS, MAX_EVENTS_PER_SECOND,
    MAX_MOUSE_MOVE_DELTA, MAX_MOUSE_SCROLL_STEPS, MAX_SEND_TEXT_LENGTH, MAX_WINDOW_FILTER_LEN,
    MIN_DELAY_MS, MIN_EVENTS_PER_SECOND,
};

static TMP_FILE_COUNTER: AtomicUsize = AtomicUsize::new(1);

/// Gestore responsabile di caricamento, salvataggio, backup e validazione della configurazione.
pub struct ConfigManager;

impl ConfigManager {
    /// Risolve la directory dati utente (`%APPDATA%\RustBind`).
    pub fn resolve_app_data_dir(base_config_dir: &Path) -> PathBuf {
        base_config_dir.join("RustBind")
    }

    /// Determina la directory dati standard (`%APPDATA%\RustBind`).
    pub fn default_app_data_dir() -> Result<PathBuf, ConfigError> {
        let base_dirs = BaseDirs::new().ok_or(ConfigError::AppDataNotFound)?;
        Ok(Self::resolve_app_data_dir(base_dirs.config_dir()))
    }

    /// Risolve la directory unica dei file di configurazione (`%APPDATA%\RustBind\configs`).
    pub fn resolve_profiles_dir(base_config_dir: &Path) -> PathBuf {
        Self::resolve_app_data_dir(base_config_dir).join("configs")
    }

    /// Determina la directory standard dei file di configurazione (`%APPDATA%\RustBind\configs`).
    pub fn default_profiles_dir() -> Result<PathBuf, ConfigError> {
        let base_dirs = BaseDirs::new().ok_or(ConfigError::AppDataNotFound)?;
        Ok(Self::resolve_profiles_dir(base_dirs.config_dir()))
    }

    /// Risolve il percorso predefinito del file principale (`%APPDATA%\RustBind\configs\config.toml`).
    ///
    /// Funzione pura per testare la risoluzione deterministica senza dipendere dall'ambiente di sistema.
    pub fn resolve_config_path(base_config_dir: &Path) -> PathBuf {
        Self::resolve_profiles_dir(base_config_dir).join("config.toml")
    }

    /// Determina il percorso standard del file predefinito (`%APPDATA%\RustBind\configs\config.toml`).
    pub fn default_config_path() -> Result<PathBuf, ConfigError> {
        let base_dirs = BaseDirs::new().ok_or(ConfigError::AppDataNotFound)?;
        Ok(Self::resolve_config_path(base_dirs.config_dir()))
    }

    /// Risolve il file marker che memorizza il nome del profilo attivo (`%APPDATA%\RustBind\.active_config`).
    pub fn resolve_active_profile_marker(base_config_dir: &Path) -> PathBuf {
        Self::resolve_app_data_dir(base_config_dir).join(".active_config")
    }

    /// Determina il percorso standard del marker profilo attivo (`%APPDATA%\RustBind\.active_config`).
    pub fn default_active_profile_marker() -> Result<PathBuf, ConfigError> {
        let base_dirs = BaseDirs::new().ok_or(ConfigError::AppDataNotFound)?;
        Ok(Self::resolve_active_profile_marker(base_dirs.config_dir()))
    }

    /// Risolve il percorso del file `.toml` attualmente attivo dentro `%APPDATA%\RustBind\configs\`.
    pub fn resolve_active_config_path(base_config_dir: &Path) -> PathBuf {
        let profiles_dir = Self::resolve_profiles_dir(base_config_dir);
        let marker = Self::resolve_active_profile_marker(base_config_dir);
        let active_name = fs::read_to_string(marker)
            .ok()
            .and_then(|raw| Self::sanitize_profile_filename(&raw).ok())
            .unwrap_or_else(|| "config.toml".to_string());
        profiles_dir.join(active_name)
    }

    /// Determina il percorso del file `.toml` attivo corrente per daemon e UI.
    pub fn default_active_config_path() -> Result<PathBuf, ConfigError> {
        let base_dirs = BaseDirs::new().ok_or(ConfigError::AppDataNotFound)?;
        Ok(Self::resolve_active_config_path(base_dirs.config_dir()))
    }

    /// Normalizza un nome file per profilo assicurando estensione `.toml` e soli caratteri sicuri.
    pub fn sanitize_profile_filename(raw_name: &str) -> Result<String, ConfigError> {
        let stem_raw = Path::new(raw_name.trim())
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .trim();
        let stem = stem_raw.strip_suffix(".toml").unwrap_or(stem_raw);
        let cleaned: String = stem
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let trimmed = cleaned.trim_matches('_');
        if trimmed.is_empty() {
            return Err(ConfigError::InvalidPath(PathBuf::from(raw_name)));
        }
        Ok(format!("{}.toml", trimmed))
    }

    /// Valida rigorosamente la struttura della configurazione e tutti i vincoli di sicurezza applicativi.
    pub fn validate(config: &Config) -> Result<(), ConfigError> {
        // 1. Validazione rigorosa della versione dello schema
        if config.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(ConfigError::UnsupportedSchemaVersion {
                expected: CURRENT_SCHEMA_VERSION,
                found: config.schema_version,
            });
        }

        // 2. Validazione rate limit globale e hotkey di sistema
        if config.settings.max_events_per_second < MIN_EVENTS_PER_SECOND
            || config.settings.max_events_per_second > MAX_EVENTS_PER_SECOND
        {
            return Err(ConfigError::InvalidRateLimit {
                min: MIN_EVENTS_PER_SECOND,
                max: MAX_EVENTS_PER_SECOND,
            });
        }
        if config.settings.resume_hotkey == Some(config.settings.kill_switch) {
            return Err(ConfigError::ResumeHotkeyConflictsWithKillSwitch);
        }
        for group in &config.groups {
            if !is_valid_ascii_identifier(group) {
                return Err(ConfigError::InvalidRuleGroup);
            }
        }
        for (group, wf) in &config.group_filters {
            let trimmed = group.trim();
            if trimmed == crate::core::rule::default_rule_group()
                || !is_valid_ascii_identifier(trimmed)
            {
                return Err(ConfigError::InvalidRuleGroup);
            }
            Self::validate_window_filter(wf)?;
        }

        // 3. Validazione regole
        let mut seen_ids = HashSet::new();
        let mut seen_names = HashSet::new();
        for rule in &config.rules {
            let id = rule.id.trim();
            if !is_valid_ascii_identifier(id) {
                return Err(ConfigError::InvalidRuleId);
            }

            let name = rule.name.trim();
            if name.is_empty() {
                return Err(ConfigError::EmptyRuleName);
            }
            if !seen_names.insert(name.to_ascii_lowercase()) {
                return Err(ConfigError::DuplicateRuleName);
            }

            for g in &rule.groups {
                if !is_valid_ascii_identifier(g) {
                    return Err(ConfigError::InvalidRuleGroup);
                }
            }
            if rule.enabled && rule.effective_groups().is_empty() {
                return Err(ConfigError::EnabledRuleWithoutGroup);
            }

            if let Some(wf) = &rule.window_filter {
                Self::validate_window_filter(wf)?;
            }

            if !seen_ids.insert(id.to_string()) {
                return Err(ConfigError::DuplicateRuleId(rule.id.clone()));
            }

            if rule.actions.is_empty() {
                return Err(ConfigError::EmptyActionSequence(rule.id.clone()));
            }

            if rule.actions.len() > MAX_ACTIONS_PER_SEQUENCE {
                return Err(ConfigError::InvalidSequenceLength {
                    rule_id: rule.id.clone(),
                    count: rule.actions.len(),
                    max: MAX_ACTIONS_PER_SEQUENCE,
                });
            }

            for action in &rule.actions {
                match action {
                    InputAction::SendText { text } => {
                        let len = text.chars().count();
                        if len == 0 {
                            return Err(ConfigError::EmptyText(rule.id.clone()));
                        }
                        if len > MAX_SEND_TEXT_LENGTH {
                            return Err(ConfigError::TextTooLong {
                                rule_id: rule.id.clone(),
                                len,
                                max: MAX_SEND_TEXT_LENGTH,
                            });
                        }
                    }
                    InputAction::Delay { milliseconds } => {
                        if *milliseconds < MIN_DELAY_MS || *milliseconds > MAX_DELAY_MS {
                            return Err(ConfigError::InvalidDelay {
                                rule_id: rule.id.clone(),
                                min: MIN_DELAY_MS,
                                max: MAX_DELAY_MS,
                            });
                        }
                    }
                    InputAction::MouseScroll { steps, .. } => {
                        let abs_steps = steps.unsigned_abs();
                        if abs_steps == 0 || abs_steps > MAX_MOUSE_SCROLL_STEPS as u32 {
                            return Err(ConfigError::InvalidMouseScroll {
                                rule_id: rule.id.clone(),
                                max: MAX_MOUSE_SCROLL_STEPS,
                            });
                        }
                    }
                    InputAction::MouseMove { dx, dy } => {
                        if (*dx == 0 && *dy == 0)
                            || dx.unsigned_abs() > MAX_MOUSE_MOVE_DELTA as u32
                            || dy.unsigned_abs() > MAX_MOUSE_MOVE_DELTA as u32
                        {
                            return Err(ConfigError::InvalidMouseMove {
                                rule_id: rule.id.clone(),
                                max: MAX_MOUSE_MOVE_DELTA,
                            });
                        }
                    }
                    InputAction::KeyClick { .. }
                    | InputAction::KeyDown { .. }
                    | InputAction::KeyUp { .. }
                    | InputAction::MouseClick { .. }
                    | InputAction::MouseDown { .. }
                    | InputAction::MouseUp { .. } => {}
                }
            }
        }

        Ok(())
    }

    fn validate_window_filter(wf: &crate::core::rule::WindowFilter) -> Result<(), ConfigError> {
        if wf.is_empty() {
            return Err(ConfigError::EmptyWindowFilter);
        }
        if let Some(proc_name) = &wf.process_name {
            if proc_name.chars().count() > MAX_WINDOW_FILTER_LEN {
                return Err(ConfigError::WindowFilterTooLong {
                    max: MAX_WINDOW_FILTER_LEN,
                });
            }
            if proc_name.contains('\\') || proc_name.contains('/') || proc_name.contains('\0') {
                return Err(ConfigError::InvalidProcessNameFilter);
            }
        }
        Ok(())
    }

    /// Deserializza e valida una stringa TOML.
    pub fn load_from_str(content: &str) -> Result<Config, ConfigError> {
        let config: Config = toml::from_str(content)
            .map_err(|err| ConfigError::from_toml_de_error(&err, content))?;
        Self::validate(&config)?;
        Ok(config)
    }

    /// Carica e valida la configurazione da un file su disco.
    pub fn load_from_file(path: &Path) -> Result<Config, ConfigError> {
        let content = fs::read_to_string(path)?;
        Self::load_from_str(&content)
    }

    /// Serializza la configurazione in formato TOML stringa.
    pub fn save_to_string(config: &Config) -> Result<String, ConfigError> {
        Self::validate(config)?;
        let toml_str =
            toml::to_string_pretty(config).map_err(|err| ConfigError::TomlSer(err.to_string()))?;
        Ok(toml_str)
    }

    /// Salva la configurazione validata su file creando le directory genitrici se necessario.
    pub fn save_to_file(config: &Config, path: &Path) -> Result<(), ConfigError> {
        let toml_str = Self::save_to_string(config)?;
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, toml_str)?;
        Ok(())
    }

    /// Salva la configurazione in modo atomico e sicuro rispetto a file esistenti non validi.
    ///
    /// Flusso:
    /// 1. Valida e serializza `config` tramite `Self::save_to_string(config)?`.
    /// 2. Se `path` esiste già, ne verifica la validità con `Self::load_from_file(path)`:
    ///    - se non valido e `!allow_backup_and_overwrite_invalid`, restituisce
    ///      `Err(ConfigError::RefusedOverwriteInvalidFile)` lasciando intatto il file originale;
    ///    - se non valido e `allow_backup_and_overwrite_invalid == true`, crea una copia di
    ///      backup (`Self::create_backup_copy(path)?`) prima di sovrascrivere.
    /// 3. Crea la directory genitrice se mancante e scrive in modo atomico tramite file
    ///    temporaneo (`<nome>.tmp.<pid>.<counter>`), `sync_all()` e `fs::rename` (rimuovendo il
    ///    file temporaneo in caso di errore).
    ///
    /// Restituisce `Ok(Some(backup_path))` se è stato creato un backup, `Ok(None)` altrimenti.
    pub fn save_to_file_safe(
        config: &Config,
        path: &Path,
        allow_backup_and_overwrite_invalid: bool,
    ) -> Result<Option<PathBuf>, ConfigError> {
        let toml_str = Self::save_to_string(config)?;

        let mut backup_path = None;
        if path.exists() && Self::load_from_file(path).is_err() {
            if !allow_backup_and_overwrite_invalid {
                return Err(ConfigError::RefusedOverwriteInvalidFile);
            }
            backup_path = Some(Self::create_backup_copy(path)?);
        }

        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ConfigError::InvalidPath(path.to_path_buf()))?;
        let counter = TMP_FILE_COUNTER.fetch_add(1, Ordering::SeqCst);
        let tmp_name = format!("{}.tmp.{}.{}", file_name, std::process::id(), counter);
        let tmp_path = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.join(&tmp_name))
            .unwrap_or_else(|| PathBuf::from(&tmp_name));

        let write_result = (|| -> Result<(), ConfigError> {
            let mut file = fs::File::create(&tmp_path)?;
            file.write_all(toml_str.as_bytes())?;
            file.sync_all()?;
            drop(file);
            fs::rename(&tmp_path, path)?;
            Ok(())
        })();

        if let Err(err) = write_result {
            let _ = fs::remove_file(&tmp_path);
            return Err(err);
        }

        Ok(backup_path)
    }

    /// Crea una copia di sicurezza del file di configurazione senza spostarlo né modificarlo.
    ///
    /// In accordo con il principio di sicurezza *fail-closed* (Sez. 3.5), i file non validi
    /// non vengono mai rinominati o spostati automaticamente all'avvio. La copia (`.bak.<timestamp>`)
    /// viene creata lasciando intatto l'originale per permettere il ripristino o l'analisi da parte dell'utente.
    pub fn backup_corrupted_file(path: &Path) -> Result<PathBuf, ConfigError> {
        Self::create_backup_copy(path)
    }

    /// Crea una copia di sicurezza esplicita (`nome.bak.<timestamp>`) del file indicato,
    /// lasciando inalterato il file di origine.
    pub fn create_backup_copy(path: &Path) -> Result<PathBuf, ConfigError> {
        if !path.exists() {
            return Err(ConfigError::InvalidPath(path.to_path_buf()));
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config.toml");

        let backup_name = format!("{}.bak.{}", file_name, now);
        let backup_path = path
            .parent()
            .map(|p| p.join(&backup_name))
            .unwrap_or_else(|| PathBuf::from(&backup_name));

        fs::copy(path, &backup_path)?;
        Ok(backup_path)
    }

    /// Esporta in modo atomico una configurazione validata nel percorso di destinazione scelto dall'utente.
    pub fn export_to_file(config: &Config, dest_path: &Path) -> Result<(), ConfigError> {
        let toml_str = Self::save_to_string(config)?;
        if let Some(parent) = dest_path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }

        let file_name = dest_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ConfigError::InvalidPath(dest_path.to_path_buf()))?;
        let counter = TMP_FILE_COUNTER.fetch_add(1, Ordering::SeqCst);
        let tmp_name = format!("{}.tmp.{}.{}", file_name, std::process::id(), counter);
        let tmp_path = dest_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.join(&tmp_name))
            .unwrap_or_else(|| PathBuf::from(&tmp_name));

        let write_result = (|| -> Result<(), ConfigError> {
            let mut file = fs::File::create(&tmp_path)?;
            file.write_all(toml_str.as_bytes())?;
            file.sync_all()?;
            drop(file);
            fs::rename(&tmp_path, dest_path)?;
            Ok(())
        })();

        if let Err(err) = write_result {
            let _ = fs::remove_file(&tmp_path);
            return Err(err);
        }

        Ok(())
    }
}

/// Verifica che un identificatore (ID regola o nome gruppo) sia non vuoto e composto
/// esclusivamente da caratteri ASCII alfanumerici, `_` o `-`.
pub fn is_valid_ascii_identifier(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}
