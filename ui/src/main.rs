#![cfg_attr(not(test), windows_subsystem = "windows")]

mod capture;
mod editor;
mod i18n;
mod state;
mod theme;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use eframe::egui;
use rustbind::config::manager::ConfigManager;
use rustbind::config::schema::Config;
use rustbind::core::action::{InputAction, MouseButton, MouseWheelDirection};
use rustbind::core::binding::KeyBinding;
use rustbind::core::key::VirtualKey;
use rustbind::core::modifiers::Modifiers;
use rustbind::core::rule::{Rule, Trigger, WindowFilter, WindowFilterMode, default_rule_group};
use rustbind::core::{
    MAX_ACTIONS_PER_SEQUENCE, MAX_DELAY_MS, MAX_EVENTS_PER_SECOND, MAX_MOUSE_MOVE_DELTA,
    MAX_MOUSE_SCROLL_STEPS, MAX_SEND_TEXT_LENGTH, MIN_DELAY_MS, MIN_EVENTS_PER_SECOND,
};
use rustbind::engine::state::{
    DaemonRuntimeDiagnostics, SkippedRuleReason, default_runtime_diagnostics_path,
    read_runtime_diagnostics,
};
use rustbind::ipc_protocol::{
    IPC_RELOAD_ACCEPTED, IPC_RESUME_REJECTED_ERROR_STATE, IPC_SUSPEND_REJECTED_ERROR_STATE,
    IpcCommand, IpcResponse,
};
use rustbind::logging::{default_log_directory, read_recent_log_warnings};
use rustbind::platform::windows::ipc_client::{IpcClientError, send_command};
use rustbind::platform::windows::{
    InstalledAppEntry, SingleInstanceGuard, apply_staged_update_from_cli_if_requested,
    check_for_updates_blocking, cleanup_stale_update_artifacts, download_stage_and_launch_updater,
    focus_window_by_title, is_autostart_enabled, launch_daemon_if_not_running,
    list_installed_applications, open_directory_in_explorer, open_https_url, pick_exe_file,
    pick_toml_export_file, pick_toml_import_file, set_autostart_enabled,
};
use rustbind::updater::{
    GITHUB_RELEASES_LATEST_URL, GITHUB_REPO_URL, ReleaseInfo, UpdateCheckOutcome,
    default_update_cache_path, read_cached_update_for,
};

use crate::capture::{ActionCaptureState, EditingActionDraft, TriggerCaptureState};
use crate::editor::{
    DuplicateActionCluster, EditorState, ImportCandidate, LoadedFileKind, NewActionKind,
    RuleGroupFilter, RuleTriggerConflict,
};
use crate::i18n::Language;
use crate::state::{DaemonBadgeKind, DaemonStatusView, RulesSubTab, SettingsSubTab, UiTab};
use crate::theme::{
    ALL_THEMES, CustomTheme, MAX_CORNER_RADIUS, ThemeConfig, ThemeId, ThemePreset,
    UI_PREFERENCES_FILENAME, apply_theme_to_style,
};

/// Titolo fisso della finestra principale UI (usato anche da `focus_window_by_title`).
pub const UI_WINDOW_TITLE: &str = "RustBind";

/// Intervallo di polling periodico silenzioso dello stato del daemon via IPC.
const IPC_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Durata di visualizzazione dei messaggi toast fluttuanti sopra al footer.
const TOAST_DURATION: Duration = Duration::from_secs(4);

/// Intervallo di inattività oltre il quale digitazioni/slider consolidano uno step di Undo.
const UNDO_DEBOUNCE_DURATION: Duration = Duration::from_millis(600);

/// Ritardo di debounce per la persistenza automatica di split ratio e geometria finestra.
const LAYOUT_SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

/// Payload trascinabile per riordinare le azioni nella sequenza tramite drag & drop.
#[derive(Clone, Copy)]
struct ActionDragPayload {
    rule_idx: usize,
    action_idx: usize,
}

/// Payload trascinabile per importare le azioni di un'altra regola tramite drag & drop.
#[derive(Clone, Copy)]
struct RuleActionsDragPayload {
    source_rule_idx: usize,
}

/// Colore d'accento principale per elementi selezionati e avvisi informativi (`#b7410e`).
#[cfg(test)]
const RUST_ACCENT_COLOR: egui::Color32 = egui::Color32::from_rgb(0xB7, 0x41, 0x0E);

/// Raggio uniforme per tutte le card e i contenitori visuali della UI.
const CARD_CORNER_RADIUS: u8 = 10;

/// Icona compatta del brand (`64x64` PNG RGBA) incorporata per finestra e taskbar.
const BRAND_ICON_PNG: &[u8] = include_bytes!("../../assets/rustbind-64.png");

/// Sostituisce i colori d'accento azzurri predefiniti di egui con il tema classico `#b7410e`.
#[cfg(test)]
fn apply_rust_accent_style(style: &mut egui::Style) {
    apply_theme_to_style(style, &ThemeConfig::default(), CARD_CORNER_RADIUS);
}

/// Appende o imposta un token separato da virgola in un campo opzionale senza duplicati case-insensitive.
fn append_filter_token(target: &mut Option<String>, token: &str) {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return;
    }
    let current = target.as_deref().unwrap_or("").trim();
    if current.is_empty() {
        *target = Some(trimmed.to_string());
        return;
    }
    let already_present = current
        .split(',')
        .map(str::trim)
        .any(|existing| existing.eq_ignore_ascii_case(trimmed));
    if !already_present {
        *target = Some(format!("{current}, {trimmed}"));
    }
}

/// Restituisce un `Frame::group` con il border-radius configurato per card e menu.
fn card_frame(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::group(ui.style()).corner_radius(ui.visuals().menu_corner_radius)
}

/// Legge il raggio corrente delle finestre modali salvato sul contesto egui.
fn modal_corner_radius(ctx: &egui::Context) -> u8 {
    ctx.data(|d| d.get_temp::<u8>(egui::Id::NULL.with("rustbind_modal_corner_radius")))
        .unwrap_or(14)
}

/// Disegna i quattro pallini di anteprima cromatica nelle card dei temi.
fn render_color_swatches(ui: &mut egui::Ui, swatches: [egui::Color32; 4], border: egui::Color32) {
    ui.horizontal(|ui| {
        for color in swatches {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 6.0, color);
            ui.painter()
                .circle_stroke(rect.center(), 6.0, egui::Stroke::new(1.0, border));
        }
    });
}

/// Restituisce un `Frame` senza bordo per raggruppare sezioni separate da divider orizzontali.
fn section_frame() -> egui::Frame {
    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(6, 2))
}

/// Disegna un divider orizzontale full-width leggermente staccato dai bordi laterali.
fn render_section_divider(ui: &mut egui::Ui) {
    ui.add_space(6.0);
    let avail_w = ui.available_width();
    let inset = 8.0;
    let line_w = (avail_w - inset * 2.0).max(20.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(avail_w, 1.0), egui::Sense::hover());
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    ui.painter().line_segment(
        [
            egui::pos2(rect.min.x + inset, rect.center().y),
            egui::pos2(rect.min.x + inset + line_w, rect.center().y),
        ],
        stroke,
    );
    ui.add_space(6.0);
}

enum IpcTaskMsg {
    Status(Result<IpcResponse, IpcClientError>),
    Suspend(Result<IpcResponse, IpcClientError>),
    Resume(Result<IpcResponse, IpcClientError>),
    Reload(Result<IpcResponse, IpcClientError>),
}

enum UpdateTaskMsg {
    Checked {
        manual: bool,
        result: Result<UpdateCheckOutcome, String>,
    },
    DownloadedAndLaunched(Result<(), String>),
}

#[derive(Clone)]
struct ToastNotice {
    message: String,
    is_error: bool,
    expires_at: Instant,
}

enum PendingDeleteTarget {
    Rule {
        idx: usize,
        name: String,
    },
    Group {
        name: String,
    },
    ResolveDuplicates {
        keep_id: String,
        keep_name: String,
        remove_ids: HashSet<String>,
        merge_groups: bool,
    },
    ImportOverwrite(ImportCandidate),
    SwitchProfileDirty {
        target_name: String,
    },
    ExitUnsaved,
    UpdateUnsaved(ReleaseInfo),
}

enum CaptureModalState {
    Trigger(TriggerCaptureState),
    Actions(Box<ActionCaptureState>),
}

struct RustBindUiApp {
    active_tab: UiTab,
    rules_subtab: RulesSubTab,
    settings_subtab: SettingsSubTab,
    status_view: DaemonStatusView,
    editor: EditorState,
    theme_config: ThemeConfig,
    theme_path: Option<PathBuf>,
    theme_editor_open: bool,
    rule_search_query: String,
    rule_group_filter: RuleGroupFilter,
    rule_filter_only_enabled: bool,
    rule_filter_only_disabled: bool,
    group_search_query: String,
    available_rules_search: String,
    group_members_search: String,
    new_group_name: String,
    focus_new_group_input: bool,
    selected_group: String,
    selected_available_rule_ids: HashSet<String>,
    selected_member_rule_ids: HashSet<String>,
    duplicate_keep_by_cluster: HashMap<usize, String>,
    duplicate_merge_by_cluster: HashMap<usize, bool>,
    rules_split_ratio: f32,
    groups_split_ratio: f32,
    focus_rule_search: bool,
    last_seen_draft: Config,
    last_draft_change_at: Option<Instant>,
    layout_dirty_since: Option<Instant>,
    pending_delete: Option<PendingDeleteTarget>,
    capture_modal: Option<CaptureModalState>,
    allow_close_with_dirty: bool,
    autostart_enabled: bool,
    toast: Option<ToastNotice>,
    ipc_in_flight: bool,
    tx: Sender<IpcTaskMsg>,
    rx: Receiver<IpcTaskMsg>,
    initial_fetch_started: bool,
    was_focused: bool,
    last_poll: Instant,
    installed_apps_cache: Vec<InstalledAppEntry>,
    installed_apps_loaded: bool,
    app_picker_search: String,
    daemon_diagnostics: Option<DaemonRuntimeDiagnostics>,
    recent_log_warnings: Vec<String>,
    recent_logs_loaded: bool,
    update_tx: Sender<UpdateTaskMsg>,
    update_rx: Receiver<UpdateTaskMsg>,
    update_check_in_flight: bool,
    update_install_in_flight: bool,
    available_update: Option<ReleaseInfo>,
    latest_changelog_url: Option<String>,
    update_notice_dismissed: bool,
    startup_update_check_started: bool,
}

#[derive(Clone, Copy)]
enum KeycapSize {
    Compact,
    Medium,
    Large,
}

