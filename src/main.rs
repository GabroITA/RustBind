#![cfg_attr(not(test), windows_subsystem = "windows")]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use rustbind::config::manager::ConfigManager;
use rustbind::engine::clock::SystemClock;
use rustbind::engine::runner::EngineRunner;
use rustbind::engine::state::{
    DaemonRuntimeDiagnostics, default_runtime_diagnostics_path, remove_runtime_diagnostics,
    write_runtime_diagnostics,
};
use rustbind::logging;
use rustbind::platform::windows::autostart::{
    close_window_by_title_best_effort, is_autostart_enabled, set_autostart_enabled,
};
use rustbind::platform::windows::foreground::{ForegroundWatcher, query_foreground_window};
use rustbind::platform::windows::hotkey::WindowsHotkeyManager;
use rustbind::platform::windows::input::WindowsInputSender;
use rustbind::platform::windows::ipc_server::{
    IPC_RESUME_SUCCESS_OR_ACTIVE, IPC_SUSPEND_SUCCESS_OR_PAUSED, IpcServer, IpcServerAction,
    apply_ipc_resume, apply_ipc_suspend,
};
use rustbind::platform::windows::message_loop::{
    MessageLoop, MessageLoopEvent, clear_pending_tray_events, set_menu_active,
};
use rustbind::platform::windows::single_instance::{
    EXIT_CODE_ALREADY_RUNNING, SingleInstanceGuard,
};
use rustbind::platform::windows::tray::{TrayEvent, TrayManager, TrayMenuAction};
use rustbind::updater::{
    UpdateCheckOutcome, default_update_cache_path, read_auto_check_updates_from_app_data,
    read_cached_update_for,
};

