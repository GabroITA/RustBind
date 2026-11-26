use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::i18n::Language;
use rustbind::config::error::ConfigError;
use rustbind::config::manager::{ConfigManager, is_valid_ascii_identifier};
use rustbind::config::schema::{Config, default_kill_switch};
use rustbind::core::MAX_ACTIONS_PER_SEQUENCE;
use rustbind::core::action::{InputAction, MouseButton, MouseWheelDirection};
use rustbind::core::binding::KeyBinding;
use rustbind::core::key::VirtualKey;
use rustbind::core::modifiers::Modifiers;
use rustbind::core::rule::{Rule, Trigger, default_rule_group, default_rule_groups};

/// Tipi di azione aggiungibili in una regola (escluso `LaunchProcess`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewActionKind {
    KeyClick,
    KeyDown,
    KeyUp,
    SendText,
    Delay,
    MouseClick,
    MouseDown,
    MouseUp,
    MouseScroll,
    MouseMove,
}

/// Stato di caricamento del file `config.toml` su disco.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadedFileKind {
    Missing,
    Valid,
    Invalid { error_message: String },
}

/// Riepilogo statistico di un gruppo di regole per la vista visuale Gruppi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupSummary {
    pub name: String,
    pub total_rules: usize,
    pub enabled_rules: usize,
}

/// Gruppo di regole che condividono esattamente la stessa sequenza di azioni (`actions`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateActionCluster {
    pub rule_indices: Vec<usize>,
}

/// Filtro per gruppi applicato nell'elenco regole (supporta selezione multipla).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RuleGroupFilter {
    /// Nessuna restrizione: mostra regole di tutti i gruppi (incluse quelle senza gruppo).
    #[default]
    All,
    /// Selezione multipla: mostra le regole prive di gruppo (`include_no_group`) e/o
    /// appartenenti ad almeno uno dei gruppi selezionati (`selected_groups`).
    Selection {
        include_no_group: bool,
        selected_groups: Vec<String>,
    },
}

impl RuleGroupFilter {
    /// Restituisce `true` se è selezionata l'opzione "Nessun gruppo".
    pub fn includes_no_group(&self) -> bool {
        matches!(
            self,
            Self::Selection {
                include_no_group: true,
                ..
            }
        )
    }

    /// Restituisce `true` se `name` è selezionato nel filtro multiplo.
    pub fn contains_group(&self, name: &str) -> bool {
        match self {
            Self::All => false,
            Self::Selection {
                selected_groups, ..
            } => selected_groups.iter().any(|g| g == name),
        }
    }

    /// Attiva o disattiva il filtro sulle regole senza gruppo, normalizzando su `All` se vuoto.
    pub fn set_no_group(&mut self, checked: bool) {
        let selected_groups = match std::mem::take(self) {
            Self::All => Vec::new(),
            Self::Selection {
                selected_groups, ..
            } => selected_groups,
        };
        *self = Self::Selection {
            include_no_group: checked,
            selected_groups,
        };
        self.normalize();
    }

    /// Attiva o disattiva la selezione di un gruppo nel filtro, normalizzando su `All` se vuoto.
    pub fn set_group(&mut self, name: &str, checked: bool) {
        let (include_no_group, mut selected_groups) = match std::mem::take(self) {
            Self::All => (false, Vec::new()),
            Self::Selection {
                include_no_group,
                selected_groups,
            } => (include_no_group, selected_groups),
        };
        if checked {
            if !selected_groups.iter().any(|g| g == name) {
                selected_groups.push(name.to_string());
            }
        } else {
            selected_groups.retain(|g| g != name);
        }
        *self = Self::Selection {
            include_no_group,
            selected_groups,
        };
        self.normalize();
    }

    /// Rimuove eventuali gruppi non più esistenti e normalizza su `All` se nulla resta selezionato.
    pub fn prune_missing(&mut self, available_groups: &[String]) {
        if let Self::Selection {
            selected_groups, ..
        } = self
        {
            selected_groups.retain(|g| available_groups.iter().any(|avail| avail == g));
            self.normalize();
        }
    }

    /// Verifica se le assegnazioni gruppo di una regola soddisfano il filtro corrente.
    pub fn matches(&self, rule_groups: &[String]) -> bool {
        match self {
            Self::All => true,
            Self::Selection {
                include_no_group,
                selected_groups,
            } => {
                (*include_no_group && rule_groups.is_empty())
                    || rule_groups
                        .iter()
                        .any(|g| selected_groups.iter().any(|sel| sel == g))
            }
        }
    }

    fn normalize(&mut self) {
        if let Self::Selection {
            include_no_group: false,
            selected_groups,
        } = self
            && selected_groups.is_empty()
        {
            *self = Self::All;
        }
    }
}

/// Statistiche riepilogative di regole e gruppi della configurazione corrente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConfigStats {
    pub total_rules: usize,
    pub enabled_rules: usize,
    pub total_groups: usize,
    pub active_groups: usize,
    pub active_rules_in_active_groups: usize,
}

/// Conflitto di trigger rilevato in tempo reale nella bozza corrente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleTriggerConflict {
    /// La regola condivide la combinazione del Kill-Switch e verrà sempre ignorata dal daemon.
    KillSwitch,
    /// La regola condivide la combinazione di `resume_hotkey` e verrà ignorata dal daemon.
    ResumeHotkey,
    /// Un'altra regola attiva con contesto sovrapponibile usa lo stesso trigger.
    DuplicateWithRule {
        other_rule_names: Vec<String>,
        shadowed: bool,
    },
}

/// Candidato di importazione validato pronto per eventuale conferma di sovrascrittura.
#[derive(Debug, Clone)]
pub struct ImportCandidate {
    pub target_name: String,
    pub config: Config,
    pub already_exists: bool,
}

const MAX_UNDO_HISTORY: usize = 64;

/// Stato completo dell'editor visuale della configurazione (bozza, validazione e persistenza).
#[derive(Debug, Clone)]
pub struct EditorState {
    pub config_path: Option<PathBuf>,
    pub profiles_dir: Option<PathBuf>,
    pub active_profile_marker: Option<PathBuf>,
    pub active_profile_name: String,
    pub path_error: Option<String>,
    pub file_kind: LoadedFileKind,
    pub saved_baseline: Config,
    pub draft: Config,
    pub history_checkpoint: Config,
    undo_stack: Vec<Config>,
    redo_stack: Vec<Config>,
    pub selected_rule_idx: Option<usize>,
    pub confirm_overwrite_invalid: bool,
    pub last_backup_path: Option<PathBuf>,
    pub language: Language,
    pub notice: Option<(bool, String)>,
}

impl EditorState {
    /// Inizializza l'editor caricando il file `.toml` attivo dentro `%APPDATA%\RustBind\configs\`.
    pub fn load_default() -> Self {
        let profiles_dir = ConfigManager::default_profiles_dir().ok();
        let marker = ConfigManager::default_active_profile_marker().ok();
        match ConfigManager::default_active_config_path() {
            Ok(path) => Self::from_paths(Some(path), profiles_dir, marker, None),
            Err(err) => Self::from_paths(None, profiles_dir, marker, Some(err.to_string())),
        }
    }