impl RustBindUiApp {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let (update_tx, update_rx) = mpsc::channel();
        let mut editor = EditorState::load_default();
        let theme_path = ConfigManager::default_app_data_dir()
            .ok()
            .map(|d| d.join(UI_PREFERENCES_FILENAME));
        let theme_config = theme_path
            .as_deref()
            .map(ThemeConfig::load_from_file)
            .unwrap_or_default();
        editor.language = theme_config.language;
        let rules_split_ratio = theme_config.rules_split_ratio;
        let groups_split_ratio = theme_config.groups_split_ratio;
        let last_seen_draft = editor.draft.clone();
        let active_tab = if editor.is_locked_invalid() {
            UiTab::Settings
        } else {
            UiTab::Rules
        };
        let cached_update = if theme_config.auto_check_updates {
            default_update_cache_path()
                .and_then(|p| read_cached_update_for(&p, env!("CARGO_PKG_VERSION")))
        } else {
            None
        };
        let latest_changelog_url = cached_update.as_ref().map(|u| u.html_url.clone());
        let mut app = Self {
            active_tab,
            rules_subtab: RulesSubTab::RuleList,
            settings_subtab: SettingsSubTab::General,
            status_view: DaemonStatusView::Loading,
            editor,
            theme_config,
            theme_path,
            theme_editor_open: false,
            rule_search_query: String::new(),
            rule_group_filter: RuleGroupFilter::All,
            rule_filter_only_enabled: false,
            rule_filter_only_disabled: false,
            group_search_query: String::new(),
            available_rules_search: String::new(),
            group_members_search: String::new(),
            new_group_name: String::new(),
            focus_new_group_input: false,
            selected_group: default_rule_group(),
            selected_available_rule_ids: HashSet::new(),
            selected_member_rule_ids: HashSet::new(),
            duplicate_keep_by_cluster: HashMap::new(),
            duplicate_merge_by_cluster: HashMap::new(),
            rules_split_ratio,
            groups_split_ratio,
            focus_rule_search: false,
            last_seen_draft,
            last_draft_change_at: None,
            layout_dirty_since: None,
            pending_delete: None,
            capture_modal: None,
            allow_close_with_dirty: false,
            autostart_enabled: false,
            toast: None,
            ipc_in_flight: false,
            tx,
            rx,
            initial_fetch_started: false,
            was_focused: false,
            last_poll: Instant::now(),
            installed_apps_cache: Vec::new(),
            installed_apps_loaded: false,
            app_picker_search: String::new(),
            daemon_diagnostics: None,
            recent_log_warnings: Vec::new(),
            recent_logs_loaded: false,
            update_tx,
            update_rx,
            update_check_in_flight: false,
            update_install_in_flight: false,
            available_update: cached_update,
            latest_changelog_url,
            update_notice_dismissed: false,
            startup_update_check_started: false,
        };
        app.refresh_autostart_state();
        app
    }

    fn show_toast(&mut self, is_error: bool, message: impl Into<String>) {
        self.toast = Some(ToastNotice {
            message: message.into(),
            is_error,
            expires_at: Instant::now() + TOAST_DURATION,
        });
    }

    fn flush_editor_notice_to_toast(&mut self) {
        if let Some((is_error, msg)) = self.editor.notice.take() {
            self.show_toast(is_error, msg);
        }
    }

    fn refresh_autostart_state(&mut self) {
        match is_autostart_enabled() {
            Ok(enabled) => self.autostart_enabled = enabled,
            Err(err) => {
                self.show_toast(
                    true,
                    format!("Lettura stato avvio automatico fallita: {}", err),
                );
            }
        }
    }

    fn request_status(&mut self, ctx: &egui::Context) {
        self.request_status_with_loading(ctx, true);
    }

    fn request_status_silent(&mut self, ctx: &egui::Context) {
        self.request_status_with_loading(ctx, false);
    }

    fn request_status_with_loading(&mut self, ctx: &egui::Context, show_loading: bool) {
        if self.ipc_in_flight {
            return;
        }
        self.ipc_in_flight = true;
        if show_loading {
            self.status_view = DaemonStatusView::Loading;
        }
        let tx = self.tx.clone();
        let repaint_ctx = ctx.clone();
        std::thread::spawn(move || {
            let res = send_command(IpcCommand::GetStatus);
            let _ = tx.send(IpcTaskMsg::Status(res));
            repaint_ctx.request_repaint();
        });
    }

    fn spawn_command_and_refresh<F>(&mut self, ctx: &egui::Context, cmd: IpcCommand, wrap: F)
    where
        F: FnOnce(Result<IpcResponse, IpcClientError>) -> IpcTaskMsg + Send + 'static,
    {
        if self.ipc_in_flight {
            return;
        }
        self.ipc_in_flight = true;
        let tx = self.tx.clone();
        let repaint_ctx = ctx.clone();
        std::thread::spawn(move || {
            let cmd_res = send_command(cmd);
            let _ = tx.send(wrap(cmd_res));
            std::thread::sleep(Duration::from_millis(50));
            let status_res = send_command(IpcCommand::GetStatus);
            let _ = tx.send(IpcTaskMsg::Status(status_res));
            repaint_ctx.request_repaint();
        });
    }

    fn request_suspend(&mut self, ctx: &egui::Context) {
        self.spawn_command_and_refresh(ctx, IpcCommand::Suspend, IpcTaskMsg::Suspend);
    }

    fn request_resume(&mut self, ctx: &egui::Context) {
        self.spawn_command_and_refresh(ctx, IpcCommand::Resume, IpcTaskMsg::Resume);
    }

    fn request_reload(&mut self, ctx: &egui::Context) {
        self.spawn_command_and_refresh(ctx, IpcCommand::ReloadConfig, IpcTaskMsg::Reload);
    }

    fn lang(&self) -> Language {
        self.theme_config.language
    }

    fn start_update_check(&mut self, ctx: &egui::Context, manual: bool) {
        if self.update_check_in_flight || self.update_install_in_flight {
            return;
        }
        self.update_check_in_flight = true;
        let tx = self.update_tx.clone();
        let repaint_ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = check_for_updates_blocking(env!("CARGO_PKG_VERSION"));
            let _ = tx.send(UpdateTaskMsg::Checked { manual, result });
            repaint_ctx.request_repaint();
        });
    }

    fn start_update_download_and_apply(&mut self, ctx: &egui::Context, release: ReleaseInfo) {
        if self.update_install_in_flight {
            return;
        }
        self.update_install_in_flight = true;
        let tx = self.update_tx.clone();
        let repaint_ctx = ctx.clone();
        std::thread::spawn(move || {
            let res = download_stage_and_launch_updater(&release);
            let _ = tx.send(UpdateTaskMsg::DownloadedAndLaunched(res));
            repaint_ctx.request_repaint();
        });
    }

    fn open_external_url(&mut self, url: &str) {
        if let Err(err) = open_https_url(url) {
            self.show_toast(true, err.to_string());
        }
    }

    fn current_changelog_url(&self) -> &str {
        self.available_update
            .as_ref()
            .map(|u| u.html_url.as_str())
            .or(self.latest_changelog_url.as_deref())
            .unwrap_or(GITHUB_RELEASES_LATEST_URL)
    }

    fn poll_update_results(&mut self, ctx: &egui::Context) {
        let lang = self.lang();
        while let Ok(msg) = self.update_rx.try_recv() {
            match msg {
                UpdateTaskMsg::Checked { manual, result } => {
                    self.update_check_in_flight = false;
                    match result {
                        Ok(UpdateCheckOutcome::UpdateAvailable(info)) => {
                            self.latest_changelog_url = Some(info.html_url.clone());
                            self.available_update = Some(info);
                            self.update_notice_dismissed = false;
                        }
                        Ok(UpdateCheckOutcome::UpToDate {
                            latest_version,
                            html_url,
                            ..
                        }) => {
                            self.latest_changelog_url = Some(html_url);
                            self.available_update = None;
                            if manual {
                                let shown_ver = latest_version
                                    .as_deref()
                                    .unwrap_or(env!("CARGO_PKG_VERSION"));
                                self.show_toast(false, lang.fmt_up_to_date(shown_ver));
                            }
                        }
                        Err(err) => {
                            if manual {
                                self.show_toast(true, lang.fmt_update_check_failed(&err));
                            }
                        }
                    }
                }
                UpdateTaskMsg::DownloadedAndLaunched(res) => {
                    self.update_install_in_flight = false;
                    match res {
                        Ok(()) => {
                            self.flush_layout_to_disk();
                            self.allow_close_with_dirty = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        Err(err) => {
                            self.show_toast(true, lang.fmt_update_install_failed(&err));
                        }
                    }
                }
            }
        }
    }

    fn poll_ipc_results(&mut self) {
        let s = self.lang().s();
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                IpcTaskMsg::Status(res) => {
                    self.status_view = DaemonStatusView::from_ipc_result(res);
                    self.daemon_diagnostics = if matches!(
                        self.status_view,
                        crate::state::DaemonStatusView::Connected { .. }
                    ) {
                        read_runtime_diagnostics(&default_runtime_diagnostics_path())
                    } else {
                        None
                    };
                    self.ipc_in_flight = false;
                    self.last_poll = Instant::now();
                }
                IpcTaskMsg::Suspend(res) => {
                    let (is_err, text) = match res {
                        Ok(IpcResponse::Suspend { outcome })
                            if outcome != IPC_SUSPEND_REJECTED_ERROR_STATE =>
                        {
                            (false, s.toast_suspended.to_string())
                        }
                        Ok(IpcResponse::Suspend { .. }) => {
                            (true, s.toast_suspend_error_state.to_string())
                        }
                        Ok(_) => (true, s.status_ipc_error.to_string()),
                        Err(err) => (true, err.to_string()),
                    };
                    self.show_toast(is_err, text);
                }
                IpcTaskMsg::Resume(res) => {
                    let (is_err, text) = match res {
                        Ok(IpcResponse::Resume { outcome })
                            if outcome != IPC_RESUME_REJECTED_ERROR_STATE =>
                        {
                            (false, s.toast_resumed.to_string())
                        }
                        Ok(IpcResponse::Resume { .. }) => {
                            (true, s.toast_resume_error_state.to_string())
                        }
                        Ok(_) => (true, s.status_ipc_error.to_string()),
                        Err(err) => (true, err.to_string()),
                    };
                    self.show_toast(is_err, text);
                }
                IpcTaskMsg::Reload(res) => {
                    let (is_err, text) = match res {
                        Ok(IpcResponse::Reload { outcome }) if outcome == IPC_RELOAD_ACCEPTED => {
                            (false, s.toast_daemon_reloaded.to_string())
                        }
                        Ok(_) => (true, s.status_ipc_error.to_string()),
                        Err(IpcClientError::DaemonNotRunning | IpcClientError::PipeBusyTimeout) => {
                            (true, s.toast_daemon_not_running_save.to_string())
                        }
                        Err(err) => (true, err.to_string()),
                    };
                    self.show_toast(is_err, text);
                }
            }
        }
    }

    fn badge_color(&self) -> egui::Color32 {
        match self.status_view.badge_kind() {
            DaemonBadgeKind::Active => egui::Color32::from_rgb(50, 205, 50),
            DaemonBadgeKind::Paused => egui::Color32::from_rgb(255, 165, 0),
            DaemonBadgeKind::Error => egui::Color32::from_rgb(220, 48, 48),
            DaemonBadgeKind::NotRunning => egui::Color32::from_rgb(140, 145, 155),
        }
    }

    fn render_header_status_badge(&self, ui: &mut egui::Ui) {
        let color = self.badge_color();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
        let center = rect.center();
        ui.painter().circle_filled(center, 5.0, color);
        ui.painter().circle_stroke(
            center,
            5.0,
            egui::Stroke::new(1.0, egui::Color32::from_black_alpha(90)),
        );
        ui.colored_label(
            color,
            egui::RichText::new(self.status_view.status_label(self.lang())).strong(),
        );
    }

    fn render_header_daemon_menu(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let s = lang.s();
        ui.menu_button(s.menu_daemon, |ui| {
            if ui
                .add_enabled(!self.ipc_in_flight, egui::Button::new(s.menu_refresh))
                .clicked()
            {
                let ctx = ui.ctx().clone();
                self.request_status(&ctx);
                ui.close();
            }

            let can_toggle = !self.ipc_in_flight && self.status_view.can_toggle_pause();
            if ui
                .add_enabled(
                    can_toggle,
                    egui::Button::new(self.status_view.toggle_action_label(lang)),
                )
                .clicked()
            {
                let ctx = ui.ctx().clone();
                if self.status_view.can_resume() {
                    self.request_resume(&ctx);
                } else if self.status_view.can_suspend() {
                    self.request_suspend(&ctx);
                }
                ui.close();
            }

            if ui
                .add_enabled(!self.ipc_in_flight, egui::Button::new(s.menu_reload))
                .clicked()
            {
                let ctx = ui.ctx().clone();
                self.request_reload(&ctx);
                ui.close();
            }
        });
    }

    fn render_modifiers_and_key(
        ui: &mut egui::Ui,
        id_salt: &str,
        modifiers: &mut Modifiers,
        key: &mut VirtualKey,
    ) {
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut modifiers.ctrl, "Ctrl");
            ui.checkbox(&mut modifiers.alt, "Alt");
            ui.checkbox(&mut modifiers.shift, "Shift");
            ui.checkbox(&mut modifiers.win, "Win");
            Self::render_virtual_key_combo(ui, id_salt, key);
        });
    }

    fn render_enum_combo<T: Copy + PartialEq>(
        ui: &mut egui::Ui,
        id_salt: &str,
        value: &mut T,
        candidates: &[T],
        label_of: impl Fn(T) -> &'static str,
        width: Option<f32>,
    ) {
        let mut combo = egui::ComboBox::from_id_salt(id_salt).selected_text(label_of(*value));
        if let Some(w) = width {
            combo = combo.width(w);
        }
        combo.show_ui(ui, |ui| {
            for &candidate in candidates {
                ui.selectable_value(value, candidate, label_of(candidate));
            }
        });
    }

    fn render_virtual_key_combo(ui: &mut egui::Ui, id_salt: &str, key: &mut VirtualKey) {
        Self::render_enum_combo(ui, id_salt, key, VirtualKey::ALL, |k| k.as_str(), None);
    }

    fn render_virtual_key_combo_with_width(
        ui: &mut egui::Ui,
        id_salt: &str,
        key: &mut VirtualKey,
        width: Option<f32>,
    ) {
        Self::render_enum_combo(ui, id_salt, key, VirtualKey::ALL, |k| k.as_str(), width);
    }

    fn render_mouse_button_combo_with_width(
        ui: &mut egui::Ui,
        id_salt: &str,
        button: &mut MouseButton,
        width: Option<f32>,
    ) {
        Self::render_enum_combo(
            ui,
            id_salt,
            button,
            MouseButton::ALL,
            MouseButton::as_str,
            width,
        );
    }

    fn render_mouse_wheel_dir_combo_with_width(
        ui: &mut egui::Ui,
        id_salt: &str,
        direction: &mut MouseWheelDirection,
        width: Option<f32>,
    ) {
        Self::render_enum_combo(
            ui,
            id_salt,
            direction,
            MouseWheelDirection::ALL,
            MouseWheelDirection::as_str,
            width,
        );
    }

    /// Intestazione di card con titolo troncato e pulsante di eliminazione `X` opzionale in alto a destra.
    fn render_card_header_with_delete(
        ui: &mut egui::Ui,
        title: &str,
        show_delete: bool,
        delete_tooltip: &str,
    ) -> bool {
        let row_w = ui.available_width().max(40.0);
        let row_h = ui.spacing().interact_size.y.max(20.0);
        let mut delete_clicked = false;
        if show_delete {
            ui.allocate_ui_with_layout(
                egui::vec2(row_w, row_h),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    if ui.button("X").on_hover_text(delete_tooltip).clicked() {
                        delete_clicked = true;
                    }
                    let title_w = ui.available_width().max(16.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(title_w, row_h),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new(title).strong())
                                    .truncate()
                                    .selectable(false),
                            );
                        },
                    );
                },
            );
        } else {
            ui.allocate_ui_with_layout(
                egui::vec2(row_w, row_h),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(title).strong())
                            .truncate()
                            .selectable(false),
                    );
                },
            );
        }
        delete_clicked
    }

    /// Disegna una card cliccabile sull'intera superficie e uniforme in larghezza orizzontale.
    ///
    /// L'area cliccabile viene registrata prima dei widget figli così pulsanti e checkbox interni
    /// conservano la priorità di interazione, e la selezione non altera le dimensioni della card.
    fn render_selectable_card<F>(
        ui: &mut egui::Ui,
        id_salt: impl egui::AsIdSalt,
        is_selected: bool,
        enabled: bool,
        add_contents: F,
    ) -> bool
    where
        F: FnOnce(&mut egui::Ui),
    {
        Self::render_selectable_card_inner(ui, id_salt, is_selected, enabled, None, add_contents).0
    }

    fn render_selectable_card_inner<F>(
        ui: &mut egui::Ui,
        id_salt: impl egui::AsIdSalt,
        is_selected: bool,
        enabled: bool,
        drag_source_rule_idx: Option<usize>,
        add_contents: F,
    ) -> (bool, egui::Rect)
    where
        F: FnOnce(&mut egui::Ui),
    {
        let card_w = (ui.available_width() - 4.0).max(100.0);
        let click_id = ui.make_persistent_id(id_salt);
        let rect_mem_id = click_id.with("card_rect");
        let prev_rect = ui
            .data(|d| d.get_temp::<egui::Rect>(rect_mem_id))
            .unwrap_or_else(|| {
                egui::Rect::from_min_size(ui.cursor().min, egui::vec2(card_w, 56.0))
            });

        // Interazione registrata prima dei controlli interni: i click su pulsanti/checkbox figli vincono.
        let sense = if drag_source_rule_idx.is_some() {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::click()
        };
        let card_resp = if enabled {
            Some(ui.interact(prev_rect, click_id, sense))
        } else {
            None
        };
        let card_clicked = card_resp.as_ref().is_some_and(egui::Response::clicked);
        if let (Some(resp), Some(source_rule_idx)) = (&card_resp, drag_source_rule_idx) {
            if resp.drag_started() {
                egui::DragAndDrop::set_payload(
                    ui.ctx(),
                    RuleActionsDragPayload { source_rule_idx },
                );
            }
            if resp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
        }

        let accent = ui.visuals().selection.bg_fill;
        let fill = if !enabled {
            ui.visuals().faint_bg_color
        } else if is_selected {
            egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 36)
        } else {
            ui.visuals().widgets.noninteractive.bg_fill
        };
        let base_stroke = ui.visuals().widgets.noninteractive.bg_stroke;

        let inner_w = (card_w - 16.0).max(60.0);
        let frame_resp = card_frame(ui)
            .fill(fill)
            .stroke(base_stroke)
            .show(ui, |ui| {
                ui.set_min_width(inner_w);
                ui.set_max_width(inner_w);
                // Impedisce che le etichette interne rubino il click destinato alla card.
                ui.style_mut().interaction.selectable_labels = false;
                ui.add_enabled_ui(enabled, add_contents);
            });

        let actual_rect = frame_resp.response.rect;
        ui.data_mut(|d| d.insert_temp(rect_mem_id, actual_rect));

        if is_selected && enabled {
            ui.painter().rect_stroke(
                actual_rect,
                f32::from(ui.visuals().menu_corner_radius.nw),
                egui::Stroke::new(1.5, accent),
                egui::StrokeKind::Inside,
            );
        }

        (card_clicked, actual_rect)
    }

    /// Suddivisione orizzontale ridimensionabile tramite drag del divider verticale con limiti minimi,
    /// con fallback verticale automatico su finestre molto strette.
    #[allow(clippy::too_many_arguments)]
    fn render_resizable_split<L, R>(
        &mut self,
        ui: &mut egui::Ui,
        id_salt: &str,
        mut ratio: f32,
        min_left: f32,
        min_right: f32,
        render_left: L,
        render_right: R,
    ) -> f32
    where
        L: FnOnce(&mut Self, &mut egui::Ui, bool),
        R: FnOnce(&mut Self, &mut egui::Ui, bool),
    {
        ui.push_id(id_salt, |ui| {
            let avail_w = ui.available_width();
            let avail_h = ui.available_height().max(200.0);
            let divider_w = 10.0;
            let spacing_x = ui.spacing().item_spacing.x;
            let overhead = divider_w + spacing_x * 2.0;

            if avail_w < min_left + min_right + overhead + 4.0 {
                egui::ScrollArea::vertical()
                    .id_salt((id_salt, "compact_scroll"))
                    .hscroll(false)
                    .show(ui, |ui| {
                        render_left(self, ui, false);
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(6.0);
                        render_right(self, ui, false);
                    });
                return ratio;
            }

            let max_left = (avail_w - min_right - overhead).max(min_left);
            let left_w = (ratio * avail_w).clamp(min_left, max_left);
            let right_w = (avail_w - left_w - overhead).max(min_right);

            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(left_w, avail_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_min_size(egui::vec2(left_w, avail_h));
                        ui.set_max_size(egui::vec2(left_w, avail_h));
                        let clip = ui.available_rect_before_wrap().intersect(ui.clip_rect());
                        ui.set_clip_rect(clip);
                        render_left(self, ui, true);
                    },
                );

                let (div_rect, div_resp) =
                    ui.allocate_exact_size(egui::vec2(divider_w, avail_h), egui::Sense::drag());
                let active_div = div_resp.hovered() || div_resp.dragged();
                if active_div {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
                if div_resp.dragged() {
                    let next_left = (left_w + div_resp.drag_delta().x).clamp(min_left, max_left);
                    ratio = next_left / avail_w;
                }
                let line_color = if active_div {
                    ui.visuals().selection.bg_fill
                } else {
                    ui.visuals().widgets.noninteractive.bg_stroke.color
                };
                let line_w = if active_div { 2.0 } else { 1.0 };
                ui.painter().line_segment(
                    [
                        egui::pos2(div_rect.center().x, div_rect.top()),
                        egui::pos2(div_rect.center().x, div_rect.bottom()),
                    ],
                    egui::Stroke::new(line_w, line_color),
                );

                ui.allocate_ui_with_layout(
                    egui::vec2(right_w, avail_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_min_size(egui::vec2(right_w, avail_h));
                        ui.set_max_size(egui::vec2(right_w, avail_h));
                        let clip = ui.available_rect_before_wrap().intersect(ui.clip_rect());
                        ui.set_clip_rect(clip);
                        render_right(self, ui, true);
                    },
                );
            });
            ratio
        })
        .inner
    }

    fn render_rules_tab(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let s = lang.s();
        if self.editor.is_locked_invalid() {
            ui.add_space(6.0);
            ui.colored_label(
                egui::Color32::from_rgb(220, 48, 48),
                s.rules_blocked_invalid,
            );
            if ui.button(s.go_to_settings_restore).clicked() {
                self.active_tab = UiTab::Settings;
                self.settings_subtab = SettingsSubTab::General;
            }
            return;
        }

        let duplicate_clusters = self.editor.duplicate_action_groups();
        if duplicate_clusters.is_empty() && self.rules_subtab == RulesSubTab::Duplicates {
            self.rules_subtab = RulesSubTab::RuleList;
        }

        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(
                &mut self.rules_subtab,
                RulesSubTab::RuleList,
                RulesSubTab::RuleList.label(lang),
            );
            ui.selectable_value(
                &mut self.rules_subtab,
                RulesSubTab::Groups,
                RulesSubTab::Groups.label(lang),
            );
            if !duplicate_clusters.is_empty() {
                ui.selectable_value(
                    &mut self.rules_subtab,
                    RulesSubTab::Duplicates,
                    format!(
                        "{} ({})",
                        RulesSubTab::Duplicates.label(lang),
                        duplicate_clusters.len()
                    ),
                );
            }
        });
        ui.separator();
        ui.add_space(4.0);

        match self.rules_subtab {
            RulesSubTab::RuleList => self.render_rules_editor(ui),
            RulesSubTab::Groups => self.render_rules_groups_subtab(ui),
            RulesSubTab::Duplicates => self.render_rules_duplicates_subtab(ui, &duplicate_clusters),
        }
    }

    fn render_rules_editor(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let s = lang.s();
        let ratio = self.rules_split_ratio;
        let mut pending_move_up: Option<usize> = None;
        let mut pending_move_down: Option<usize> = None;
        let mut pending_move_rule_to: Option<(usize, usize)> = None;
        let mut pending_duplicate_rule: Option<usize> = None;
        let mut pending_import_rule_actions: Option<usize> = None;
        let mut pending_add_rule = false;
        let mut pending_request_delete: Option<PendingDeleteTarget> = None;
        let mut pending_jump_to_new_group = false;

        let active_rule_drag = egui::DragAndDrop::payload::<RuleActionsDragPayload>(ui.ctx());
        let dragged_rule_actions: Option<(usize, Vec<InputAction>)> =
            active_rule_drag.as_ref().and_then(|payload| {
                self.editor
                    .draft
                    .rules
                    .get(payload.source_rule_idx)
                    .map(|r| (payload.source_rule_idx, r.actions.clone()))
            });
        let pointer_latest = ui.ctx().pointer_latest_pos();

        let available_groups = self.editor.available_groups();
        self.rule_group_filter.prune_missing(&available_groups);
        let filtered_indices = self.editor.filtered_rule_indices_with_filters(
            &self.rule_search_query,
            &self.rule_group_filter,
            self.rule_filter_only_enabled,
            self.rule_filter_only_disabled,
        );
        let live_conflicts = self.editor.trigger_conflicts();
        let daemon_skipped_by_id: HashMap<String, SkippedRuleReason> = self
            .daemon_diagnostics
            .as_ref()
            .map(|d| {
                d.skipped_rules
                    .iter()
                    .map(|item| (item.rule_id.clone(), item.reason))
                    .collect()
            })
            .unwrap_or_default();

        let mut pending_add_action: Option<NewActionKind> = None;
        let mut pending_remove_action: Option<usize> = None;
        let mut pending_duplicate_action: Option<usize> = None;
        let mut pending_action_up: Option<usize> = None;
        let mut pending_action_down: Option<usize> = None;
        let mut pending_move_action_to: Option<(usize, usize)> = None;
        let mut pending_start_trigger_capture: Option<usize> = None;
        let mut pending_start_action_capture: Option<usize> = None;

        self.rules_split_ratio = self.render_resizable_split(
            ui,
            "rules_split",
            ratio,
            230.0,
            310.0,
            |app, ui, fill_height| {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(lang.fmt_rules_count(app.editor.draft.rules.len()));
                    if ui.button(s.btn_new).on_hover_text("Ctrl+N").clicked() {
                        pending_add_rule = true;
                    }
                });
                ui.add_space(4.0);

                egui::Grid::new(("rules_search_grid", lang.code()))
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(s.label_groups);
                        let filter_label = match &app.rule_group_filter {
                            RuleGroupFilter::All => s.filter_all_groups.to_string(),
                            RuleGroupFilter::Selection {
                                include_no_group,
                                selected_groups,
                            } => {
                                let mut parts: Vec<&str> = Vec::new();
                                if *include_no_group {
                                    parts.push(s.filter_no_group);
                                }
                                for group_name in selected_groups {
                                    parts.push(group_name.as_str());
                                }
                                if parts.is_empty() {
                                    s.filter_all_groups.to_string()
                                } else {
                                    parts.join(", ")
                                }
                            }
                        };
                        let combo_w = (ui.available_width() - 4.0).max(80.0);
                        let combo_resp = egui::ComboBox::from_id_salt("rule_list_group_filter")
                            .width(combo_w)
                            .wrap_mode(egui::TextWrapMode::Truncate)
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .selected_text(&filter_label)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(
                                        matches!(app.rule_group_filter, RuleGroupFilter::All),
                                        s.filter_all_groups,
                                    )
                                    .clicked()
                                {
                                    app.rule_group_filter = RuleGroupFilter::All;
                                }
                                ui.separator();
                                let mut no_group_checked =
                                    app.rule_group_filter.includes_no_group();
                                if ui
                                    .checkbox(&mut no_group_checked, s.filter_no_group)
                                    .changed()
                                {
                                    app.rule_group_filter.set_no_group(no_group_checked);
                                }
                                for group_name in &available_groups {
                                    let mut checked =
                                        app.rule_group_filter.contains_group(group_name);
                                    if ui.checkbox(&mut checked, group_name).changed() {
                                        app.rule_group_filter.set_group(group_name, checked);
                                    }
                                }
                            });
                        if matches!(app.rule_group_filter, RuleGroupFilter::Selection { .. }) {
                            combo_resp.response.on_hover_text(&filter_label);
                        }
                        ui.end_row();

                        ui.label(s.label_search);
                        let cell_w = (ui.available_width() - 4.0).max(80.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(cell_w, 22.0),
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if !app.rule_search_query.is_empty()
                                    && ui.button(s.btn_clear).clicked()
                                {
                                    app.rule_search_query.clear();
                                }
                                let search_w = ui.available_width().max(60.0);
                                let search_resp = ui
                                    .add(
                                        egui::TextEdit::singleline(&mut app.rule_search_query)
                                            .desired_width(search_w)
                                            .hint_text(s.search_hint_rules),
                                    )
                                    .on_hover_text("Ctrl+F");
                                if app.focus_rule_search {
                                    search_resp.request_focus();
                                    app.focus_rule_search = false;
                                }
                            },
                        );
                        ui.end_row();
                    });
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .checkbox(&mut app.rule_filter_only_enabled, s.filter_only_active)
                        .changed()
                        && app.rule_filter_only_enabled
                    {
                        app.rule_filter_only_disabled = false;
                    }
                    if ui
                        .checkbox(&mut app.rule_filter_only_disabled, s.filter_only_disabled)
                        .changed()
                        && app.rule_filter_only_disabled
                    {
                        app.rule_filter_only_enabled = false;
                    }
                });
                ui.add_space(4.0);

                let list_h = if fill_height {
                    ui.available_height().max(120.0)
                } else {
                    220.0
                };
                egui::ScrollArea::vertical()
                    .id_salt("rules_list_scroll")
                    .hscroll(false)
                    .max_height(list_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let rule_count = app.editor.draft.rules.len();
                        if filtered_indices.is_empty() {
                            ui.small(s.empty_rules_filtered);
                        }
                        for idx in filtered_indices {
                            let is_selected = app.editor.selected_rule_idx == Some(idx);
                            let (title, preview_text, context_badge, conflict_badge) = {
                                let r = &app.editor.draft.rules[idx];
                                let status_tag = if r.enabled { "ON" } else { "OFF" };
                                let preview = format_rule_card_preview(r);
                                let ctx_badge = r
                                    .window_filter
                                    .as_ref()
                                    .filter(|wf| !wf.is_empty())
                                    .and_then(|wf| {
                                        wf.process_name.as_deref().map(|proc_name| {
                                            lang.fmt_context_badge(
                                                matches!(wf.mode, WindowFilterMode::Exclude),
                                                proc_name.trim(),
                                            )
                                        })
                                    })
                                    .or_else(|| {
                                        let inherited = app.editor.inherited_group_contexts(r);
                                        if inherited.is_empty() {
                                            None
                                        } else {
                                            let parts: Vec<String> = inherited
                                                .iter()
                                                .filter_map(|(g, wf)| {
                                                    wf.process_name.as_deref().map(|p| {
                                                        format!(
                                                            "{g}: {}",
                                                            lang.fmt_context_badge(
                                                                matches!(
                                                                    wf.mode,
                                                                    WindowFilterMode::Exclude
                                                                ),
                                                                p.trim(),
                                                            )
                                                        )
                                                    })
                                                })
                                                .collect();
                                            (!parts.is_empty()).then(|| parts.join(" | "))
                                        }
                                    });
                                let conflict_badge =
                                    if let Some(reason) = daemon_skipped_by_id.get(&r.id) {
                                        Some((
                                            egui::Color32::from_rgb(220, 48, 48),
                                            format!(
                                                "{}: {}",
                                                s.badge_daemon_skipped,
                                                lang.tr_skipped_reason(*reason)
                                            ),
                                        ))
                                    } else {
                                        match live_conflicts.get(&idx) {
                                            Some(RuleTriggerConflict::KillSwitch) => Some((
                                                egui::Color32::from_rgb(220, 48, 48),
                                                s.badge_trigger_conflict.to_string(),
                                            )),
                                            Some(RuleTriggerConflict::ResumeHotkey) => Some((
                                                egui::Color32::from_rgb(220, 48, 48),
                                                s.badge_trigger_conflict.to_string(),
                                            )),
                                            Some(RuleTriggerConflict::DuplicateWithRule {
                                                shadowed: true,
                                                ..
                                            }) => Some((
                                                egui::Color32::from_rgb(220, 48, 48),
                                                s.badge_trigger_conflict.to_string(),
                                            )),
                                            Some(RuleTriggerConflict::DuplicateWithRule {
                                                shadowed: false,
                                                ..
                                            }) => Some((
                                                egui::Color32::from_rgb(210, 140, 20),
                                                s.badge_trigger_shared.to_string(),
                                            )),
                                            None => None,
                                        }
                                    };
                                (
                                    format!("[{}] {}", status_tag, r.name),
                                    preview,
                                    ctx_badge,
                                    conflict_badge,
                                )
                            };

                            let (card_clicked, card_rect) = Self::render_selectable_card_inner(
                                ui,
                                ("rule_card", idx),
                                is_selected,
                                true,
                                Some(idx),
                                |ui| {
                                    if Self::render_card_header_with_delete(
                                        ui,
                                        &title,
                                        true,
                                        s.btn_delete,
                                    ) {
                                        pending_request_delete = Some(PendingDeleteTarget::Rule {
                                            idx,
                                            name: app.editor.draft.rules[idx].name.clone(),
                                        });
                                    }
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&preview_text).small(),
                                        )
                                        .truncate()
                                        .selectable(false),
                                    );
                                    if let Some(badge) = &context_badge {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(badge).small().weak(),
                                            )
                                            .truncate()
                                            .selectable(false),
                                        );
                                    }
                                    if let Some((color, msg)) = &conflict_badge {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(msg).small().color(*color),
                                            )
                                            .truncate()
                                            .selectable(false),
                                        );
                                    }
                                    ui.horizontal_wrapped(|ui| {
                                        let has_group = !app.editor.draft.rules[idx]
                                            .effective_groups()
                                            .is_empty();
                                        if !has_group {
                                            app.editor.draft.rules[idx].enabled = false;
                                        }
                                        ui.add_enabled(
                                            has_group,
                                            egui::Checkbox::new(
                                                &mut app.editor.draft.rules[idx].enabled,
                                                s.checkbox_enabled,
                                            ),
                                        )
                                        .on_disabled_hover_text(s.disabled_no_group_tip);
                                        if ui
                                            .add_enabled(idx > 0, egui::Button::new(s.btn_up))
                                            .clicked()
                                        {
                                            pending_move_up = Some(idx);
                                        }
                                        if ui
                                            .add_enabled(
                                                idx + 1 < rule_count,
                                                egui::Button::new(s.btn_down),
                                            )
                                            .clicked()
                                        {
                                            pending_move_down = Some(idx);
                                        }
                                        if ui.button(s.btn_duplicate).clicked() {
                                            pending_duplicate_rule = Some(idx);
                                        }
                                    });
                                },
                            );
                            if card_clicked {
                                app.editor.selected_rule_idx = Some(idx);
                            }
                            if let Some(drag_payload) = &active_rule_drag
                                && drag_payload.source_rule_idx != idx
                                && pointer_latest.is_some_and(|pos| card_rect.contains(pos))
                            {
                                let line_y = if drag_payload.source_rule_idx > idx {
                                    card_rect.min.y
                                } else {
                                    card_rect.max.y
                                };
                                ui.painter().line_segment(
                                    [
                                        egui::pos2(card_rect.min.x + 6.0, line_y),
                                        egui::pos2(card_rect.max.x - 6.0, line_y),
                                    ],
                                    egui::Stroke::new(2.0, ui.visuals().selection.bg_fill),
                                );
                                if ui.ctx().input(|i| i.pointer.any_released())
                                    && let Some(released) =
                                        egui::DragAndDrop::take_payload::<RuleActionsDragPayload>(
                                            ui.ctx(),
                                        )
                                    && released.source_rule_idx != idx
                                {
                                    pending_move_rule_to = Some((released.source_rule_idx, idx));
                                }
                            }
                        }
                    });
            },
            |app, ui, fill_height| {
                let Some(rule_idx) = app.editor.selected_rule_idx else {
                    ui.label(s.select_or_create_rule);
                    return;
                };
                if rule_idx >= app.editor.draft.rules.len() {
                    ui.label(s.select_or_create_rule);
                    return;
                }

                if app.editor.draft.rules[rule_idx].window_filter.is_some()
                    && !app.installed_apps_loaded
                {
                    app.installed_apps_cache = list_installed_applications();
                    app.installed_apps_loaded = true;
                }

                let inherited_group_ctxs = app
                    .editor
                    .inherited_group_contexts(&app.editor.draft.rules[rule_idx]);
                let empty_ctx_intersection = app
                    .editor
                    .rule_has_empty_context_intersection(&app.editor.draft.rules[rule_idx]);
                let installed_apps = &app.installed_apps_cache;
                let app_picker_search = &mut app.app_picker_search;
                let mut pending_refresh_apps = false;
                let mut browse_exe_err: Option<String> = None;
                let rule = &mut app.editor.draft.rules[rule_idx];
                ui.strong(s.rule_detail_title);
                ui.add_space(4.0);

                egui::Grid::new(("rule_detail_aligned_grid", lang.code()))
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(s.label_name);
                        let input_w = (ui.available_width() - 8.0).max(120.0);
                        ui.add(egui::TextEdit::singleline(&mut rule.name).desired_width(input_w));
                        ui.end_row();

                        ui.label(s.label_groups);
                        let cell_w = (ui.available_width() - 8.0).max(140.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(cell_w, 22.0),
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui.button(s.btn_new_group).clicked() {
                                    pending_jump_to_new_group = true;
                                }
                                let effective = rule.effective_groups();
                                let combo_text = if effective.is_empty() {
                                    s.no_group_paren.to_string()
                                } else {
                                    effective.join(", ")
                                };
                                let mut next_groups = effective;
                                let mut groups_changed = false;
                                let combo_w = ui.available_width().max(90.0);
                                egui::ComboBox::from_id_salt("rule_group_selector")
                                    .width(combo_w)
                                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                                    .selected_text(combo_text)
                                    .show_ui(ui, |ui| {
                                        if ui
                                            .selectable_label(
                                                next_groups.is_empty(),
                                                s.no_group_paren,
                                            )
                                            .clicked()
                                        {
                                            next_groups.clear();
                                            groups_changed = true;
                                        }
                                        ui.separator();
                                        for group_name in &available_groups {
                                            let mut checked =
                                                next_groups.iter().any(|g| g == group_name);
                                            if ui.checkbox(&mut checked, group_name).clicked() {
                                                if checked {
                                                    if !next_groups.iter().any(|g| g == group_name)
                                                    {
                                                        next_groups.push(group_name.clone());
                                                    }
                                                } else {
                                                    next_groups.retain(|g| g != group_name);
                                                }
                                                groups_changed = true;
                                            }
                                        }
                                    });
                                if groups_changed {
                                    let is_empty = next_groups.is_empty();
                                    rule.set_groups(next_groups);
                                    if is_empty {
                                        rule.enabled = false;
                                    }
                                }
                            },
                        );
                        ui.end_row();

                        ui.label(s.label_trigger);
                        ui.horizontal_wrapped(|ui| {
                            match &mut rule.trigger {
                                Trigger::Hotkey { binding } => {
                                    Self::render_modifiers_and_key(
                                        ui,
                                        "rule_trigger_key",
                                        &mut binding.modifiers,
                                        &mut binding.key,
                                    );
                                }
                            }
                            if ui.button(s.btn_record_trigger).clicked() {
                                pending_start_trigger_capture = Some(rule_idx);
                            }
                        });
                        ui.end_row();

                        Self::render_window_filter_controls(
                            ui,
                            lang,
                            "rule_window_filter_mode",
                            "choose_app_scroll_rule",
                            s.rule_context_inherit,
                            &mut rule.window_filter,
                            installed_apps,
                            app_picker_search,
                            &mut pending_refresh_apps,
                            &mut browse_exe_err,
                        );
                    });

                if !inherited_group_ctxs.is_empty() {
                    let parts: Vec<String> = inherited_group_ctxs
                        .iter()
                        .filter_map(|(g, wf)| {
                            wf.process_name.as_deref().map(|p| {
                                format!(
                                    "{g} ({})",
                                    lang.fmt_context_badge(
                                        matches!(wf.mode, WindowFilterMode::Exclude),
                                        p.trim(),
                                    )
                                )
                            })
                        })
                        .collect();
                    if !parts.is_empty() {
                        ui.add_space(2.0);
                        ui.small(lang.fmt_group_inherited_context(&parts.join(", ")));
                    }
                }
                if empty_ctx_intersection {
                    ui.add_space(2.0);
                    ui.colored_label(
                        egui::Color32::from_rgb(210, 140, 20),
                        s.warning_empty_context_intersection,
                    );
                }

                ui.add_space(4.0);
                ui.checkbox(&mut rule.wait_for_modifier_release, s.checkbox_wait_release);

                if let Some(reason) = daemon_skipped_by_id.get(&rule.id) {
                    ui.add_space(4.0);
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 48, 48),
                        format!(
                            "{}: {}",
                            s.badge_daemon_skipped,
                            lang.tr_skipped_reason(*reason)
                        ),
                    );
                }
                if let Some(conflict) = live_conflicts.get(&rule_idx) {
                    ui.add_space(2.0);
                    let (color, msg) = match conflict {
                        RuleTriggerConflict::KillSwitch => (
                            egui::Color32::from_rgb(220, 48, 48),
                            s.conflict_kill_switch.to_string(),
                        ),
                        RuleTriggerConflict::ResumeHotkey => (
                            egui::Color32::from_rgb(220, 48, 48),
                            s.conflict_resume_hotkey.to_string(),
                        ),
                        RuleTriggerConflict::DuplicateWithRule {
                            other_rule_names,
                            shadowed: true,
                        } => (
                            egui::Color32::from_rgb(220, 48, 48),
                            lang.fmt_conflict_duplicate_shadowed(&other_rule_names.join(", ")),
                        ),
                        RuleTriggerConflict::DuplicateWithRule {
                            other_rule_names,
                            shadowed: false,
                        } => (
                            egui::Color32::from_rgb(210, 140, 20),
                            lang.fmt_conflict_duplicate_ahead(&other_rule_names.join(", ")),
                        ),
                    };
                    ui.colored_label(color, msg);
                }

                if pending_refresh_apps {
                    app.installed_apps_cache = list_installed_applications();
                    app.installed_apps_loaded = true;
                }
                if let Some(err) = browse_exe_err {
                    app.toast = Some(ToastNotice {
                        message: err,
                        is_error: true,
                        expires_at: Instant::now() + TOAST_DURATION,
                    });
                }

                ui.add_space(4.0);
                ui.separator();
                let actions_top = ui.cursor().min.y;
                let pane_left = ui.max_rect().min.x;
                let pane_right = ui.max_rect().max.x;
                let actions_drop_rect_id = ui.make_persistent_id(("actions_drop_rect", rule_idx));
                let prev_actions_rect =
                    ui.data(|d| d.get_temp::<egui::Rect>(actions_drop_rect_id));
                let pointer_in_actions = prev_actions_rect
                    .zip(pointer_latest)
                    .is_some_and(|(rect, pos)| rect.contains(pos));

                let action_count = rule.actions.len();
                let remaining_cap = MAX_ACTIONS_PER_SEQUENCE.saturating_sub(action_count);
                let preview_actions: &[InputAction] = if pointer_in_actions {
                    dragged_rule_actions
                        .as_ref()
                        .filter(|(src_idx, _)| *src_idx != rule_idx)
                        .map_or(&[][..], |(_, list)| list.as_slice())
                } else {
                    &[]
                };
                let preview_take = preview_actions.len().min(remaining_cap);

                ui.strong(lang.fmt_actions_header(action_count, MAX_ACTIONS_PER_SEQUENCE));

                let can_add = action_count < MAX_ACTIONS_PER_SEQUENCE;
                #[derive(Clone, Copy)]
                enum ActionToolbarItem {
                    Add(NewActionKind),
                    Record,
                }
                let top_row: [(&str, ActionToolbarItem); 6] = [
                    (s.btn_record_actions, ActionToolbarItem::Record),
                    ("KeyClick", ActionToolbarItem::Add(NewActionKind::KeyClick)),
                    ("KeyDown", ActionToolbarItem::Add(NewActionKind::KeyDown)),
                    ("KeyUp", ActionToolbarItem::Add(NewActionKind::KeyUp)),
                    ("SendText", ActionToolbarItem::Add(NewActionKind::SendText)),
                    ("Delay", ActionToolbarItem::Add(NewActionKind::Delay)),
                ];
                let bottom_row: [(&str, ActionToolbarItem); 5] = [
                    (
                        "MouseClick",
                        ActionToolbarItem::Add(NewActionKind::MouseClick),
                    ),
                    (
                        "MouseDown",
                        ActionToolbarItem::Add(NewActionKind::MouseDown),
                    ),
                    ("MouseUp", ActionToolbarItem::Add(NewActionKind::MouseUp)),
                    (
                        "MouseScroll",
                        ActionToolbarItem::Add(NewActionKind::MouseScroll),
                    ),
                    (
                        "MouseMove",
                        ActionToolbarItem::Add(NewActionKind::MouseMove),
                    ),
                ];

                let btn_font = egui::TextStyle::Button.resolve(ui.style());
                let body_font = egui::TextStyle::Body.resolve(ui.style());
                let btn_pad_x = ui.spacing().button_padding.x * 2.0 + 2.0;
                let measure_btn_w = |label: &str| -> f32 {
                    let text_w = ui
                        .painter()
                        .layout_no_wrap(label.to_string(), btn_font.clone(), egui::Color32::WHITE)
                        .size()
                        .x;
                    (text_w + btn_pad_x).ceil()
                };
                let mut col_widths = [0.0f32; 6];
                for col in 0..6 {
                    let top_w = measure_btn_w(top_row[col].0);
                    let bottom_w = bottom_row
                        .get(col)
                        .map_or(0.0, |(lbl, _)| measure_btn_w(lbl));
                    col_widths[col] = top_w.max(bottom_w);
                }

                for row in [&top_row[..], &bottom_row[..]] {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for (col, (label, item)) in row.iter().enumerate() {
                            let btn = egui::Button::new(*label)
                                .min_size(egui::vec2(col_widths[col], 0.0));
                            if ui.add_enabled(can_add, btn).clicked() {
                                match item {
                                    ActionToolbarItem::Add(kind) => {
                                        pending_add_action = Some(*kind)
                                    }
                                    ActionToolbarItem::Record => {
                                        pending_start_action_capture = Some(rule_idx);
                                    }
                                }
                            }
                        }
                    });
                }
                ui.add_space(4.0);

                let action_type_label = |act: &InputAction| -> &'static str {
                    match act {
                        InputAction::KeyClick { .. } => "KeyClick",
                        InputAction::KeyDown { .. } => "KeyDown",
                        InputAction::KeyUp { .. } => "KeyUp",
                        InputAction::SendText { .. } => "SendText",
                        InputAction::Delay { .. } => "Delay (ms)",
                        InputAction::MouseClick { .. } => "MouseClick",
                        InputAction::MouseDown { .. } => "MouseDown",
                        InputAction::MouseUp { .. } => "MouseUp",
                        InputAction::MouseScroll { .. } => "MouseScroll",
                        InputAction::MouseMove { .. } => "MouseMove",
                    }
                };
                let item_space_x = ui.spacing().item_spacing.x;
                let combo_chrome_w = ui.spacing().button_padding.x * 2.0
                    + ui.spacing().icon_width
                    + ui.spacing().item_spacing.x
                    + 8.0;
                let mut max_action_prefix_w = 0.0f32;
                let mut max_action_select_w = ui.spacing().combo_width;
                for (a_idx, act) in rule
                    .actions
                    .iter()
                    .chain(preview_actions.iter().take(preview_take))
                    .enumerate()
                {
                    let num_w = ui
                        .painter()
                        .layout_no_wrap(
                            format!("{}.", a_idx + 1),
                            body_font.clone(),
                            egui::Color32::WHITE,
                        )
                        .size()
                        .x;
                    let kind_w = ui
                        .painter()
                        .layout_no_wrap(
                            action_type_label(act).to_string(),
                            body_font.clone(),
                            egui::Color32::WHITE,
                        )
                        .size()
                        .x;
                    max_action_prefix_w =
                        max_action_prefix_w.max((num_w + item_space_x + kind_w + 2.0).ceil());
                    let choice_label = match act {
                        InputAction::KeyClick { key }
                        | InputAction::KeyDown { key }
                        | InputAction::KeyUp { key } => Some(key.as_str()),
                        InputAction::MouseClick { button }
                        | InputAction::MouseDown { button }
                        | InputAction::MouseUp { button } => Some(button.as_str()),
                        InputAction::MouseScroll { direction, .. } => Some(direction.as_str()),
                        InputAction::SendText { .. }
                        | InputAction::Delay { .. }
                        | InputAction::MouseMove { .. } => None,
                    };
                    if let Some(choice) = choice_label {
                        let choice_w = ui
                            .painter()
                            .layout_no_wrap(choice.to_string(), btn_font.clone(), egui::Color32::WHITE)
                            .size()
                            .x;
                        max_action_select_w =
                            max_action_select_w.max((choice_w + combo_chrome_w).ceil());
                    }
                }

                let actions_h = if fill_height {
                    ui.available_height().max(110.0)
                } else {
                    200.0
                };
                egui::ScrollArea::vertical()
                    .id_salt("actions_list_scroll")
                    .hscroll(false)
                    .max_height(actions_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let row_line_h = ui.spacing().interact_size.y.max(20.0);
                        for (a_idx, action) in rule.actions.iter_mut().enumerate() {
                            let row_w = ui.available_width().max(120.0);
                            let kind_label = action_type_label(action);
                            let (_, dropped_payload) =
                                ui.dnd_drop_zone::<ActionDragPayload, ()>(card_frame(ui), |ui| {
                                    ui.set_min_width((row_w - 14.0).max(80.0));
                                    ui.horizontal_wrapped(|ui| {
                                        let drag_resp = ui.dnd_drag_source(
                                            ui.make_persistent_id(("act_drag", rule_idx, a_idx)),
                                            ActionDragPayload {
                                                rule_idx,
                                                action_idx: a_idx,
                                            },
                                            |ui| {
                                                ui.label(
                                                    egui::RichText::new("::").weak().monospace(),
                                                );
                                            },
                                        );
                                        drag_resp.response.on_hover_text(s.drag_action_tooltip);

                                        ui.allocate_ui_with_layout(
                                            egui::vec2(max_action_prefix_w, row_line_h),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.set_min_width(max_action_prefix_w);
                                                ui.set_max_width(max_action_prefix_w);
                                                ui.label(format!("{}.", a_idx + 1));
                                                ui.label(kind_label);
                                            },
                                        );

                                        match action {
                                            InputAction::KeyClick { key }
                                            | InputAction::KeyDown { key }
                                            | InputAction::KeyUp { key } => {
                                                Self::render_virtual_key_combo_with_width(
                                                    ui,
                                                    &format!("act_key_{}_{}", rule_idx, a_idx),
                                                    key,
                                                    Some(max_action_select_w),
                                                );
                                            }
                                            InputAction::SendText { text } => {
                                                let text_w = (ui.available_width() - 160.0)
                                                    .clamp(110.0, 380.0);
                                                ui.add(
                                                    egui::TextEdit::singleline(text)
                                                        .desired_width(text_w),
                                                );
                                                let chars = text.chars().count();
                                                ui.small(format!(
                                                    "{} / {}",
                                                    chars, MAX_SEND_TEXT_LENGTH
                                                ));
                                            }
                                            InputAction::Delay { milliseconds } => {
                                                ui.add_sized(
                                                    [max_action_select_w, row_line_h],
                                                    egui::DragValue::new(milliseconds)
                                                        .range(MIN_DELAY_MS..=MAX_DELAY_MS),
                                                );
                                            }
                                            InputAction::MouseClick { button }
                                            | InputAction::MouseDown { button }
                                            | InputAction::MouseUp { button } => {
                                                Self::render_mouse_button_combo_with_width(
                                                    ui,
                                                    &format!("act_mbtn_{}_{}", rule_idx, a_idx),
                                                    button,
                                                    Some(max_action_select_w),
                                                );
                                            }
                                            InputAction::MouseScroll { direction, steps } => {
                                                Self::render_mouse_wheel_dir_combo_with_width(
                                                    ui,
                                                    &format!("act_wdir_{}_{}", rule_idx, a_idx),
                                                    direction,
                                                    Some(max_action_select_w),
                                                );
                                                ui.label("steps:");
                                                ui.add(egui::DragValue::new(steps).range(
                                                    -MAX_MOUSE_SCROLL_STEPS
                                                        ..=MAX_MOUSE_SCROLL_STEPS,
                                                ));
                                            }
                                            InputAction::MouseMove { dx, dy } => {
                                                ui.label("dx:");
                                                ui.add(egui::DragValue::new(dx).range(
                                                    -MAX_MOUSE_MOVE_DELTA..=MAX_MOUSE_MOVE_DELTA,
                                                ));
                                                ui.label("dy:");
                                                ui.add(egui::DragValue::new(dy).range(
                                                    -MAX_MOUSE_MOVE_DELTA..=MAX_MOUSE_MOVE_DELTA,
                                                ));
                                            }
                                        }

                                        if ui
                                            .add_enabled(a_idx > 0, egui::Button::new(s.btn_up))
                                            .clicked()
                                        {
                                            pending_action_up = Some(a_idx);
                                        }
                                        if ui
                                            .add_enabled(
                                                a_idx + 1 < action_count,
                                                egui::Button::new(s.btn_down),
                                            )
                                            .clicked()
                                        {
                                            pending_action_down = Some(a_idx);
                                        }
                                        if ui
                                            .add_enabled(can_add, egui::Button::new(s.btn_duplicate))
                                            .clicked()
                                        {
                                            pending_duplicate_action = Some(a_idx);
                                        }
                                        if ui.button("X").clicked() {
                                            pending_remove_action = Some(a_idx);
                                        }
                                    });
                                });
                            if let Some(payload) = dropped_payload
                                && payload.rule_idx == rule_idx
                                && payload.action_idx != a_idx
                            {
                                pending_move_action_to = Some((payload.action_idx, a_idx));
                            }
                        }

                        if preview_take > 0 {
                            ui.scope(|ui| {
                                ui.multiply_opacity(0.45);
                                ui.add_enabled_ui(false, |ui| {
                                    for (p_offset, preview_action) in
                                        preview_actions.iter().take(preview_take).enumerate()
                                    {
                                        let a_idx = action_count + p_offset;
                                        let row_w = ui.available_width().max(120.0);
                                        let kind_label = action_type_label(preview_action);
                                        let mut preview_copy = preview_action.clone();
                                        card_frame(ui).show(ui, |ui| {
                                            ui.set_min_width((row_w - 14.0).max(80.0));
                                            ui.horizontal_wrapped(|ui| {
                                                ui.label(
                                                    egui::RichText::new("::").weak().monospace(),
                                                );
                                                ui.allocate_ui_with_layout(
                                                    egui::vec2(max_action_prefix_w, row_line_h),
                                                    egui::Layout::left_to_right(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        ui.set_min_width(max_action_prefix_w);
                                                        ui.set_max_width(max_action_prefix_w);
                                                        ui.label(format!("{}.", a_idx + 1));
                                                        ui.label(kind_label);
                                                    },
                                                );
                                                match &mut preview_copy {
                                                    InputAction::KeyClick { key }
                                                    | InputAction::KeyDown { key }
                                                    | InputAction::KeyUp { key } => {
                                                        Self::render_virtual_key_combo_with_width(
                                                            ui,
                                                            &format!(
                                                                "act_prev_key_{}_{}",
                                                                rule_idx, a_idx
                                                            ),
                                                            key,
                                                            Some(max_action_select_w),
                                                        );
                                                    }
                                                    InputAction::SendText { text } => {
                                                        let text_w = (ui.available_width() - 160.0)
                                                            .clamp(110.0, 380.0);
                                                        ui.add(
                                                            egui::TextEdit::singleline(text)
                                                                .desired_width(text_w),
                                                        );
                                                        let chars = text.chars().count();
                                                        ui.small(format!(
                                                            "{} / {}",
                                                            chars, MAX_SEND_TEXT_LENGTH
                                                        ));
                                                    }
                                                    InputAction::Delay { milliseconds } => {
                                                        ui.add_sized(
                                                            [max_action_select_w, row_line_h],
                                                            egui::DragValue::new(milliseconds)
                                                                .range(MIN_DELAY_MS..=MAX_DELAY_MS),
                                                        );
                                                    }
                                                    InputAction::MouseClick { button }
                                                    | InputAction::MouseDown { button }
                                                    | InputAction::MouseUp { button } => {
                                                        Self::render_mouse_button_combo_with_width(
                                                            ui,
                                                            &format!(
                                                                "act_prev_mbtn_{}_{}",
                                                                rule_idx, a_idx
                                                            ),
                                                            button,
                                                            Some(max_action_select_w),
                                                        );
                                                    }
                                                    InputAction::MouseScroll {
                                                        direction,
                                                        steps,
                                                    } => {
                                                        Self::render_mouse_wheel_dir_combo_with_width(
                                                            ui,
                                                            &format!(
                                                                "act_prev_wdir_{}_{}",
                                                                rule_idx, a_idx
                                                            ),
                                                            direction,
                                                            Some(max_action_select_w),
                                                        );
                                                        ui.label("steps:");
                                                        ui.add(egui::DragValue::new(steps).range(
                                                            -MAX_MOUSE_SCROLL_STEPS
                                                                ..=MAX_MOUSE_SCROLL_STEPS,
                                                        ));
                                                    }
                                                    InputAction::MouseMove { dx, dy } => {
                                                        ui.label("dx:");
                                                        ui.add(egui::DragValue::new(dx).range(
                                                            -MAX_MOUSE_MOVE_DELTA
                                                                ..=MAX_MOUSE_MOVE_DELTA,
                                                        ));
                                                        ui.label("dy:");
                                                        ui.add(egui::DragValue::new(dy).range(
                                                            -MAX_MOUSE_MOVE_DELTA
                                                                ..=MAX_MOUSE_MOVE_DELTA,
                                                        ));
                                                    }
                                                }
                                                let _ = ui.button(s.btn_up);
                                                let _ = ui.button(s.btn_down);
                                                let _ = ui.button(s.btn_duplicate);
                                                let _ = ui.button("X");
                                            });
                                        });
                                    }
                                });
                            });
                        }
                    });
                let actions_bottom = ui.min_rect().max.y.max(actions_top);
                let actions_rect = egui::Rect::from_min_max(
                    egui::pos2(pane_left, actions_top),
                    egui::pos2(pane_right, actions_bottom),
                );
                ui.data_mut(|d| d.insert_temp(actions_drop_rect_id, actions_rect));
                if pointer_in_actions
                    && ui.ctx().input(|i| i.pointer.any_released())
                    && let Some(released) =
                        egui::DragAndDrop::take_payload::<RuleActionsDragPayload>(ui.ctx())
                    && released.source_rule_idx != rule_idx
                {
                    pending_import_rule_actions = Some(released.source_rule_idx);
                }
            },
        );

        let _ = ratio;

        if let Some(payload) = egui::DragAndDrop::payload::<RuleActionsDragPayload>(ui.ctx())
            && let Some(pointer_pos) = ui.ctx().pointer_latest_pos()
            && let Some(src_rule) = self.editor.draft.rules.get(payload.source_rule_idx)
        {
            let ghost_text = format!("{} ({})", src_rule.name, src_rule.actions.len());
            let ghost_id = ui.make_persistent_id("rule_actions_drag_ghost");
            egui::Area::new(ghost_id)
                .order(egui::Order::Tooltip)
                .interactable(false)
                .pivot(egui::Align2::CENTER_CENTER)
                .fixed_pos(pointer_pos)
                .show(ui.ctx(), |ui| {
                    card_frame(ui)
                        .stroke(egui::Stroke::new(1.5, ui.visuals().selection.bg_fill))
                        .show(ui, |ui| {
                            ui.strong(ghost_text);
                        });
                });
        }

        if pending_add_rule {
            self.editor.add_rule();
        }
        if let Some(idx) = pending_duplicate_rule {
            self.editor.duplicate_rule(idx);
        }
        if let Some(idx) = pending_move_up {
            self.editor.move_rule_up(idx);
        }
        if let Some(idx) = pending_move_down {
            self.editor.move_rule_down(idx);
        }
        if let Some((from_idx, to_idx)) = pending_move_rule_to {
            self.editor.move_rule_to(from_idx, to_idx);
        }
        if let Some(target) = pending_request_delete {
            self.pending_delete = Some(target);
        }
        if pending_jump_to_new_group {
            self.rules_subtab = RulesSubTab::Groups;
            self.focus_new_group_input = true;
        }
        if let Some(rule_idx) = self.editor.selected_rule_idx {
            if let Some(kind) = pending_add_action {
                self.editor.add_action(rule_idx, kind);
            }
            if let Some(a_idx) = pending_duplicate_action {
                self.editor.duplicate_action(rule_idx, a_idx);
            }
            if let Some((from_idx, to_idx)) = pending_move_action_to {
                self.editor.move_action_to(rule_idx, from_idx, to_idx);
            }
            if let Some(a_idx) = pending_action_up {
                self.editor.move_action_up(rule_idx, a_idx);
            }
            if let Some(a_idx) = pending_action_down {
                self.editor.move_action_down(rule_idx, a_idx);
            }
            if let Some(a_idx) = pending_remove_action {
                self.editor.remove_action(rule_idx, a_idx);
            }
            if let Some(source_idx) = pending_import_rule_actions {
                let source_name = self
                    .editor
                    .draft
                    .rules
                    .get(source_idx)
                    .map(|r| r.name.clone())
                    .unwrap_or_default();
                let added = self.editor.import_rule_actions(rule_idx, source_idx);
                if added > 0 {
                    self.toast = Some(ToastNotice {
                        message: lang.fmt_imported_rule_actions(added, &source_name),
                        is_error: false,
                        expires_at: Instant::now() + TOAST_DURATION,
                    });
                }
            }
        }
        if let Some(rule_idx) = pending_start_trigger_capture {
            if let Some(focused_id) = ui.ctx().memory(|m| m.focused()) {
                ui.ctx().memory_mut(|m| m.surrender_focus(focused_id));
            }
            self.capture_modal = Some(CaptureModalState::Trigger(TriggerCaptureState::new(
                rule_idx,
            )));
        }
        if let Some(rule_idx) = pending_start_action_capture {
            if let Some(focused_id) = ui.ctx().memory(|m| m.focused()) {
                ui.ctx().memory_mut(|m| m.surrender_focus(focused_id));
            }
            self.capture_modal = Some(CaptureModalState::Actions(Box::new(
                ActionCaptureState::new(rule_idx),
            )));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_window_filter_controls(
        ui: &mut egui::Ui,
        lang: Language,
        combo_id_salt: impl egui::AsIdSalt,
        scroll_id_salt: &str,
        global_label: &str,
        window_filter: &mut Option<WindowFilter>,
        installed_apps: &[InstalledAppEntry],
        app_picker_search: &mut String,
        pending_refresh_apps: &mut bool,
        browse_exe_err: &mut Option<String>,
    ) {
        let s = lang.s();
        ui.label(s.label_context);
        let selected_ctx_label = match window_filter.as_ref().map(|wf| wf.mode) {
            None => global_label,
            Some(WindowFilterMode::Include) => s.context_mode_include,
            Some(WindowFilterMode::Exclude) => s.context_mode_exclude,
        };
        let ctx_combo_w = (ui.available_width() - 8.0).clamp(140.0, 260.0);
        egui::ComboBox::from_id_salt(combo_id_salt)
            .width(ctx_combo_w)
            .selected_text(selected_ctx_label)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(window_filter.is_none(), global_label)
                    .clicked()
                {
                    *window_filter = None;
                }
                if ui
                    .selectable_label(
                        window_filter
                            .as_ref()
                            .is_some_and(|wf| wf.mode == WindowFilterMode::Include),
                        s.context_mode_include,
                    )
                    .clicked()
                {
                    match window_filter {
                        Some(wf) => wf.mode = WindowFilterMode::Include,
                        None => {
                            *window_filter = Some(WindowFilter {
                                mode: WindowFilterMode::Include,
                                process_name: None,
                            });
                        }
                    }
                }
                if ui
                    .selectable_label(
                        window_filter
                            .as_ref()
                            .is_some_and(|wf| wf.mode == WindowFilterMode::Exclude),
                        s.context_mode_exclude,
                    )
                    .clicked()
                {
                    match window_filter {
                        Some(wf) => wf.mode = WindowFilterMode::Exclude,
                        None => {
                            *window_filter = Some(WindowFilter {
                                mode: WindowFilterMode::Exclude,
                                process_name: None,
                            });
                        }
                    }
                }
            });
        ui.end_row();

        if let Some(wf) = window_filter {
            ui.label(s.label_process_exe);
            let proc_cell_w = (ui.available_width() - 8.0).max(160.0);
            ui.allocate_ui_with_layout(
                egui::vec2(proc_cell_w, 22.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    if ui.button(s.btn_browse_exe).clicked() {
                        match pick_exe_file() {
                            Ok(Some(exe_name)) => {
                                append_filter_token(&mut wf.process_name, &exe_name);
                            }
                            Ok(None) => {}
                            Err(err) => *browse_exe_err = Some(err),
                        }
                    }
                    egui::containers::menu::MenuButton::new(s.btn_choose_app)
                        .config(
                            egui::containers::menu::MenuConfig::new()
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                        )
                        .ui(ui, |ui| {
                            ui.set_min_width(300.0);
                            ui.set_max_width(360.0);
                            ui.horizontal(|ui| {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button(s.btn_refresh_apps).clicked() {
                                            *pending_refresh_apps = true;
                                        }
                                        let search_w = ui.available_width().max(100.0);
                                        ui.add(
                                            egui::TextEdit::singleline(app_picker_search)
                                                .desired_width(search_w)
                                                .hint_text(s.search_apps_hint),
                                        );
                                    },
                                );
                            });
                            ui.separator();
                            let query = app_picker_search.trim().to_lowercase();
                            let filtered: Vec<&InstalledAppEntry> = installed_apps
                                .iter()
                                .filter(|entry| {
                                    query.is_empty()
                                        || entry.display_label().to_lowercase().contains(&query)
                                })
                                .collect();
                            let mut close_menu = false;
                            if filtered.is_empty() {
                                ui.small(s.no_apps_found);
                            } else {
                                egui::ScrollArea::vertical()
                                    .id_salt(scroll_id_salt)
                                    .min_scrolled_height(260.0)
                                    .max_height(280.0)
                                    .auto_shrink([false, true])
                                    .show(ui, |ui| {
                                        for entry in filtered {
                                            if ui.button(entry.display_label()).clicked() {
                                                append_filter_token(
                                                    &mut wf.process_name,
                                                    &entry.process_name,
                                                );
                                                close_menu = true;
                                            }
                                        }
                                    });
                            }
                            if close_menu {
                                ui.close();
                            }
                        });
                    let mut proc_buf = wf.process_name.clone().unwrap_or_default();
                    let edit_w = ui.available_width().max(70.0);
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut proc_buf)
                                .desired_width(edit_w)
                                .hint_text(s.process_exe_hint),
                        )
                        .changed()
                    {
                        let trimmed = proc_buf.trim();
                        wf.process_name = if trimmed.is_empty() {
                            None
                        } else {
                            Some(proc_buf)
                        };
                    }
                },
            );
            ui.end_row();
        }
    }

    fn render_rules_groups_subtab(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let s = lang.s();
        let available_groups = self.editor.available_groups();
        if !available_groups.iter().any(|g| g == &self.selected_group) {
            self.selected_group = default_rule_group();
            self.selected_available_rule_ids.clear();
            self.selected_member_rule_ids.clear();
        }

        let ratio = self.groups_split_ratio;
        let filtered_groups = self
            .editor
            .filtered_group_summaries(&self.group_search_query);
        let mut pending_create_group = false;
        let mut pending_select_group: Option<String> = None;
        let mut pending_active_toggle: Option<(String, bool)> = None;
        let mut pending_request_delete: Option<PendingDeleteTarget> = None;
        let mut pending_add_to_group = false;
        let mut pending_remove_from_group = false;
        let mut pending_open_rule: Option<usize> = None;

        self.groups_split_ratio = self.render_resizable_split(
            ui,
            "groups_split",
            ratio,
            230.0,
            340.0,
            |app, ui, fill_height| {
                ui.strong(s.groups_title);
                egui::Grid::new(("groups_left_inputs_grid", lang.code()))
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(s.label_new);
                        let cell_w = (ui.available_width() - 4.0).max(80.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(cell_w, 22.0),
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                let create_clicked = ui.button(s.btn_create).clicked();
                                let input_w = ui.available_width().max(60.0);
                                let resp = ui.add(
                                    egui::TextEdit::singleline(&mut app.new_group_name)
                                        .desired_width(input_w)
                                        .hint_text(s.group_hint),
                                );
                                if app.focus_new_group_input {
                                    resp.request_focus();
                                    app.focus_new_group_input = false;
                                }
                                if create_clicked
                                    || (resp.lost_focus()
                                        && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                                {
                                    pending_create_group = true;
                                }
                            },
                        );
                        ui.end_row();

                        ui.label(s.label_search);
                        let search_cell_w = (ui.available_width() - 4.0).max(80.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(search_cell_w, 22.0),
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if !app.group_search_query.is_empty()
                                    && ui.button(s.btn_clear).clicked()
                                {
                                    app.group_search_query.clear();
                                }
                                let search_w = ui.available_width().max(60.0);
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.group_search_query)
                                        .desired_width(search_w)
                                        .hint_text(s.filter_groups_hint),
                                );
                            },
                        );
                        ui.end_row();
                    });
                ui.add_space(4.0);

                let list_h = if fill_height {
                    ui.available_height().max(120.0)
                } else {
                    200.0
                };
                egui::ScrollArea::vertical()
                    .id_salt("groups_summary_scroll")
                    .hscroll(false)
                    .max_height(list_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if filtered_groups.is_empty() {
                            ui.small(s.empty_groups);
                        }
                        for summary in &filtered_groups {
                            let is_selected = app.selected_group == summary.name;
                            let is_default = summary.name == default_rule_group();
                            if Self::render_selectable_card(
                                ui,
                                ("group_card", &summary.name),
                                is_selected,
                                true,
                                |ui| {
                                    let title = if is_default {
                                        format!(
                                            "{} ({} — {}/{})",
                                            summary.name,
                                            s.default_suffix,
                                            summary.enabled_rules,
                                            summary.total_rules
                                        )
                                    } else {
                                        format!(
                                            "{} ({}/{})",
                                            summary.name,
                                            summary.enabled_rules,
                                            summary.total_rules
                                        )
                                    };
                                    if Self::render_card_header_with_delete(
                                        ui,
                                        &title,
                                        !is_default,
                                        s.btn_delete,
                                    ) {
                                        pending_request_delete = Some(PendingDeleteTarget::Group {
                                            name: summary.name.clone(),
                                        });
                                    }

                                    if let Some(wf) = app.editor.draft.group_filter(&summary.name)
                                        && let Some(proc_name) = wf.process_name.as_deref()
                                    {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(lang.fmt_context_badge(
                                                    matches!(wf.mode, WindowFilterMode::Exclude),
                                                    proc_name.trim(),
                                                ))
                                                .small()
                                                .weak(),
                                            )
                                            .truncate()
                                            .selectable(false),
                                        );
                                    }

                                    ui.horizontal_wrapped(|ui| {
                                        let mut group_active =
                                            app.editor.is_group_active_effective(&summary.name);
                                        if ui
                                            .checkbox(&mut group_active, s.checkbox_group_active)
                                            .changed()
                                        {
                                            pending_active_toggle =
                                                Some((summary.name.clone(), group_active));
                                        }
                                    });
                                },
                            ) {
                                pending_select_group = Some(summary.name.clone());
                            }
                        }
                    });
            },
            |app, ui, fill_height| {
                let target_group = app.selected_group.clone();
                ui.horizontal_wrapped(|ui| {
                    ui.strong(lang.fmt_rules_in_group_title(&target_group));
                    if app.editor.is_all_groups_active() {
                        ui.small(s.all_groups_active_note);
                    }
                });
                ui.add_space(4.0);

                let is_default_group = target_group == default_rule_group();
                let mut group_filter = if is_default_group {
                    None
                } else {
                    app.editor.draft.group_filters.get(&target_group).cloned()
                };
                if group_filter.is_some() && !app.installed_apps_loaded {
                    app.installed_apps_cache = list_installed_applications();
                    app.installed_apps_loaded = true;
                }
                let installed_apps = &app.installed_apps_cache;
                let app_picker_search = &mut app.app_picker_search;
                let mut pending_refresh_apps = false;
                let mut browse_exe_err: Option<String> = None;

                ui.add_enabled_ui(!is_default_group, |ui| {
                    egui::Grid::new(("group_context_grid", lang.code()))
                        .num_columns(2)
                        .spacing([10.0, 6.0])
                        .show(ui, |ui| {
                            Self::render_window_filter_controls(
                                ui,
                                lang,
                                ("group_window_filter_mode", &target_group),
                                "choose_app_scroll_group",
                                s.context_mode_global,
                                &mut group_filter,
                                installed_apps,
                                app_picker_search,
                                &mut pending_refresh_apps,
                                &mut browse_exe_err,
                            );
                        });
                });

                if !is_default_group {
                    if let Some(wf) = group_filter {
                        app.editor
                            .draft
                            .group_filters
                            .insert(target_group.clone(), wf);
                    } else {
                        app.editor.draft.group_filters.remove(&target_group);
                    }
                    if pending_refresh_apps {
                        app.installed_apps_cache = list_installed_applications();
                        app.installed_apps_loaded = true;
                    }
                    if let Some(err) = browse_exe_err {
                        app.toast = Some(ToastNotice {
                            message: err,
                            is_error: true,
                            expires_at: Instant::now() + TOAST_DURATION,
                        });
                    }
                }
                ui.add_space(6.0);

                let dual_avail_w = ui.available_width();
                let dual_h = if fill_height {
                    ui.available_height().max(150.0)
                } else {
                    240.0
                };
                let spacing_x = ui.spacing().item_spacing.x;
                let sep_w = 6.0;
                let min_col_w = 175.0;
                let side_by_side = dual_avail_w >= min_col_w * 2.0 + sep_w + spacing_x * 2.0;

                if side_by_side {
                    let half_w = ((dual_avail_w - sep_w - spacing_x * 2.0) * 0.5).max(min_col_w);
                    ui.horizontal_top(|ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(half_w, dual_h),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_min_size(egui::vec2(half_w, dual_h));
                                ui.set_max_size(egui::vec2(half_w, dual_h));
                                let clip =
                                    ui.available_rect_before_wrap().intersect(ui.clip_rect());
                                ui.set_clip_rect(clip);
                                app.render_available_rules_column(
                                    ui,
                                    &target_group,
                                    true,
                                    &mut pending_add_to_group,
                                );
                            },
                        );
                        ui.separator();
                        ui.allocate_ui_with_layout(
                            egui::vec2(half_w, dual_h),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_min_size(egui::vec2(half_w, dual_h));
                                ui.set_max_size(egui::vec2(half_w, dual_h));
                                let clip =
                                    ui.available_rect_before_wrap().intersect(ui.clip_rect());
                                ui.set_clip_rect(clip);
                                app.render_group_members_column(
                                    ui,
                                    &target_group,
                                    true,
                                    &mut pending_remove_from_group,
                                    &mut pending_open_rule,
                                );
                            },
                        );
                    });
                } else {
                    app.render_available_rules_column(
                        ui,
                        &target_group,
                        false,
                        &mut pending_add_to_group,
                    );
                    ui.add_space(8.0);
                    app.render_group_members_column(
                        ui,
                        &target_group,
                        false,
                        &mut pending_remove_from_group,
                        &mut pending_open_rule,
                    );
                }
            },
        );

        let _ = ratio;

        if pending_create_group {
            match self.editor.add_custom_group(&self.new_group_name) {
                Ok(created) => {
                    self.selected_group = created.clone();
                    self.selected_available_rule_ids.clear();
                    self.selected_member_rule_ids.clear();
                    self.new_group_name.clear();
                    self.show_toast(false, format!("+ '{}'", created));
                }
                Err(msg) => self.show_toast(true, msg),
            }
        }
        if let Some(next_group) = pending_select_group
            && next_group != self.selected_group
        {
            self.selected_group = next_group;
            self.selected_available_rule_ids.clear();
            self.selected_member_rule_ids.clear();
        }
        if let Some((group_name, active)) = pending_active_toggle {
            let fallback_triggered = self.editor.toggle_group_active_now(&group_name, active);
            if fallback_triggered {
                self.show_toast(false, s.toast_fallback_default);
            }
        }
        if let Some(target) = pending_request_delete {
            self.pending_delete = Some(target);
        }
        if pending_add_to_group {
            let group = self.selected_group.clone();
            match self
                .editor
                .add_rules_to_group(&self.selected_available_rule_ids, &group)
            {
                Ok(count) => {
                    self.selected_available_rule_ids.clear();
                    self.show_toast(false, lang.fmt_group_rules_added(count, &group));
                }
                Err(err) => self.show_toast(true, err),
            }
        }
        if pending_remove_from_group {
            let group = self.selected_group.clone();
            let count = self
                .editor
                .remove_rules_from_group(&self.selected_member_rule_ids, &group);
            self.selected_member_rule_ids.clear();
            self.show_toast(false, lang.fmt_group_rules_removed(count, &group));
        }
        if let Some(idx) = pending_open_rule {
            self.editor.selected_rule_idx = Some(idx);
            self.rules_subtab = RulesSubTab::RuleList;
        }
    }

    fn render_available_rules_column(
        &mut self,
        ui: &mut egui::Ui,
        target_group: &str,
        fill_height: bool,
        pending_add: &mut bool,
    ) {
        let s = self.lang().s();
        let query = self.available_rules_search.trim().to_ascii_lowercase();
        let selectable_visible_ids: Vec<String> = self
            .editor
            .draft
            .rules
            .iter()
            .filter(|rule| {
                if rule.belongs_to_group(target_group) {
                    return false;
                }
                if query.is_empty() {
                    return true;
                }
                let trigger_str = match &rule.trigger {
                    Trigger::Hotkey { binding } => binding.to_string(),
                };
                rule.name.to_ascii_lowercase().contains(&query)
                    || trigger_str.to_ascii_lowercase().contains(&query)
            })
            .map(|r| r.id.clone())
            .collect();

        let all_visible_selected = !selectable_visible_ids.is_empty()
            && selectable_visible_ids
                .iter()
                .all(|id| self.selected_available_rule_ids.contains(id));

        ui.horizontal_wrapped(|ui| {
            ui.strong(s.available_rules_title);
            if ui
                .add_enabled(
                    !self.selected_available_rule_ids.is_empty(),
                    egui::Button::new(s.btn_add_to_group),
                )
                .clicked()
            {
                *pending_add = true;
            }
            let toggle_label = if all_visible_selected {
                s.btn_deselect_all
            } else {
                s.btn_select_all
            };
            if ui
                .add_enabled(
                    !selectable_visible_ids.is_empty(),
                    egui::Button::new(toggle_label),
                )
                .clicked()
            {
                if all_visible_selected {
                    for id in &selectable_visible_ids {
                        self.selected_available_rule_ids.remove(id);
                    }
                } else {
                    for id in &selectable_visible_ids {
                        self.selected_available_rule_ids.insert(id.clone());
                    }
                }
            }
        });
        egui::Grid::new(("available_rules_search_grid", self.lang().code()))
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label(s.label_search);
                let cell_w = (ui.available_width() - 4.0).max(70.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(cell_w, 22.0),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        if !self.available_rules_search.is_empty()
                            && ui.button(s.btn_clear).clicked()
                        {
                            self.available_rules_search.clear();
                        }
                        let search_w = ui.available_width().max(50.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.available_rules_search)
                                .desired_width(search_w)
                                .hint_text(s.search_hint_name_hotkey),
                        );
                    },
                );
                ui.end_row();
            });
        ui.add_space(4.0);

        let list_h = if fill_height {
            ui.available_height().max(110.0)
        } else {
            190.0
        };
        egui::ScrollArea::vertical()
            .id_salt("available_rules_scroll")
            .hscroll(false)
            .max_height(list_h)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.editor.draft.rules.is_empty() {
                    ui.small(s.empty_rules_configured);
                    return;
                }
                let mut shown = 0usize;
                for rule in &self.editor.draft.rules {
                    let trigger_str = match &rule.trigger {
                        Trigger::Hotkey { binding } => binding.to_string(),
                    };
                    if !query.is_empty()
                        && !rule.name.to_ascii_lowercase().contains(&query)
                        && !trigger_str.to_ascii_lowercase().contains(&query)
                    {
                        continue;
                    }
                    shown += 1;
                    let already_in_group = rule.belongs_to_group(target_group);
                    let mut checked =
                        !already_in_group && self.selected_available_rule_ids.contains(&rule.id);
                    let label = format!("{} ({})", rule.name, trigger_str);
                    if already_in_group {
                        let mut dummy_checked = false;
                        ui.add_enabled(false, egui::Checkbox::new(&mut dummy_checked, &label))
                            .on_disabled_hover_text(s.already_in_group_tip);
                    } else if ui.checkbox(&mut checked, &label).changed() {
                        if checked {
                            self.selected_available_rule_ids.insert(rule.id.clone());
                        } else {
                            self.selected_available_rule_ids.remove(&rule.id);
                        }
                    }
                }
                if shown == 0 {
                    ui.small(s.empty_rules_filtered);
                }
            });
    }

    fn render_group_members_column(
        &mut self,
        ui: &mut egui::Ui,
        target_group: &str,
        fill_height: bool,
        pending_remove: &mut bool,
        pending_open_rule: &mut Option<usize>,
    ) {
        let s = self.lang().s();
        let member_indices = self.editor.rule_indices_in_group(target_group);
        let query = self.group_members_search.trim().to_ascii_lowercase();
        let visible_member_ids: Vec<String> = member_indices
            .iter()
            .filter_map(|&idx| {
                let r = &self.editor.draft.rules[idx];
                if query.is_empty() {
                    return Some(r.id.clone());
                }
                let trigger_str = match &r.trigger {
                    Trigger::Hotkey { binding } => binding.to_string(),
                };
                if r.name.to_ascii_lowercase().contains(&query)
                    || trigger_str.to_ascii_lowercase().contains(&query)
                {
                    Some(r.id.clone())
                } else {
                    None
                }
            })
            .collect();

        let all_visible_selected = !visible_member_ids.is_empty()
            && visible_member_ids
                .iter()
                .all(|id| self.selected_member_rule_ids.contains(id));

        ui.horizontal_wrapped(|ui| {
            ui.strong(s.group_members_title);
            if ui
                .add_enabled(
                    !self.selected_member_rule_ids.is_empty(),
                    egui::Button::new(s.btn_remove_from_group),
                )
                .clicked()
            {
                *pending_remove = true;
            }
            let toggle_label = if all_visible_selected {
                s.btn_deselect_all
            } else {
                s.btn_select_all
            };
            if ui
                .add_enabled(
                    !visible_member_ids.is_empty(),
                    egui::Button::new(toggle_label),
                )
                .clicked()
            {
                if all_visible_selected {
                    for id in &visible_member_ids {
                        self.selected_member_rule_ids.remove(id);
                    }
                } else {
                    for id in &visible_member_ids {
                        self.selected_member_rule_ids.insert(id.clone());
                    }
                }
            }
        });
        egui::Grid::new(("group_members_search_grid", self.lang().code()))
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label(s.label_search);
                let cell_w = (ui.available_width() - 4.0).max(70.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(cell_w, 22.0),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        if !self.group_members_search.is_empty() && ui.button(s.btn_clear).clicked()
                        {
                            self.group_members_search.clear();
                        }
                        let search_w = ui.available_width().max(50.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.group_members_search)
                                .desired_width(search_w)
                                .hint_text(s.search_hint_name_hotkey),
                        );
                    },
                );
                ui.end_row();
            });
        ui.add_space(4.0);

        let list_h = if fill_height {
            ui.available_height().max(110.0)
        } else {
            190.0
        };
        egui::ScrollArea::vertical()
            .id_salt("group_members_scroll")
            .hscroll(false)
            .max_height(list_h)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if member_indices.is_empty() {
                    ui.small(s.empty_group_members);
                    return;
                }
                let mut rows: Vec<(usize, String, bool, String)> = Vec::new();
                for idx in member_indices {
                    let (id, name, enabled, trigger_str) = {
                        let r = &self.editor.draft.rules[idx];
                        let t = match &r.trigger {
                            Trigger::Hotkey { binding } => binding.to_string(),
                        };
                        (r.id.clone(), r.name.clone(), r.enabled, t)
                    };
                    if !query.is_empty()
                        && !name.to_ascii_lowercase().contains(&query)
                        && !trigger_str.to_ascii_lowercase().contains(&query)
                    {
                        continue;
                    }
                    let checked = self.selected_member_rule_ids.contains(&id);
                    let status_tag = if enabled { "ON" } else { "OFF" };
                    let label = format!("[{}] {} ({})", status_tag, name, trigger_str);
                    rows.push((idx, id, checked, label));
                }
                if rows.is_empty() {
                    ui.small(s.empty_rules_filtered);
                } else {
                    let font_id = egui::TextStyle::Button.resolve(ui.style());
                    let text_color = ui.visuals().text_color();
                    let chrome_w = ui.spacing().icon_width + ui.spacing().icon_spacing + 6.0;
                    let open_btn_w = ui
                        .painter()
                        .layout_no_wrap(s.btn_open.to_string(), font_id.clone(), text_color)
                        .size()
                        .x
                        + ui.spacing().button_padding.x * 2.0
                        + 6.0;
                    let max_avail_for_check =
                        (ui.available_width() - open_btn_w - ui.spacing().item_spacing.x - 4.0)
                            .max(120.0);
                    let checkbox_col_w = rows
                        .iter()
                        .map(|(_, _, _, label)| {
                            let text_w = ui
                                .painter()
                                .layout_no_wrap(label.clone(), font_id.clone(), text_color)
                                .size()
                                .x;
                            text_w + chrome_w
                        })
                        .fold(0.0_f32, f32::max)
                        .min(max_avail_for_check);
                    let row_h = ui.spacing().interact_size.y;

                    for (idx, id, mut checked, label) in rows {
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(checkbox_col_w, row_h),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.set_min_width(checkbox_col_w);
                                    ui.set_max_width(checkbox_col_w);
                                    if ui.checkbox(&mut checked, label).changed() {
                                        if checked {
                                            self.selected_member_rule_ids.insert(id);
                                        } else {
                                            self.selected_member_rule_ids.remove(&id);
                                        }
                                    }
                                },
                            );
                            if ui.button(s.btn_open).clicked() {
                                *pending_open_rule = Some(idx);
                            }
                        });
                    }
                }
            });
    }

    fn render_rules_duplicates_subtab(
        &mut self,
        ui: &mut egui::Ui,
        clusters: &[DuplicateActionCluster],
    ) {
        let lang = self.lang();
        let s = lang.s();
        let mut pending_confirm: Option<PendingDeleteTarget> = None;

        egui::ScrollArea::vertical()
            .id_salt("duplicates_tab_scroll")
            .hscroll(false)
            .show(ui, |ui| {
                for (cluster_idx, cluster) in clusters.iter().enumerate() {
                    let first_rule = &self.editor.draft.rules[cluster.rule_indices[0]];
                    let default_keep_id = first_rule.id.clone();
                    let keep_id = self
                        .duplicate_keep_by_cluster
                        .entry(cluster_idx)
                        .or_insert_with(|| default_keep_id.clone());
                    let keep_still_in_cluster = cluster
                        .rule_indices
                        .iter()
                        .any(|&idx| self.editor.draft.rules[idx].id == *keep_id);
                    if !keep_still_in_cluster {
                        *keep_id = default_keep_id;
                    }
                    let selected_keep_id = keep_id.clone();
                    let selected_keep_name = cluster
                        .rule_indices
                        .iter()
                        .find_map(|&idx| {
                            let r = &self.editor.draft.rules[idx];
                            if r.id == selected_keep_id {
                                Some(r.name.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default();

                    let merge_flag = self
                        .duplicate_merge_by_cluster
                        .entry(cluster_idx)
                        .or_insert(true);

                    card_frame(ui).show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.strong(lang.fmt_duplicate_cluster_title(
                                cluster_idx + 1,
                                cluster.rule_indices.len(),
                                first_rule.actions.len(),
                            ));
                        });
                        ui.add_space(6.0);

                        egui::Grid::new(("dup_cluster_grid", cluster_idx))
                            .num_columns(4)
                            .spacing([14.0, 6.0])
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(s.dup_col_keep).strong());
                                ui.label(egui::RichText::new(s.dup_col_status).strong());
                                ui.label(egui::RichText::new(s.dup_col_trigger).strong());
                                ui.label(egui::RichText::new(s.dup_col_groups).strong());
                                ui.end_row();

                                for &rule_idx in &cluster.rule_indices {
                                    let rule = &self.editor.draft.rules[rule_idx];
                                    let trigger_str = match &rule.trigger {
                                        Trigger::Hotkey { binding } => binding.to_string(),
                                    };
                                    let groups = rule.effective_groups();
                                    let groups_text = if groups.is_empty() {
                                        s.filter_no_group.to_string()
                                    } else {
                                        groups.join(", ")
                                    };
                                    let is_keep = selected_keep_id == rule.id;

                                    if ui.radio(is_keep, &rule.name).clicked() {
                                        self.duplicate_keep_by_cluster
                                            .insert(cluster_idx, rule.id.clone());
                                    }
                                    if rule.enabled {
                                        ui.colored_label(
                                            egui::Color32::from_rgb(46, 160, 67),
                                            s.status_word_active,
                                        );
                                    } else {
                                        ui.small(s.status_word_inactive);
                                    }
                                    ui.monospace(trigger_str);
                                    ui.label(groups_text);
                                    ui.end_row();
                                }
                            });

                        ui.add_space(6.0);
                        egui::Frame::group(ui.style())
                            .corner_radius(ui.visuals().widgets.noninteractive.corner_radius)
                            .show(ui, |ui| {
                                ui.strong(lang.fmt_shared_actions_title(first_rule.actions.len()));
                                ui.add_space(2.0);
                                for (a_idx, action) in first_rule.actions.iter().enumerate() {
                                    ui.monospace(lang.format_action_preview(a_idx, action));
                                }
                            });

                        ui.add_space(6.0);
                        ui.checkbox(merge_flag, s.dup_merge_checkbox);
                        ui.add_space(4.0);

                        let remove_count = cluster.rule_indices.len().saturating_sub(1);
                        if ui
                            .button(lang.fmt_resolve_duplicates_btn(remove_count))
                            .clicked()
                        {
                            let remove_ids: HashSet<String> = cluster
                                .rule_indices
                                .iter()
                                .map(|&idx| self.editor.draft.rules[idx].id.clone())
                                .filter(|id| id != &selected_keep_id)
                                .collect();
                            pending_confirm = Some(PendingDeleteTarget::ResolveDuplicates {
                                keep_id: selected_keep_id,
                                keep_name: selected_keep_name,
                                remove_ids,
                                merge_groups: *merge_flag,
                            });
                        }
                    });
                    ui.add_space(6.0);
                }
            });

        if let Some(target) = pending_confirm {
            self.pending_delete = Some(target);
        }
    }

    fn apply_and_save_theme(&mut self, ctx: &egui::Context) {
        let cfg = self.theme_config;
        self.editor.language = cfg.language;
        let radii = cfg.active_radii(CARD_CORNER_RADIUS);
        ctx.data_mut(|d| {
            d.insert_temp(
                egui::Id::NULL.with("rustbind_modal_corner_radius"),
                radii.modals,
            );
        });
        ctx.set_theme(if cfg.effective_is_dark() {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        });
        ctx.all_styles_mut(|style| {
            apply_theme_to_style(style, &cfg, CARD_CORNER_RADIUS);
        });
        self.save_theme_preferences();
    }

    fn save_theme_preferences(&mut self) {
        if let Some(path) = &self.theme_path
            && let Err(err) = self.theme_config.save_to_file(path)
        {
            self.show_toast(true, err);
        }
    }

    fn render_settings_tab(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let prev_subtab = self.settings_subtab;
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.settings_subtab,
                SettingsSubTab::General,
                SettingsSubTab::General.label(lang),
            );
            ui.selectable_value(
                &mut self.settings_subtab,
                SettingsSubTab::Themes,
                SettingsSubTab::Themes.label(lang),
            );
            ui.selectable_value(
                &mut self.settings_subtab,
                SettingsSubTab::Shortcuts,
                SettingsSubTab::Shortcuts.label(lang),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.small(egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).weak());
                if self.settings_subtab == SettingsSubTab::General {
                    let checking = self.update_check_in_flight || self.update_install_in_flight;
                    let btn_label = if self.update_check_in_flight {
                        lang.s().btn_checking_updates
                    } else {
                        lang.s().btn_check_updates
                    };
                    if ui
                        .add_enabled(!checking, egui::Button::new(btn_label))
                        .clicked()
                    {
                        let ctx = ui.ctx().clone();
                        self.start_update_check(&ctx, true);
                    }
                }
            });
        });
        if self.settings_subtab != prev_subtab {
            self.theme_editor_open = false;
        }
        ui.separator();
        ui.add_space(4.0);

        match self.settings_subtab {
            SettingsSubTab::General => self.render_settings_general_subtab(ui),
            SettingsSubTab::Themes => self.render_settings_themes_subtab(ui),
            SettingsSubTab::Shortcuts => self.render_settings_shortcuts_subtab(ui),
        }
    }

    fn render_settings_shortcuts_subtab(&self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let s = lang.s();
        let accent = Some(ui.visuals().selection.bg_fill);
        let shortcuts: [(&[&str], &str); 6] = [
            (&["Ctrl", "S"], s.shortcut_save),
            (&["Ctrl", "N"], s.shortcut_new_rule),
            (&["Ctrl", "F"], s.shortcut_search),
            (&["Ctrl", "Z"], s.shortcut_undo),
            (&["Ctrl", "Y"], s.shortcut_redo),
            (&["Del"], s.shortcut_delete_rule),
        ];

        egui::ScrollArea::vertical()
            .id_salt("shortcuts_tab_scroll")
            .hscroll(false)
            .show(ui, |ui| {
                section_frame().show(ui, |ui| {
                    ui.strong(s.section_editor_shortcuts);
                    ui.add_space(4.0);
                    ui.small(s.shortcuts_intro);
                    ui.add_space(8.0);
                    egui::Grid::new(("editor_shortcuts_grid", lang.code()))
                        .num_columns(2)
                        .spacing([18.0, 8.0])
                        .show(ui, |ui| {
                            for (keys, desc) in shortcuts {
                                ui.horizontal(|ui| {
                                    for (i, key) in keys.iter().enumerate() {
                                        if i > 0 {
                                            ui.weak("+");
                                        }
                                        Self::render_keycap_chip(
                                            ui,
                                            None,
                                            key,
                                            None,
                                            accent,
                                            None,
                                            KeycapSize::Compact,
                                        );
                                    }
                                });
                                ui.label(desc);
                                ui.end_row();
                            }
                        });
                });
            });
    }

    fn render_settings_general_subtab(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let s = lang.s();
        let mut lang_changed = false;

        egui::ScrollArea::vertical()
            .id_salt("settings_tab_scroll")
            .hscroll(false)
            .show(ui, |ui| {
                section_frame().show(ui, |ui| {
                    ui.strong(s.section_language);
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(s.label_ui_language);
                        egui::ComboBox::from_id_salt("ui_language_selector")
                            .width(180.0)
                            .selected_text(self.theme_config.language.display_name())
                            .show_ui(ui, |ui| {
                                for candidate in Language::ALL {
                                    if ui
                                        .selectable_value(
                                            &mut self.theme_config.language,
                                            *candidate,
                                            candidate.display_name(),
                                        )
                                        .changed()
                                    {
                                        lang_changed = true;
                                    }
                                }
                            });
                    });

                    ui.add_space(8.0);
                    ui.strong(s.section_autostart);
                    ui.add_space(4.0);
                    let mut enabled = self.autostart_enabled;
                    if ui.checkbox(&mut enabled, s.checkbox_autostart).changed() {
                        match set_autostart_enabled(enabled) {
                            Ok(()) => {
                                self.autostart_enabled = enabled;
                                let msg = if enabled {
                                    s.toast_autostart_on
                                } else {
                                    s.toast_autostart_off
                                };
                                self.show_toast(false, msg);
                            }
                            Err(err) => {
                                self.show_toast(true, err.to_string());
                            }
                        }
                    }
                    ui.small(s.autostart_note);
                });

                render_section_divider(ui);

                if self.editor.is_locked_invalid() {
                    section_frame().show(ui, |ui| {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 48, 48),
                            s.settings_blocked_invalid,
                        );
                    });
                } else {
                    self.render_general_settings(ui);
                    ui.add_space(6.0);
                    self.render_startup_groups_section(ui);
                }

                render_section_divider(ui);

                self.render_config_profiles_section(ui);

                render_section_divider(ui);

                let mut open_changelog = false;
                let mut open_github = false;
                let mut auto_check_toggled_on = false;
                section_frame().show(ui, |ui| {
                    ui.strong(s.section_updates_project);
                    ui.add_space(4.0);
                    if ui
                        .checkbox(
                            &mut self.theme_config.auto_check_updates,
                            s.checkbox_auto_check_updates,
                        )
                        .changed()
                    {
                        lang_changed = true;
                        if self.theme_config.auto_check_updates {
                            auto_check_toggled_on = true;
                        }
                    }
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(s.btn_changelog).clicked() {
                            open_changelog = true;
                        }
                        if ui.button(s.btn_github_repo).clicked() {
                            open_github = true;
                        }
                    });
                });
                if auto_check_toggled_on && self.available_update.is_none() {
                    let ctx = ui.ctx().clone();
                    self.start_update_check(&ctx, false);
                }
                if open_changelog {
                    let url = self.current_changelog_url().to_string();
                    self.open_external_url(&url);
                }
                if open_github {
                    self.open_external_url(GITHUB_REPO_URL);
                }
            });

        if lang_changed {
            let ctx = ui.ctx().clone();
            self.apply_and_save_theme(&ctx);
        }
    }

    fn render_settings_themes_subtab(&mut self, ui: &mut egui::Ui) {
        if self.theme_editor_open {
            self.render_theme_editor_view(ui);
            return;
        }

        let lang = self.lang();
        let s = lang.s();
        let mut theme_changed = false;
        let mut open_custom_editor = false;
        let mut import_theme_clicked = false;
        let mut export_theme_clicked = false;
        let current_preset = self.theme_config.theme_id.preset();

        egui::ScrollArea::vertical()
            .id_salt("themes_tab_scroll")
            .hscroll(false)
            .show(ui, |ui| {
                section_frame().show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(s.section_custom_theme);
                        if ui.button(s.btn_import_theme).clicked() {
                            import_theme_clicked = true;
                        }
                        if ui.button(s.btn_export_theme).clicked() {
                            export_theme_clicked = true;
                        }
                    });
                    ui.add_space(6.0);

                    let custom_snapshot = self.theme_config.custom_theme;
                    let avail_w = ui.available_width();
                    let card_w = if avail_w >= 560.0 {
                        ((avail_w - 10.0) / 2.0).max(200.0)
                    } else {
                        avail_w.max(200.0)
                    };
                    let is_custom_selected = self.theme_config.theme_id == ThemeId::Custom;
                    let mut edit_btn_clicked = false;

                    ui.vertical(|ui| {
                        ui.set_min_width(card_w);
                        ui.set_max_width(card_w);
                        let card_clicked = Self::render_selectable_card(
                            ui,
                            ("theme_card", "custom_theme"),
                            is_custom_selected,
                            true,
                            |ui| {
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(s.custom_theme_card_title).strong(),
                                        )
                                        .selectable(false),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let swatches = [
                                                custom_snapshot.secondary_accent,
                                                custom_snapshot.accent,
                                                custom_snapshot.bg_card,
                                                if custom_snapshot.is_dark
                                                    && custom_snapshot.true_black
                                                {
                                                    egui::Color32::BLACK
                                                } else {
                                                    custom_snapshot.bg_panel
                                                },
                                            ];
                                            render_color_swatches(
                                                ui,
                                                swatches,
                                                custom_snapshot.border,
                                            );
                                        },
                                    );
                                });
                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.button(s.btn_edit_theme).clicked() {
                                                edit_btn_clicked = true;
                                            }
                                            ui.with_layout(
                                                egui::Layout::left_to_right(egui::Align::Center),
                                                |ui| {
                                                    ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(
                                                                lang.fmt_custom_theme_details(
                                                                    &custom_snapshot,
                                                                ),
                                                            )
                                                            .small(),
                                                        )
                                                        .truncate()
                                                        .selectable(false),
                                                    );
                                                },
                                            );
                                        },
                                    );
                                });
                            },
                        );
                        if edit_btn_clicked {
                            open_custom_editor = true;
                        } else if card_clicked {
                            self.theme_config.theme_id = ThemeId::Custom;
                            theme_changed = true;
                        }
                    });
                });

                render_section_divider(ui);

                section_frame().show(ui, |ui| {
                    ui.strong(s.section_dark_themes);
                    ui.add_space(4.0);
                    let dark_preset_active =
                        self.theme_config.theme_id != ThemeId::Custom && current_preset.is_dark;
                    if ui
                        .add_enabled(
                            dark_preset_active,
                            egui::Checkbox::new(
                                &mut self.theme_config.true_black,
                                s.checkbox_true_black,
                            ),
                        )
                        .on_disabled_hover_text(s.true_black_light_tip)
                        .changed()
                    {
                        theme_changed = true;
                    }
                    ui.add_space(6.0);
                    Self::render_theme_preset_grid(
                        ui,
                        lang,
                        "dark_themes_grid",
                        &ALL_THEMES
                            .iter()
                            .filter(|p| p.is_dark)
                            .copied()
                            .collect::<Vec<_>>(),
                        &mut self.theme_config,
                        &mut theme_changed,
                    );
                });

                render_section_divider(ui);

                section_frame().show(ui, |ui| {
                    ui.strong(s.section_light_themes);
                    ui.add_space(6.0);
                    Self::render_theme_preset_grid(
                        ui,
                        lang,
                        "light_themes_grid",
                        &ALL_THEMES
                            .iter()
                            .filter(|p| !p.is_dark)
                            .copied()
                            .collect::<Vec<_>>(),
                        &mut self.theme_config,
                        &mut theme_changed,
                    );
                });
            });

        if open_custom_editor {
            self.theme_editor_open = true;
        }

        if import_theme_clicked {
            match pick_toml_import_file() {
                Ok(Some(path)) => match std::fs::read_to_string(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|raw| CustomTheme::from_toml_str(&raw))
                {
                    Ok(imported) => {
                        self.theme_config.custom_theme = imported;
                        self.theme_config.theme_id = ThemeId::Custom;
                        theme_changed = true;
                        self.show_toast(false, s.toast_theme_imported);
                    }
                    Err(err) => self.show_toast(true, err),
                },
                Ok(None) => {}
                Err(err) => self.show_toast(true, err),
            }
        }

        if export_theme_clicked {
            match pick_toml_export_file("rustbind-custom-theme.toml") {
                Ok(Some(path)) => match self.theme_config.custom_theme.to_toml_string() {
                    Ok(body) => match std::fs::write(&path, body) {
                        Ok(()) => self.show_toast(false, s.toast_theme_exported),
                        Err(err) => self.show_toast(true, err.to_string()),
                    },
                    Err(err) => self.show_toast(true, err),
                },
                Ok(None) => {}
                Err(err) => self.show_toast(true, err),
            }
        }

        if theme_changed {
            let ctx = ui.ctx().clone();
            self.apply_and_save_theme(&ctx);
        }
    }

    fn render_theme_editor_view(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let s = lang.s();
        let mut custom = self.theme_config.custom_theme;
        let mut changed = false;
        let mut go_back = false;

        ui.horizontal(|ui| {
            if ui.button(s.btn_back).clicked() {
                go_back = true;
            }
            ui.strong(s.theme_editor_title);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(s.btn_reset_custom_theme).clicked() {
                    custom = CustomTheme::default();
                    changed = true;
                }
            });
        });
        ui.separator();

        egui::ScrollArea::vertical()
            .id_salt("theme_editor_scroll")
            .hscroll(false)
            .show(ui, |ui| {
                section_frame().show(ui, |ui| {
                    ui.strong(s.theme_cat_mode);
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(s.theme_mode_label);
                        if ui
                            .selectable_value(&mut custom.is_dark, true, s.theme_mode_dark)
                            .changed()
                            || ui
                                .selectable_value(&mut custom.is_dark, false, s.theme_mode_light)
                                .changed()
                        {
                            changed = true;
                        }
                    });
                    ui.add_space(4.0);
                    if ui
                        .add_enabled(
                            custom.is_dark,
                            egui::Checkbox::new(&mut custom.true_black, s.checkbox_true_black),
                        )
                        .on_disabled_hover_text(s.true_black_light_tip)
                        .changed()
                    {
                        changed = true;
                    }
                });

                render_section_divider(ui);

                section_frame().show(ui, |ui| {
                    ui.strong(s.theme_cat_colors);
                    ui.add_space(6.0);
                    egui::Grid::new("custom_theme_colors_grid")
                        .num_columns(3)
                        .spacing([14.0, 6.0])
                        .show(ui, |ui| {
                            let mut color_rows: [(&str, &mut egui::Color32); 9] = [
                                (s.color_bg_panel, &mut custom.bg_panel),
                                (s.color_bg_card, &mut custom.bg_card),
                                (s.color_bg_input, &mut custom.bg_input),
                                (s.color_bg_widget, &mut custom.bg_widget),
                                (s.color_accent, &mut custom.accent),
                                (s.color_secondary_accent, &mut custom.secondary_accent),
                                (s.color_text, &mut custom.text),
                                (s.color_muted_text, &mut custom.muted_text),
                                (s.color_border, &mut custom.border),
                            ];
                            for (label, color) in &mut color_rows {
                                ui.label(*label);
                                if egui::color_picker::color_edit_button_srgba(
                                    ui,
                                    color,
                                    egui::color_picker::Alpha::Opaque,
                                )
                                .changed()
                                {
                                    **color =
                                        egui::Color32::from_rgb(color.r(), color.g(), color.b());
                                    changed = true;
                                }
                                ui.monospace(format!(
                                    "#{:02X}{:02X}{:02X}",
                                    color.r(),
                                    color.g(),
                                    color.b()
                                ));
                                ui.end_row();
                            }
                        });
                });

                render_section_divider(ui);

                section_frame().show(ui, |ui| {
                    ui.strong(s.theme_cat_radii);
                    ui.add_space(6.0);
                    egui::Grid::new("custom_theme_radii_grid")
                        .num_columns(2)
                        .spacing([14.0, 8.0])
                        .show(ui, |ui| {
                            let mut radius_rows: [(&str, &mut u8); 4] = [
                                (s.radius_controls, &mut custom.radii.controls),
                                (s.radius_inner_boxes, &mut custom.radii.inner_boxes),
                                (s.radius_cards_menus, &mut custom.radii.cards_menus),
                                (s.radius_modals, &mut custom.radii.modals),
                            ];
                            for (label, val) in &mut radius_rows {
                                ui.label(*label);
                                if ui
                                    .add(
                                        egui::Slider::new(*val, 0..=MAX_CORNER_RADIUS)
                                            .suffix(" px"),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                                ui.end_row();
                            }
                        });
                });
            });

        if changed {
            self.theme_config.custom_theme = custom;
            if self.theme_config.theme_id == ThemeId::Custom {
                let ctx = ui.ctx().clone();
                self.apply_and_save_theme(&ctx);
            } else {
                self.save_theme_preferences();
            }
        }

        if go_back {
            self.theme_editor_open = false;
        }
    }

    fn render_theme_preset_grid(
        ui: &mut egui::Ui,
        lang: Language,
        grid_id: &str,
        presets: &[ThemePreset],
        theme_config: &mut ThemeConfig,
        theme_changed: &mut bool,
    ) {
        let avail_w = ui.available_width();
        let cols = if avail_w >= 560.0 { 2 } else { 1 };
        let col_w = ((avail_w - 10.0 * (cols - 1) as f32) / cols as f32).max(200.0);

        egui::Grid::new((grid_id, lang.code()))
            .num_columns(cols)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                for (i, preset) in presets.iter().enumerate() {
                    let is_selected = theme_config.theme_id == preset.id;
                    ui.vertical(|ui| {
                        ui.set_min_width(col_w);
                        ui.set_max_width(col_w);
                        if Self::render_selectable_card(
                            ui,
                            ("theme_card", preset.id.as_str()),
                            is_selected,
                            true,
                            |ui| {
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Label::new(egui::RichText::new(preset.name).strong())
                                            .selectable(false),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            render_color_swatches(
                                                ui,
                                                [
                                                    preset.secondary_accent,
                                                    preset.accent,
                                                    preset.bg_card,
                                                    if preset.is_dark && theme_config.true_black {
                                                        egui::Color32::BLACK
                                                    } else {
                                                        preset.bg_panel
                                                    },
                                                ],
                                                preset.border,
                                            );
                                        },
                                    );
                                });
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(lang.theme_description(preset.id))
                                            .small(),
                                    )
                                    .wrap()
                                    .selectable(false),
                                );
                            },
                        ) {
                            theme_config.theme_id = preset.id;
                            *theme_changed = true;
                        }
                    });
                    if (i + 1) % cols == 0 {
                        ui.end_row();
                    }
                }
            });
    }

    fn render_config_profiles_section(&mut self, ui: &mut egui::Ui) {
        let s = self.lang().s();
        let is_locked = self.editor.is_locked_invalid();
        let profiles = self.editor.available_profiles();
        let active_profile = self.editor.active_profile_name.clone();
        let mut pending_switch: Option<String> = None;
        let ctx = ui.ctx().clone();

        section_frame().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(s.section_config_file);
                if ui.button(s.btn_reload_from_disk).clicked() {
                    self.editor.reload_from_disk();
                    self.show_toast(false, s.toast_reloaded_disk);
                }
                if ui
                    .add_enabled(!is_locked, egui::Button::new(s.btn_import_config))
                    .clicked()
                {
                    match pick_toml_import_file() {
                        Ok(Some(src_path)) => self.import_config_file_from_path(&ctx, &src_path),
                        Ok(None) => {}
                        Err(err) => self.show_toast(true, err),
                    }
                }
                if ui
                    .add_enabled(!is_locked, egui::Button::new(s.btn_export_config))
                    .clicked()
                {
                    match pick_toml_export_file(&active_profile) {
                        Ok(Some(dest_path)) => match self.editor.export_current_draft(&dest_path) {
                            Ok(()) => self.show_toast(false, dest_path.display().to_string()),
                            Err(err) => self.show_toast(true, err),
                        },
                        Ok(None) => {}
                        Err(err) => self.show_toast(true, err),
                    }
                }
                if ui.button(s.btn_open_config_folder).clicked() {
                    let config_dir = self
                        .editor
                        .config_path
                        .as_deref()
                        .and_then(|p| p.parent())
                        .map(PathBuf::from)
                        .or_else(|| ConfigManager::default_profiles_dir().ok());
                    if let Some(dir) = config_dir {
                        if let Err(err) = open_directory_in_explorer(&dir) {
                            self.show_toast(true, err.to_string());
                        }
                    } else if let Some(err) = &self.editor.path_error {
                        self.show_toast(true, err.clone());
                    }
                }
            });

            if let Some(path) = &self.editor.config_path {
                ui.add_space(4.0);
                ui.label(format!("{}: {}", s.label_path_prefix, path.display()));
            } else if let Some(err) = &self.editor.path_error {
                ui.add_space(4.0);
                ui.colored_label(
                    egui::Color32::from_rgb(220, 48, 48),
                    format!("{}: {}", s.label_path_prefix, err),
                );
            }

            if let LoadedFileKind::Missing = &self.editor.file_kind {
                ui.colored_label(ui.visuals().selection.bg_fill, s.missing_file_note);
            }

            if let Some(bak) = &self.editor.last_backup_path {
                ui.small(format!(".bak: {}", bak.display()));
            }

            if !is_locked {
                ui.add_space(4.0);
                ui.small(s.select_active_config_hint);
                ui.horizontal_wrapped(|ui| {
                    for profile_name in &profiles {
                        let is_active = *profile_name == active_profile;
                        if ui.radio(is_active, profile_name).clicked() && !is_active {
                            pending_switch = Some(profile_name.clone());
                        }
                    }
                });
            }
        });

        if let LoadedFileKind::Invalid { error_message } = &self.editor.file_kind.clone() {
            self.render_invalid_file_gate(ui, error_message);
        }

        if let Some(target_name) = pending_switch {
            if self.editor.is_dirty() {
                self.pending_delete = Some(PendingDeleteTarget::SwitchProfileDirty { target_name });
            } else if self.editor.switch_active_profile(&target_name).is_ok() {
                self.selected_available_rule_ids.clear();
                self.selected_member_rule_ids.clear();
                self.duplicate_keep_by_cluster.clear();
                self.duplicate_merge_by_cluster.clear();
                self.flush_editor_notice_to_toast();
                self.request_reload(&ctx);
            } else {
                self.flush_editor_notice_to_toast();
            }
        }
    }

    fn render_general_settings(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let s = lang.s();
        let (ks_conflicts, resume_conflicts) = self.editor.settings_hotkey_conflicts();
        let resume_failed_in_daemon = self
            .daemon_diagnostics
            .as_ref()
            .is_some_and(|d| d.resume_hotkey_failed);
        section_frame().show(ui, |ui| {
            ui.strong(s.section_input_controls);
            ui.add_space(6.0);

            egui::Grid::new("settings_general_grid")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label(s.label_kill_switch);
                    ui.vertical(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            let ks = &mut self.editor.draft.settings.kill_switch;
                            Self::render_modifiers_and_key(
                                ui,
                                "kill_switch_key",
                                &mut ks.modifiers,
                                &mut ks.key,
                            );
                            if ui.button(s.btn_restore_default).clicked() {
                                self.editor.restore_default_kill_switch();
                            }
                        });
                        if !ks_conflicts.is_empty() {
                            ui.colored_label(
                                egui::Color32::from_rgb(210, 140, 20),
                                lang.fmt_settings_hotkey_conflict(&ks_conflicts.join(", ")),
                            );
                        }
                    });
                    ui.end_row();

                    ui.label(s.label_resume_daemon);
                    ui.vertical(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            let mut enabled = self.editor.draft.settings.resume_hotkey.is_some();
                            if ui
                                .checkbox(&mut enabled, s.checkbox_enable_hotkey)
                                .changed()
                            {
                                if enabled {
                                    self.editor.draft.settings.resume_hotkey =
                                        Some(KeyBinding::new(
                                            Modifiers {
                                                ctrl: true,
                                                alt: true,
                                                shift: true,
                                                win: false,
                                            },
                                            VirtualKey::F11,
                                        ));
                                } else {
                                    self.editor.draft.settings.resume_hotkey = None;
                                }
                            }
                            if let Some(resume_binding) =
                                &mut self.editor.draft.settings.resume_hotkey
                            {
                                Self::render_modifiers_and_key(
                                    ui,
                                    "resume_hotkey_key",
                                    &mut resume_binding.modifiers,
                                    &mut resume_binding.key,
                                );
                            }
                        });
                        if !resume_conflicts.is_empty() {
                            ui.colored_label(
                                egui::Color32::from_rgb(210, 140, 20),
                                lang.fmt_settings_hotkey_conflict(&resume_conflicts.join(", ")),
                            );
                        }
                        if resume_failed_in_daemon {
                            ui.colored_label(
                                egui::Color32::from_rgb(220, 48, 48),
                                s.daemon_resume_hotkey_failed,
                            );
                        }
                    });
                    ui.end_row();

                    ui.label(s.label_rate_limit);
                    ui.add(
                        egui::Slider::new(
                            &mut self.editor.draft.settings.max_events_per_second,
                            MIN_EVENTS_PER_SECOND..=MAX_EVENTS_PER_SECOND,
                        )
                        .clamping(egui::SliderClamping::Always),
                    );
                    ui.end_row();
                });
        });
    }

    fn render_startup_groups_section(&mut self, ui: &mut egui::Ui) {
        let s = self.lang().s();
        let mut fallback_toast = false;
        section_frame().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(s.section_startup_groups);
                let mut all_active = self.editor.is_all_groups_active();
                if ui
                    .checkbox(&mut all_active, s.checkbox_all_groups_enabled)
                    .changed()
                {
                    if all_active {
                        self.editor.set_all_groups_active();
                    } else {
                        self.editor.draft.settings.active_groups = self.editor.available_groups();
                    }
                }
            });

            let groups = self.editor.available_groups();
            ui.horizontal_wrapped(|ui| {
                for group in &groups {
                    let mut checked = self.editor.is_group_active_effective(group);
                    if ui.checkbox(&mut checked, group).changed()
                        && self.editor.toggle_group_active_now(group, checked)
                    {
                        fallback_toast = true;
                    }
                }
            });
            ui.small(s.startup_groups_note);
        });
        if fallback_toast {
            self.show_toast(false, s.toast_fallback_default);
        }
    }

    fn render_security_tab(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let s = lang.s();
        let stats = self.editor.config_stats();
        egui::ScrollArea::vertical()
            .id_salt("security_tab_scroll")
            .hscroll(false)
            .show(ui, |ui| {
                section_frame().show(ui, |ui| {
                    ui.strong(s.section_daemon_status);
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        self.render_header_status_badge(ui);
                        if let DaemonStatusView::Connected { active_rules, .. } = &self.status_view
                        {
                            ui.separator();
                            ui.small(lang.fmt_ipc_loaded_hotkeys(*active_rules));
                        }
                    });
                    if let DaemonStatusView::Error(msg) = &self.status_view {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 48, 48),
                            format!("{}: {}", s.status_ipc_error, msg),
                        );
                    }
                    ui.add_space(6.0);
                    egui::Grid::new("security_stats_grid")
                        .num_columns(2)
                        .spacing([16.0, 4.0])
                        .show(ui, |ui| {
                            ui.label(s.stat_total_rules);
                            ui.strong(stats.total_rules.to_string());
                            ui.end_row();

                            ui.label(s.stat_enabled_rules);
                            ui.strong(stats.enabled_rules.to_string());
                            ui.end_row();

                            ui.label(s.stat_total_groups);
                            ui.strong(stats.total_groups.to_string());
                            ui.end_row();

                            ui.label(s.stat_active_groups);
                            ui.strong(stats.active_groups.to_string());
                            ui.end_row();

                            ui.label(s.stat_active_rules_in_active_groups);
                            ui.strong(stats.active_rules_in_active_groups.to_string());
                            ui.end_row();
                        });
                    ui.add_space(4.0);
                    ui.small(s.daemon_menu_hint);
                });

                if matches!(
                    self.status_view,
                    crate::state::DaemonStatusView::Connected { .. }
                ) {
                    let mut pending_jump_rule_idx: Option<usize> = None;
                    let diag = self.daemon_diagnostics.clone().unwrap_or_default();
                    let has_conflicts = diag.resume_hotkey_failed || !diag.skipped_rules.is_empty();
                    render_section_divider(ui);
                    section_frame().show(ui, |ui| {
                        ui.strong(s.section_daemon_conflicts);
                        ui.add_space(4.0);
                        if !has_conflicts {
                            ui.colored_label(
                                egui::Color32::from_rgb(46, 160, 67),
                                s.no_daemon_conflicts,
                            );
                        } else {
                            if diag.resume_hotkey_failed {
                                ui.colored_label(
                                    egui::Color32::from_rgb(220, 48, 48),
                                    s.daemon_resume_hotkey_failed,
                                );
                            }
                            for skipped in &diag.skipped_rules {
                                let found_idx = self
                                    .editor
                                    .draft
                                    .rules
                                    .iter()
                                    .position(|r| r.id == skipped.rule_id);
                                let display_name = found_idx
                                    .map(|idx| self.editor.draft.rules[idx].name.clone())
                                    .unwrap_or_else(|| skipped.rule_id.clone());
                                ui.horizontal_wrapped(|ui| {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(220, 48, 48),
                                        format!(
                                            "• {}: {}",
                                            display_name,
                                            lang.tr_skipped_reason(skipped.reason)
                                        ),
                                    );
                                    if let Some(idx) = found_idx
                                        && ui.button(s.btn_open_rule).clicked()
                                    {
                                        pending_jump_rule_idx = Some(idx);
                                    }
                                });
                            }
                        }
                    });
                    if let Some(idx) = pending_jump_rule_idx {
                        self.editor.selected_rule_idx = Some(idx);
                        self.active_tab = UiTab::Rules;
                        self.rules_subtab = RulesSubTab::RuleList;
                    }
                }

                if !self.recent_logs_loaded {
                    let log_file = default_log_directory().join("rustbind.log");
                    self.recent_log_warnings = read_recent_log_warnings(&log_file, 10);
                    self.recent_logs_loaded = true;
                }
                render_section_divider(ui);
                section_frame().show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(s.section_recent_log_warnings);
                        if ui.button(s.btn_refresh_logs).clicked() {
                            let log_file = default_log_directory().join("rustbind.log");
                            self.recent_log_warnings = read_recent_log_warnings(&log_file, 10);
                        }
                        if ui.button(s.btn_open_logs_folder).clicked() {
                            let logs_dir = default_log_directory();
                            if let Err(err) = open_directory_in_explorer(&logs_dir) {
                                self.show_toast(true, err.to_string());
                            }
                        }
                    });
                    ui.add_space(4.0);
                    if self.recent_log_warnings.is_empty() {
                        ui.colored_label(
                            egui::Color32::from_rgb(46, 160, 67),
                            s.no_recent_log_warnings,
                        );
                    } else {
                        egui::ScrollArea::vertical()
                            .id_salt("security_recent_logs_scroll")
                            .hscroll(false)
                            .max_height(140.0)
                            .show(ui, |ui| {
                                for line in &self.recent_log_warnings {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(line).monospace().small(),
                                        )
                                        .wrap(),
                                    );
                                }
                            });
                    }
                });

                render_section_divider(ui);
                section_frame().show(ui, |ui| {
                    ui.strong(s.section_guarantees);
                    ui.add_space(4.0);
                    ui.small(s.guarantee_1);
                    ui.small(s.guarantee_2);
                    ui.small(s.guarantee_3);
                    ui.small(s.guarantee_4);
                    ui.small(s.guarantee_5);
                });
            });
    }

    fn render_invalid_file_gate(&mut self, ui: &mut egui::Ui, error_message: &str) {
        let s = self.lang().s();
        ui.add_space(6.0);
        card_frame(ui)
            .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(220, 48, 48)))
            .show(ui, |ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(220, 48, 48),
                    egui::RichText::new(s.invalid_file_title).strong(),
                );
                ui.label(error_message);
                ui.small(s.invalid_file_untouched);
                ui.add_space(8.0);

                ui.checkbox(
                    &mut self.editor.confirm_overwrite_invalid,
                    s.checkbox_confirm_backup,
                );
                ui.add_space(4.0);
                if ui
                    .add_enabled(
                        self.editor.confirm_overwrite_invalid,
                        egui::Button::new(s.btn_backup_restore),
                    )
                    .clicked()
                    && self.editor.restore_with_backup().is_ok()
                {
                    let ctx = ui.ctx().clone();
                    self.request_reload(&ctx);
                }
            });
    }

    fn render_footer_validation_and_save(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let s = lang.s();
        let (val_color, raw_val_text) = if self.editor.is_locked_invalid() {
            (
                egui::Color32::from_rgb(220, 48, 48),
                s.footer_invalid_disk.to_string(),
            )
        } else {
            match self.editor.validate_live() {
                Ok(()) => (
                    egui::Color32::from_rgb(46, 160, 67),
                    s.footer_valid.to_string(),
                ),
                Err(err) => (
                    egui::Color32::from_rgb(220, 48, 48),
                    lang.tr_config_error(&err),
                ),
            }
        };
        let val_text = raw_val_text.trim().trim_end_matches('.').to_string();

        let compact = ui.available_width() < 540.0;
        let can_save = self.editor.can_save();
        let is_dirty = self.editor.is_dirty();

        if compact {
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(&val_text).color(val_color))
                        .truncate()
                        .show_tooltip_when_elided(false),
                )
                .on_hover_text(egui::RichText::new(&val_text).color(val_color));
                if is_dirty {
                    ui.colored_label(egui::Color32::from_rgb(210, 140, 20), s.footer_unsaved);
                }
            });
            ui.add_space(4.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(can_save, egui::Button::new(s.btn_save))
                    .on_hover_text("Ctrl+S")
                    .clicked()
                {
                    if self.editor.save().is_ok() {
                        let ctx = ui.ctx().clone();
                        self.request_reload(&ctx);
                    } else {
                        self.flush_editor_notice_to_toast();
                    }
                }
                if ui
                    .add_enabled(is_dirty, egui::Button::new(s.btn_discard))
                    .clicked()
                {
                    self.editor.discard_changes();
                    self.flush_editor_notice_to_toast();
                }
            });
        } else {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(can_save, egui::Button::new(s.btn_save))
                        .on_hover_text("Ctrl+S")
                        .clicked()
                    {
                        if self.editor.save().is_ok() {
                            let ctx = ui.ctx().clone();
                            self.request_reload(&ctx);
                        } else {
                            self.flush_editor_notice_to_toast();
                        }
                    }
                    if ui
                        .add_enabled(is_dirty, egui::Button::new(s.btn_discard))
                        .clicked()
                    {
                        self.editor.discard_changes();
                        self.flush_editor_notice_to_toast();
                    }
                    if is_dirty {
                        ui.colored_label(egui::Color32::from_rgb(210, 140, 20), s.footer_unsaved);
                    }

                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(&val_text).color(val_color))
                                .truncate()
                                .show_tooltip_when_elided(false),
                        )
                        .on_hover_text(egui::RichText::new(&val_text).color(val_color));
                    });
                });
            });
        }
    }

    fn render_modal_backdrop_and_frame(
        ctx: &egui::Context,
        backdrop_salt: &str,
    ) -> (std::sync::Arc<egui::Style>, egui::Color32, egui::Frame) {
        let backdrop_id = egui::Id::from(ctx.viewport_id()).with(backdrop_salt);
        let screen_rect = ctx.content_rect();
        let modal_style = ctx.style_of(ctx.theme());
        let is_dark = modal_style.visuals.dark_mode;
        let backdrop_color = if is_dark {
            egui::Color32::from_black_alpha(175)
        } else {
            egui::Color32::from_black_alpha(105)
        };
        egui::Area::new(backdrop_id)
            .order(egui::Order::Middle)
            .fixed_pos(screen_rect.min)
            .interactable(true)
            .show(ctx, |ui| {
                let _ = ui.allocate_exact_size(screen_rect.size(), egui::Sense::click_and_drag());
                ui.painter().rect_filled(screen_rect, 0.0, backdrop_color);
            });

        let title_color = if is_dark {
            egui::Color32::WHITE
        } else {
            modal_style.visuals.strong_text_color()
        };
        let modal_frame = egui::Frame::window(&modal_style)
            .corner_radius(egui::CornerRadius::same(modal_corner_radius(ctx)))
            .inner_margin(egui::Margin::same(16))
            .shadow(modal_style.visuals.window_shadow);
        (modal_style, title_color, modal_frame)
    }

    fn render_delete_confirmation_modal(&mut self, ctx: &egui::Context) {
        let Some(target) = self.pending_delete.take() else {
            return;
        };

        let lang = self.lang();
        let s = lang.s();
        let mut keep_open = true;
        let mut confirm = false;
        let mut save_and_close = false;
        let mut cancel = false;
        let is_exit_unsaved = matches!(target, PendingDeleteTarget::ExitUnsaved);
        let is_update_unsaved = matches!(target, PendingDeleteTarget::UpdateUnsaved(_));
        let has_save_action = is_exit_unsaved || is_update_unsaved;
        let can_save_exit = has_save_action && self.editor.can_save();

        let (title_text, message, confirm_label, is_destructive) = match &target {
            PendingDeleteTarget::Rule { name, .. } => (
                s.modal_title_delete,
                lang.fmt_delete_rule_confirm(name),
                s.btn_delete,
                true,
            ),
            PendingDeleteTarget::Group { name } => (
                s.modal_title_delete,
                lang.fmt_delete_group_confirm(name),
                s.btn_delete,
                true,
            ),
            PendingDeleteTarget::ResolveDuplicates {
                keep_name,
                remove_ids,
                merge_groups,
                ..
            } => (
                s.modal_title_duplicates,
                lang.fmt_resolve_duplicates_confirm(keep_name, remove_ids.len(), *merge_groups),
                s.btn_delete,
                true,
            ),
            PendingDeleteTarget::ImportOverwrite(candidate) => (
                s.modal_title_overwrite,
                lang.fmt_import_overwrite_confirm(&candidate.target_name),
                s.btn_overwrite,
                true,
            ),
            PendingDeleteTarget::SwitchProfileDirty { target_name } => (
                s.modal_title_unsaved,
                lang.fmt_switch_profile_dirty_confirm(target_name),
                s.btn_switch_file,
                false,
            ),
            PendingDeleteTarget::ExitUnsaved => (
                s.modal_title_unsaved,
                s.modal_exit_unsaved_confirm.to_string(),
                s.btn_close_without_saving,
                true,
            ),
            PendingDeleteTarget::UpdateUnsaved(_) => (
                s.modal_title_unsaved,
                s.modal_update_unsaved_confirm.to_string(),
                s.btn_update_without_saving,
                true,
            ),
        };

        let (modal_style, title_color, modal_frame) =
            Self::render_modal_backdrop_and_frame(ctx, "confirm_modal_backdrop");
        let (min_w, max_w) = if has_save_action {
            (410.0, 460.0)
        } else {
            (290.0, 380.0)
        };

        egui::Window::new(egui::RichText::new(title_text).color(title_color).strong())
            .open(&mut keep_open)
            .collapsible(false)
            .resizable(false)
            .auto_sized()
            .order(egui::Order::Foreground)
            .frame(modal_frame)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_min_width(min_w);
                ui.set_max_width(max_w);
                ui.vertical_centered(|ui| {
                    ui.add(egui::Label::new(message).wrap());
                });
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if has_save_action {
                            let save_btn_text = if is_update_unsaved {
                                s.btn_save_and_update
                            } else {
                                s.btn_save_and_close
                            };
                            let save_label = if can_save_exit {
                                egui::RichText::new(save_btn_text).color(egui::Color32::WHITE)
                            } else {
                                egui::RichText::new(save_btn_text)
                            };
                            let mut save_btn = egui::Button::new(save_label);
                            if can_save_exit {
                                save_btn = save_btn.fill(modal_style.visuals.selection.bg_fill);
                            }
                            if ui.add_enabled(can_save_exit, save_btn).clicked() {
                                save_and_close = true;
                            }
                        }
                        let btn_fill = if is_destructive {
                            egui::Color32::from_rgb(190, 45, 45)
                        } else {
                            modal_style.visuals.selection.bg_fill
                        };
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(confirm_label).color(egui::Color32::WHITE),
                                )
                                .fill(btn_fill),
                            )
                            .clicked()
                        {
                            confirm = true;
                        }
                        if ui.button(s.btn_cancel).clicked() {
                            cancel = true;
                        }
                    });
                });
            });

        if save_and_close {
            match target {
                PendingDeleteTarget::UpdateUnsaved(release) => {
                    if self.editor.save().is_ok() {
                        let _ = send_command(IpcCommand::ReloadConfig);
                        self.start_update_download_and_apply(ctx, release);
                    } else {
                        self.flush_editor_notice_to_toast();
                    }
                }
                _ => {
                    if self.editor.save().is_ok() {
                        let _ = send_command(IpcCommand::ReloadConfig);
                        self.allow_close_with_dirty = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    } else {
                        self.flush_editor_notice_to_toast();
                    }
                }
            }
        } else if confirm {
            match target {
                PendingDeleteTarget::Rule { idx, name } => {
                    if idx < self.editor.draft.rules.len() {
                        let id = self.editor.draft.rules[idx].id.clone();
                        self.editor.remove_rule(idx);
                        self.selected_available_rule_ids.remove(&id);
                        self.selected_member_rule_ids.remove(&id);
                        self.show_toast(false, lang.fmt_rule_deleted(&name));
                    }
                }
                PendingDeleteTarget::Group { name } => match self.editor.delete_group(&name) {
                    Ok(()) => {
                        if self.selected_group == name {
                            self.selected_group = default_rule_group();
                            self.selected_available_rule_ids.clear();
                            self.selected_member_rule_ids.clear();
                        }
                        self.show_toast(false, lang.fmt_group_deleted(&name));
                    }
                    Err(err) => self.show_toast(true, err),
                },
                PendingDeleteTarget::ResolveDuplicates {
                    keep_id,
                    remove_ids,
                    merge_groups,
                    ..
                } => match self
                    .editor
                    .resolve_duplicate_rules(&keep_id, &remove_ids, merge_groups)
                {
                    Ok(removed) => {
                        self.duplicate_keep_by_cluster.clear();
                        self.duplicate_merge_by_cluster.clear();
                        self.show_toast(false, lang.fmt_duplicates_removed(removed));
                    }
                    Err(msg) => self.show_toast(true, msg),
                },
                PendingDeleteTarget::ImportOverwrite(candidate) => {
                    self.apply_imported_candidate(ctx, candidate);
                }
                PendingDeleteTarget::SwitchProfileDirty { target_name } => {
                    if self.editor.switch_active_profile(&target_name).is_ok() {
                        self.clear_transient_ui_selections();
                        self.flush_editor_notice_to_toast();
                        self.request_reload(ctx);
                    } else {
                        self.flush_editor_notice_to_toast();
                    }
                }
                PendingDeleteTarget::ExitUnsaved => {
                    self.flush_layout_to_disk();
                    self.allow_close_with_dirty = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                PendingDeleteTarget::UpdateUnsaved(release) => {
                    self.start_update_download_and_apply(ctx, release);
                }
            }
        } else if !cancel && keep_open {
            self.pending_delete = Some(target);
        }
    }

    fn measure_keycap_chip_width(
        ui: &egui::Ui,
        step: Option<usize>,
        key_label: &str,
        kind_tag: Option<&str>,
        size: KeycapSize,
    ) -> f32 {
        let (margin, key_size) = match size {
            KeycapSize::Compact => (egui::Margin::symmetric(6, 2), 11.5),
            KeycapSize::Medium => (egui::Margin::symmetric(8, 4), 12.5),
            KeycapSize::Large => (egui::Margin::symmetric(11, 6), 15.0),
        };
        let style = ui.style();
        let text_color = style.visuals.text_color();

        let step_w = step.map_or(0.0, |idx| {
            let galley =
                egui::WidgetText::from(egui::RichText::new(format!("{}.", idx + 1)).small())
                    .into_galley(
                        ui,
                        Some(egui::TextWrapMode::Extend),
                        f32::INFINITY,
                        egui::TextStyle::Small,
                    );
            galley.size().x + 4.0
        });
        let key_galley = egui::WidgetText::from(
            egui::RichText::new(key_label)
                .size(key_size)
                .strong()
                .color(text_color),
        )
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Body,
        );
        let tag_w = kind_tag.map_or(0.0, |tag| {
            let galley = egui::WidgetText::from(egui::RichText::new(tag).small()).into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::TextStyle::Small,
            );
            galley.size().x + 4.0
        });

        f32::from(margin.left + margin.right) + step_w + key_galley.size().x + tag_w + 4.0
    }

    fn render_keycap_chip(
        ui: &mut egui::Ui,
        step: Option<usize>,
        key_label: &str,
        kind_tag: Option<&str>,
        accent_border: Option<egui::Color32>,
        tag_color: Option<egui::Color32>,
        size: KeycapSize,
    ) -> egui::Response {
        let visuals = ui.visuals();
        let is_dark = visuals.dark_mode;
        let weak_color = visuals.weak_text_color();
        let stroke_color = accent_border.unwrap_or(visuals.widgets.noninteractive.bg_stroke.color);
        let bg_fill = visuals.widgets.inactive.bg_fill;
        let (margin, key_size) = match size {
            KeycapSize::Compact => (egui::Margin::symmetric(6, 2), 11.5),
            KeycapSize::Medium => (egui::Margin::symmetric(8, 4), 12.5),
            KeycapSize::Large => (egui::Margin::symmetric(11, 6), 15.0),
        };

        let response = egui::Frame::NONE
            .fill(bg_fill)
            .corner_radius(visuals.widgets.inactive.corner_radius)
            .stroke(egui::Stroke::new(1.0, stroke_color))
            .inner_margin(margin)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if let Some(idx) = step {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!("{}.", idx + 1)).small().weak(),
                            )
                            .selectable(false),
                        );
                    }
                    ui.add(
                        egui::Label::new(egui::RichText::new(key_label).size(key_size).strong())
                            .selectable(false),
                    );
                    if let Some(tag) = kind_tag {
                        let resolved_tag_color = tag_color.or(accent_border).unwrap_or(weak_color);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(tag).small().color(resolved_tag_color),
                            )
                            .selectable(false),
                        );
                    }
                });
            });

        // Bordo inferiore leggermente più marcato per dare profondità tipo keycap fisico.
        let rect = response.response.rect;
        let bottom_y = rect.bottom() - 1.0;
        let bevel_color = accent_border.unwrap_or_else(|| {
            if is_dark {
                egui::Color32::from_white_alpha(45)
            } else {
                egui::Color32::from_black_alpha(55)
            }
        });
        ui.painter().line_segment(
            [
                egui::pos2(rect.left() + 4.0, bottom_y),
                egui::pos2(rect.right() - 4.0, bottom_y),
            ],
            egui::Stroke::new(1.5, bevel_color),
        );
        response.response
    }

    fn render_modifier_keycaps_row(
        ui: &mut egui::Ui,
        id_salt: &str,
        modifiers: Modifiers,
        main_key: Option<&str>,
        accent: egui::Color32,
        size: KeycapSize,
    ) {
        let mut parts: Vec<(&str, bool)> = Vec::with_capacity(5);
        if modifiers.ctrl {
            parts.push(("Ctrl", false));
        }
        if modifiers.alt {
            parts.push(("Alt", false));
        }
        if modifiers.shift {
            parts.push(("Shift", false));
        }
        if modifiers.win {
            parts.push(("Win", false));
        }
        if let Some(key) = main_key {
            parts.push((key, true));
        }

        let avail_w = ui.available_width();
        let mem_id = ui.make_persistent_id(id_salt).with("keycaps_row_w");
        let prev_w = ui.data(|d| d.get_temp::<f32>(mem_id)).unwrap_or(180.0);
        let left_pad = ((avail_w - prev_w) * 0.5).max(0.0);

        let mut measured_w = prev_w;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            if left_pad > 0.0 {
                ui.add_space(left_pad);
            }
            let content_start_x = ui.cursor().min.x;
            for (idx, (label, is_main)) in parts.iter().enumerate() {
                if idx > 0 {
                    ui.add(
                        egui::Label::new(egui::RichText::new("+").strong().weak())
                            .selectable(false),
                    );
                }
                Self::render_keycap_chip(
                    ui,
                    None,
                    label,
                    None,
                    if *is_main { Some(accent) } else { None },
                    None,
                    size,
                );
            }
            if main_key.is_none() && !parts.is_empty() {
                ui.add(
                    egui::Label::new(egui::RichText::new("+ ...").strong().weak())
                        .selectable(false),
                );
            }
            measured_w = (ui.min_rect().right() - content_start_x).max(0.0);
        });

        if (measured_w - prev_w).abs() > 0.5 {
            ui.data_mut(|d| d.insert_temp(mem_id, measured_w));
            ui.ctx().request_repaint();
        }
    }

    fn render_capture_modal(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.capture_modal.take() else {
            return;
        };

        let is_editing_chip = matches!(
            &state,
            CaptureModalState::Actions(cap) if cap.editing_action.is_some()
        );

        let (events, frame_mods, pointer_pos, pixels_per_point) = ctx.input(|i| {
            (
                i.events.clone(),
                i.modifiers,
                i.pointer.latest_pos(),
                i.pixels_per_point,
            )
        });
        // Finché è aperto il menu di modifica chip non rubiamo focus né consumiamo tasti per gli input numerici.
        if !is_editing_chip {
            if let Some(focused_id) = ctx.memory(|m| m.focused()) {
                ctx.memory_mut(|m| m.surrender_focus(focused_id));
            }
            ctx.input_mut(|i| {
                let mods = i.modifiers;
                for key in [
                    egui::Key::Tab,
                    egui::Key::Enter,
                    egui::Key::Space,
                    egui::Key::Escape,
                    egui::Key::ArrowUp,
                    egui::Key::ArrowDown,
                    egui::Key::ArrowLeft,
                    egui::Key::ArrowRight,
                ] {
                    i.consume_key(mods, key);
                }
            });
        }

        match &mut state {
            CaptureModalState::Trigger(cap) => cap.process_events(&events, frame_mods),
            CaptureModalState::Actions(cap) => cap.process_events(
                &events,
                frame_mods,
                pointer_pos,
                pixels_per_point,
                MAX_ACTIONS_PER_SEQUENCE,
            ),
        }

        let lang = self.lang();
        let s = lang.s();
        let mut keep_open = true;
        let mut confirm = false;
        let mut cancel = false;
        let mut reset = false;

        let (modal_style, title_color, modal_frame) =
            Self::render_modal_backdrop_and_frame(ctx, "capture_modal_backdrop");
        let title_text = match &state {
            CaptureModalState::Trigger(_) => s.modal_record_trigger_title,
            CaptureModalState::Actions(_) => s.modal_record_actions_title,
        };

        const CAPTURE_MODAL_W: f32 = 460.0;
        const CAPTURE_CARD_W: f32 = 436.0;
        const CAPTURE_CARD_CONTENT_W: f32 = 416.0;

        let accent = modal_style.visuals.selection.bg_fill;

        egui::Window::new(egui::RichText::new(title_text).color(title_color).strong())
            .open(&mut keep_open)
            .collapsible(false)
            .resizable(false)
            .auto_sized()
            .min_width(CAPTURE_MODAL_W)
            .max_width(CAPTURE_MODAL_W)
            .order(egui::Order::Foreground)
            .frame(modal_frame)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_width(CAPTURE_MODAL_W);
                ui.set_max_width(CAPTURE_MODAL_W);
                ui.add_enabled_ui(!is_editing_chip, |ui| {
                    match &mut state {
                        CaptureModalState::Trigger(cap) => {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(s.record_trigger_hint).small(),
                                )
                                .wrap(),
                            );
                            ui.add_space(8.0);
                            card_frame(ui)
                                .inner_margin(egui::Margin::same(12))
                                .show(ui, |ui| {
                                    let area_h = 74.0;
                                    let is_waiting =
                                        cap.captured.is_none() && !cap.live_modifiers.any();
                                    let v_mem_id = ui.make_persistent_id(if is_waiting {
                                        "trigger_preview_content_h_waiting"
                                    } else {
                                        "trigger_preview_content_h_active"
                                    });
                                    let prev_h = ui
                                        .data(|d| d.get_temp::<f32>(v_mem_id))
                                        .unwrap_or(if is_waiting { 18.0 } else { 54.0 });

                                    let (card_rect, _) = ui.allocate_exact_size(
                                        egui::vec2(CAPTURE_CARD_CONTENT_W, area_h),
                                        egui::Sense::hover(),
                                    );
                                    let top_y =
                                        (card_rect.center().y - prev_h * 0.5).max(card_rect.top());
                                    let target_rect = egui::Rect::from_min_max(
                                        egui::pos2(card_rect.left(), top_y),
                                        card_rect.max,
                                    );

                                    let mut child = ui.new_child(
                                        egui::UiBuilder::new()
                                            .max_rect(target_rect)
                                            .layout(egui::Layout::top_down(egui::Align::Center)),
                                    );
                                    child.vertical_centered(|ui| {
                                        if let Some(binding) = cap.captured {
                                            ui.small(s.record_captured_trigger);
                                            ui.add_space(6.0);
                                            Self::render_modifier_keycaps_row(
                                                ui,
                                                "trigger_captured_row",
                                                binding.modifiers,
                                                Some(binding.key.as_str()),
                                                accent,
                                                KeycapSize::Large,
                                            );
                                        } else if cap.live_modifiers.any() {
                                            ui.small(s.record_current_modifiers);
                                            ui.add_space(6.0);
                                            Self::render_modifier_keycaps_row(
                                                ui,
                                                "trigger_live_mods_row",
                                                cap.live_modifiers,
                                                None,
                                                accent,
                                                KeycapSize::Large,
                                            );
                                        } else {
                                            ui.weak(s.record_waiting_keys);
                                        }
                                    });
                                    let measured_h = child.min_rect().height();

                                    if (measured_h - prev_h).abs() > 0.5 {
                                        ui.data_mut(|d| d.insert_temp(v_mem_id, measured_h));
                                        ui.ctx().request_repaint();
                                    }
                                });
                        }
                        CaptureModalState::Actions(cap) => {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(s.record_actions_hint).small(),
                                )
                                .wrap(),
                            );
                            ui.add_space(8.0);
                            let pointer_inside_capture = !is_editing_chip
                                && cap
                                    .capture_rect
                                    .is_some_and(|r| pointer_pos.is_some_and(|p| r.contains(p)));
                            let mut box_frame = card_frame(ui).inner_margin(egui::Margin::same(12));
                            if pointer_inside_capture {
                                // Cambia solo il colore mantenendo identico lo spessore per non alterare le dimensioni.
                                box_frame.stroke.color = accent;
                            }
                            let mut scroll_inner_rect = egui::Rect::NOTHING;
                            let mut has_v_scrollbar = false;
                            let mut next_editable_chips: Vec<(usize, egui::Rect)> = Vec::new();
                            let mut next_delete_chips: Vec<(usize, egui::Rect)> = Vec::new();
                            let mut pending_open_edit: Option<(egui::Rect, EditingActionDraft)> =
                                None;
                            let mut pending_remove_chip: Option<usize> = None;

                            let card_resp = box_frame.show(ui, |ui| {
                                ui.set_width(CAPTURE_CARD_CONTENT_W);
                                ui.small(s.record_captured_actions);
                                ui.add_space(8.0);

                                let scroll_h = 136.0;
                                let scroll_out = egui::ScrollArea::vertical()
                                    .id_salt("capture_actions_preview_scroll")
                                    .max_width(CAPTURE_CARD_CONTENT_W)
                                    .min_scrolled_height(scroll_h)
                                    .max_height(scroll_h)
                                    .auto_shrink([false, false])
                                    .scroll_source(egui::scroll_area::ScrollSource::SCROLL_BAR)
                                    .scroll_bar_visibility(
                                        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                                    )
                                    .hscroll(false)
                                    .show(ui, |ui| {
                                        if cap.recorded_actions.is_empty() {
                                            ui.allocate_ui_with_layout(
                                                egui::vec2(CAPTURE_CARD_CONTENT_W, scroll_h),
                                                egui::Layout::top_down(egui::Align::Center),
                                                |ui| {
                                                    let line_h = ui
                                                        .text_style_height(&egui::TextStyle::Body);
                                                    ui.add_space(
                                                        ((scroll_h - line_h) * 0.5).max(0.0),
                                                    );
                                                    ui.weak(s.record_waiting_actions);
                                                },
                                            );
                                        } else {
                                            // Calcolo esplicito delle righe entro `wrap_w` perché egui `Frame`
                                            // dentro `horizontal_wrapped` non va a capo automaticamente.
                                            let wrap_w = CAPTURE_CARD_CONTENT_W - 18.0;
                                            let items: Vec<(
                                                String,
                                                Option<&'static str>,
                                                Option<egui::Color32>,
                                            )> = cap
                                                .recorded_actions
                                                .iter()
                                                .enumerate()
                                                .map(|(idx, action)| match action {
                                                    InputAction::KeyClick { key } => {
                                                        (key.as_str().to_string(), None, None)
                                                    }
                                                    InputAction::KeyDown { key } => (
                                                        key.as_str().to_string(),
                                                        Some("Down"),
                                                        Some(accent),
                                                    ),
                                                    InputAction::KeyUp { key } => {
                                                        (key.as_str().to_string(), Some("Up"), None)
                                                    }
                                                    InputAction::MouseClick { button } => (
                                                        format!("Mouse {}", button.as_str()),
                                                        None,
                                                        None,
                                                    ),
                                                    InputAction::MouseDown { button } => (
                                                        format!("Mouse {}", button.as_str()),
                                                        Some("Down"),
                                                        Some(accent),
                                                    ),
                                                    InputAction::MouseUp { button } => (
                                                        format!("Mouse {}", button.as_str()),
                                                        Some("Up"),
                                                        None,
                                                    ),
                                                    InputAction::MouseScroll {
                                                        direction,
                                                        steps,
                                                    } => {
                                                        let sign =
                                                            if *steps > 0 { "+" } else { "" };
                                                        (
                                                            format!(
                                                                "Wheel {} {}{}",
                                                                direction.as_str(),
                                                                sign,
                                                                steps
                                                            ),
                                                            None,
                                                            None,
                                                        )
                                                    }
                                                    InputAction::MouseMove { dx, dy } => {
                                                        (format!("Move ({dx}, {dy})"), None, None)
                                                    }
                                                    other => (
                                                        lang.format_action_preview(idx, other),
                                                        None,
                                                        None,
                                                    ),
                                                })
                                                .collect();

                                            let mut rows: Vec<Vec<usize>> = Vec::new();
                                            let mut current_row: Vec<usize> = Vec::new();
                                            let mut current_w = 0.0f32;
                                            for (idx, (label, tag, _)) in items.iter().enumerate() {
                                                let w = Self::measure_keycap_chip_width(
                                                    ui,
                                                    Some(idx),
                                                    label,
                                                    *tag,
                                                    KeycapSize::Medium,
                                                );
                                                if !current_row.is_empty()
                                                    && current_w + 6.0 + w > wrap_w
                                                {
                                                    rows.push(current_row);
                                                    current_row = vec![idx];
                                                    current_w = w;
                                                } else {
                                                    if !current_row.is_empty() {
                                                        current_w += 6.0;
                                                    }
                                                    current_row.push(idx);
                                                    current_w += w;
                                                }
                                            }
                                            if !current_row.is_empty() {
                                                rows.push(current_row);
                                            }

                                            let visible_clip = ui.clip_rect();
                                            ui.add_space(5.0);
                                            for (r_idx, row) in rows.iter().enumerate() {
                                                if r_idx > 0 {
                                                    ui.add_space(5.0);
                                                }
                                                ui.horizontal(|ui| {
                                                    ui.spacing_mut().item_spacing.x = 6.0;
                                                    for &idx in row {
                                                        let (label, tag, tag_color) = &items[idx];
                                                        let chip_resp = Self::render_keycap_chip(
                                                            ui,
                                                            Some(idx),
                                                            label,
                                                            *tag,
                                                            None,
                                                            *tag_color,
                                                            KeycapSize::Medium,
                                                        );

                                                        let btn_center = egui::pos2(
                                                            chip_resp.rect.right() - 2.0,
                                                            chip_resp.rect.top() + 2.0,
                                                        );
                                                        let btn_radius = 6.5f32;
                                                        let btn_rect = egui::Rect::from_center_size(
                                                            btn_center,
                                                            egui::vec2(14.0, 14.0),
                                                        );
                                                        let hover_zone = chip_resp
                                                            .rect
                                                            .union(btn_rect)
                                                            .intersect(visible_clip);
                                                        let chip_hovered = !is_editing_chip
                                                            && pointer_pos.is_some_and(|p| {
                                                                hover_zone.is_positive()
                                                                    && hover_zone.contains(p)
                                                            });

                                                        let mut remove_clicked = false;
                                                        let mut pointer_on_delete = false;
                                                        if chip_hovered {
                                                            ui.painter().rect_stroke(
                                                                chip_resp.rect,
                                                                ui.visuals()
                                                                    .widgets
                                                                    .inactive
                                                                    .corner_radius,
                                                                egui::Stroke::new(1.0, accent),
                                                                egui::StrokeKind::Inside,
                                                            );

                                                            let del_resp = ui
                                                                .interact(
                                                                    btn_rect,
                                                                    ui.make_persistent_id((
                                                                        "cap_chip_del",
                                                                        idx,
                                                                    )),
                                                                    egui::Sense::click(),
                                                                )
                                                                .on_hover_cursor(
                                                                    egui::CursorIcon::PointingHand,
                                                                )
                                                                .on_hover_text(s.btn_delete);

                                                            let del_vis =
                                                                btn_rect.intersect(visible_clip);
                                                            if del_vis.is_positive() {
                                                                next_delete_chips
                                                                    .push((idx, del_vis));
                                                            }
                                                            pointer_on_delete = del_resp.hovered()
                                                                || pointer_pos.is_some_and(|p| {
                                                                    btn_rect.contains(p)
                                                                });

                                                            let circle_fill = if del_resp.hovered()
                                                            {
                                                                accent
                                                            } else {
                                                                modal_style
                                                                    .visuals
                                                                    .widgets
                                                                    .inactive
                                                                    .bg_fill
                                                            };
                                                            let cross_color = if del_resp.hovered()
                                                            {
                                                                egui::Color32::WHITE
                                                            } else {
                                                                modal_style.visuals.text_color()
                                                            };
                                                            let painter = ui.painter();
                                                            painter.circle_filled(
                                                                btn_center,
                                                                btn_radius,
                                                                circle_fill,
                                                            );
                                                            painter.circle_stroke(
                                                                btn_center,
                                                                btn_radius,
                                                                egui::Stroke::new(1.0, accent),
                                                            );
                                                            let d = 2.6;
                                                            painter.line_segment(
                                                                [
                                                                    btn_center + egui::vec2(-d, -d),
                                                                    btn_center + egui::vec2(d, d),
                                                                ],
                                                                egui::Stroke::new(1.4, cross_color),
                                                            );
                                                            painter.line_segment(
                                                                [
                                                                    btn_center + egui::vec2(-d, d),
                                                                    btn_center + egui::vec2(d, -d),
                                                                ],
                                                                egui::Stroke::new(1.4, cross_color),
                                                            );

                                                            if del_resp.clicked() {
                                                                remove_clicked = true;
                                                                pending_remove_chip = Some(idx);
                                                            }
                                                        }

                                                        let edit_candidate = match cap
                                                            .recorded_actions
                                                            .get(idx)
                                                        {
                                                            Some(InputAction::MouseScroll {
                                                                direction,
                                                                steps,
                                                            }) => Some(
                                                                EditingActionDraft::MouseScroll {
                                                                    action_idx: idx,
                                                                    direction: *direction,
                                                                    steps: *steps,
                                                                },
                                                            ),
                                                            Some(InputAction::MouseMove {
                                                                dx,
                                                                dy,
                                                            }) => Some(
                                                                EditingActionDraft::MouseMove {
                                                                    action_idx: idx,
                                                                    dx: *dx,
                                                                    dy: *dy,
                                                                },
                                                            ),
                                                            _ => None,
                                                        };
                                                        if let Some(draft) = edit_candidate
                                                            && !remove_clicked
                                                            && !pointer_on_delete
                                                        {
                                                            let click_resp = ui
                                                                .interact(
                                                                    chip_resp.rect,
                                                                    ui.make_persistent_id((
                                                                        "cap_chip_edit",
                                                                        idx,
                                                                    )),
                                                                    egui::Sense::click(),
                                                                )
                                                                .on_hover_cursor(
                                                                    egui::CursorIcon::PointingHand,
                                                                )
                                                                .on_hover_text(
                                                                    s.capture_edit_chip_tip,
                                                                );
                                                            let vis_rect = chip_resp
                                                                .rect
                                                                .intersect(visible_clip);
                                                            if vis_rect.is_positive() {
                                                                next_editable_chips
                                                                    .push((idx, vis_rect));
                                                            }
                                                            if click_resp.clicked()
                                                                && cap.editing_action.is_none()
                                                            {
                                                                pending_open_edit =
                                                                    Some((chip_resp.rect, draft));
                                                            }
                                                        }
                                                    }
                                                });
                                            }
                                        }
                                    });
                                scroll_inner_rect = scroll_out.inner_rect;
                                has_v_scrollbar = scroll_out.content_size.y
                                    > scroll_out.inner_rect.height() + 1.0;
                            });
                            cap.editable_chip_rects = next_editable_chips;
                            cap.delete_chip_rects = next_delete_chips;
                            if let Some(rm_idx) = pending_remove_chip {
                                cap.remove_recorded_action(rm_idx);
                            } else if let Some((anchor, draft)) = pending_open_edit {
                                cap.editing_anchor_rect = Some(anchor);
                                cap.editing_action = Some(draft);
                            }
                            if has_v_scrollbar {
                                let bar_w = 24.0f32;
                                let split_x = (scroll_inner_rect.max.x - bar_w)
                                    .max(scroll_inner_rect.min.x + 20.0);
                                cap.capture_rect = Some(egui::Rect::from_min_max(
                                    card_resp.response.rect.min,
                                    egui::pos2(split_x, card_resp.response.rect.max.y),
                                ));
                                cap.scrollbar_rect = Some(egui::Rect::from_min_max(
                                    egui::pos2(split_x, card_resp.response.rect.min.y),
                                    card_resp.response.rect.max,
                                ));
                            } else {
                                cap.capture_rect = Some(card_resp.response.rect);
                                cap.scrollbar_rect = None;
                            }
                            ui.add_space(6.0);
                            ui.checkbox(&mut cap.record_mouse_move, s.checkbox_record_mouse_move);
                        }
                    }

                    let _ = CAPTURE_CARD_W;
                    ui.add_space(10.0);
                    let can_confirm = match &state {
                        CaptureModalState::Trigger(cap) => cap.captured.is_some(),
                        CaptureModalState::Actions(cap) => !cap.recorded_actions.is_empty(),
                    };
                    let can_reset = match &state {
                        CaptureModalState::Trigger(cap) => {
                            cap.captured.is_some() || cap.live_modifiers.any()
                        }
                        CaptureModalState::Actions(cap) => {
                            !cap.recorded_actions.is_empty()
                                || !cap.held_keys.is_empty()
                                || !cap.held_mouse_buttons.is_empty()
                        }
                    };

                    ui.allocate_ui(egui::vec2(CAPTURE_MODAL_W, 28.0), |ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let confirm_btn = egui::Button::new(
                                egui::RichText::new(s.btn_confirm).color(if can_confirm {
                                    egui::Color32::WHITE
                                } else {
                                    modal_style.visuals.weak_text_color()
                                }),
                            )
                            .fill(if can_confirm {
                                accent
                            } else {
                                modal_style.visuals.widgets.inactive.bg_fill
                            });
                            if ui.add_enabled(can_confirm, confirm_btn).clicked() {
                                confirm = true;
                            }
                            if ui.button(s.btn_cancel).clicked() {
                                cancel = true;
                            }
                            if ui
                                .add_enabled(can_reset, egui::Button::new(s.btn_reset))
                                .clicked()
                            {
                                reset = true;
                            }

                            if let CaptureModalState::Actions(cap) = &state
                                && (!cap.held_keys.is_empty() || !cap.held_mouse_buttons.is_empty())
                            {
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.spacing_mut().item_spacing.x = 4.0;
                                        ui.small(s.record_current_modifiers);
                                        for key in cap.held_keys.iter().take(2) {
                                            Self::render_keycap_chip(
                                                ui,
                                                None,
                                                key.as_str(),
                                                None,
                                                Some(accent),
                                                None,
                                                KeycapSize::Compact,
                                            );
                                        }
                                        for btn in cap.held_mouse_buttons.iter().take(2) {
                                            Self::render_keycap_chip(
                                                ui,
                                                None,
                                                &format!("Mouse {}", btn.as_str()),
                                                None,
                                                Some(accent),
                                                None,
                                                KeycapSize::Compact,
                                            );
                                        }
                                    },
                                );
                            }
                        });
                    });
                });
            });

        // Menu a tendina dedicato per la modifica di un chip MouseScroll / MouseMove registrato.
        if let CaptureModalState::Actions(cap) = &mut state
            && let Some(mut draft) = cap.editing_action.take()
        {
            keep_open = true;
            let screen = ctx.content_rect();
            let base_id = egui::Id::from(ctx.viewport_id());
            let backdrop_id = base_id.with("capture_chip_edit_backdrop");
            let menu_id = base_id.with("capture_chip_edit_menu");
            let backdrop_layer = egui::LayerId::new(egui::Order::Foreground, backdrop_id);
            let menu_layer = egui::LayerId::new(egui::Order::Foreground, menu_id);

            let parent_info = match &draft {
                EditingActionDraft::MouseScroll { action_idx, .. } => {
                    cap.recorded_actions.get(*action_idx).and_then(|a| match a {
                        InputAction::MouseScroll { direction, steps } => {
                            let sign = if *steps > 0 { "+" } else { "" };
                            Some((
                                *action_idx,
                                format!("Wheel {} {}{}", direction.as_str(), sign, steps),
                            ))
                        }
                        _ => None,
                    })
                }
                EditingActionDraft::MouseMove { action_idx, .. } => {
                    cap.recorded_actions.get(*action_idx).and_then(|a| match a {
                        InputAction::MouseMove { dx, dy } => {
                            Some((*action_idx, format!("Move ({dx}, {dy})")))
                        }
                        _ => None,
                    })
                }
            };

            // Velo overlay che assorbe i click fuori dal menu e ridisegna sopra di sé il chip padre.
            egui::Area::new(backdrop_id)
                .order(egui::Order::Foreground)
                .fixed_pos(screen.min)
                .show(ctx, |ui| {
                    let (_, resp) =
                        ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());
                    ui.painter()
                        .rect_filled(resp.rect, 0.0, egui::Color32::from_black_alpha(75));

                    if let (Some(anchor_rect), Some((parent_idx, parent_label))) =
                        (cap.editing_anchor_rect, parent_info)
                    {
                        let parent_bounds = egui::Rect::from_min_size(
                            anchor_rect.min,
                            egui::vec2(anchor_rect.width() + 24.0, anchor_rect.height() + 8.0),
                        );
                        let mut child = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(parent_bounds)
                                .layout(egui::Layout::left_to_right(egui::Align::Min)),
                        );
                        Self::render_keycap_chip(
                            &mut child,
                            Some(parent_idx),
                            &parent_label,
                            None,
                            Some(accent),
                            None,
                            KeycapSize::Medium,
                        );
                    }
                });

            let raw_pos = cap
                .editing_anchor_rect
                .map(|r| r.left_bottom() + egui::vec2(0.0, 4.0))
                .unwrap_or_else(|| screen.center() - egui::vec2(90.0, 50.0));
            let popup_pos = egui::pos2(
                raw_pos.x.clamp(
                    screen.left() + 8.0,
                    (screen.right() - 205.0).max(screen.left() + 8.0),
                ),
                raw_pos.y.clamp(
                    screen.top() + 8.0,
                    (screen.bottom() - 125.0).max(screen.top() + 8.0),
                ),
            );

            let mut close_edit_cancel = false;
            let mut close_edit_save = false;

            egui::Area::new(menu_id)
                .order(egui::Order::Foreground)
                .fixed_pos(popup_pos)
                .show(ctx, |ui| {
                    egui::Frame::popup(&modal_style)
                        .corner_radius(modal_style.visuals.menu_corner_radius)
                        .shadow(modal_style.visuals.popup_shadow)
                        .stroke(egui::Stroke::new(1.2, accent))
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            let (can_save_edit, content_w) = match &mut draft {
                                EditingActionDraft::MouseScroll {
                                    direction, steps, ..
                                } => {
                                    ui.strong("MouseScroll");
                                    ui.add_space(4.0);
                                    let r1 = ui.horizontal(|ui| {
                                        for &dir in MouseWheelDirection::ALL {
                                            ui.selectable_value(direction, dir, dir.as_str());
                                        }
                                    });
                                    ui.add_space(2.0);
                                    let r2 = ui.horizontal(|ui| {
                                        ui.label("steps:");
                                        ui.add(egui::DragValue::new(steps).range(
                                            -MAX_MOUSE_SCROLL_STEPS..=MAX_MOUSE_SCROLL_STEPS,
                                        ));
                                    });
                                    (
                                        *steps != 0,
                                        r1.response.rect.width().max(r2.response.rect.width()),
                                    )
                                }
                                EditingActionDraft::MouseMove { dx, dy, .. } => {
                                    ui.strong("MouseMove");
                                    ui.add_space(4.0);
                                    let r1 =
                                        ui.horizontal(|ui| {
                                            ui.label("dx:");
                                            ui.add(egui::DragValue::new(dx).range(
                                                -MAX_MOUSE_MOVE_DELTA..=MAX_MOUSE_MOVE_DELTA,
                                            ));
                                            ui.add_space(4.0);
                                            ui.label("dy:");
                                            ui.add(egui::DragValue::new(dy).range(
                                                -MAX_MOUSE_MOVE_DELTA..=MAX_MOUSE_MOVE_DELTA,
                                            ));
                                        });
                                    (*dx != 0 || *dy != 0, r1.response.rect.width())
                                }
                            };

                            ui.add_space(6.0);
                            let btn_row_w = content_w.max(125.0);
                            ui.allocate_ui_with_layout(
                                egui::vec2(btn_row_w, 22.0),
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let save_btn = egui::Button::new(
                                        egui::RichText::new(s.btn_save_short).color(
                                            if can_save_edit {
                                                egui::Color32::WHITE
                                            } else {
                                                modal_style.visuals.weak_text_color()
                                            },
                                        ),
                                    )
                                    .fill(if can_save_edit {
                                        accent
                                    } else {
                                        modal_style.visuals.widgets.inactive.bg_fill
                                    });
                                    if ui.add_enabled(can_save_edit, save_btn).clicked() {
                                        close_edit_save = true;
                                    }
                                    if ui.button(s.btn_cancel).clicked() {
                                        close_edit_cancel = true;
                                    }
                                },
                            );
                        });
                });

            ctx.move_to_top(backdrop_layer);
            ctx.set_sublayer(backdrop_layer, menu_layer);

            if close_edit_save {
                match draft {
                    EditingActionDraft::MouseScroll {
                        action_idx,
                        direction,
                        steps,
                    } if steps != 0 => {
                        if let Some(slot) = cap.recorded_actions.get_mut(action_idx) {
                            *slot = InputAction::MouseScroll { direction, steps };
                        }
                    }
                    EditingActionDraft::MouseMove { action_idx, dx, dy } if dx != 0 || dy != 0 => {
                        if let Some(slot) = cap.recorded_actions.get_mut(action_idx) {
                            *slot = InputAction::MouseMove { dx, dy };
                        }
                    }
                    _ => {}
                }
                cap.editing_anchor_rect = None;
            } else if close_edit_cancel {
                cap.editing_anchor_rect = None;
            } else {
                cap.editing_action = Some(draft);
            }
        }

        if confirm {
            match state {
                CaptureModalState::Trigger(cap) => {
                    if let (Some(binding), Some(rule)) =
                        (cap.captured, self.editor.draft.rules.get_mut(cap.rule_idx))
                    {
                        rule.trigger = Trigger::Hotkey { binding };
                    }
                }
                CaptureModalState::Actions(cap) => {
                    let actions = cap.finalize_actions(MAX_ACTIONS_PER_SEQUENCE);
                    self.editor.add_actions(cap.rule_idx, actions);
                }
            }
        } else if reset {
            match &mut state {
                CaptureModalState::Trigger(cap) => cap.reset(),
                CaptureModalState::Actions(cap) => cap.reset(),
            }
            self.capture_modal = Some(state);
        } else if !cancel && keep_open {
            self.capture_modal = Some(state);
        }
    }

    fn has_visible_update_notice(&self) -> bool {
        self.available_update.is_some() && !self.update_notice_dismissed
    }

    fn render_floating_update_notice(&mut self, ctx: &egui::Context, area_id: egui::Id) {
        if self.has_open_modal() || self.update_notice_dismissed {
            return;
        }
        let Some(release) = self.available_update.clone() else {
            return;
        };
        let lang = self.lang();
        let s = lang.s();
        let accent = ctx.style_of(ctx.theme()).visuals.selection.bg_fill;
        let installing = self.update_install_in_flight;

        let mut close_clicked = false;
        let mut changelog_clicked = false;
        let mut update_clicked = false;

        egui::Area::new(area_id)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -44.0))
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .corner_radius(ui.style().visuals.menu_corner_radius)
                    .stroke(egui::Stroke::new(1.5, accent))
                    .shadow(ui.style().visuals.popup_shadow)
                    .show(ui, |ui| {
                        ui.set_max_width(480.0);
                        ui.vertical(|ui| {
                            ui.strong(lang.fmt_update_available(&release.tag_name));
                            ui.add_space(6.0);
                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .add_enabled(!installing, egui::Button::new(s.btn_close))
                                    .clicked()
                                {
                                    close_clicked = true;
                                }
                                if ui.button(s.btn_changelog).clicked() {
                                    changelog_clicked = true;
                                }
                                let upd_label = if installing {
                                    s.btn_updating
                                } else {
                                    s.btn_update_now
                                };
                                let upd_btn = egui::Button::new(
                                    egui::RichText::new(upd_label).color(egui::Color32::WHITE),
                                )
                                .fill(accent);
                                if ui.add_enabled(!installing, upd_btn).clicked() {
                                    update_clicked = true;
                                }
                            });
                        });
                    });
            });

        if close_clicked {
            self.update_notice_dismissed = true;
        } else if changelog_clicked {
            self.open_external_url(&release.html_url);
        } else if update_clicked {
            if self.editor.is_dirty() {
                self.pending_delete = Some(PendingDeleteTarget::UpdateUnsaved(release));
            } else {
                self.start_update_download_and_apply(ctx, release);
            }
        }
    }

    fn render_floating_toast(&mut self, ctx: &egui::Context, area_id: egui::Id) {
        if let Some(toast) = &self.toast {
            if Instant::now() >= toast.expires_at {
                self.toast = None;
                return;
            }
            ctx.request_repaint_after(Duration::from_millis(250));

            let border_color = if toast.is_error {
                egui::Color32::from_rgb(220, 48, 48)
            } else {
                egui::Color32::from_rgb(46, 160, 67)
            };
            let y_offset = if self.has_visible_update_notice() {
                -118.0
            } else {
                -44.0
            };

            egui::Area::new(area_id)
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, y_offset))
                .order(egui::Order::Foreground)
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style())
                        .corner_radius(ui.style().visuals.menu_corner_radius)
                        .stroke(egui::Stroke::new(1.5, border_color))
                        .shadow(ui.style().visuals.popup_shadow)
                        .show(ui, |ui| {
                            ui.set_max_width(420.0);
                            ui.colored_label(border_color, &toast.message);
                        });
                });
        }
    }
}

