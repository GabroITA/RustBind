use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::core::binding::KeyBinding;
use crate::core::key::VirtualKey;
use crate::core::modifiers::Modifiers;
use crate::core::rule::{
    ForegroundWindow, Rule, WindowFilter, default_rule_group, window_filters_intersection_non_empty,
};
use crate::core::{CURRENT_SCHEMA_VERSION, MAX_EVENTS_PER_SECOND};

/// Restituisce la combinazione di default per il Kill-Switch di emergenza (`Ctrl+Alt+Shift+F12`).
pub fn default_kill_switch() -> KeyBinding {
    KeyBinding::new(
        Modifiers {
            ctrl: true,
            alt: true,
            shift: true,
            win: false,
        },
        VirtualKey::F12,
    )
}

const fn default_max_events_per_second() -> u32 {
    MAX_EVENTS_PER_SECOND
}

const fn default_schema_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}

/// Impostazioni generali del motore e di sicurezza applicativa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralSettings {
    /// Hotkey globale di emergenza per interrompere istantaneamente qualsiasi sequenza in esecuzione.
    #[serde(default = "default_kill_switch")]
    pub kill_switch: KeyBinding,

    /// Hotkey globale opzionale per riprendere il daemon quando si trova nello stato `Paused`.
    /// Non consente mai di uscire da `ServiceState::Error` (fail-closed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_hotkey: Option<KeyBinding>,

    /// Limite massimo di eventi di input sintetico generabili al secondo.
    #[serde(default = "default_max_events_per_second")]
    pub max_events_per_second: u32,

    /// Elenco opzionale dei gruppi di regole da attivare all'avvio o alla ricarica.
    /// Se vuoto, nessun filtro di gruppo è applicato e tutte le regole abilitate vengono registrate.
    #[serde(default)]
    pub active_groups: Vec<String>,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            kill_switch: default_kill_switch(),
            resume_hotkey: None,
            max_events_per_second: default_max_events_per_second(),
            active_groups: Vec::new(),
        }
    }
}

/// Rappresentazione completa del file di configurazione `config.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Versione dello schema per la validazione e la compatibilità delle migrazioni future.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,

    /// Impostazioni globali e di sicurezza.
    #[serde(default)]
    pub settings: GeneralSettings,

    /// Elenco esplicito dei gruppi definiti dall'utente (oltre a `"default"`), così da persistere
    /// anche gruppi temporaneamente privi di regole.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,

    /// Filtri contestuali per applicazione associati ai singoli gruppi personalizzati (`gruppo -> WindowFilter`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub group_filters: BTreeMap<String, WindowFilter>,

    /// Elenco delle regole di automazione registrate.
    #[serde(default)]
    pub rules: Vec<Rule>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            settings: GeneralSettings::default(),
            groups: Vec::new(),
            group_filters: BTreeMap::new(),
            rules: Vec::new(),
        }
    }
}

impl Config {
    /// Restituisce tutti i nomi di gruppo noti (`"default"` per primo, seguiti da gruppi espliciti,
    /// filtri di gruppo, gruppi assegnati alle regole e gruppi attivi ordinati alfabeticamente).
    pub fn available_groups(&self) -> Vec<String> {
        let mut set: HashSet<String> = HashSet::new();
        let mut out = vec![default_rule_group()];
        set.insert(default_rule_group());

        let rule_groups: Vec<String> = self
            .rules
            .iter()
            .flat_map(|r| r.effective_groups())
            .collect();

        let candidates = self
            .groups
            .iter()
            .map(|g| g.trim())
            .chain(self.group_filters.keys().map(|g| g.trim()))
            .chain(rule_groups.iter().map(|g| g.trim()))
            .chain(self.settings.active_groups.iter().map(|g| g.trim()));

        let mut extras: Vec<String> = Vec::new();
        for group in candidates {
            if !group.is_empty() && set.insert(group.to_string()) {
                extras.push(group.to_string());
            }
        }
        extras.sort();
        out.extend(extras);
        out
    }

    /// Attiva o disattiva un gruppo in `settings.active_groups` (anche partendo da modalità "tutti attivi"),
    /// garantendo il fallback automatico su `"default"` quando nessun gruppo resterebbe attivo.
    /// Restituisce `true` se è scattato il fallback automatico su `"default"`.
    pub fn toggle_group_active(&mut self, group: &str, active: bool) -> bool {
        let target = group.trim();
        if target.is_empty() {
            return false;
        }
        if self.settings.active_groups.is_empty() {
            if active {
                return false;
            }
            let remaining: Vec<String> = self
                .available_groups()
                .into_iter()
                .filter(|g| g != target)
                .collect();
            if remaining.is_empty() {
                self.settings.active_groups = vec![default_rule_group()];
                return target == default_rule_group();
            }
            self.settings.active_groups = remaining;
            false
        } else if active {
            if !self
                .settings
                .active_groups
                .iter()
                .any(|g| g.trim() == target)
            {
                self.settings.active_groups.push(target.to_string());
            }
            false
        } else {
            let was_only_default = target == default_rule_group()
                && self.settings.active_groups.len() == 1
                && self.settings.active_groups[0].trim() == default_rule_group();
            self.settings.active_groups.retain(|g| g.trim() != target);
            if self.settings.active_groups.is_empty() {
                self.settings.active_groups.push(default_rule_group());
            }
            was_only_default
                || (self.settings.active_groups.len() == 1
                    && self.settings.active_groups[0].trim() == default_rule_group()
                    && target != default_rule_group())
        }
    }

