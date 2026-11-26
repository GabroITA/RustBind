pub mod autostart;
pub mod conversion;
pub mod file_dialog;
pub mod foreground;
pub mod handles;
pub mod hotkey;
pub mod input;
pub mod ipc_client;
pub mod ipc_server;
pub mod message_loop;
pub mod single_instance;
pub mod tray;
pub mod updater;
pub mod worker_threads;

pub use autostart::{
    close_window_by_title_best_effort, is_autostart_enabled, launch_daemon_if_not_running,
    open_directory_in_explorer, resolve_sibling_exe_path, set_autostart_enabled,
};
pub use file_dialog::{pick_exe_file, pick_toml_export_file, pick_toml_import_file};
pub use foreground::{
    ForegroundWatcher, InstalledAppEntry, list_installed_applications, query_foreground_window,
};
pub use handles::{SafeHandle, SafeHicon, SafeHwnd};
pub use hotkey::WindowsHotkeyManager;
pub use input::WindowsInputSender;
pub use ipc_client::{IpcClientError, send_command, send_command_to_pipe};
pub use ipc_server::{
    IpcCommand, IpcProtocolError, IpcResponse, IpcServer, IpcServerAction, apply_ipc_resume,
    apply_ipc_suspend,
};
pub use message_loop::{MessageLoop, MessageLoopEvent, WakeHandle};
pub use single_instance::{EXIT_CODE_ALREADY_RUNNING, SingleInstanceGuard, focus_window_by_title};
pub use tray::{TrayManager, TrayMenuAction};
pub use updater::{
    apply_staged_update_from_cli_if_requested, check_for_updates_blocking,
    cleanup_stale_update_artifacts, download_stage_and_launch_updater, open_https_url,
};
pub use worker_threads::WorkerThreads;