impl RustBindUiApp {
    fn should_confirm_exit(&self) -> bool {
        !self.allow_close_with_dirty && self.editor.is_dirty()
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) {
            self.capture_viewport_geometry(ctx);
            self.flush_layout_to_disk();
            if self.should_confirm_exit() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.capture_modal = None;
                self.pending_delete = Some(PendingDeleteTarget::ExitUnsaved);
            }
        }
    }

    fn handle_editor_shortcuts(&mut self, ctx: &egui::Context) {
        if self.capture_modal.is_some() || self.pending_delete.is_some() {
            return;
        }
        let wants_text = ctx.egui_wants_keyboard_input();
        let (save, new_rule, search, undo, redo, delete_rule) = ctx.input_mut(|i| {
            let save = i.consume_key(egui::Modifiers::COMMAND, egui::Key::S);
            let new_rule = i.consume_key(egui::Modifiers::COMMAND, egui::Key::N);
            let search = i.consume_key(egui::Modifiers::COMMAND, egui::Key::F);
            let redo = if !wants_text {
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
                    || i.consume_key(
                        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                        egui::Key::Z,
                    )
            } else {
                false
            };
            let undo = if !wants_text {
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)
            } else {
                false
            };
            let delete_rule = if !wants_text {
                i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
            } else {
                false
            };
            (save, new_rule, search, undo, redo, delete_rule)
        });

        if save && self.editor.can_save() {
            if self.editor.save().is_ok() {
                self.request_reload(ctx);
            } else {
                self.flush_editor_notice_to_toast();
            }
        } else if new_rule && !self.editor.is_locked_invalid() {
            self.active_tab = UiTab::Rules;
            self.rules_subtab = RulesSubTab::RuleList;
            self.editor.add_rule();
        } else if search && !self.editor.is_locked_invalid() {
            self.active_tab = UiTab::Rules;
            self.rules_subtab = RulesSubTab::RuleList;
            self.focus_rule_search = true;
        } else if undo {
            if self.editor.undo() {
                self.last_seen_draft = self.editor.draft.clone();
                self.last_draft_change_at = None;
            }
        } else if redo {
            if self.editor.redo() {
                self.last_seen_draft = self.editor.draft.clone();
                self.last_draft_change_at = None;
            }
        } else if delete_rule && self.active_tab == UiTab::Rules && !self.editor.is_locked_invalid()
        {
            match self.rules_subtab {
                RulesSubTab::RuleList => {
                    if let Some(idx) = self.editor.selected_rule_idx
                        && let Some(rule) = self.editor.draft.rules.get(idx)
                    {
                        self.pending_delete = Some(PendingDeleteTarget::Rule {
                            idx,
                            name: rule.name.clone(),
                        });
                    }
                }
                RulesSubTab::Groups => {
                    if !self
                        .selected_group
                        .eq_ignore_ascii_case(&default_rule_group())
                        && self
                            .editor
                            .available_groups()
                            .iter()
                            .any(|g| g == &self.selected_group)
                    {
                        self.pending_delete = Some(PendingDeleteTarget::Group {
                            name: self.selected_group.clone(),
                        });
                    }
                }
                RulesSubTab::Duplicates => {}
            }
        }
    }

    fn update_undo_history(&mut self, ctx: &egui::Context) {
        if self.editor.is_locked_invalid() {
            self.last_seen_draft = self.editor.draft.clone();
            self.last_draft_change_at = None;
            return;
        }
        let now = Instant::now();
        if self.editor.draft != self.last_seen_draft {
            self.last_seen_draft = self.editor.draft.clone();
            self.last_draft_change_at = Some(now);
        }
        let is_busy_input = ctx.egui_wants_keyboard_input() || ctx.input(|i| i.pointer.any_down());
        if let Some(changed_at) = self.last_draft_change_at {
            if !is_busy_input || now.duration_since(changed_at) >= UNDO_DEBOUNCE_DURATION {
                self.editor.sync_history_from_draft();
                self.last_draft_change_at = None;
            } else {
                ctx.request_repaint_after(UNDO_DEBOUNCE_DURATION);
            }
        }
    }

    fn capture_viewport_geometry(&mut self, ctx: &egui::Context) {
        let mut changed = false;
        let rules_ratio = self.rules_split_ratio.clamp(0.18, 0.75);
        if (self.theme_config.rules_split_ratio - rules_ratio).abs() > 0.002 {
            self.theme_config.rules_split_ratio = rules_ratio;
            changed = true;
        }
        let groups_ratio = self.groups_split_ratio.clamp(0.18, 0.75);
        if (self.theme_config.groups_split_ratio - groups_ratio).abs() > 0.002 {
            self.theme_config.groups_split_ratio = groups_ratio;
            changed = true;
        }

        let (minimized, inner_rect, outer_rect) = ctx.input(|i| {
            let vp = i.viewport();
            (vp.minimized.unwrap_or(false), vp.inner_rect, vp.outer_rect)
        });
        if !minimized {
            if let Some(rect) = inner_rect {
                let w = rect.width();
                let h = rect.height();
                if w >= 380.0
                    && h >= 440.0
                    && ((self.theme_config.window_w - w).abs() > 1.0
                        || (self.theme_config.window_h - h).abs() > 1.0)
                {
                    self.theme_config.window_w = w;
                    self.theme_config.window_h = h;
                    changed = true;
                }
            }
            if let Some(rect) = outer_rect {
                let x = rect.min.x;
                let y = rect.min.y;
                if (-10_000.0..=20_000.0).contains(&x) && (-10_000.0..=20_000.0).contains(&y) {
                    let prev_x = self.theme_config.window_x;
                    let prev_y = self.theme_config.window_y;
                    let moved = match (prev_x, prev_y) {
                        (Some(px), Some(py)) => (px - x).abs() > 1.0 || (py - y).abs() > 1.0,
                        _ => true,
                    };
                    if moved {
                        self.theme_config.window_x = Some(x);
                        self.theme_config.window_y = Some(y);
                        changed = true;
                    }
                }
            }
        }

        if changed {
            self.layout_dirty_since = Some(Instant::now());
        }
    }

    fn flush_layout_to_disk(&mut self) {
        if self.layout_dirty_since.take().is_some()
            && let Some(path) = &self.theme_path
        {
            let _ = self.theme_config.save_to_file(path);
        }
    }

    fn render_header_undo_redo(&mut self, ui: &mut egui::Ui, reversed_for_rtl: bool) {
        let s = self.lang().s();
        let can_undo = self.editor.can_undo();
        let can_redo = self.editor.can_redo();
        let undo_tip = format!("{} (Ctrl+Z)", s.btn_undo);
        let redo_tip = format!("{} (Ctrl+Y)", s.btn_redo);

        let order = if reversed_for_rtl {
            [false, true]
        } else {
            [true, false]
        };
        for is_undo in order {
            let (enabled, label, tip) = if is_undo {
                (can_undo, "<", undo_tip.as_str())
            } else {
                (can_redo, ">", redo_tip.as_str())
            };
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new(label).min_size(egui::vec2(24.0, 0.0)),
                )
                .on_hover_text(tip)
                .clicked()
            {
                let applied = if is_undo {
                    self.editor.undo()
                } else {
                    self.editor.redo()
                };
                if applied {
                    self.last_seen_draft = self.editor.draft.clone();
                    self.last_draft_change_at = None;
                }
            }
        }
    }

    fn sync_and_persist_layout(&mut self, ctx: &egui::Context) {
        self.capture_viewport_geometry(ctx);
        if let Some(since) = self.layout_dirty_since
            && since.elapsed() >= LAYOUT_SAVE_DEBOUNCE
            && !ctx.input(|i| i.pointer.any_down())
        {
            self.flush_layout_to_disk();
        }
    }

    /// Indica se almeno un modale o overlay di cattura/conferma e' attualmente aperto.
    fn has_open_modal(&self) -> bool {
        self.pending_delete.is_some() || self.capture_modal.is_some()
    }

    fn is_toml_config_path(path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
    }

    fn clear_transient_ui_selections(&mut self) {
        self.selected_available_rule_ids.clear();
        self.selected_member_rule_ids.clear();
        self.duplicate_keep_by_cluster.clear();
        self.duplicate_merge_by_cluster.clear();
    }

    fn apply_imported_candidate(&mut self, ctx: &egui::Context, candidate: ImportCandidate) {
        match self.editor.apply_import(candidate) {
            Ok(reload_needed) => {
                self.clear_transient_ui_selections();
                self.flush_editor_notice_to_toast();
                if reload_needed {
                    self.request_reload(ctx);
                }
            }
            Err(err) => self.show_toast(true, err),
        }
    }

    /// Importa un file `.toml` trascinato sulla finestra: prima prova come tema custom, poi come configurazione.
    fn import_dropped_toml_file(&mut self, ctx: &egui::Context, src_path: &Path) {
        let s = self.lang().s();
        if !Self::is_toml_config_path(src_path) {
            self.show_toast(true, s.toast_drop_only_toml);
            return;
        }
        if let Ok(raw) = std::fs::read_to_string(src_path) {
            if let Ok(imported) = CustomTheme::from_toml_str(&raw) {
                self.theme_config.custom_theme = imported;
                self.theme_config.theme_id = ThemeId::Custom;
                self.apply_and_save_theme(ctx);
                self.show_toast(false, s.toast_theme_imported);
                return;
            }
        }
        self.import_config_file_from_path(ctx, src_path);
    }

    /// Importa un file `.toml` di configurazione da percorso (usato sia dal dialog sia dal drag & drop).
    fn import_config_file_from_path(&mut self, ctx: &egui::Context, src_path: &Path) {
        let s = self.lang().s();
        if self.editor.is_locked_invalid() {
            self.show_toast(true, s.settings_blocked_invalid);
            return;
        }
        if !Self::is_toml_config_path(src_path) {
            self.show_toast(true, s.toast_drop_only_toml);
            return;
        }
        match self.editor.prepare_import_from_path(src_path) {
            Ok(candidate) => {
                if candidate.already_exists {
                    self.pending_delete = Some(PendingDeleteTarget::ImportOverwrite(candidate));
                } else {
                    self.apply_imported_candidate(ctx, candidate);
                }
            }
            Err(err) => self.show_toast(true, err),
        }
    }

    fn render_file_drop_overlay(&self, ctx: &egui::Context, hovered: &[egui::HoveredFile]) {
        let s = self.lang().s();
        let screen = ctx.content_rect();
        let overlay_id = egui::Id::from(ctx.viewport_id()).with("config_file_drop_overlay");

        let is_invalid_drop = hovered.len() > 1
            || hovered
                .first()
                .and_then(|f| f.path.as_deref())
                .is_some_and(|p| !Self::is_toml_config_path(p));

        let status_text = if hovered.len() > 1 {
            s.toast_drop_single_file
        } else if is_invalid_drop {
            s.toast_drop_only_toml
        } else {
            s.drop_overlay_import
        };

        let file_name_line = if hovered.len() == 1 {
            hovered[0]
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .map(str::to_string)
        } else {
            None
        };

        egui::Area::new(overlay_id)
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .interactable(false)
            .show(ctx, |ui| {
                let (_, rect_resp) = ui.allocate_exact_size(screen.size(), egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect_resp.rect, 0.0, egui::Color32::from_black_alpha(70));

                let style = ui.style().clone();
                let border_color = if is_invalid_drop {
                    egui::Color32::from_rgb(220, 48, 48)
                } else {
                    style.visuals.widgets.noninteractive.bg_stroke.color
                };
                let text_color = if is_invalid_drop {
                    egui::Color32::from_rgb(220, 48, 48)
                } else {
                    style.visuals.strong_text_color()
                };

                let box_w = (screen.width() - 48.0).clamp(180.0, 300.0);
                let box_h = if file_name_line.is_some() { 92.0 } else { 72.0 };
                let card_rect =
                    egui::Rect::from_center_size(screen.center(), egui::vec2(box_w, box_h));
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(card_rect)
                        .layout(egui::Layout::top_down(egui::Align::Center)),
                );
                egui::Frame::window(&style)
                    .fill(style.visuals.window_fill)
                    .corner_radius(style.visuals.menu_corner_radius)
                    .stroke(egui::Stroke::new(1.0, border_color))
                    .shadow(style.visuals.popup_shadow)
                    .inner_margin(egui::Margin::symmetric(14, 12))
                    .show(&mut child, |ui| {
                        ui.set_max_width(box_w - 28.0);
                        ui.vertical_centered(|ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(status_text).strong().color(text_color),
                                )
                                .wrap(),
                            );
                            if let Some(name) = &file_name_line {
                                ui.add_space(4.0);
                                ui.add(
                                    egui::Label::new(egui::RichText::new(name).small()).truncate(),
                                );
                            }
                        });
                    });
            });
    }

    fn handle_config_file_drag_and_drop(&mut self, ctx: &egui::Context) {
        if self.has_open_modal() {
            return;
        }

        let (hovered, dropped) =
            ctx.input(|i| (i.raw.hovered_files.clone(), i.raw.dropped_files.clone()));

        if !hovered.is_empty() {
            self.render_file_drop_overlay(ctx, &hovered);
        }

        if !dropped.is_empty() {
            if dropped.len() > 1 {
                let s = self.lang().s();
                self.show_toast(true, s.toast_drop_single_file);
            } else {
                let path = dropped[0].path().to_path_buf();
                if path.as_os_str().is_empty() {
                    let s = self.lang().s();
                    self.show_toast(true, s.toast_drop_only_toml);
                } else {
                    self.import_dropped_toml_file(ctx, &path);
                }
            }
        }
    }
}

