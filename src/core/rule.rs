use serde::{Deserialize, Serialize};

use crate::core::action::InputAction;
use crate::core::binding::KeyBinding;

/// Lunghezza massima consentita per il campo testuale del filtro processo.
pub const MAX_WINDOW_FILTER_LEN: usize = 256;

/// Contesto corrente della finestra in primo piano (solo nome eseguibile del processo).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ForegroundWindow {
    /// Nome del solo file eseguibile del processo (es. `"code.exe"`), mai percorso completo.
    pub process_name: Option<String>,
}

/// Modalità di applicazione del filtro contestuale per applicazione.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowFilterMode {
    /// La regola è attiva solo quando il processo in primo piano corrisponde all'elenco.
    #[default]
    Include,
    /// La regola è attiva ovunque tranne quando il processo in primo piano corrisponde all'elenco.
    Exclude,
}

/// Filtro contestuale opzionale basato sul nome eseguibile dell'applicazione attiva.
///
/// Accetta uno o più nomi `.exe` separati da virgola (OR case-insensitive).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowFilter {
    #[serde(default)]
    pub mode: WindowFilterMode,
    /// Nome eseguibile del processo (es. `"code.exe"` o `"code, chrome"`), case-insensitive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_name: Option<String>,
}

impl WindowFilter {
    /// Restituisce l'elenco normalizzato dei token di processo separati da virgola.
    pub fn process_tokens(&self) -> Vec<&str> {
        self.process_name
            .as_deref()
            .map(|raw| {
                raw.split(',')
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Indica se il filtro non contiene alcun criterio non vuoto.
    pub fn is_empty(&self) -> bool {
        self.process_tokens().is_empty()
    }

    /// Verifica se il filtro contestuale è soddisfatto rispetto al processo in primo piano.
    pub fn matches(&self, fg: &ForegroundWindow) -> bool {
        let proc_tokens = self.process_tokens();
        if proc_tokens.is_empty() {
            return true;
        }

        let matched = fg.process_name.as_deref().is_some_and(|fg_proc| {
            let fg_low = fg_proc.trim().to_ascii_lowercase();
            if fg_low.is_empty() {
                return false;
            }
            let fg_stem = fg_low.strip_suffix(".exe").unwrap_or(&fg_low);
            proc_tokens.iter().any(|tok| {
                let tok_low = tok.to_ascii_lowercase();
                let tok_stem = tok_low.strip_suffix(".exe").unwrap_or(&tok_low);
                fg_low == tok_low || fg_stem == tok_stem
            })
        });

        match self.mode {
            WindowFilterMode::Include => matched,
            WindowFilterMode::Exclude => !matched,
        }
    }
}

/// Verifica se esiste almeno un processo in primo piano in grado di soddisfare
/// contemporaneamente tutti i filtri contestuali forniti.
pub fn window_filters_intersection_non_empty(filters: &[&WindowFilter]) -> bool {
    let active: Vec<&WindowFilter> = filters
        .iter()
        .copied()
        .filter(|wf| !wf.is_empty())
        .collect();
    if active.is_empty() {
        return true;
    }

    let mut candidate_stems: Vec<String> = Vec::new();
    for wf in &active {
        for tok in wf.process_tokens() {
            let low = tok.to_ascii_lowercase();
            let stem = low.strip_suffix(".exe").unwrap_or(&low);
            if !stem.is_empty() && !candidate_stems.iter().any(|s| s == stem) {
                candidate_stems.push(stem.to_string());
            }
        }
    }

    // Rappresenta qualsiasi processo non menzionato esplicitamente nei filtri (es. app generica).
    let other_fg = ForegroundWindow {
        process_name: Some("__rustbind_other_process__.exe".to_string()),
    };
    if active.iter().all(|wf| wf.matches(&other_fg)) {
        return true;
    }

    candidate_stems.into_iter().any(|stem| {
        let fg = ForegroundWindow {
            process_name: Some(format!("{stem}.exe")),
        };
        active.iter().all(|wf| wf.matches(&fg))
    })
}

/// Trigger che attiva l'esecuzione di una regola.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Trigger {
    /// Attivazione tramite hotkey globale registrata.
    Hotkey { binding: KeyBinding },
}

/// Restituisce il nome del gruppo predefinito per le regole (`"default"`).
pub fn default_rule_group() -> String {
    "default".to_string()
}

/// Restituisce l'elenco predefinito dei gruppi per una nuova regola (`["default"]`).
pub fn default_rule_groups() -> Vec<String> {
    vec![default_rule_group()]
}

/// Regola dichiarativa che mappa un trigger a una sequenza ordinata di azioni.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Identificatore univoco della regola (alfanumerico, trattini, underscore).
    pub id: String,
    /// Nome descrittivo leggibile dall'utente per la visualizzazione nella UI.
    pub name: String,
    /// Elenco dei gruppi di appartenenza (default `["default"]`; vuoto se disabilitata/senza gruppo).
    #[serde(default = "default_rule_groups")]
    pub groups: Vec<String>,
    /// Flag di attivazione della regola.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Se `true`, attende il rilascio dei tasti modificatori fisici del trigger prima di eseguire la sequenza.
    #[serde(default)]
    pub wait_for_modifier_release: bool,
    /// Filtro contestuale opzionale su applicazione e/o titolo della finestra attiva.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_filter: Option<WindowFilter>,
    /// Condizione di attivazione.
    pub trigger: Trigger,
    /// Sequenza di azioni da eseguire.
    pub actions: Vec<InputAction>,
}

impl Rule {
    /// Indica se la regola ha un filtro contestuale attivo su processo o titolo finestra.
    pub fn is_contextual(&self) -> bool {
        self.window_filter.as_ref().is_some_and(|wf| !wf.is_empty())
    }