    /// Indica se il gruppo indicato è attivo secondo `settings.active_groups`.
    pub fn is_group_active(&self, group: &str) -> bool {
        let target = group.trim();
        !target.is_empty()
            && (self.settings.active_groups.is_empty()
                || self
                    .settings
                    .active_groups
                    .iter()
                    .any(|ag| ag.trim() == target))
    }

    /// Restituisce l'eventuale filtro contestuale del gruppo (sempre `None` per `"default"`).
    pub fn group_filter(&self, group: &str) -> Option<&WindowFilter> {
        let target = group.trim();
        if target.is_empty() || target == default_rule_group() {
            return None;
        }
        self.group_filters.get(target).filter(|wf| !wf.is_empty())
    }

    /// Restituisce i gruppi attivi a cui appartiene la regola.
    pub fn active_groups_for_rule(&self, rule: &Rule) -> Vec<String> {
        rule.effective_groups()
            .into_iter()
            .filter(|g| self.is_group_active(g))
            .collect()
    }

    /// Indica se la regola appartiene ad almeno un gruppo attivo.
    pub fn is_rule_in_active_groups(&self, rule: &Rule) -> bool {
        !self.active_groups_for_rule(rule).is_empty()
    }

    /// Indica se la regola dipende dal contesto della finestra attiva (per filtro proprio o di gruppo).
    pub fn is_rule_contextual(&self, rule: &Rule) -> bool {
        rule.is_contextual()
            || self
                .active_groups_for_rule(rule)
                .iter()
                .any(|g| self.group_filter(g).is_some())
    }

    /// Verifica se la regola può attivarsi nella finestra in primo piano secondo `Gruppi(OR) && Regola(AND)`.
    pub fn rule_matches_context(&self, rule: &Rule, fg: &ForegroundWindow) -> bool {
        let active_groups = self.active_groups_for_rule(rule);
        if active_groups.is_empty() {
            return false;
        }
        let group_matched = active_groups
            .iter()
            .any(|g| self.group_filter(g).map_or(true, |wf| wf.matches(fg)));
        group_matched && rule.matches_foreground(fg)
    }

    /// Indica se l'intersezione tra i gruppi attivi della regola e il suo eventuale filtro è soddisfacibile.
    pub fn rule_can_ever_match_context(&self, rule: &Rule) -> bool {
        let active_groups = self.active_groups_for_rule(rule);
        if active_groups.is_empty() {
            return false;
        }
        let rule_wf = rule.window_filter.as_ref().filter(|wf| !wf.is_empty());
        active_groups.iter().any(|g| {
            let mut filters: Vec<&WindowFilter> = Vec::with_capacity(2);
            if let Some(gwf) = self.group_filter(g) {
                filters.push(gwf);
            }
            if let Some(rwf) = rule_wf {
                filters.push(rwf);
            }
            window_filters_intersection_non_empty(&filters)
        })
    }

    /// Verifica se due regole possono attivarsi contemporaneamente sulla stessa finestra in primo piano.
    pub fn rule_contexts_can_overlap(&self, a: &Rule, b: &Rule) -> bool {
        let groups_a = self.active_groups_for_rule(a);
        let groups_b = self.active_groups_for_rule(b);
        if groups_a.is_empty() || groups_b.is_empty() {
            return false;
        }
        let wf_a = a.window_filter.as_ref().filter(|wf| !wf.is_empty());
        let wf_b = b.window_filter.as_ref().filter(|wf| !wf.is_empty());

        for ga in &groups_a {
            let gwf_a = self.group_filter(ga);
            for gb in &groups_b {
                let gwf_b = self.group_filter(gb);
                let mut filters: Vec<&WindowFilter> = Vec::with_capacity(4);
                if let Some(f) = gwf_a {
                    filters.push(f);
                }
                if let Some(f) = wf_a {
                    filters.push(f);
                }
                if let Some(f) = gwf_b {
                    filters.push(f);
                }
                if let Some(f) = wf_b {
                    filters.push(f);
                }
                if window_filters_intersection_non_empty(&filters) {
                    return true;
                }
            }
        }
        false
    }
}