impl eframe::App for RustBindUiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let root_ctx = ui.ctx().clone();
        self.handle_close_request(&root_ctx);
        self.handle_editor_shortcuts(&root_ctx);

        if !self.initial_fetch_started {
            self.initial_fetch_started = true;
            let ctx = ui.ctx().clone();
            self.request_status(&ctx);
        }
        if !self.startup_update_check_started {
            self.startup_update_check_started = true;
            if self.theme_config.auto_check_updates {
                let ctx = ui.ctx().clone();
                self.start_update_check(&ctx, false);
            }
        }

        let is_focused = ui.ctx().input(|i| i.focused);
        if is_focused && !self.was_focused {
            self.refresh_autostart_state();
            if self.theme_config.auto_check_updates
                && self.available_update.is_none()
                && let Some(cached) = default_update_cache_path()
                    .and_then(|p| read_cached_update_for(&p, env!("CARGO_PKG_VERSION")))
            {
                self.latest_changelog_url = Some(cached.html_url.clone());
                self.available_update = Some(cached);
                self.update_notice_dismissed = false;
            }
            if !self.ipc_in_flight {
                let ctx = ui.ctx().clone();
                self.request_status(&ctx);
            }
        }
        self.was_focused = is_focused;

        self.poll_ipc_results();
        self.poll_update_results(&root_ctx);
        self.flush_editor_notice_to_toast();

        if !self.ipc_in_flight && self.last_poll.elapsed() >= IPC_POLL_INTERVAL {
            let ctx = ui.ctx().clone();
            self.request_status_silent(&ctx);
        }
        ui.ctx().request_repaint_after(IPC_POLL_INTERVAL);

        egui::Panel::top("editor_tabs").show(ui, |ui| {
            let lang = self.lang();
            let s = lang.s();
            ui.add_space(4.0);
            let compact = ui.available_width() < 560.0;
            let security_label = if self.editor.is_locked_invalid() {
                s.tab_security_alert
            } else {
                UiTab::Security.label(lang)
            };

            let prev_tab = self.active_tab;
            if compact {
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(
                        &mut self.active_tab,
                        UiTab::Rules,
                        UiTab::Rules.label(lang),
                    );
                    ui.selectable_value(
                        &mut self.active_tab,
                        UiTab::Settings,
                        UiTab::Settings.label(lang),
                    );
                    ui.selectable_value(&mut self.active_tab, UiTab::Security, security_label);
                });
                ui.horizontal_wrapped(|ui| {
                    self.render_header_undo_redo(ui, false);
                    ui.separator();
                    self.render_header_status_badge(ui);
                    self.render_header_daemon_menu(ui);
                });
            } else {
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.active_tab,
                        UiTab::Rules,
                        UiTab::Rules.label(lang),
                    );
                    ui.selectable_value(
                        &mut self.active_tab,
                        UiTab::Settings,
                        UiTab::Settings.label(lang),
                    );
                    ui.selectable_value(&mut self.active_tab, UiTab::Security, security_label);

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        self.render_header_daemon_menu(ui);
                        ui.separator();
                        self.render_header_status_badge(ui);
                        ui.separator();
                        self.render_header_undo_redo(ui, true);
                    });
                });
            }
            if self.active_tab != prev_tab {
                self.rules_subtab = RulesSubTab::RuleList;
                self.settings_subtab = SettingsSubTab::General;
                self.theme_editor_open = false;
            }
            ui.add_space(2.0);
        });

        egui::Panel::bottom("editor_footer").show(ui, |ui| {
            ui.add_space(4.0);
            self.render_footer_validation_and_save(ui);
            ui.add_space(4.0);
        });

        egui::CentralPanel::default().show(ui, |ui| match self.active_tab {
            UiTab::Rules => self.render_rules_tab(ui),
            UiTab::Settings => self.render_settings_tab(ui),
            UiTab::Security => self.render_security_tab(ui),
        });

        self.update_undo_history(&root_ctx);
        self.sync_and_persist_layout(&root_ctx);

        let toast_id = ui.make_persistent_id("toast_notice_area");
        let update_notice_id = ui.make_persistent_id("update_notice_area");
        let ctx = ui.ctx().clone();
        self.handle_config_file_drag_and_drop(&ctx);
        self.render_delete_confirmation_modal(&ctx);
        self.render_capture_modal(&ctx);
        self.render_floating_update_notice(&ctx, update_notice_id);
        self.render_floating_toast(&ctx, toast_id);
    }
}