    /// Inizializza l'editor a partire da un percorso esplicito (utile per test su directory temporanee).
    #[cfg(test)]
    pub fn from_path(config_path: Option<PathBuf>, path_error: Option<String>) -> Self {
        let profiles_dir = config_path
            .as_ref()
            .and_then(|p| p.parent())
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_path_buf());
        let marker = profiles_dir.as_ref().map(|d| {
            if d.file_name().is_some_and(|n| n == "configs") {
                d.parent().unwrap_or(d).join(".active_config")
            } else {
                d.join(".active_config")
            }
        });
        Self::from_paths(config_path, profiles_dir, marker, path_error)
    }

    fn actual_config_filename(config_path: Option<&Path>) -> String {
        config_path
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| "config.toml".to_string())
    }

    fn from_paths(
        config_path: Option<PathBuf>,
        profiles_dir: Option<PathBuf>,
        active_profile_marker: Option<PathBuf>,
        path_error: Option<String>,
    ) -> Self {
        let initial_profile_name = Self::actual_config_filename(config_path.as_deref());
        let mut state = Self {
            config_path,
            profiles_dir,
            active_profile_marker,
            active_profile_name: initial_profile_name,
            path_error,
            file_kind: LoadedFileKind::Missing,
            saved_baseline: Config::default(),
            draft: Config::default(),
            history_checkpoint: Config::default(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            selected_rule_idx: None,
            confirm_overwrite_invalid: false,
            last_backup_path: None,
            language: Language::default(),
            notice: None,
        };
        state.reload_from_disk();
        state
    }

    /// Ricarica il file `.toml` attivo da disco senza mai creare o modificare il file originale.
    pub fn reload_from_disk(&mut self) {
        self.confirm_overwrite_invalid = false;
        self.active_profile_name = self.read_active_profile_name();
        if let Some(dir) = &self.profiles_dir {
            self.config_path = Some(dir.join(&self.active_profile_name));
        }
        let Some(path) = &self.config_path else {
            return;
        };

        if !path.exists() {
            self.file_kind = LoadedFileKind::Missing;
            self.saved_baseline = Config::default();
            self.draft = Config::default();
            self.selected_rule_idx = None;
            self.reset_history();
            return;
        }

        match ConfigManager::load_from_file(path) {
            Ok(cfg) => {
                self.file_kind = LoadedFileKind::Valid;
                self.saved_baseline = cfg.clone();
                self.draft = cfg;
                self.selected_rule_idx = if self.draft.rules.is_empty() {
                    None
                } else {
                    Some(0)
                };
            }
            Err(err) => {
                self.file_kind = LoadedFileKind::Invalid {
                    error_message: err.to_string(),
                };
                self.saved_baseline = Config::default();
                self.draft = Config::default();
                self.selected_rule_idx = None;
            }
        }
        self.reset_history();
    }

    /// Reimposta la cronologia Undo/Redo allineandola alla bozza corrente.
    pub fn reset_history(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.history_checkpoint = self.draft.clone();
    }

    /// Registra nella cronologia Undo lo stato precedente se la bozza corrente è cambiata.
    pub fn sync_history_from_draft(&mut self) {
        if self.is_locked_invalid() || self.draft == self.history_checkpoint {
            return;
        }
        if self.undo_stack.len() >= MAX_UNDO_HISTORY {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(self.history_checkpoint.clone());
        self.redo_stack.clear();
        self.history_checkpoint = self.draft.clone();
    }

    fn clamp_selection_after_restore(&mut self) {
        if self.draft.rules.is_empty() {
            self.selected_rule_idx = None;
        } else if let Some(idx) = self.selected_rule_idx {
            if idx >= self.draft.rules.len() {
                self.selected_rule_idx = Some(self.draft.rules.len() - 1);
            }
        } else {
            self.selected_rule_idx = Some(0);
        }
    }

    /// Indica se è disponibile almeno una modifica annullabile via Undo.
    pub fn can_undo(&self) -> bool {
        !self.is_locked_invalid()
            && (!self.undo_stack.is_empty() || self.draft != self.history_checkpoint)
    }

    /// Indica se è disponibile almeno una modifica ripristinabile via Redo.
    pub fn can_redo(&self) -> bool {
        !self.is_locked_invalid() && !self.redo_stack.is_empty()
    }

    /// Annulla l'ultima modifica salvando lo stato corrente sullo stack Redo.
    pub fn undo(&mut self) -> bool {
        if self.is_locked_invalid() {
            return false;
        }
        let target = if self.draft != self.history_checkpoint {
            self.history_checkpoint.clone()
        } else if let Some(prev) = self.undo_stack.pop() {
            prev
        } else {
            return false;
        };
        if self.redo_stack.len() >= MAX_UNDO_HISTORY {
            self.redo_stack.remove(0);
        }
        self.redo_stack.push(self.draft.clone());
        self.draft = target;
        self.history_checkpoint = self.draft.clone();
        self.clamp_selection_after_restore();
        true
    }

    /// Ripristina l'ultima modifica annullata tramite Undo.
    pub fn redo(&mut self) -> bool {
        if self.is_locked_invalid() {
            return false;
        }
        let Some(next) = self.redo_stack.pop() else {
            return false;
        };
        if self.undo_stack.len() >= MAX_UNDO_HISTORY {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(self.draft.clone());
        self.draft = next;
        self.history_checkpoint = self.draft.clone();
        self.clamp_selection_after_restore();
        true
    }

    /// Indica se l'editor è bloccato in scrittura perché il file esistente su disco è non valido.
    pub fn is_locked_invalid(&self) -> bool {
        matches!(self.file_kind, LoadedFileKind::Invalid { .. })
    }

    /// Indica se la bozza differisce dall'ultima configurazione salvata/caricata.
    pub fn is_dirty(&self) -> bool {
        !self.is_locked_invalid() && self.draft != self.saved_baseline
    }

    /// Ripristina la bozza all'ultima versione caricata/salvata.
    pub fn discard_changes(&mut self) {
        if self.is_locked_invalid() {
            return;
        }
        self.draft = self.saved_baseline.clone();
        self.selected_rule_idx = if self.draft.rules.is_empty() {
            None
        } else {
            Some(0)
        };
        self.reset_history();
        self.notice = Some((false, self.language.s().toast_discarded.to_string()));
    }

    /// Esegue la validazione live tramite `ConfigManager::validate`.
    pub fn validate_live(&self) -> Result<(), ConfigError> {
        ConfigManager::validate(&self.draft)
    }

    /// Indica se il pulsante "Salva configurazione" può essere abilitato.
    pub fn can_save(&self) -> bool {
        if self.is_locked_invalid() || self.config_path.is_none() {
            return false;
        }
        let has_changes_or_new_rules = self.is_dirty()
            || (matches!(self.file_kind, LoadedFileKind::Missing) && !self.draft.rules.is_empty());
        has_changes_or_new_rules && self.validate_live().is_ok()
    }

    /// Salva la bozza su disco in modo atomico e sicuro (`allow_backup_and_overwrite_invalid = false`).
    pub fn save(&mut self) -> Result<(), String> {
        if self.is_locked_invalid() {
            let msg =
                "Impossibile salvare: il file su disco non è valido (usa il ripristino esplicito)."
                    .to_string();
            self.notice = Some((true, msg.clone()));
            return Err(msg);
        }

        let Some(path) = self.config_path.as_ref() else {
            let msg = self
                .path_error
                .clone()
                .unwrap_or_else(|| "Percorso config.toml non disponibile.".to_string());
            self.notice = Some((true, msg.clone()));
            return Err(msg);
        };

        match ConfigManager::save_to_file_safe(&self.draft, path, false) {
            Ok(_) => {
                self.file_kind = LoadedFileKind::Valid;
                self.saved_baseline = self.draft.clone();
                self.confirm_overwrite_invalid = false;
                self.notice = Some((false, self.language.s().toast_saved.to_string()));
                Ok(())
            }
            Err(err) => {
                let msg = format!("Salvataggio fallito: {}", err);
                self.notice = Some((true, msg.clone()));
                Err(msg)
            }
        }
    }

    fn read_active_profile_name(&self) -> String {
        if let Some(marker) = &self.active_profile_marker
            && let Ok(raw) = fs::read_to_string(marker)
            && let Ok(name) = ConfigManager::sanitize_profile_filename(&raw)
        {
            return name;
        }
        "config.toml".to_string()
    }

    fn persist_active_profile_marker(&self, profile_name: &str) -> Result<(), String> {
        let Some(marker) = &self.active_profile_marker else {
            return Ok(());
        };
        if let Some(parent) = marker.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(marker, profile_name).map_err(|e| e.to_string())
    }

    /// Restituisce i file `.toml` disponibili nella cartella `configs/` (includendo sempre `config.toml` e il file attivo).
    pub fn available_profiles(&self) -> Vec<String> {
        let mut names = HashSet::new();
        names.insert("config.toml".to_string());
        names.insert(self.active_profile_name.clone());

        if let Some(dir) = &self.profiles_dir
            && let Ok(entries) = fs::read_dir(dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file()
                    && let Some(ext) = path.extension().and_then(|e| e.to_str())
                    && ext.eq_ignore_ascii_case("toml")
                    && let Some(fname) = path.file_name().and_then(|n| n.to_str())
                {
                    names.insert(fname.to_string());
                }
            }
        }

        let mut list: Vec<String> = names.into_iter().collect();
        list.sort_by_key(|n| {
            let is_not_base = n != "config.toml";
            (is_not_base, n.to_ascii_lowercase())
        });
        list
    }

    /// Valida un file `.toml` esterno per l'importazione e segnala se sovrascriverà un profilo esistente.
    pub fn prepare_import_from_path(&self, src_path: &Path) -> Result<ImportCandidate, String> {
        let cfg = ConfigManager::load_from_file(src_path).map_err(|e| e.to_string())?;
        let raw_name = src_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| "Nome file di origine non valido.".to_string())?;
        let target_name = ConfigManager::sanitize_profile_filename(raw_name)
            .map_err(|e| format!("Nome file destinazione non valido: {}", e))?;

        let exists_in_profiles = self
            .profiles_dir
            .as_ref()
            .is_some_and(|dir| dir.join(&target_name).exists());
        let exists_as_active = target_name == self.active_profile_name
            && self.config_path.as_ref().is_some_and(|p| p.exists());

        Ok(ImportCandidate {
            target_name,
            config: cfg,
            already_exists: exists_in_profiles || exists_as_active,
        })
    }

    /// Salva un candidato importato nella cartella `configs/` e lo attiva immediatamente.
    ///
    /// Restituisce `Ok(true)` per attivare la ricarica nel daemon.
    pub fn apply_import(&mut self, candidate: ImportCandidate) -> Result<bool, String> {
        if self.is_locked_invalid() {
            return Err(
                "File attivo non valido: ripristina prima dalla sezione File di configurazione."
                    .to_string(),
            );
        }
        let Some(profiles_dir) = &self.profiles_dir else {
            return Err("Directory dei profili non disponibile.".to_string());
        };
        let profile_path = profiles_dir.join(&candidate.target_name);
        ConfigManager::save_to_file_safe(&candidate.config, &profile_path, true)
            .map_err(|e| format!("Importazione fallita: {}", e))?;
        self.persist_active_profile_marker(&candidate.target_name)?;

        self.active_profile_name = candidate.target_name.clone();
        self.config_path = Some(profile_path);
        self.file_kind = LoadedFileKind::Valid;
        self.saved_baseline = candidate.config.clone();
        self.draft = candidate.config;
        self.selected_rule_idx = if self.draft.rules.is_empty() {
            None
        } else {
            Some(0)
        };
        self.reset_history();
        self.show_imported_notice(&candidate.target_name, true);
        Ok(true)
    }

    fn show_imported_notice(&mut self, name: &str, activated: bool) {
        let msg = if activated {
            format!(
                "Configurazione '{}' importata e applicata al profilo attivo.",
                name
            )
        } else {
            format!("Configurazione '{}' importata con successo.", name)
        };
        self.notice = Some((false, msg));
    }

    /// Attiva un singolo file `.toml` dentro `configs/` (senza duplicare o sovrascrivere altri file).
    pub fn switch_active_profile(&mut self, target_name: &str) -> Result<(), String> {
        if target_name == self.active_profile_name {
            return Ok(());
        }
        if self.is_locked_invalid() {
            return Err(
                "Impossibile cambiare configurazione mentre il file attivo è bloccato in stato non valido."
                    .to_string(),
            );
        }
        let Some(profiles_dir) = &self.profiles_dir else {
            return Err("Directory dei profili non disponibile.".to_string());
        };

        let target_path = profiles_dir.join(target_name);
        let (next_cfg, next_kind) = if target_path.exists() {
            let cfg = ConfigManager::load_from_file(&target_path)
                .map_err(|e| format!("Impossibile caricare '{}': {}", target_name, e))?;
            (cfg, LoadedFileKind::Valid)
        } else if target_name == "config.toml" {
            (Config::default(), LoadedFileKind::Missing)
        } else {
            return Err(format!(
                "File di configurazione '{}' non trovato.",
                target_name
            ));
        };

        self.persist_active_profile_marker(target_name)?;

        self.active_profile_name = target_name.to_string();
        self.config_path = Some(target_path);
        self.file_kind = next_kind;
        self.saved_baseline = next_cfg.clone();
        self.draft = next_cfg;
        self.selected_rule_idx = if self.draft.rules.is_empty() {
            None
        } else {
            Some(0)
        };
        self.reset_history();
        self.notice = Some((
            false,
            format!("Configurazione attiva impostata su '{}'.", target_name),
        ));
        Ok(())
    }

    /// Esporta la configurazione corrente in bozza nel percorso scelto dall'utente.
    pub fn export_current_draft(&self, dest_path: &Path) -> Result<(), String> {
        ConfigManager::export_to_file(&self.draft, dest_path)
            .map_err(|e| format!("Esportazione fallita: {}", e))
    }

    /// Flusso esplicito di ripristino per file corrotto: richiede conferma e crea copia `.bak.<timestamp>`.
    pub fn restore_with_backup(&mut self) -> Result<PathBuf, String> {
        if !self.confirm_overwrite_invalid {
            let msg = "Spunta la casella di conferma prima di creare il backup e ripristinare."
                .to_string();
            self.notice = Some((true, msg.clone()));
            return Err(msg);
        }

        let Some(path) = self.config_path.clone() else {
            let msg = "Percorso config.toml non disponibile.".to_string();
            self.notice = Some((true, msg.clone()));
            return Err(msg);
        };

        let empty_cfg = Config::default();
        match ConfigManager::save_to_file_safe(&empty_cfg, &path, true) {
            Ok(Some(backup_path)) => {
                self.file_kind = LoadedFileKind::Valid;
                self.saved_baseline = empty_cfg.clone();
                self.draft = empty_cfg;
                self.selected_rule_idx = None;
                self.confirm_overwrite_invalid = false;
                self.last_backup_path = Some(backup_path.clone());
                self.reset_history();
                self.notice = Some((
                    false,
                    format!(
                        "Backup creato in {} e configurazione vuota ripristinata.",
                        backup_path.display()
                    ),
                ));
                Ok(backup_path)
            }
            Ok(None) => {
                self.file_kind = LoadedFileKind::Valid;
                self.saved_baseline = empty_cfg.clone();
                self.draft = empty_cfg;
                self.selected_rule_idx = None;
                self.confirm_overwrite_invalid = false;
                self.reset_history();
                self.notice = Some((
                    false,
                    "Configurazione ripristinata con valori predefiniti.".to_string(),
                ));
                Ok(path)
            }
            Err(err) => {
                let msg = format!("Ripristino fallito: {}", err);
                self.notice = Some((true, msg.clone()));
                Err(msg)
            }
        }
    }

    /// Ripristina il kill-switch della bozza al valore predefinito (`Ctrl+Alt+Shift+F12`).
    pub fn restore_default_kill_switch(&mut self) {
        self.draft.settings.kill_switch = default_kill_switch();
    }

    /// Genera un identificatore regola univoco (`regola_1`, `regola_2`, ...) non presente in bozza.
    pub fn next_unique_rule_id(&self) -> String {
        let existing: HashSet<&str> = self.draft.rules.iter().map(|r| r.id.as_str()).collect();
        let mut idx = 1usize;
        loop {
            let candidate = format!("regola_{}", idx);
            if !existing.contains(candidate.as_str()) {
                return candidate;
            }
            idx += 1;
        }
    }

    /// Genera un nome regola univoco (case-insensitive) a partire da un prefisso/base.
    pub fn next_unique_rule_name(&self, base_name: &str) -> String {
        let existing: HashSet<String> = self
            .draft
            .rules
            .iter()
            .map(|r| r.name.trim().to_ascii_lowercase())
            .collect();
        let trimmed_base = base_name.trim();
        let seed = if trimmed_base.is_empty() {
            "Regola"
        } else {
            trimmed_base
        };
        if !existing.contains(&seed.to_ascii_lowercase()) {
            return seed.to_string();
        }
        let mut idx = 2usize;
        loop {
            let candidate = format!("{} {}", seed, idx);
            if !existing.contains(&candidate.to_ascii_lowercase()) {
                return candidate;
            }
            idx += 1;
        }
    }

    /// Aggiunge una nuova regola predefinita (`Ctrl+Alt+F1`, senza azioni iniziali) e la seleziona.
    pub fn add_rule(&mut self) -> usize {
        let id = self.next_unique_rule_id();
        let raw_num = id.strip_prefix("regola_").unwrap_or("1");
        let base_name = self.language.fmt_default_rule_name(raw_num);
        let name = self.next_unique_rule_name(&base_name);
        let rule = Rule {
            id,
            name,
            groups: default_rule_groups(),
            enabled: true,
            wait_for_modifier_release: false,
            window_filter: None,
            trigger: Trigger::Hotkey {
                binding: KeyBinding::new(
                    Modifiers {
                        ctrl: true,
                        alt: true,
                        shift: false,
                        win: false,
                    },
                    VirtualKey::F1,
                ),
            },
            actions: Vec::new(),
        };
        self.draft.rules.push(rule);
        let new_idx = self.draft.rules.len().saturating_sub(1);
        self.selected_rule_idx = Some(new_idx);
        new_idx
    }

    /// Duplica la regola all'indice `idx` assegnando ID e nome univoci e selezionando la copia.
    pub fn duplicate_rule(&mut self, idx: usize) -> Option<usize> {
        let source = self.draft.rules.get(idx)?.clone();
        let new_id = self.next_unique_rule_id();
        let copy_base = self.language.fmt_duplicate_rule_name(&source.name);
        let new_name = self.next_unique_rule_name(&copy_base);
        let duplicated = Rule {
            id: new_id,
            name: new_name,
            groups: source.groups,
            enabled: source.enabled,
            wait_for_modifier_release: source.wait_for_modifier_release,
            window_filter: source.window_filter,
            trigger: source.trigger,
            actions: source.actions,
        };
        let insert_idx = (idx + 1).min(self.draft.rules.len());
        self.draft.rules.insert(insert_idx, duplicated);
        self.selected_rule_idx = Some(insert_idx);
        Some(insert_idx)
    }

    /// Restituisce gli indici delle regole che soddisfano testo di ricerca, gruppo e stato attivo/disattivo.
    pub fn filtered_rule_indices_with_filters(
        &self,
        query: &str,
        group_filter: &RuleGroupFilter,
        only_enabled: bool,
        only_disabled: bool,
    ) -> Vec<usize> {
        let needle = query.trim().to_ascii_lowercase();

        self.draft
            .rules
            .iter()
            .enumerate()
            .filter(|(_, rule)| {
                if only_enabled && !only_disabled && !rule.enabled {
                    return false;
                }
                if only_disabled && !only_enabled && rule.enabled {
                    return false;
                }

                let groups = rule.effective_groups();
                if !group_filter.matches(&groups) {
                    return false;
                }

                if needle.is_empty() {
                    return true;
                }
                let trigger_str = match &rule.trigger {
                    Trigger::Hotkey { binding } => binding.to_string(),
                };
                let groups_match = groups
                    .iter()
                    .any(|g| g.to_ascii_lowercase().contains(&needle));
                let rule_ctx_match = rule.window_filter.as_ref().is_some_and(|wf| {
                    wf.process_name
                        .as_deref()
                        .is_some_and(|p| p.to_ascii_lowercase().contains(&needle))
                });
                let group_ctx_match = groups.iter().any(|g| {
                    self.draft
                        .group_filter(g)
                        .and_then(|wf| wf.process_name.as_deref())
                        .is_some_and(|p| p.to_ascii_lowercase().contains(&needle))
                });
                rule.id.to_ascii_lowercase().contains(&needle)
                    || rule.name.to_ascii_lowercase().contains(&needle)
                    || groups_match
                    || rule_ctx_match
                    || group_ctx_match
                    || trigger_str.to_ascii_lowercase().contains(&needle)
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    /// Restituisce gli indici delle regole che soddisfano il filtro di ricerca (id, nome, gruppo o trigger).
    #[cfg(test)]
    pub fn filtered_rule_indices(&self, query: &str) -> Vec<usize> {
        self.filtered_rule_indices_with_filters(query, &RuleGroupFilter::All, false, false)
    }

    /// Calcola le 5 statistiche riepilogative di regole e gruppi sulla bozza corrente.
    pub fn config_stats(&self) -> ConfigStats {
        let groups = self.available_groups();
        let total_groups = groups.len();
        let active_groups = if self.is_all_groups_active() {
            total_groups
        } else {
            groups
                .iter()
                .filter(|g| self.is_group_active_effective(g))
                .count()
        };

        let total_rules = self.draft.rules.len();
        let enabled_rules = self.draft.rules.iter().filter(|r| r.enabled).count();
        let active_rules_in_active_groups = self
            .draft
            .rules
            .iter()
            .filter(|r| {
                if !r.enabled {
                    return false;
                }
                let gs = r.effective_groups();
                !gs.is_empty()
                    && (self.is_all_groups_active()
                        || gs.iter().any(|g| self.is_group_active_effective(g)))
            })
            .count();

        ConfigStats {
            total_rules,
            enabled_rules,
            total_groups,
            active_groups,
            active_rules_in_active_groups,
        }
    }

    /// Indica se la regola appartiene ad almeno un gruppo attualmente attivo.
    pub fn is_rule_in_active_groups(&self, rule: &Rule) -> bool {
        self.draft.is_rule_in_active_groups(rule)
    }

    /// Restituisce eventuali filtri contestuali ereditati dai gruppi attivi della regola.
    pub fn inherited_group_contexts(
        &self,
        rule: &Rule,
    ) -> Vec<(String, rustbind::core::rule::WindowFilter)> {
        self.draft
            .active_groups_for_rule(rule)
            .into_iter()
            .filter_map(|g| self.draft.group_filter(&g).cloned().map(|wf| (g, wf)))
            .collect()
    }

    /// Indica se l'intersezione tra il filtro della regola e quelli dei suoi gruppi attivi è vuota.
    pub fn rule_has_empty_context_intersection(&self, rule: &Rule) -> bool {
        self.is_rule_in_active_groups(rule) && !self.draft.rule_can_ever_match_context(rule)
    }

    /// Analizza in tempo reale i conflitti trigger sulla bozza (Kill-Switch, Resume e duplicati sovrapposti).
    pub fn trigger_conflicts(&self) -> HashMap<usize, RuleTriggerConflict> {
        let mut out: HashMap<usize, RuleTriggerConflict> = HashMap::new();
        let kill_switch = self.draft.settings.kill_switch;
        let resume_hotkey = self.draft.settings.resume_hotkey;

        let mut active_indices: Vec<(usize, KeyBinding)> = Vec::new();
        for (idx, rule) in self.draft.rules.iter().enumerate() {
            let Trigger::Hotkey { binding } = rule.trigger;
            if binding == kill_switch {
                out.insert(idx, RuleTriggerConflict::KillSwitch);
                continue;
            }
            if resume_hotkey == Some(binding) {
                out.insert(idx, RuleTriggerConflict::ResumeHotkey);
                continue;
            }
            if rule.enabled && self.is_rule_in_active_groups(rule) {
                active_indices.push((idx, binding));
            }
        }

        for pos_a in 0..active_indices.len() {
            let (idx_a, bind_a) = active_indices[pos_a];
            let rule_a = &self.draft.rules[idx_a];
            for &(idx_b, bind_b) in &active_indices[(pos_a + 1)..] {
                if bind_a != bind_b {
                    continue;
                }
                let rule_b = &self.draft.rules[idx_b];
                if !Config::rule_contexts_can_overlap(&self.draft, rule_a, rule_b) {
                    continue;
                }

                // La regola successiva (`idx_b`) verrà saltata dal daemon perché oscurata da `idx_a`.
                match out.get_mut(&idx_b) {
                    Some(RuleTriggerConflict::DuplicateWithRule {
                        other_rule_names,
                        shadowed,
                    }) => {
                        *shadowed = true;
                        if !other_rule_names.iter().any(|n| n == &rule_a.name) {
                            other_rule_names.push(rule_a.name.clone());
                        }
                    }
                    None => {
                        out.insert(
                            idx_b,
                            RuleTriggerConflict::DuplicateWithRule {
                                other_rule_names: vec![rule_a.name.clone()],
                                shadowed: true,
                            },
                        );
                    }
                    _ => {}
                }

                // La regola precedente resta attiva ma avvisa della collisione con la successiva.
                match out.get_mut(&idx_a) {
                    Some(RuleTriggerConflict::DuplicateWithRule {
                        other_rule_names, ..
                    }) if !other_rule_names.iter().any(|n| n == &rule_b.name) => {
                        other_rule_names.push(rule_b.name.clone());
                    }
                    Some(RuleTriggerConflict::DuplicateWithRule { .. }) => {}
                    None => {
                        out.insert(
                            idx_a,
                            RuleTriggerConflict::DuplicateWithRule {
                                other_rule_names: vec![rule_b.name.clone()],
                                shadowed: false,
                            },
                        );
                    }
                    _ => {}
                }
            }
        }

        out
    }

    /// Restituisce i nomi delle regole in conflitto con `kill_switch` e con `resume_hotkey`.
    pub fn settings_hotkey_conflicts(&self) -> (Vec<String>, Vec<String>) {
        let mut ks_conflicts = Vec::new();
        let mut resume_conflicts = Vec::new();
        for rule in &self.draft.rules {
            let Trigger::Hotkey { binding } = rule.trigger;
            if binding == self.draft.settings.kill_switch {
                ks_conflicts.push(rule.name.clone());
            }
            if self.draft.settings.resume_hotkey == Some(binding) {
                resume_conflicts.push(rule.name.clone());
            }
        }
        (ks_conflicts, resume_conflicts)
    }

    /// Restituisce tutti i nomi di gruppo noti (sempre `"default"` per primo, più gruppi espliciti/regole/attivi).
    pub fn available_groups(&self) -> Vec<String> {
        self.draft.available_groups()
    }

    /// Indica se nessun filtro di gruppo è attivo (tutte le regole abilitate vengono registrate).
    pub fn is_all_groups_active(&self) -> bool {
        self.draft.settings.active_groups.is_empty()
    }

    /// Rimuove ogni filtro di gruppo abilitando tutti i gruppi all'avvio.
    pub fn set_all_groups_active(&mut self) {
        self.draft.settings.active_groups.clear();
    }

    /// Indica se uno specifico gruppo è selezionato esplicitamente in `active_groups`.
    pub fn is_group_selected(&self, group: &str) -> bool {
        let target = group.trim();
        self.draft
            .settings
            .active_groups
            .iter()
            .any(|g| g.trim() == target)
    }

    /// Indica se un gruppo risulta attualmente attivo (o perché tutti attivi, o perché incluso in `active_groups`).
    pub fn is_group_active_effective(&self, group: &str) -> bool {
        self.is_all_groups_active() || self.is_group_selected(group)
    }

    /// Gestisce l'attivazione/disattivazione di un gruppo anche partendo dalla modalità "tutti attivi",
    /// mantenendo il fallback garantito su `"default"`. Restituisce `true` se è scattato il fallback su `"default"`.
    pub fn toggle_group_active_now(&mut self, group: &str, active: bool) -> bool {
        self.draft.toggle_group_active(group, active)
    }

    /// Registra un nuovo gruppo personalizzato persistente in `Config.groups` previa validazione.
    pub fn add_custom_group(&mut self, raw_name: &str) -> Result<String, &'static str> {
        let trimmed = raw_name.trim();
        if !is_valid_ascii_identifier(trimmed) {
            return Err(
                "Nome gruppo non valido: usa solo caratteri ASCII alfanumerici, '_' o '-'.",
            );
        }
        let group = trimmed.to_string();
        if group != default_rule_group() && !self.draft.groups.iter().any(|g| g == &group) {
            self.draft.groups.push(group.clone());
        }
        Ok(group)
    }

    /// Elimina un gruppo personalizzato (vietato per `"default"`), rimuovendolo da tutte le regole
    /// (lasciando senza gruppo e disattivate le regole che restano prive di gruppi) e da `active_groups`.
    pub fn delete_group(&mut self, group: &str) -> Result<(), &'static str> {
        let target = group.trim();
        if target == default_rule_group() {
            return Err("Il gruppo predefinito 'default' non può essere eliminato.");
        }
        self.draft.groups.retain(|g| g.trim() != target);
        self.draft.group_filters.remove(target);

        if !self.draft.settings.active_groups.is_empty() {
            self.draft
                .settings
                .active_groups
                .retain(|g| g.trim() != target);
            if self.draft.settings.active_groups.is_empty() {
                self.draft.settings.active_groups.push(default_rule_group());
            }
        }

        for rule in &mut self.draft.rules {
            if rule.belongs_to_group(target) {
                let next: Vec<String> = rule
                    .effective_groups()
                    .into_iter()
                    .filter(|g| g != target)
                    .collect();
                if next.is_empty() {
                    rule.set_groups(Vec::new());
                    rule.enabled = false;
                } else {
                    rule.set_groups(next);
                }
            }
        }

        Ok(())
    }

    /// Restituisce il riepilogo (totale e attive) per tutti i gruppi filtrati opzionalmente per nome.
    pub fn filtered_group_summaries(&self, query: &str) -> Vec<GroupSummary> {
        let needle = query.trim().to_ascii_lowercase();
        self.group_summaries()
            .into_iter()
            .filter(|s| needle.is_empty() || s.name.to_ascii_lowercase().contains(&needle))
            .collect()
    }

    /// Restituisce il riepilogo (totale e attive) per tutti i gruppi noti.
    pub fn group_summaries(&self) -> Vec<GroupSummary> {
        self.available_groups()
            .into_iter()
            .map(|name| {
                let mut total_rules = 0usize;
                let mut enabled_rules = 0usize;
                for rule in &self.draft.rules {
                    if rule.belongs_to_group(&name) {
                        total_rules += 1;
                        if rule.enabled {
                            enabled_rules += 1;
                        }
                    }
                }
                GroupSummary {
                    name,
                    total_rules,
                    enabled_rules,
                }
            })
            .collect()
    }

    /// Restituisce gli indici delle regole appartenenti al gruppo indicato.
    pub fn rule_indices_in_group(&self, group: &str) -> Vec<usize> {
        let target = group.trim();
        self.draft
            .rules
            .iter()
            .enumerate()
            .filter(|(_, r)| r.belongs_to_group(target))
            .map(|(idx, _)| idx)
            .collect()
    }

    /// Abilita o disabilita in blocco tutte le regole appartenenti al gruppo indicato.
    /// Restituisce il numero di regole presenti nel gruppo.
    #[cfg(test)]
    pub fn set_group_rules_enabled(&mut self, group: &str, enabled: bool) -> usize {
        let target = group.trim();
        let mut count = 0usize;
        for rule in &mut self.draft.rules {
            if rule.belongs_to_group(target) {
                rule.enabled = enabled;
                count += 1;
            }
        }
        count
    }

    /// Aggiunge un insieme di regole (identificate per ID) al gruppo indicato senza rimuoverle dagli altri gruppi.
    pub fn add_rules_to_group(
        &mut self,
        rule_ids: &HashSet<String>,
        raw_group: &str,
    ) -> Result<usize, &'static str> {
        let group = self.add_custom_group(raw_group)?;
        let mut added = 0usize;
        for rule in &mut self.draft.rules {
            if rule_ids.contains(&rule.id) && !rule.belongs_to_group(&group) {
                let mut groups = rule.effective_groups();
                groups.push(group.clone());
                rule.set_groups(groups);
                added += 1;
            }
        }
        Ok(added)
    }

    /// Rimuove un insieme di regole (identificate per ID) dal gruppo indicato.
    /// Se una regola resta priva di gruppi viene automaticamente disattivata.
    pub fn remove_rules_from_group(&mut self, rule_ids: &HashSet<String>, group: &str) -> usize {
        let target = group.trim();
        let mut removed = 0usize;
        for rule in &mut self.draft.rules {
            if rule_ids.contains(&rule.id) && rule.belongs_to_group(target) {
                let remaining: Vec<String> = rule
                    .effective_groups()
                    .into_iter()
                    .filter(|g| g != target)
                    .collect();
                if remaining.is_empty() {
                    rule.set_groups(Vec::new());
                    rule.enabled = false;
                } else {
                    rule.set_groups(remaining);
                }
                removed += 1;
            }
        }
        removed
    }

    /// Individua tutti i gruppi di regole (`>= 2`) che eseguono esattamente la stessa sequenza di azioni.
    pub fn duplicate_action_groups(&self) -> Vec<DuplicateActionCluster> {
        let mut assigned = vec![false; self.draft.rules.len()];
        let mut clusters = Vec::new();

        for i in 0..self.draft.rules.len() {
            if assigned[i] || self.draft.rules[i].actions.is_empty() {
                continue;
            }
            let mut group = vec![i];
            for (j, slot) in assigned.iter_mut().enumerate().skip(i + 1) {
                if !*slot
                    && !self.draft.rules[j].actions.is_empty()
                    && self.draft.rules[i].actions == self.draft.rules[j].actions
                {
                    *slot = true;
                    group.push(j);
                }
            }
            if group.len() >= 2 {
                assigned[i] = true;
                clusters.push(DuplicateActionCluster {
                    rule_indices: group,
                });
            }
        }

        clusters
    }

    /// Mantiene una singola regola tra i duplicati ed elimina le altre, opzionalmente
    /// ereditando i gruppi delle regole eliminate nella regola mantenuta.
    pub fn resolve_duplicate_rules(
        &mut self,
        keep_rule_id: &str,
        remove_rule_ids: &HashSet<String>,
        merge_groups_from_removed: bool,
    ) -> Result<usize, &'static str> {
        let Some(keep_idx) = self.draft.rules.iter().position(|r| r.id == keep_rule_id) else {
            return Err("La regola selezionata da mantenere non esiste più.");
        };
        if remove_rule_ids.contains(keep_rule_id) {
            return Err("La regola da mantenere non può essere inclusa tra quelle da eliminare.");
        }

        if merge_groups_from_removed {
            let mut merged_groups = self.draft.rules[keep_idx].effective_groups();
            for rule in &self.draft.rules {
                if remove_rule_ids.contains(&rule.id) {
                    for g in rule.effective_groups() {
                        if !merged_groups.iter().any(|existing| existing == &g) {
                            merged_groups.push(g);
                        }
                    }
                }
            }
            self.draft.rules[keep_idx].set_groups(merged_groups);
        }

        let before = self.draft.rules.len();
        self.draft
            .rules
            .retain(|rule| !remove_rule_ids.contains(&rule.id));
        let removed = before.saturating_sub(self.draft.rules.len());

        if removed > 0 {
            self.selected_rule_idx = if self.draft.rules.is_empty() {
                None
            } else {
                Some(
                    self.draft
                        .rules
                        .iter()
                        .position(|r| r.id == keep_rule_id)
                        .unwrap_or_else(|| {
                            self.selected_rule_idx
                                .unwrap_or(0)
                                .min(self.draft.rules.len() - 1)
                        }),
                )
            };
        }

        Ok(removed)
    }

    /// Rimuove la regola all'indice indicato aggiornando in sicurezza la selezione corrente.
    pub fn remove_rule(&mut self, idx: usize) {
        if idx >= self.draft.rules.len() {
            return;
        }
        self.draft.rules.remove(idx);
        self.selected_rule_idx = if self.draft.rules.is_empty() {
            None
        } else {
            Some(idx.min(self.draft.rules.len() - 1))
        };
    }

    /// Sposta la regola `idx` verso l'alto di una posizione.
    pub fn move_rule_up(&mut self, idx: usize) {
        if idx == 0 || idx >= self.draft.rules.len() {
            return;
        }
        self.draft.rules.swap(idx, idx - 1);
        if self.selected_rule_idx == Some(idx) {
            self.selected_rule_idx = Some(idx - 1);
        } else if self.selected_rule_idx == Some(idx - 1) {
            self.selected_rule_idx = Some(idx);
        }
    }

    /// Sposta la regola `idx` verso il basso di una posizione.
    pub fn move_rule_down(&mut self, idx: usize) {
        if idx + 1 >= self.draft.rules.len() {
            return;
        }
        self.draft.rules.swap(idx, idx + 1);
        if self.selected_rule_idx == Some(idx) {
            self.selected_rule_idx = Some(idx + 1);
        } else if self.selected_rule_idx == Some(idx + 1) {
            self.selected_rule_idx = Some(idx);
        }
    }

    /// Sposta la regola da `from_idx` a `to_idx` aggiornando l'eventuale selezione attiva.
    pub fn move_rule_to(&mut self, from_idx: usize, to_idx: usize) -> bool {
        let len = self.draft.rules.len();
        if from_idx >= len || to_idx >= len || from_idx == to_idx {
            return false;
        }
        let rule = self.draft.rules.remove(from_idx);
        self.draft.rules.insert(to_idx, rule);
        if let Some(sel) = self.selected_rule_idx {
            if sel == from_idx {
                self.selected_rule_idx = Some(to_idx);
            } else if from_idx < to_idx && (from_idx + 1..=to_idx).contains(&sel) {
                self.selected_rule_idx = Some(sel - 1);
            } else if from_idx > to_idx && (to_idx..from_idx).contains(&sel) {
                self.selected_rule_idx = Some(sel + 1);
            }
        }
        true
    }

    /// Aggiunge una nuova azione in coda alla regola `rule_idx` (rispettando `MAX_ACTIONS_PER_SEQUENCE`).
    pub fn add_action(&mut self, rule_idx: usize, kind: NewActionKind) -> bool {
        let Some(rule) = self.draft.rules.get_mut(rule_idx) else {
            return false;
        };
        if rule.actions.len() >= MAX_ACTIONS_PER_SEQUENCE {
            return false;
        }
        let action = match kind {
            NewActionKind::KeyClick => InputAction::KeyClick {
                key: VirtualKey::KeyA,
            },
            NewActionKind::KeyDown => InputAction::KeyDown {
                key: VirtualKey::KeyA,
            },
            NewActionKind::KeyUp => InputAction::KeyUp {
                key: VirtualKey::KeyA,
            },
            NewActionKind::SendText => InputAction::SendText {
                text: String::new(),
            },
            NewActionKind::Delay => InputAction::Delay { milliseconds: 50 },
            NewActionKind::MouseClick => InputAction::MouseClick {
                button: MouseButton::Left,
            },
            NewActionKind::MouseDown => InputAction::MouseDown {
                button: MouseButton::Left,
            },
            NewActionKind::MouseUp => InputAction::MouseUp {
                button: MouseButton::Left,
            },
            NewActionKind::MouseScroll => InputAction::MouseScroll {
                direction: MouseWheelDirection::Vertical,
                steps: 1,
            },
            NewActionKind::MouseMove => InputAction::MouseMove { dx: 0, dy: 50 },
        };
        rule.actions.push(action);
        true
    }

    /// Aggiunge in coda più azioni registrate rispettando `MAX_ACTIONS_PER_SEQUENCE`.
    pub fn add_actions<I>(&mut self, rule_idx: usize, actions: I) -> usize
    where
        I: IntoIterator<Item = InputAction>,
    {
        let Some(rule) = self.draft.rules.get_mut(rule_idx) else {
            return 0;
        };
        let mut added = 0usize;
        for action in actions {
            if rule.actions.len() >= MAX_ACTIONS_PER_SEQUENCE {
                break;
            }
            rule.actions.push(action);
            added += 1;
        }
        added
    }

    /// Rimuove l'azione `action_idx` dalla regola `rule_idx`.
    pub fn remove_action(&mut self, rule_idx: usize, action_idx: usize) {
        let Some(rule) = self.draft.rules.get_mut(rule_idx) else {
            return;
        };
        if action_idx < rule.actions.len() {
            rule.actions.remove(action_idx);
        }
    }

    /// Sposta l'azione `action_idx` verso l'alto nella sequenza della regola `rule_idx`.
    pub fn move_action_up(&mut self, rule_idx: usize, action_idx: usize) {
        let Some(rule) = self.draft.rules.get_mut(rule_idx) else {
            return;
        };
        if action_idx > 0 && action_idx < rule.actions.len() {
            rule.actions.swap(action_idx, action_idx - 1);
        }
    }

    /// Sposta l'azione `action_idx` verso il basso nella sequenza della regola `rule_idx`.
    pub fn move_action_down(&mut self, rule_idx: usize, action_idx: usize) {
        let Some(rule) = self.draft.rules.get_mut(rule_idx) else {
            return;
        };
        if action_idx + 1 < rule.actions.len() {
            rule.actions.swap(action_idx, action_idx + 1);
        }
    }

    /// Duplica la singola azione `action_idx` inserendo la copia subito dopo (`MAX_ACTIONS_PER_SEQUENCE`).
    pub fn duplicate_action(&mut self, rule_idx: usize, action_idx: usize) -> bool {
        let Some(rule) = self.draft.rules.get_mut(rule_idx) else {
            return false;
        };
        if action_idx >= rule.actions.len() || rule.actions.len() >= MAX_ACTIONS_PER_SEQUENCE {
            return false;
        }
        let cloned = rule.actions[action_idx].clone();
        rule.actions.insert(action_idx + 1, cloned);
        true
    }

    /// Sposta un'azione da `from_idx` a `to_idx` (usato dal riordino drag & drop).
    pub fn move_action_to(&mut self, rule_idx: usize, from_idx: usize, to_idx: usize) -> bool {
        let Some(rule) = self.draft.rules.get_mut(rule_idx) else {
            return false;
        };
        if from_idx >= rule.actions.len() || to_idx >= rule.actions.len() || from_idx == to_idx {
            return false;
        }
        let item = rule.actions.remove(from_idx);
        rule.actions.insert(to_idx, item);
        true
    }

    /// Importa in coda a `target_rule_idx` le azioni della regola `source_rule_idx`,
    /// rispettando `MAX_ACTIONS_PER_SEQUENCE` e ignorando l'auto-import sulla stessa regola.
    pub fn import_rule_actions(&mut self, target_rule_idx: usize, source_rule_idx: usize) -> usize {
        if target_rule_idx == source_rule_idx {
            return 0;
        }
        let Some(actions) = self
            .draft
            .rules
            .get(source_rule_idx)
            .map(|source| source.actions.clone())
        else {
            return 0;
        };
        self.add_actions(target_rule_idx, actions)
    }
}

