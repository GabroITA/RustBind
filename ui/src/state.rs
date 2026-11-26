use crate::i18n::Language;
use rustbind::ipc_protocol::{IPC_STATE_ACTIVE, IPC_STATE_ERROR, IPC_STATE_PAUSED, IpcResponse};
use rustbind::platform::windows::ipc_client::IpcClientError;

/// Tab principali della finestra di configurazione.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiTab {
    Rules,
    Settings,
    Security,
}

impl UiTab {
    /// Restituisce l'etichetta localizzata della tab principale.
    pub fn label(self, lang: Language) -> &'static str {
        let s = lang.s();
        match self {
            Self::Rules => s.tab_rules,
            Self::Settings => s.tab_settings,
            Self::Security => s.tab_security,
        }
    }
}

/// Subtab interni alla scheda "Regole".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulesSubTab {
    RuleList,
    Groups,
    Duplicates,
}

impl RulesSubTab {
    /// Restituisce l'etichetta localizzata della sottoscheda.
    pub fn label(self, lang: Language) -> &'static str {
        let s = lang.s();
        match self {
            Self::RuleList => s.subtab_rule_list,
            Self::Groups => s.subtab_groups,
            Self::Duplicates => s.subtab_duplicates,
        }
    }
}

/// Subtab interni alla scheda "Impostazioni".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSubTab {
    General,
    Themes,
    Shortcuts,
}

impl SettingsSubTab {
    /// Restituisce l'etichetta localizzata della sottoscheda Impostazioni.
    pub fn label(self, lang: Language) -> &'static str {
        let s = lang.s();
        match self {
            Self::General => s.subtab_general,
            Self::Themes => s.subtab_themes,
            Self::Shortcuts => s.subtab_shortcuts,
        }
    }
}

/// Categoria cromatica dell'indicatore di stato nell'header (coerente con la tray icon).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonBadgeKind {
    Active,
    Paused,
    Error,
    NotRunning,
}

/// Stato operativo sintetico del daemon esposto dalla UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonRuntimeState {
    Active,
    Paused,
    Error,
}

/// Vista dello stato di esecuzione del daemon ottenuta tramite IPC locale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonStatusView {
    Loading,
    Connected {
        state: DaemonRuntimeState,
        active_rules: u32,
    },
    NotRunning,
    Error(String),
}

impl DaemonStatusView {
    /// Converte l'esito della chiamata IPC `GetStatus` nello stato visualizzabile dalla UI.
    pub fn from_ipc_result(result: Result<IpcResponse, IpcClientError>) -> Self {
        match result {
            Ok(IpcResponse::Status {
                state,
                active_rules,
            }) => match state {
                IPC_STATE_ACTIVE => Self::Connected {
                    state: DaemonRuntimeState::Active,
                    active_rules,
                },
                IPC_STATE_PAUSED => Self::Connected {
                    state: DaemonRuntimeState::Paused,
                    active_rules,
                },
                IPC_STATE_ERROR => Self::Connected {
                    state: DaemonRuntimeState::Error,
                    active_rules,
                },
                other => Self::Error(format!("Stato IPC sconosciuto: 0x{:02X}", other)),
            },
            Ok(IpcResponse::Error) => {
                Self::Error("Il daemon ha restituito un errore di protocollo IPC.".to_string())
            }
            Ok(
                IpcResponse::Suspend { .. }
                | IpcResponse::Reload { .. }
                | IpcResponse::Resume { .. },
            ) => Self::Error("Risposta IPC inattesa per la richiesta GetStatus.".to_string()),
            Err(IpcClientError::DaemonNotRunning | IpcClientError::PipeBusyTimeout) => {
                Self::NotRunning
            }
            Err(err) => Self::Error(err.to_string()),
        }
    }

    /// Indica se il pulsante "Sospendi daemon" deve essere abilitato (solo se Attivo).
    pub fn can_suspend(&self) -> bool {
        matches!(
            self,
            Self::Connected {
                state: DaemonRuntimeState::Active,
                ..
            }
        )
    }

    /// Indica se il pulsante "Riprendi daemon" deve essere abilitato (solo se In Pausa; mai in Errore).
    pub fn can_resume(&self) -> bool {
        matches!(
            self,
            Self::Connected {
                state: DaemonRuntimeState::Paused,
                ..
            }
        )
    }

    /// Restituisce l'etichetta localizzata dello stato corrente.
    pub fn status_label(&self, lang: Language) -> &'static str {
        let s = lang.s();
        match self {
            Self::Loading => s.status_loading,
            Self::Connected { state, .. } => match state {
                DaemonRuntimeState::Active => s.status_active,
                DaemonRuntimeState::Paused => s.status_paused,
                DaemonRuntimeState::Error => s.status_error,
            },
            Self::NotRunning => s.status_not_running,
            Self::Error(_) => s.status_ipc_error,
        }
    }

    /// Restituisce la categoria visiva del badge nell'header coerente con la tray icon.
    pub fn badge_kind(&self) -> DaemonBadgeKind {
        match self {
            Self::Connected { state, .. } => match state {
                DaemonRuntimeState::Active => DaemonBadgeKind::Active,
                DaemonRuntimeState::Paused => DaemonBadgeKind::Paused,
                DaemonRuntimeState::Error => DaemonBadgeKind::Error,
            },
            Self::Error(_) => DaemonBadgeKind::Error,
            Self::NotRunning | Self::Loading => DaemonBadgeKind::NotRunning,
        }
    }

    /// Etichetta dinamica localizzata per l'azione di controllo Sospendi/Riprendi nel menu header.
    pub fn toggle_action_label(&self, lang: Language) -> &'static str {
        let s = lang.s();
        if matches!(
            self,
            Self::Connected {
                state: DaemonRuntimeState::Paused,
                ..
            }
        ) {
            s.menu_resume
        } else {
            s.menu_suspend
        }
    }

    /// Indica se l'azione di controllo dinamica (`Sospendi` o `Riprendi`) è consentita nello stato attuale.
    pub fn can_toggle_pause(&self) -> bool {
        self.can_suspend() || self.can_resume()
    }
}