fn main() {
    // 1. Controllo Single-Instance tramite Named Mutex (Local\RustBind_Daemon_SingleInstance)
    // Controllato tassativamente PRIMA dell'inizializzazione del log: se un'altra istanza è attiva,
    // esce con codice 2 senza creare né scrivere il file di log.
    let _single_instance = match SingleInstanceGuard::acquire() {
        Ok(guard) => guard,
        Err(_) => {
            std::process::exit(EXIT_CODE_ALREADY_RUNNING);
        }
    };

    // 2. Inizializzazione del logging su file in %LOCALAPPDATA%\RustBind\logs\rustbind.log
    // Il flag opzionale --debug-log abilita il livello DEBUG per tracciare i callback tray.
    let debug_mode = std::env::args().any(|arg| arg == "--debug-log");
    let _ = logging::init_file_logging(debug_mode);

    // 3. Installazione del panic hook sicuro (privacy-first, NIM_DELETE best-effort, terminazione)
    logging::install_panic_hook();

    tracing::info!("Avvio RustBind Daemon v{}", env!("CARGO_PKG_VERSION"));

    // 3. Creazione della finestra top-level invisibile e dell'unico message pump Win32
    let mut message_loop = match MessageLoop::new() {
        Ok(ml) => ml,
        Err(err) => {
            tracing::error!(error = %err, "Inizializzazione message loop fallita");
            return;
        }
    };

    // 4. Caricamento iniziale della configurazione TOML (comportamento Fail-Closed)
    // Se config.toml è assente: avvio con stato Attivo, 0 regole, nessun file creato su disco.
    // Se config.toml è presente ma non valido: errore tipizzato (fail-closed -> stato Error).
    let config_result = match ConfigManager::default_active_config_path() {
        Ok(path) => {
            if !path.exists() {
                tracing::info!(
                    path = %path.display(),
                    "File di configurazione non trovato: avvio con stato Attivo e nessuna regola configurata"
                );
                Ok(rustbind::config::schema::Config::default())
            } else {
                ConfigManager::load_from_file(&path)
            }
        }
        Err(err) => Err(err),
    };

    // 5. Inizializzazione dei gestori di piattaforma
    let hotkey_manager = WindowsHotkeyManager::new(message_loop.hwnd());
    let input_sender = WindowsInputSender::new();
    let clock = std::sync::Arc::new(SystemClock::new());

    // 6. Inizializzazione EngineRunner (registra PRIMA il kill-switch, poi le regole)
    let mut runner = EngineRunner::new(hotkey_manager, input_sender, clock, &config_result);
    let mut foreground_watcher: Option<ForegroundWatcher> = None;
    sync_foreground_watcher(&mut runner, &mut foreground_watcher, message_loop.hwnd());
    persist_runtime_diagnostics(&runner);

    // 7. Inizializzazione Tray Icon (tre forme geometriche diverse, tooltip e menu nativo)
    let mut tray = match TrayManager::new(message_loop.hwnd()) {
        Ok(tm) => tm,
        Err(err) => {
            tracing::error!(error = %err, "Inizializzazione tray icon fallita");
            return;
        }
    };

    if let Err(err) = tray.add_icon(&runner.state()) {
        tracing::error!(error = %err, "Aggiunta iniziale tray icon fallita");
    }

    // 7b. Inizializzazione Server IPC Named Pipe (connessione locale DACL-restricted, worker thread)
    let ipc_server = match IpcServer::start(
        message_loop.wake_handle(),
        runner.state_arc(),
        runner.active_rules_arc(),
        message_loop.worker_threads_mut(),
    ) {
        Ok(srv) => {
            tracing::info!("Server IPC in ascolto");
            Some(srv)
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                "Inizializzazione server IPC fallita (il daemon prosegue senza IPC)"
            );
            None
        }
    };

    tracing::info!(
        state = %runner.state(),
        active_rules = runner.active_rules_count(),
        "RustBind Daemon operativo nel message loop"
    );

    let update_available_flag = Arc::new(AtomicBool::new(
        read_auto_check_updates_from_app_data()
            && default_update_cache_path()
                .and_then(|p| read_cached_update_for(&p, env!("CARGO_PKG_VERSION")))
                .is_some(),
    ));
    let update_stop_flag = Arc::new(AtomicBool::new(false));
    {
        let upd_flag = Arc::clone(&update_available_flag);
        let stop_flag = Arc::clone(&update_stop_flag);
        thread::spawn(move || {
            // Attesa breve post-avvio per non ritardare il boot del daemon.
            for _ in 0..20 {
                if stop_flag.load(Ordering::Relaxed) {
                    return;
                }
                thread::sleep(Duration::from_millis(250));
            }
            loop {
                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }
                if read_auto_check_updates_from_app_data() {
                    match rustbind::platform::windows::check_for_updates_blocking(env!(
                        "CARGO_PKG_VERSION"
                    )) {
                        Ok(UpdateCheckOutcome::UpdateAvailable(_)) => {
                            upd_flag.store(true, Ordering::Relaxed);
                        }
                        Ok(UpdateCheckOutcome::UpToDate { .. }) => {
                            upd_flag.store(false, Ordering::Relaxed);
                        }
                        Err(err) => {
                            tracing::debug!(error = %err, "Controllo automatico aggiornamenti non riuscito");
                        }
                    }
                }
                // Ricontrolla ogni 6 ore in modo interrompibile.
                for _ in 0..(6 * 60 * 60) {
                    if stop_flag.load(Ordering::Relaxed) {
                        return;
                    }
                    thread::sleep(Duration::from_secs(1));
                }
            }
        });
    }

    // 8. Loop di smistamento messaggi Win32
    loop {
        let event = message_loop.pump_one_event();

        match event {
            MessageLoopEvent::Hotkey(id) => {
                runner.handle_hotkey(id);
                if let Err(err) = tray.update_state(&runner.state()) {
                    tracing::warn!(error = %err, "Aggiornamento tray icon fallito dopo evento hotkey");
                }
            }
            MessageLoopEvent::Tray(tray_event) => {
                match tray_event {
                    TrayEvent::ContextMenu => {
                        // Il menu si apre solo con WM_CONTEXTMENU (tasto destro, Shift+F10 o tasto Menu).
                        // Imposta il flag di menu attivo e scarta gli eventi tray accumulati alla chiusura.
                        set_menu_active(true);
                        let autostart_enabled = is_autostart_enabled().unwrap_or_else(|err| {
                            tracing::warn!(
                                error = %err,
                                "Lettura stato avvio automatico fallita all'apertura del menu tray"
                            );
                            false
                        });
                        let show_update = read_auto_check_updates_from_app_data()
                            && (update_available_flag.load(Ordering::Relaxed)
                                || default_update_cache_path()
                                    .and_then(|p| {
                                        read_cached_update_for(&p, env!("CARGO_PKG_VERSION"))
                                    })
                                    .is_some());
                        let group_items: Vec<(String, bool)> = if runner.state().is_error() {
                            Vec::new()
                        } else if let Some(cfg) = runner.active_config() {
                            cfg.available_groups()
                                .into_iter()
                                .map(|g| {
                                    let active = cfg.is_group_active(&g);
                                    (g, active)
                                })
                                .collect()
                        } else {
                            Vec::new()
                        };
                        let action = tray.show_context_menu(
                            &runner.state(),
                            autostart_enabled,
                            show_update,
                            &group_items,
                        );
                        set_menu_active(false);
                        clear_pending_tray_events();

                        if let Some(action) = action {
                            match action {
                                TrayMenuAction::TogglePause => {
                                    if runner.state().is_active() {
                                        runner.pause();
                                    } else if runner.state().is_paused() {
                                        runner.resume();
                                    }
                                    if let Err(err) = tray.update_state(&runner.state()) {
                                        tracing::warn!(error = %err, "Aggiornamento tray icon fallito dopo toggle pausa");
                                    }
                                }
                                TrayMenuAction::ToggleGroup(group_name) => {
                                    if let Some(mut next_cfg) = runner.active_config().cloned() {
                                        let was_paused = runner.state().is_paused();
                                        let next_active = !next_cfg.is_group_active(&group_name);
                                        let _ =
                                            next_cfg.toggle_group_active(&group_name, next_active);
                                        if let Ok(path) =
                                            ConfigManager::default_active_config_path()
                                            && path.exists()
                                        {
                                            if let Err(err) = ConfigManager::save_to_file_safe(
                                                &next_cfg, &path, false,
                                            ) {
                                                tracing::warn!(
                                                    error = %err,
                                                    "Salvataggio gruppi attivi dalla tray fallito"
                                                );
                                            }
                                            reload_config_and_update_tray(
                                                &mut runner,
                                                &mut tray,
                                                &mut foreground_watcher,
                                                message_loop.hwnd(),
                                            );
                                        } else {
                                            runner.apply_config(&Ok(next_cfg));
                                            sync_foreground_watcher(
                                                &mut runner,
                                                &mut foreground_watcher,
                                                message_loop.hwnd(),
                                            );
                                            persist_runtime_diagnostics(&runner);
                                        }
                                        if was_paused && runner.state().is_active() {
                                            runner.pause();
                                        }
                                        if let Err(err) = tray.update_state(&runner.state()) {
                                            tracing::warn!(
                                                error = %err,
                                                "Aggiornamento tray icon fallito dopo toggle gruppo"
                                            );
                                        }
                                    }
                                }
                                TrayMenuAction::OpenConfig | TrayMenuAction::OpenUpdate => {
                                    launch_ui_companion();
                                }
                                TrayMenuAction::ReloadConfig => {
                                    tracing::info!(
                                        "Ricaricamento configurazione su richiesta utente"
                                    );
                                    reload_config_and_update_tray(
                                        &mut runner,
                                        &mut tray,
                                        &mut foreground_watcher,
                                        message_loop.hwnd(),
                                    );
                                }
                                TrayMenuAction::ToggleAutostart => {
                                    let current = is_autostart_enabled().unwrap_or(false);
                                    let next = !current;
                                    match set_autostart_enabled(next) {
                                        Ok(()) => {
                                            tracing::info!(
                                                enabled = next,
                                                "Stato avvio automatico utente (HKCU\\...\\Run) aggiornato dalla tray"
                                            );
                                        }
                                        Err(err) => {
                                            tracing::warn!(
                                                enabled = next,
                                                error = %err,
                                                "Aggiornamento avvio automatico utente fallito"
                                            );
                                        }
                                    }
                                }
                                TrayMenuAction::Exit => {
                                    tracing::info!("Richiesta di chiusura dal menu tray");
                                    break;
                                }
                            }
                        }
                    }
                    TrayEvent::DoubleClick => {
                        // Doppio click sinistro: può unicamente sospendere (se già sospeso o in errore non fa nulla).
                        // La ripresa è possibile esclusivamente tramite il menu contestuale.
                        if runner.state().is_active() {
                            runner.pause();
                            if let Err(err) = tray.update_state(&runner.state()) {
                                tracing::warn!(error = %err, "Aggiornamento tray icon fallito dopo doppio click");
                            }
                        }
                    }
                }
            }
            MessageLoopEvent::TaskbarCreated => {
                tracing::info!("Explorer riavviato (TaskbarCreated): ripristino dell'icona tray");
                if let Err(err) = tray.on_taskbar_created(&runner.state()) {
                    tracing::warn!(error = %err, "Ripristino tray icon fallito su TaskbarCreated");
                }
            }
            MessageLoopEvent::Wakeup => {
                // Risveglio asincrono (es. comandi dal server IPC)
                if let Some(srv) = &ipc_server {
                    while let Some(action) = srv.pop_action() {
                        match action {
                            IpcServerAction::Suspend => {
                                let (_, outcome) = apply_ipc_suspend(&runner.state());
                                if outcome == IPC_SUSPEND_SUCCESS_OR_PAUSED
                                    && runner.state().is_active()
                                {
                                    runner.pause();
                                    if let Err(err) = tray.update_state(&runner.state()) {
                                        tracing::warn!(
                                            error = %err,
                                            "Aggiornamento tray icon fallito dopo comando IPC Suspend"
                                        );
                                    }
                                    tracing::info!(
                                        "Comando IPC 'Suspend' applicato: servizio impostato in Pausa"
                                    );
                                }
                            }
                            IpcServerAction::ReloadConfig => {
                                tracing::info!(
                                    "Ricaricamento configurazione su richiesta IPC ReloadConfig"
                                );
                                reload_config_and_update_tray(
                                    &mut runner,
                                    &mut tray,
                                    &mut foreground_watcher,
                                    message_loop.hwnd(),
                                );
                            }
                            IpcServerAction::Resume => {
                                let (_, outcome) = apply_ipc_resume(&runner.state());
                                if outcome == IPC_RESUME_SUCCESS_OR_ACTIVE
                                    && runner.state()
                                        == rustbind::engine::state::ServiceState::Paused
                                {
                                    runner.resume();
                                    if let Err(err) = tray.update_state(&runner.state()) {
                                        tracing::warn!(
                                            error = %err,
                                            "Aggiornamento tray icon fallito dopo comando IPC Resume"
                                        );
                                    }
                                    tracing::info!(
                                        "Comando IPC 'Resume' applicato: servizio impostato Attivo"
                                    );
                                }
                            }
                        }
                    }
                }
            }
            MessageLoopEvent::ForegroundChanged => {
                if runner.has_contextual_rules() {
                    runner.on_foreground_changed(query_foreground_window());
                    persist_runtime_diagnostics(&runner);
                }
            }
            MessageLoopEvent::Quit => {
                tracing::info!("Ricevuto segnale di terminazione loop");
                break;
            }
        }
    }

    // 9. Procedura di uscita pulita:
    // - Arresta il server IPC (segnalando stop_event ai worker thread)
    // - Imposta il flag atomico di annullamento per il thread di esecuzione input
    // - Attende che il thread di esecuzione completi o esca con timeout (max 2 secondi)
    // - Rimuove la tray icon (NIM_DELETE)
    // - Drop di runner deregistra tutte le hotkey
    // - Drop di message_loop distrugge la finestra dopo che tutti i worker sono terminati (WorkerThreads)
    // - Drop di _single_instance rilascia il Named Mutex
    tracing::info!("Avvio procedura di arresto del daemon...");
    update_stop_flag.store(true, Ordering::Relaxed);
    if let Some(srv) = &ipc_server {
        srv.stop();
    }
    runner.cancel_flag().store(true, Ordering::SeqCst);

    let shutdown_deadline = Instant::now() + Duration::from_secs(2);
    while runner.is_running() && Instant::now() < shutdown_deadline {
        thread::sleep(Duration::from_millis(10));
    }

    if let Err(err) = tray.remove_icon() {
        tracing::warn!(error = %err, "Rimozione tray icon fallita all'arresto");
    }
    remove_runtime_diagnostics(&default_runtime_diagnostics_path());
    if close_window_by_title_best_effort("RustBind") {
        tracing::info!("Inviato WM_CLOSE alla finestra UI compagno all'arresto del daemon");
    }
    tracing::info!("RustBind Daemon terminato regolarmente.");
}