#[cfg(test)]
mod tests {
    use rustbind::core::rule::{WindowFilter, WindowFilterMode};
    use rustbind::core::{MAX_DELAY_MS, MAX_SEND_TEXT_LENGTH};

    use super::*;

    fn test_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rustbind_ui_editor_{}_{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn test_editor_missing_file_starts_with_default_without_writing_disk() {
        let dir = test_temp_dir("missing");
        let path = dir.join("config.toml");

        let editor = EditorState::from_path(Some(path.clone()), None);
        assert_eq!(editor.file_kind, LoadedFileKind::Missing);
        assert_eq!(editor.draft, Config::default());
        assert!(!editor.is_dirty());
        assert!(!editor.can_save());
        assert!(
            !path.exists(),
            "File assente non deve essere creato all'avvio"
        );
    }

    #[test]
    fn test_editor_add_rule_reorder_and_actions_crud() {
        let dir = test_temp_dir("crud");
        let path = dir.join("config.toml");
        let mut editor = EditorState::from_path(Some(path), None);

        let idx1 = editor.add_rule();
        assert_eq!(idx1, 0);
        assert_eq!(editor.draft.rules[0].id, "regola_1");
        assert!(
            editor.draft.rules[0].actions.is_empty(),
            "Una nuova regola deve partire senza azioni (nessun Delay predefinito)"
        );
        assert!(editor.is_dirty());
        assert!(matches!(
            editor.validate_live(),
            Err(ConfigError::EmptyActionSequence(_))
        ));
        assert!(!editor.can_save());

        let idx2 = editor.add_rule();
        assert_eq!(idx2, 1);
        assert_eq!(editor.draft.rules[1].id, "regola_2");

        // Riordino regole
        editor.move_rule_up(1);
        assert_eq!(editor.draft.rules[0].id, "regola_2");
        assert_eq!(editor.selected_rule_idx, Some(0));

        // Aggiunta e riordino azioni su regola 0 (partendo da 0 azioni)
        assert!(editor.add_action(0, NewActionKind::KeyClick));
        assert!(editor.add_action(0, NewActionKind::Delay));
        assert_eq!(editor.draft.rules[0].actions.len(), 2);

        editor.move_action_up(0, 1);
        assert!(matches!(
            editor.draft.rules[0].actions[0],
            InputAction::Delay { .. }
        ));
        editor.move_action_down(0, 0);
        assert!(matches!(
            editor.draft.rules[0].actions[0],
            InputAction::KeyClick { .. }
        ));

        editor.remove_action(0, 1);
        assert_eq!(editor.draft.rules[0].actions.len(), 1);

        // Aggiunta batch di azioni registrate
        let added = editor.add_actions(
            0,
            vec![
                InputAction::KeyDown {
                    key: VirtualKey::Shift,
                },
                InputAction::KeyClick {
                    key: VirtualKey::End,
                },
                InputAction::KeyUp {
                    key: VirtualKey::Shift,
                },
            ],
        );
        assert_eq!(added, 3);
        assert_eq!(editor.draft.rules[0].actions.len(), 4);

        editor.remove_rule(0);
        assert_eq!(editor.draft.rules.len(), 1);
        assert_eq!(editor.draft.rules[0].id, "regola_1");
    }

    #[test]
    fn test_editor_live_validation_blocks_invalid_states() {
        let dir = test_temp_dir("validation");
        let path = dir.join("config.toml");
        let mut editor = EditorState::from_path(Some(path), None);

        editor.add_rule();
        editor.add_rule();
        assert!(editor.add_action(0, NewActionKind::Delay));
        assert!(editor.add_action(1, NewActionKind::Delay));
        // ID duplicato -> invalido
        editor.draft.rules[1].id = "regola_1".to_string();
        assert!(matches!(
            editor.validate_live(),
            Err(ConfigError::DuplicateRuleId(_))
        ));
        assert!(!editor.can_save());

        // Ripristina ID univoco, prova SendText vuoto -> invalido
        editor.draft.rules[1].id = "regola_2".to_string();
        assert!(editor.add_action(0, NewActionKind::SendText));
        assert!(matches!(
            editor.validate_live(),
            Err(ConfigError::EmptyText(_))
        ));
        assert!(!editor.can_save());

        // SendText > MAX_SEND_TEXT_LENGTH -> invalido e privacy-safe
        if let Some(InputAction::SendText { text }) = editor.draft.rules[0].actions.last_mut() {
            *text = "A".repeat(MAX_SEND_TEXT_LENGTH + 1);
        }
        assert!(matches!(
            editor.validate_live(),
            Err(ConfigError::TextTooLong { .. })
        ));
        assert!(!editor.can_save());

        // Ripristina testo valido, imposta Delay fuori range -> invalido
        if let Some(InputAction::SendText { text }) = editor.draft.rules[0].actions.last_mut() {
            *text = "ok".to_string();
        }
        if let Some(InputAction::Delay { milliseconds }) = editor.draft.rules[0].actions.first_mut()
        {
            *milliseconds = MAX_DELAY_MS + 1;
        }
        assert!(matches!(
            editor.validate_live(),
            Err(ConfigError::InvalidDelay { .. })
        ));
        assert!(!editor.can_save());

        // Ripristina Delay valido -> salvabile
        if let Some(InputAction::Delay { milliseconds }) = editor.draft.rules[0].actions.first_mut()
        {
            *milliseconds = 100;
        }
        assert!(editor.validate_live().is_ok());
        assert!(editor.can_save());
    }

    #[test]
    fn test_editor_atomic_save_and_discard_on_valid_file() {
        let dir = test_temp_dir("save");
        let path = dir.join("config.toml");

        let mut editor = EditorState::from_path(Some(path.clone()), None);
        editor.add_rule();
        assert!(editor.add_action(0, NewActionKind::KeyClick));
        editor.save().expect("Salvataggio riuscito");

        assert!(path.exists());
        assert_eq!(editor.file_kind, LoadedFileKind::Valid);
        assert!(!editor.is_dirty());

        let reloaded = ConfigManager::load_from_file(&path).unwrap();
        assert_eq!(reloaded.rules.len(), 1);
        assert_eq!(reloaded.rules[0].id, "regola_1");

        // Modifica poi annulla
        editor.add_rule();
        assert!(editor.is_dirty());
        editor.discard_changes();
        assert!(!editor.is_dirty());
        assert_eq!(editor.draft.rules.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_editor_corrupted_file_refuses_save_and_explicit_restore_creates_backup() {
        let dir = test_temp_dir("corrupted");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let raw_corrupted = "schema_version = 1\nunknown_secret_key = \"SEGRETO123\"\n";
        std::fs::write(&path, raw_corrupted).unwrap();

        let mut editor = EditorState::from_path(Some(path.clone()), None);
        assert!(editor.is_locked_invalid());
        if let LoadedFileKind::Invalid { error_message } = &editor.file_kind {
            assert!(!error_message.contains("SEGRETO123"));
            assert!(error_message.contains("campo sconosciuto"));
        } else {
            panic!("Atteso LoadedFileKind::Invalid");
        }

        // Il salvataggio diretto deve essere rifiutato e il file corrotto intatto
        assert!(!editor.can_save());
        assert!(editor.save().is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), raw_corrupted);

        // Il ripristino senza checkbox spuntata viene rifiutato
        assert!(editor.restore_with_backup().is_err());
        assert!(editor.is_locked_invalid());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), raw_corrupted);

        // Con checkbox spuntata crea backup `.bak.*`, sovrascrive con default e sblocca l'editor
        editor.confirm_overwrite_invalid = true;
        let backup_path = editor
            .restore_with_backup()
            .expect("Ripristino esplicito riuscito");
        assert!(backup_path.exists());
        assert!(backup_path.to_string_lossy().contains("config.toml.bak."));
        assert_eq!(
            std::fs::read_to_string(&backup_path).unwrap(),
            raw_corrupted
        );
        assert!(!editor.is_locked_invalid());
        assert_eq!(editor.file_kind, LoadedFileKind::Valid);
        assert_eq!(
            ConfigManager::load_from_file(&path).unwrap(),
            Config::default()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_editor_per_rule_modifier_release_duplicate_filter_and_groups() {
        let dir = test_temp_dir("groups_and_duplicate");
        let path = dir.join("config.toml");

        let mut editor = EditorState::from_path(Some(path.clone()), None);
        let idx1 = editor.add_rule();
        assert!(editor.add_action(idx1, NewActionKind::KeyClick));
        assert_eq!(editor.draft.rules[idx1].groups, vec!["default".to_string()]);
        assert!(!editor.draft.rules[idx1].wait_for_modifier_release);

        // Creazione gruppo e assegnazione alla regola 1 con wait_for_modifier_release=true
        let custom = editor.add_custom_group("gaming_1").expect("Gruppo valido");
        assert_eq!(custom, "gaming_1");
        assert!(editor.add_custom_group("non valido!").is_err());
        editor.draft.rules[idx1].set_groups(vec![custom.clone()]);
        editor.draft.rules[idx1].wait_for_modifier_release = true;

        // Duplicazione regola (genera nuovo ID e nome univoco nella lingua UI selezionata)
        let idx2 = editor.duplicate_rule(idx1).expect("Regola duplicata");
        assert_eq!(idx2, 1);
        assert_eq!(editor.draft.rules[idx2].id, "regola_2");
        assert_eq!(editor.draft.rules[idx2].name, "Rule 1 (Copy)");
        assert_eq!(
            editor.draft.rules[idx2].groups,
            vec!["gaming_1".to_string()]
        );
        assert!(editor.draft.rules[idx2].wait_for_modifier_release);
        assert_eq!(editor.selected_rule_idx, Some(1));

        // Seconda duplicazione sullo stesso sorgente -> nome ancora univoco
        let idx3 = editor.duplicate_rule(idx1).expect("Seconda copia");
        assert_eq!(editor.draft.rules[idx3].name, "Rule 1 (Copy) 2");
        editor.remove_rule(idx3);

        // Verifica nomi localizzati per le altre lingue supportate
        editor.language = Language::Italian;
        let idx_it = editor.add_rule();
        assert_eq!(editor.draft.rules[idx_it].name, "Regola 3");
        editor.remove_rule(idx_it);
        editor.language = Language::French;
        let idx_fr = editor.add_rule();
        assert_eq!(editor.draft.rules[idx_fr].name, "Règle 3");
        editor.remove_rule(idx_fr);
        editor.language = Language::German;
        let idx_de = editor.add_rule();
        assert_eq!(editor.draft.rules[idx_de].name, "Regel 3");
        editor.remove_rule(idx_de);
        editor.language = Language::Spanish;
        let idx_es = editor.add_rule();
        assert_eq!(editor.draft.rules[idx_es].name, "Regla 3");
        editor.remove_rule(idx_es);
        editor.language = Language::English;

        // Filtro di ricerca (per gruppo, nome, trigger)
        editor.draft.rules[0].set_groups(vec!["lavoro".to_string()]);
        assert_eq!(editor.filtered_rule_indices(""), vec![0, 1]);
        assert_eq!(editor.filtered_rule_indices("lavoro"), vec![0]);
        assert_eq!(editor.filtered_rule_indices("copy"), vec![1]);
        assert_eq!(editor.filtered_rule_indices("ctrl+alt+f1"), vec![0, 1]);

        // Gruppi all'avvio (active_groups) con fallback su default
        assert!(editor.is_all_groups_active());
        assert!(!editor.toggle_group_active_now("default", false));
        assert!(!editor.toggle_group_active_now("lavoro", false));
        assert!(!editor.is_all_groups_active());
        assert!(editor.is_group_selected("gaming_1"));
        assert!(!editor.is_group_selected("lavoro"));

        // Disabilitando l'unico gruppo personalizzato scatta il fallback su "default"
        assert!(editor.toggle_group_active_now("gaming_1", false));
        assert_eq!(
            editor.draft.settings.active_groups,
            vec!["default".to_string()]
        );
        assert!(!editor.toggle_group_active_now("gaming_1", true));
        assert!(!editor.toggle_group_active_now("default", false));

        // Salvataggio su disco e verifica persistenza per singola regola + active_groups
        assert!(editor.is_dirty());
        assert!(editor.can_save());
        editor.save().expect("Salvataggio riuscito");
        assert!(!editor.is_dirty());

        let reloaded = ConfigManager::load_from_file(&path).unwrap();
        assert_eq!(
            reloaded.settings.active_groups,
            vec!["gaming_1".to_string()]
        );
        assert!(reloaded.rules[0].wait_for_modifier_release);
        assert_eq!(reloaded.rules[0].groups, vec!["lavoro".to_string()]);
        assert_eq!(reloaded.rules[1].groups, vec!["gaming_1".to_string()]);

        editor.set_all_groups_active();
        assert!(editor.is_all_groups_active());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_editor_group_visual_management_summary_bulk_toggle_and_assign() {
        let dir = test_temp_dir("group_visual");
        let path = dir.join("config.toml");
        let mut editor = EditorState::from_path(Some(path), None);

        let idx0 = editor.add_rule();
        let idx1 = editor.add_rule();
        let idx2 = editor.add_rule();
        assert!(editor.add_action(idx0, NewActionKind::KeyClick));
        assert!(editor.add_action(idx1, NewActionKind::KeyClick));
        assert!(editor.add_action(idx2, NewActionKind::KeyClick));
        let id1 = editor.draft.rules[idx1].id.clone();
        let id2 = editor.draft.rules[idx2].id.clone();

        // Aggiunta multipla al gruppo "lavoro" (le regole appartengono sia a default che a lavoro)
        let mut to_add = HashSet::new();
        to_add.insert(id1.clone());
        to_add.insert(id2.clone());
        assert_eq!(editor.add_rules_to_group(&to_add, "lavoro").unwrap(), 2);
        assert!(editor.draft.rules[idx1].belongs_to_group("default"));
        assert!(editor.draft.rules[idx1].belongs_to_group("lavoro"));
        assert!(editor.add_rules_to_group(&to_add, "bad group!").is_err());
        assert_eq!(editor.draft.rules[idx0].groups, vec!["default".to_string()]);

        // Rimozione da "default" delle due regole così restano solo in "lavoro"
        assert_eq!(editor.remove_rules_from_group(&to_add, "default"), 2);
        assert!(!editor.draft.rules[idx1].belongs_to_group("default"));
        assert!(editor.draft.rules[idx1].belongs_to_group("lavoro"));

        let summaries = editor.group_summaries();
        assert_eq!(
            summaries,
            vec![
                GroupSummary {
                    name: "default".to_string(),
                    total_rules: 1,
                    enabled_rules: 1,
                },
                GroupSummary {
                    name: "lavoro".to_string(),
                    total_rules: 2,
                    enabled_rules: 2,
                },
            ]
        );
        assert_eq!(editor.filtered_group_summaries("lav").len(), 1);

        assert_eq!(editor.rule_indices_in_group("lavoro"), vec![1, 2]);
        assert_eq!(editor.set_group_rules_enabled("lavoro", false), 2);
        assert!(!editor.draft.rules[1].enabled);
        assert!(!editor.draft.rules[2].enabled);
        assert!(editor.draft.rules[0].enabled);

        assert_eq!(editor.set_group_rules_enabled("lavoro", true), 2);
        assert!(editor.draft.rules[1].enabled);
        assert!(editor.draft.rules[2].enabled);

        // Il gruppo predefinito "default" non può essere eliminato
        assert!(editor.delete_group("default").is_err());

        // Eliminando un gruppo personalizzato, le regole che erano solo lì restano senza gruppo e disattivate
        editor.delete_group("lavoro").expect("Eliminazione lavoro");
        assert!(editor.draft.rules[1].effective_groups().is_empty());
        assert!(!editor.draft.rules[1].enabled);
        assert!(editor.validate_live().is_ok());
        assert!(!editor.available_groups().iter().any(|g| g == "lavoro"));
    }

    #[test]
    fn test_editor_filters_stats_and_profile_import_switch_export() {
        let dir = test_temp_dir("filters_and_profiles");
        let configs_dir = dir.join("configs");
        std::fs::create_dir_all(&configs_dir).unwrap();
        let path = configs_dir.join("config.toml");
        let mut editor = EditorState::from_path(Some(path.clone()), None);
        editor.add_rule();
        editor.add_rule();
        assert!(editor.add_action(0, NewActionKind::KeyClick));
        assert!(editor.add_action(1, NewActionKind::KeyClick));
        editor.add_custom_group("gaming").unwrap();

        let id1 = editor.draft.rules[1].id.clone();
        let mut set = HashSet::new();
        set.insert(id1.clone());
        editor.add_rules_to_group(&set, "gaming").unwrap();
        editor.remove_rules_from_group(&set, "default");
        assert!(editor.draft.rules[1].belongs_to_group("gaming"));
        editor.draft.rules[1].enabled = false;

        let stats = editor.config_stats();
        assert_eq!(stats.total_rules, 2);
        assert_eq!(stats.enabled_rules, 1);
        assert_eq!(stats.total_groups, 2);
        assert_eq!(stats.active_groups, 2);
        assert_eq!(stats.active_rules_in_active_groups, 1);

        // Filtri per gruppo (singolo e multiplo) e stato attivo/disattivo
        let mut no_group_filter = RuleGroupFilter::All;
        no_group_filter.set_no_group(true);
        assert_eq!(
            editor.filtered_rule_indices_with_filters("", &no_group_filter, false, false),
            Vec::<usize>::new()
        );
        let mut gaming_filter = RuleGroupFilter::All;
        gaming_filter.set_group("gaming", true);
        assert_eq!(
            editor.filtered_rule_indices_with_filters("", &gaming_filter, false, true),
            vec![1]
        );
        let mut multi_group_filter = gaming_filter.clone();
        multi_group_filter.set_group("default", true);
        assert_eq!(
            editor.filtered_rule_indices_with_filters("", &multi_group_filter, false, false),
            vec![0, 1]
        );
        multi_group_filter.set_group("default", false);
        multi_group_filter.set_group("gaming", false);
        assert_eq!(multi_group_filter, RuleGroupFilter::All);
        assert_eq!(
            editor.filtered_rule_indices_with_filters("", &RuleGroupFilter::All, true, false),
            vec![0]
        );

        // Eliminando gaming, la regola 1 resta senza gruppo e disattivata
        editor.delete_group("gaming").unwrap();
        assert_eq!(
            editor.filtered_rule_indices_with_filters("", &no_group_filter, false, false),
            vec![1]
        );
        assert!(!editor.draft.rules[1].enabled);

        editor.save().expect("Salvataggio config.toml");
        assert!(path.exists());
        assert!(!dir.join("config.toml").exists());

        // Esporta, importa come nuovo profilo, verifica avviso duplicato e cambio profilo attivo
        let export_path = dir.join("gaming_export.toml");
        editor
            .export_current_draft(&export_path)
            .expect("Esportazione");
        assert!(export_path.exists());

        let candidate = editor
            .prepare_import_from_path(&export_path)
            .expect("Prepara import");
        assert_eq!(candidate.target_name, "gaming_export.toml");
        assert!(!candidate.already_exists);
        let reload_needed = editor.apply_import(candidate).expect("Applica import");
        assert!(reload_needed);
        assert_eq!(editor.active_profile_name, "gaming_export.toml");

        let second_candidate = editor
            .prepare_import_from_path(&export_path)
            .expect("Secondo import stesso nome");
        assert!(second_candidate.already_exists);

        let profiles = editor.available_profiles();
        assert!(profiles.contains(&"config.toml".to_string()));
        assert!(profiles.contains(&"gaming_export.toml".to_string()));

        editor
            .switch_active_profile("config.toml")
            .expect("Ritorno a config.toml");
        assert_eq!(editor.active_profile_name, "config.toml");
        editor
            .switch_active_profile("gaming_export.toml")
            .expect("Switch a gaming_export");
        assert_eq!(editor.active_profile_name, "gaming_export.toml");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_duplicate_action_groups_and_resolution_with_group_merge() {
        let dir = test_temp_dir("duplicate_resolution");
        let path = dir.join("config.toml");
        let mut editor = EditorState::from_path(Some(path), None);
        editor.add_rule();
        editor.add_rule();
        editor.add_rule();

        // Le regole vuote appena create non vanno considerate duplicate tra loro
        assert!(editor.duplicate_action_groups().is_empty());

        let id0 = editor.draft.rules[0].id.clone();
        let id1 = editor.draft.rules[1].id.clone();
        let id2 = editor.draft.rules[2].id.clone();

        // Regola 0 e Regola 1 condividono la stessa sequenza; Regola 2 ha un Delay diverso.
        assert!(editor.add_action(0, NewActionKind::KeyClick));
        assert!(editor.add_action(1, NewActionKind::KeyClick));
        editor.draft.rules[0].set_groups(vec!["default".to_string()]);
        editor.draft.rules[1].set_groups(vec!["gaming".to_string(), "lavoro".to_string()]);
        editor.draft.rules[2].actions = vec![InputAction::Delay { milliseconds: 120 }];

        let clusters = editor.duplicate_action_groups();
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].rule_indices, vec![0, 1]);

        let mut remove_ids = HashSet::new();
        remove_ids.insert(id1.clone());
        let removed = editor
            .resolve_duplicate_rules(&id0, &remove_ids, true)
            .expect("Risoluzione duplicati");
        assert_eq!(removed, 1);
        assert_eq!(editor.draft.rules.len(), 2);
        assert!(editor.draft.rules[0].belongs_to_group("default"));
        assert!(editor.draft.rules[0].belongs_to_group("gaming"));
        assert!(editor.draft.rules[0].belongs_to_group("lavoro"));
        assert!(editor.duplicate_action_groups().is_empty());

        // Verifica ramo senza merge dei gruppi
        editor.duplicate_rule(0);
        let id_dup = editor.draft.rules[1].id.clone();
        editor.draft.rules[1].set_groups(vec!["extra".to_string()]);
        let mut remove_no_merge = HashSet::new();
        remove_no_merge.insert(id_dup);
        editor
            .resolve_duplicate_rules(&id2, &HashSet::new(), false)
            .expect("Nessun elemento rimosso");
        editor
            .resolve_duplicate_rules(&id0, &remove_no_merge, false)
            .expect("Rimozione senza merge");
        assert!(!editor.draft.rules[0].belongs_to_group("extra"));
        assert!(editor.duplicate_action_groups().is_empty());

        // Filtro ricerca per processo contestuale e validazione live
        editor.draft.rules[0].window_filter = Some(rustbind::core::rule::WindowFilter {
            mode: rustbind::core::rule::WindowFilterMode::Include,
            process_name: Some("code.exe".to_string()),
        });
        assert_eq!(editor.filtered_rule_indices("code.exe"), vec![0]);
        assert!(editor.validate_live().is_ok());

        editor.draft.rules[0].window_filter = Some(rustbind::core::rule::WindowFilter {
            mode: rustbind::core::rule::WindowFilterMode::Include,
            process_name: None,
        });
        assert_eq!(editor.validate_live(), Err(ConfigError::EmptyWindowFilter));

        // Rilevamento conflitti trigger in tempo reale: regole disgiunte per app NON confliggono
        editor.draft.rules[0].window_filter = Some(WindowFilter {
            mode: WindowFilterMode::Include,
            process_name: Some("code.exe".to_string()),
        });
        editor.draft.rules[0].enabled = true;
        editor.draft.rules[0].set_groups(vec!["default".to_string()]);
        editor.draft.rules[1].window_filter = Some(WindowFilter {
            mode: WindowFilterMode::Include,
            process_name: Some("chrome.exe".to_string()),
        });
        editor.draft.rules[1].trigger = editor.draft.rules[0].trigger.clone();
        editor.draft.rules[1].enabled = true;
        editor.draft.rules[1].set_groups(vec!["default".to_string()]);
        assert!(!editor.trigger_conflicts().contains_key(&0));
        assert!(!editor.trigger_conflicts().contains_key(&1));

        // Regole sovrapposte sullo stesso trigger -> la seconda risulta shadowed
        editor.draft.rules[1].window_filter = None;
        let conflicts = editor.trigger_conflicts();
        assert!(matches!(
            conflicts.get(&0),
            Some(RuleTriggerConflict::DuplicateWithRule {
                shadowed: false,
                ..
            })
        ));
        assert!(matches!(
            conflicts.get(&1),
            Some(RuleTriggerConflict::DuplicateWithRule { shadowed: true, .. })
        ));

        // Conflitto con kill-switch
        editor.draft.rules[0].trigger = Trigger::Hotkey {
            binding: editor.draft.settings.kill_switch,
        };
        let conflicts_ks = editor.trigger_conflicts();
        assert_eq!(conflicts_ks.get(&0), Some(&RuleTriggerConflict::KillSwitch));
        let (ks_names, _) = editor.settings_hotkey_conflicts();
        assert_eq!(ks_names, vec![editor.draft.rules[0].name.clone()]);

        // Duplicazione singola azione, spostamento drag&drop e cronologia Undo/Redo
        editor.reset_history();
        assert!(!editor.can_undo());
        assert!(!editor.can_redo());

        assert!(editor.add_action(0, NewActionKind::KeyClick));
        assert!(editor.duplicate_action(0, 0));
        editor.sync_history_from_draft();
        assert!(editor.can_undo());

        let count_before_move = editor.draft.rules[0].actions.len();
        assert!(count_before_move >= 2);
        assert!(editor.add_action(0, NewActionKind::Delay));
        assert!(editor.move_action_to(0, count_before_move, 0));
        assert!(matches!(
            editor.draft.rules[0].actions[0],
            InputAction::Delay { .. }
        ));
        editor.sync_history_from_draft();

        assert!(editor.undo());
        assert_eq!(editor.draft.rules[0].actions.len(), count_before_move);
        assert!(editor.can_redo());
        assert!(editor.redo());
        assert!(matches!(
            editor.draft.rules[0].actions[0],
            InputAction::Delay { .. }
        ));
    }

    #[test]
    fn test_editor_mouse_actions_and_drag_import_v1() {
        let dir = test_temp_dir("mouse_v1");
        fs::create_dir_all(&dir).unwrap();
        let cfg_path = dir.join("config.toml");
        let v1_toml = r#"
schema_version = 1

[[rules]]
id = "r1"
name = "Regola Base"
enabled = true

[rules.trigger]
type = "hotkey"
binding = "Ctrl+Alt+A"

[[rules.actions]]
type = "key_click"
key = "B"
"#;
        fs::write(&cfg_path, v1_toml).unwrap();

        let mut editor = EditorState::from_path(Some(cfg_path.clone()), None);
        assert_eq!(editor.file_kind, LoadedFileKind::Valid);
        assert_eq!(editor.draft.schema_version, 1);

        assert!(editor.add_action(0, NewActionKind::MouseClick));
        assert!(editor.add_action(0, NewActionKind::MouseScroll));
        assert!(editor.add_action(0, NewActionKind::MouseMove));
        assert_eq!(editor.draft.schema_version, 1);
        assert!(editor.validate_live().is_ok());
        assert!(editor.save().is_ok());

        let reloaded = ConfigManager::load_from_file(&cfg_path).unwrap();
        assert_eq!(reloaded.schema_version, 1);
        assert_eq!(reloaded.rules[0].actions.len(), 4);

        // Importazione azioni da un'altra regola via drag & drop
        let mut import_editor = EditorState::from_path(Some(cfg_path.clone()), None);
        let target_idx = import_editor.add_rule();
        let source_idx = import_editor.add_rule();
        import_editor.add_action(source_idx, NewActionKind::MouseClick);
        import_editor.add_action(source_idx, NewActionKind::Delay);
        import_editor.sync_history_from_draft();
        assert_eq!(import_editor.import_rule_actions(source_idx, source_idx), 0);
        let before_len = import_editor.draft.rules[target_idx].actions.len();
        assert_eq!(import_editor.import_rule_actions(target_idx, source_idx), 2);
        assert_eq!(
            import_editor.draft.rules[target_idx].actions.len(),
            before_len + 2
        );
        import_editor.sync_history_from_draft();
        assert!(import_editor.undo());
        assert_eq!(
            import_editor.draft.rules[target_idx].actions.len(),
            before_len
        );

        let moved_id = import_editor.draft.rules[target_idx].id.clone();
        import_editor.selected_rule_idx = Some(target_idx);
        assert!(import_editor.move_rule_to(target_idx, 0));
        assert_eq!(import_editor.draft.rules[0].id, moved_id);
        assert_eq!(import_editor.selected_rule_idx, Some(0));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_editor_group_context_filters_conflicts_and_cleanup() {
        use rustbind::core::rule::{WindowFilter, WindowFilterMode};

        let mut editor = EditorState::from_path(None, None);
        editor.add_custom_group("coding").unwrap();
        editor.add_custom_group("browsers").unwrap();
        editor.draft.group_filters.insert(
            "coding".to_string(),
            WindowFilter {
                mode: WindowFilterMode::Include,
                process_name: Some("code.exe".to_string()),
            },
        );
        editor.draft.group_filters.insert(
            "browsers".to_string(),
            WindowFilter {
                mode: WindowFilterMode::Include,
                process_name: Some("chrome.exe".to_string()),
            },
        );

        let idx_a = editor.add_rule();
        let idx_b = editor.add_rule();
        editor.draft.rules[idx_a].set_groups(vec!["coding".to_string()]);
        editor.draft.rules[idx_a].window_filter = None;
        editor.draft.rules[idx_b].set_groups(vec!["browsers".to_string()]);
        editor.draft.rules[idx_b].window_filter = None;
        editor.draft.rules[idx_b].trigger = editor.draft.rules[idx_a].trigger.clone();

        // Gruppi disgiunti con stesso trigger: nessun conflitto
        assert!(editor.trigger_conflicts().is_empty());
        assert_eq!(editor.filtered_rule_indices("code.exe"), vec![idx_a]);

        // Filtro regola incompatibile col suo gruppo: intersezione vuota
        editor.draft.rules[idx_a].window_filter = Some(WindowFilter {
            mode: WindowFilterMode::Include,
            process_name: Some("chrome.exe".to_string()),
        });
        assert!(editor.rule_has_empty_context_intersection(&editor.draft.rules[idx_a]));

        // Eliminazione gruppo rimuove anche il filtro associato
        editor.delete_group("browsers").unwrap();
        assert!(!editor.draft.group_filters.contains_key("browsers"));
    }
}