#[cfg(test)]
mod tests {
    use rustbind::ipc_protocol::IpcProtocolError;

    use super::*;

    #[test]
    fn test_ui_tabs_and_subtabs_labels() {
        assert_eq!(UiTab::Rules.label(Language::English), "Rules");
        assert_eq!(UiTab::Settings.label(Language::English), "Settings");
        assert_eq!(
            UiTab::Security.label(Language::English),
            "Status & security"
        );

        assert_eq!(UiTab::Rules.label(Language::Italian), "Regole");
        assert_eq!(UiTab::Settings.label(Language::Italian), "Impostazioni");
        assert_eq!(
            UiTab::Security.label(Language::Italian),
            "Stato e sicurezza"
        );

        assert_eq!(
            RulesSubTab::RuleList.label(Language::Italian),
            "Elenco regole"
        );
        assert_eq!(RulesSubTab::Groups.label(Language::Italian), "Gruppi");
        assert_eq!(
            RulesSubTab::Duplicates.label(Language::Italian),
            "Regole duplicate"
        );
        assert_eq!(SettingsSubTab::General.label(Language::Italian), "Generali");
        assert_eq!(SettingsSubTab::Themes.label(Language::Italian), "Temi");

        let mut tab = UiTab::Rules;
        assert_eq!(tab, UiTab::Rules);
        let mut subtab = RulesSubTab::RuleList;
        assert_eq!(subtab, RulesSubTab::RuleList);

        subtab = RulesSubTab::Groups;
        assert_eq!(subtab, RulesSubTab::Groups);
        subtab = RulesSubTab::Duplicates;
        assert_eq!(subtab, RulesSubTab::Duplicates);
        tab = UiTab::Security;
        assert_eq!(tab, UiTab::Security);
    }

    #[test]
    fn test_daemon_status_view_mapping_and_suspend_availability() {
        let lang = Language::Italian;
        let active = DaemonStatusView::from_ipc_result(Ok(IpcResponse::Status {
            state: IPC_STATE_ACTIVE,
            active_rules: 4,
        }));
        assert_eq!(active.status_label(lang), "Attivo");
        assert_eq!(active.badge_kind(), DaemonBadgeKind::Active);
        assert_eq!(active.toggle_action_label(lang), "Sospendi daemon");
        assert!(active.can_suspend());
        assert!(!active.can_resume());
        assert!(active.can_toggle_pause());

        let paused = DaemonStatusView::from_ipc_result(Ok(IpcResponse::Status {
            state: IPC_STATE_PAUSED,
            active_rules: 4,
        }));
        assert_eq!(paused.status_label(lang), "In Pausa");
        assert_eq!(paused.badge_kind(), DaemonBadgeKind::Paused);
        assert_eq!(paused.toggle_action_label(lang), "Riprendi daemon");
        assert!(!paused.can_suspend());
        assert!(paused.can_resume());
        assert!(paused.can_toggle_pause());

        let err_state = DaemonStatusView::from_ipc_result(Ok(IpcResponse::Status {
            state: IPC_STATE_ERROR,
            active_rules: 0,
        }));
        assert_eq!(err_state.status_label(lang), "Errore");
        assert_eq!(err_state.badge_kind(), DaemonBadgeKind::Error);
        assert_eq!(err_state.toggle_action_label(lang), "Sospendi daemon");
        assert!(!err_state.can_suspend());
        assert!(!err_state.can_resume());
        assert!(!err_state.can_toggle_pause());

        let not_running = DaemonStatusView::from_ipc_result(Err(IpcClientError::DaemonNotRunning));
        assert_eq!(not_running, DaemonStatusView::NotRunning);
        assert_eq!(not_running.status_label(lang), "Non in esecuzione");
        assert_eq!(not_running.badge_kind(), DaemonBadgeKind::NotRunning);
        assert!(!not_running.can_suspend());
        assert!(!not_running.can_resume());
        assert!(!not_running.can_toggle_pause());

        let busy = DaemonStatusView::from_ipc_result(Err(IpcClientError::PipeBusyTimeout));
        assert_eq!(busy, DaemonStatusView::NotRunning);
        assert_eq!(busy.badge_kind(), DaemonBadgeKind::NotRunning);
        assert!(!busy.can_suspend());

        let io_err = DaemonStatusView::from_ipc_result(Err(IpcClientError::IoTimeout));
        assert_eq!(io_err.status_label(lang), "Errore IPC");
        assert_eq!(io_err.badge_kind(), DaemonBadgeKind::Error);
        assert!(!io_err.can_suspend());
        assert!(!io_err.can_resume());

        let proto_err = DaemonStatusView::from_ipc_result(Err(IpcClientError::Protocol(
            IpcProtocolError::Empty,
        )));
        assert_eq!(proto_err.status_label(lang), "Errore IPC");
        assert_eq!(proto_err.badge_kind(), DaemonBadgeKind::Error);
    }
}