fn main() {
    if let Some(exit_code) = apply_staged_update_from_cli_if_requested() {
        std::process::exit(exit_code);
    }

    // Controllo single-instance UI prima di aprire il backend grafico:
    // se già aperta, porta in primo piano la finestra esistente ed esce con 0.
    let _ui_guard = match SingleInstanceGuard::acquire_ui() {
        Ok(guard) => guard,
        Err(_) => {
            let _ = focus_window_by_title(UI_WINDOW_TITLE);
            return;
        }
    };

    cleanup_stale_update_artifacts();

    // Avvio simultaneo del daemon nella tray se non già in esecuzione (percorso fisso da current_exe()).
    let _ = launch_daemon_if_not_running();

    let startup_theme = ConfigManager::default_app_data_dir()
        .ok()
        .map(|d| d.join(UI_PREFERENCES_FILENAME))
        .map(|p| ThemeConfig::load_from_file(&p))
        .unwrap_or_default();

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([startup_theme.window_w, startup_theme.window_h])
        .with_min_inner_size([380.0, 440.0])
        .with_drag_and_drop(true)
        .with_resizable(true);
    if let (Some(x), Some(y)) = (startup_theme.window_x, startup_theme.window_y) {
        viewport = viewport.with_position([x, y]);
    }
    if let Ok(icon) = eframe::icon_data::from_png_bytes(BRAND_ICON_PNG) {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let _ = eframe::run_native(
        UI_WINDOW_TITLE,
        options,
        Box::new(|cc| {
            let app = RustBindUiApp::new();
            let theme_cfg = app.theme_config;
            let radii = theme_cfg.active_radii(CARD_CORNER_RADIUS);
            cc.egui_ctx.data_mut(|d| {
                d.insert_temp(
                    egui::Id::NULL.with("rustbind_modal_corner_radius"),
                    radii.modals,
                );
            });
            cc.egui_ctx.set_theme(if theme_cfg.effective_is_dark() {
                egui::Theme::Dark
            } else {
                egui::Theme::Light
            });
            cc.egui_ctx.all_styles_mut(|style| {
                apply_theme_to_style(style, &theme_cfg, CARD_CORNER_RADIUS);
            });
            Ok(Box::new(app))
        }),
    );
}

/// Formatta una singola azione in modo compatto per la preview su una riga nelle card regole.
fn format_compact_action(action: &InputAction) -> String {
    match action {
        InputAction::KeyClick { key } => key.as_str().to_string(),
        InputAction::KeyDown { key } => format!("{} Down", key.as_str()),
        InputAction::KeyUp { key } => format!("{} Up", key.as_str()),
        InputAction::SendText { text } => {
            let clean: String = text
                .chars()
                .map(|c| match c {
                    '\r' | '\n' | '\t' => ' ',
                    other => other,
                })
                .take(16)
                .collect();
            let ellipsis = if text.chars().count() > 16 { "…" } else { "" };
            format!("\"{clean}{ellipsis}\"")
        }
        InputAction::Delay { milliseconds } => format!("{milliseconds}ms"),
        InputAction::MouseClick { button } => format!("Mouse {}", button.as_str()),
        InputAction::MouseDown { button } => format!("Mouse {} Down", button.as_str()),
        InputAction::MouseUp { button } => format!("Mouse {} Up", button.as_str()),
        InputAction::MouseScroll { direction, steps } => {
            let sign = if *steps > 0 { "+" } else { "" };
            format!("Wheel {} {}{}", direction.as_str(), sign, steps)
        }
        InputAction::MouseMove { dx, dy } => format!("Move ({dx}, {dy})"),
    }
}

/// Formatta la preview compatta `<trigger>: <elenco azioni>` su singola riga per le card regole.
fn format_rule_card_preview(rule: &Rule) -> String {
    let trigger_str = match &rule.trigger {
        Trigger::Hotkey { binding } => binding.to_string(),
    };
    if rule.actions.is_empty() {
        return format!("{trigger_str}: --");
    }
    let actions_joined: Vec<String> = rule.actions.iter().map(format_compact_action).collect();
    format!("{trigger_str}: {}", actions_joined.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_accent_color_applied_to_style() {
        assert_eq!(RUST_ACCENT_COLOR, egui::Color32::from_rgb(183, 65, 14));

        let mut dark_style = egui::Style {
            visuals: egui::Visuals::dark(),
            ..Default::default()
        };
        apply_rust_accent_style(&mut dark_style);
        assert_eq!(dark_style.visuals.selection.bg_fill, RUST_ACCENT_COLOR);
        assert_eq!(dark_style.visuals.hyperlink_color, RUST_ACCENT_COLOR);
        assert_eq!(
            dark_style.visuals.text_cursor.stroke.color,
            RUST_ACCENT_COLOR
        );
    }

    #[test]
    fn test_exit_confirmation_required_only_when_dirty_and_not_allowed() {
        let mut app = RustBindUiApp::new();
        app.editor = EditorState::from_path(None, None);
        assert!(!app.should_confirm_exit());

        app.editor.draft.settings.max_events_per_second = app
            .editor
            .draft
            .settings
            .max_events_per_second
            .saturating_sub(1)
            .max(1);
        if app.editor.draft == app.editor.saved_baseline {
            app.editor.draft.settings.max_events_per_second += 1;
        }
        assert!(app.editor.is_dirty());
        assert!(app.should_confirm_exit());

        app.allow_close_with_dirty = true;
        assert!(!app.should_confirm_exit());

        app.allow_close_with_dirty = false;
        app.editor.discard_changes();
        assert!(!app.should_confirm_exit());
    }

    #[test]
    fn test_format_rule_card_preview_single_line_and_compact() {
        let mut app = RustBindUiApp::new();
        app.editor.add_rule();
        let rule = &mut app.editor.draft.rules[0];
        assert_eq!(rule.effective_groups(), vec!["default".to_string()]);
        rule.actions = vec![
            InputAction::KeyDown {
                key: VirtualKey::KeyA,
            },
            InputAction::MouseClick {
                button: MouseButton::Left,
            },
            InputAction::SendText {
                text: "ciao\nmondo molto lungo oltre sedici".to_string(),
            },
            InputAction::Delay { milliseconds: 25 },
            InputAction::MouseScroll {
                direction: MouseWheelDirection::Vertical,
                steps: 2,
            },
            InputAction::MouseMove { dx: 10, dy: -5 },
        ];
        let preview = format_rule_card_preview(rule);
        assert!(!preview.contains('\n'));
        assert!(preview.contains(
            ": A Down, Mouse Left, \"ciao mondo molto…\", 25ms, Wheel Vertical +2, Move (10, -5)"
        ));

        rule.actions.clear();
        assert!(format_rule_card_preview(rule).ends_with(": --"));
    }

    #[test]
    fn test_file_drop_allowed_only_without_open_modals_and_for_toml() {
        let mut app = RustBindUiApp::new();
        assert!(!app.has_open_modal());

        app.pending_delete = Some(PendingDeleteTarget::ExitUnsaved);
        assert!(app.has_open_modal());
        app.pending_delete = None;
        assert!(!app.has_open_modal());

        app.capture_modal = Some(CaptureModalState::Trigger(TriggerCaptureState::new(0)));
        assert!(app.has_open_modal());
        app.capture_modal = None;
        assert!(!app.has_open_modal());

        assert!(RustBindUiApp::is_toml_config_path(Path::new(
            r"C:\tmp\gaming.toml"
        )));
        assert!(RustBindUiApp::is_toml_config_path(Path::new("TEST.TOML")));
        assert!(!RustBindUiApp::is_toml_config_path(Path::new("notes.txt")));
    }
}