fn persist_runtime_diagnostics<H, C>(runner: &EngineRunner<H, C>)
where
    H: rustbind::platform::traits::HotkeyManager,
    C: rustbind::engine::clock::Clock,
{
    let diag = DaemonRuntimeDiagnostics {
        pid: std::process::id(),
        resume_hotkey_failed: runner.resume_hotkey_failed(),
        skipped_rules: runner.skipped_rules(),
    };
    if let Err(err) = write_runtime_diagnostics(&default_runtime_diagnostics_path(), &diag) {
        tracing::debug!(
            error = %err,
            "Scrittura diagnostica runtime del daemon non riuscita"
        );
    }
}

fn sync_foreground_watcher<H, C>(
    runner: &mut EngineRunner<H, C>,
    watcher: &mut Option<ForegroundWatcher>,
    hwnd: windows::Win32::Foundation::HWND,
) where
    H: rustbind::platform::traits::HotkeyManager,
    C: rustbind::engine::clock::Clock,
{
    if runner.has_contextual_rules() {
        if watcher.is_none() {
            *watcher = Some(ForegroundWatcher::new(hwnd));
        }
        runner.on_foreground_changed(query_foreground_window());
    } else {
        *watcher = None;
    }
}

/// Ricarica la configurazione TOML dal percorso canonico, la applica al runner (fail-closed)
/// e aggiorna lo stato della tray icon e i log informativi risultanti.
fn reload_config_and_update_tray<H, C>(
    runner: &mut EngineRunner<H, C>,
    tray: &mut TrayManager,
    foreground_watcher: &mut Option<ForegroundWatcher>,
    hwnd: windows::Win32::Foundation::HWND,
) where
    H: rustbind::platform::traits::HotkeyManager,
    C: rustbind::engine::clock::Clock,
{
    let new_config_result = match ConfigManager::default_active_config_path() {
        Ok(path) => {
            if !path.exists() {
                tracing::info!(
                    path = %path.display(),
                    "File di configurazione non trovato: nessuna regola attiva"
                );
                Ok(rustbind::config::schema::Config::default())
            } else {
                ConfigManager::load_from_file(&path)
            }
        }
        Err(err) => Err(err),
    };
    runner.apply_config(&new_config_result);
    sync_foreground_watcher(runner, foreground_watcher, hwnd);
    persist_runtime_diagnostics(runner);
    if let Err(err) = tray.update_state(&runner.state()) {
        tracing::warn!(error = %err, "Aggiornamento tray icon fallito dopo ricarica configurazione");
    }

    match runner.state() {
        rustbind::engine::state::ServiceState::Active => {
            tracing::info!(
                state = "Attivo",
                active_rules = runner.active_rules_count(),
                "Ricarica configurazione completata: servizio attivo"
            );
        }
        rustbind::engine::state::ServiceState::Paused => {
            tracing::info!(
                state = "In Pausa",
                active_rules = runner.active_rules_count(),
                "Ricarica configurazione completata: servizio in pausa"
            );
        }
        rustbind::engine::state::ServiceState::Error(_) => {
            tracing::info!(
                state = "Errore",
                "Ricarica configurazione completata: servizio in stato di errore (nessuna regola attiva)"
            );
        }
    }
}

/// Avvia il processo UI compagno (`rustbind-ui.exe`) dalla stessa directory dell'eseguibile daemon.
///
/// Il percorso è fisso e determinato esclusivamente da `std::env::current_exe()` (mai da file di
/// configurazione, variabile `PATH` o shell esterna).
fn launch_ui_companion() {
    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(err) => {
            tracing::warn!(error = %err, "Impossibile determinare current_exe() per avviare rustbind-ui.exe");
            return;
        }
    };

    let Some(ui_path) =
        rustbind::platform::windows::resolve_sibling_exe_path(&exe_path, "rustbind-ui.exe")
    else {
        tracing::warn!("Eseguibile UI compagno (rustbind-ui.exe) non trovato accanto al daemon");
        return;
    };

    match std::process::Command::new(&ui_path).spawn() {
        Ok(_) => {
            tracing::info!("Avviata UI di configurazione (rustbind-ui.exe)");
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                "Avvio UI di configurazione (rustbind-ui.exe) fallito"
            );
        }
    }
}