    /// Verifica se la regola può essere attivata con la finestra in primo piano specificata.
    pub fn matches_foreground(&self, fg: &ForegroundWindow) -> bool {
        self.window_filter
            .as_ref()
            .map_or(true, |wf| wf.matches(fg))
    }

    /// Restituisce tutti i nomi dei gruppi a cui appartiene la regola (normalizzati e senza duplicati).
    pub fn effective_groups(&self) -> Vec<String> {
        let mut out = Vec::new();
        for g in &self.groups {
            let trimmed = g.trim();
            if !trimmed.is_empty() && !out.iter().any(|x| x == trimmed) {
                out.push(trimmed.to_string());
            }
        }
        out
    }

    /// Indica se la regola appartiene al gruppo specificato.
    pub fn belongs_to_group(&self, group: &str) -> bool {
        let target = group.trim();
        self.effective_groups().iter().any(|g| g == target)
    }

    /// Restituisce il gruppo primario della regola, se presente.
    pub fn primary_group(&self) -> Option<String> {
        self.effective_groups().into_iter().next()
    }

    /// Imposta in modo coerente i gruppi normalizzati di appartenenza della regola.
    pub fn set_groups(&mut self, groups: Vec<String>) {
        let mut normalized = Vec::new();
        for g in groups {
            let trimmed = g.trim();
            if !trimmed.is_empty() && !normalized.iter().any(|x| x == trimmed) {
                normalized.push(trimmed.to_string());
            }
        }
        self.groups = normalized;
    }
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_filter_matching_by_process_name() {
        let fg_vscode = ForegroundWindow {
            process_name: Some("Code.exe".to_string()),
        };

        // Include su processo (supporta .exe opzionale e lista separata da virgola, case-insensitive)
        let f_proc = WindowFilter {
            mode: WindowFilterMode::Include,
            process_name: Some("notepad.exe, code".to_string()),
        };
        assert!(f_proc.matches(&fg_vscode));

        // Non deve fare falsi match parziali sul nome processo (es. "ode" != "code.exe")
        let f_partial = WindowFilter {
            mode: WindowFilterMode::Include,
            process_name: Some("ode.exe".to_string()),
        };
        assert!(!f_partial.matches(&fg_vscode));

        // Modalità Exclude
        let f_exclude = WindowFilter {
            mode: WindowFilterMode::Exclude,
            process_name: Some("code.exe".to_string()),
        };
        assert!(!f_exclude.matches(&fg_vscode));
        assert!(f_exclude.matches(&ForegroundWindow {
            process_name: Some("chrome.exe".to_string()),
        }));
    }
}
