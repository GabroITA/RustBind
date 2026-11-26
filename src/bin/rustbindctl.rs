use rustbind::ipc_protocol::{
    IPC_RELOAD_ACCEPTED, IPC_RESUME_REJECTED_ERROR_STATE, IPC_RESUME_SUCCESS_OR_ACTIVE,
    IPC_STATE_ACTIVE, IPC_STATE_ERROR, IPC_STATE_PAUSED, IPC_SUSPEND_REJECTED_ERROR_STATE,
    IPC_SUSPEND_SUCCESS_OR_PAUSED, IpcCommand, IpcResponse,
};
use rustbind::platform::windows::ipc_client::{IpcClientError, send_command};

const EXIT_OK: i32 = 0;
const EXIT_INVALID_ARGS: i32 = 1;
const EXIT_DAEMON_UNAVAILABLE: i32 = 3;
const EXIT_IPC_OR_ERROR_STATE: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CliAction {
    Help,
    Command(IpcCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOutcome {
    exit_code: i32,
    stdout: Option<String>,
    stderr: Option<String>,
}

fn usage_text() -> &'static str {
    "Uso: rustbindctl <comando>\n\nComandi disponibili:\n  status      Interroga lo stato del daemon e il numero di regole attive\n  suspend     Sospende cautelativamente l'esecuzione del daemon\n  resume      Riprende l'esecuzione del daemon dallo stato in pausa\n  reload      Ricarica la configurazione TOML nel daemon in esecuzione\n  -h, --help  Mostra questo messaggio di aiuto"
}

fn parse_cli_args<I, S>(args: I) -> Result<CliAction, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut iter = args.into_iter();
    let _program = iter.next();

    let Some(first) = iter.next() else {
        return Err("Errore: comando mancante.".to_string());
    };

    if iter.next().is_some() {
        return Err("Errore: troppi argomenti specificati.".to_string());
    }

    match first.as_ref() {
        "-h" | "--help" => Ok(CliAction::Help),
        "status" => Ok(CliAction::Command(IpcCommand::GetStatus)),
        "suspend" => Ok(CliAction::Command(IpcCommand::Suspend)),
        "resume" => Ok(CliAction::Command(IpcCommand::Resume)),
        "reload" => Ok(CliAction::Command(IpcCommand::ReloadConfig)),
        other => Err(format!("Errore: comando non riconosciuto '{}'.", other)),
    }
}

fn map_client_error(err: &IpcClientError) -> CliOutcome {
    match err {
        IpcClientError::DaemonNotRunning | IpcClientError::PipeBusyTimeout => CliOutcome {
            exit_code: EXIT_DAEMON_UNAVAILABLE,
            stdout: None,
            stderr: Some(format!("Errore connessione daemon: {}", err)),
        },
        IpcClientError::IoTimeout | IpcClientError::Protocol(_) | IpcClientError::OsError(_) => {
            CliOutcome {
                exit_code: EXIT_IPC_OR_ERROR_STATE,
                stdout: None,
                stderr: Some(format!("Errore comunicazione IPC: {}", err)),
            }
        }
    }
}

fn format_ipc_outcome(cmd: IpcCommand, result: Result<IpcResponse, IpcClientError>) -> CliOutcome {
    let resp = match result {
        Ok(r) => r,
        Err(err) => return map_client_error(&err),
    };

    match (cmd, resp) {
        (
            IpcCommand::GetStatus,
            IpcResponse::Status {
                state,
                active_rules,
            },
        ) => {
            let state_label = match state {
                IPC_STATE_ACTIVE => "active",
                IPC_STATE_PAUSED => "paused",
                IPC_STATE_ERROR => "error",
                other => {
                    return CliOutcome {
                        exit_code: EXIT_IPC_OR_ERROR_STATE,
                        stdout: None,
                        stderr: Some(format!(
                            "Errore: byte di stato IPC inatteso (0x{:02X}).",
                            other
                        )),
                    };
                }
            };
            CliOutcome {
                exit_code: EXIT_OK,
                stdout: Some(format!(
                    "status={} active_rules={}",
                    state_label, active_rules
                )),
                stderr: None,
            }
        }
        (IpcCommand::Suspend, IpcResponse::Suspend { outcome }) => match outcome {
            IPC_SUSPEND_SUCCESS_OR_PAUSED => CliOutcome {
                exit_code: EXIT_OK,
                stdout: Some("suspended".to_string()),
                stderr: None,
            },
            IPC_SUSPEND_REJECTED_ERROR_STATE => CliOutcome {
                exit_code: EXIT_IPC_OR_ERROR_STATE,
                stdout: None,
                stderr: Some(
                    "Errore: il daemon e in stato Error, nessuna modifica applicata.".to_string(),
                ),
            },
            other => CliOutcome {
                exit_code: EXIT_IPC_OR_ERROR_STATE,
                stdout: None,
                stderr: Some(format!(
                    "Errore: esito Suspend inatteso dal daemon (0x{:02X}).",
                    other
                )),
            },
        },
        (IpcCommand::ReloadConfig, IpcResponse::Reload { outcome }) => match outcome {
            IPC_RELOAD_ACCEPTED => CliOutcome {
                exit_code: EXIT_OK,
                stdout: Some("reloaded".to_string()),
                stderr: None,
            },
            other => CliOutcome {
                exit_code: EXIT_IPC_OR_ERROR_STATE,
                stdout: None,
                stderr: Some(format!(
                    "Errore: esito Reload inatteso dal daemon (0x{:02X}).",
                    other
                )),
            },
        },
        (IpcCommand::Resume, IpcResponse::Resume { outcome }) => match outcome {
            IPC_RESUME_SUCCESS_OR_ACTIVE => CliOutcome {
                exit_code: EXIT_OK,
                stdout: Some("resumed".to_string()),
                stderr: None,
            },
            IPC_RESUME_REJECTED_ERROR_STATE => CliOutcome {
                exit_code: EXIT_IPC_OR_ERROR_STATE,
                stdout: None,
                stderr: Some(
                    "Errore: il daemon e in stato Error, impossibile riprendere (fail-closed)."
                        .to_string(),
                ),
            },
            other => CliOutcome {
                exit_code: EXIT_IPC_OR_ERROR_STATE,
                stdout: None,
                stderr: Some(format!(
                    "Errore: esito Resume inatteso dal daemon (0x{:02X}).",
                    other
                )),
            },
        },
        (_, IpcResponse::Error) => CliOutcome {
            exit_code: EXIT_IPC_OR_ERROR_STATE,
            stdout: None,
            stderr: Some("Errore: il daemon ha restituito errore di protocollo IPC.".to_string()),
        },
        _ => CliOutcome {
            exit_code: EXIT_IPC_OR_ERROR_STATE,
            stdout: None,
            stderr: Some("Errore: risposta IPC inattesa per il comando richiesto.".to_string()),
        },
    }
}

fn main() {
    let action = match parse_cli_args(std::env::args()) {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{}\n\n{}", msg, usage_text());
            std::process::exit(EXIT_INVALID_ARGS);
        }
    };

    let cmd = match action {
        CliAction::Help => {
            println!("{}", usage_text());
            std::process::exit(EXIT_OK);
        }
        CliAction::Command(c) => c,
    };

    let outcome = format_ipc_outcome(cmd, send_command(cmd));
    if let Some(out) = outcome.stdout {
        println!("{}", out);
    }
    if let Some(err) = outcome.stderr {
        eprintln!("{}", err);
    }
    std::process::exit(outcome.exit_code);
}

#[cfg(test)]
mod tests {
    use rustbind::ipc_protocol::IpcProtocolError;

    use super::*;

    #[test]
    fn test_parse_cli_args_valid() {
        assert_eq!(
            parse_cli_args(["rustbindctl", "status"]),
            Ok(CliAction::Command(IpcCommand::GetStatus))
        );
        assert_eq!(
            parse_cli_args(["rustbindctl", "suspend"]),
            Ok(CliAction::Command(IpcCommand::Suspend))
        );
        assert_eq!(
            parse_cli_args(["rustbindctl", "resume"]),
            Ok(CliAction::Command(IpcCommand::Resume))
        );
        assert_eq!(
            parse_cli_args(["rustbindctl", "reload"]),
            Ok(CliAction::Command(IpcCommand::ReloadConfig))
        );
        assert_eq!(parse_cli_args(["rustbindctl", "-h"]), Ok(CliAction::Help));
        assert_eq!(
            parse_cli_args(["rustbindctl", "--help"]),
            Ok(CliAction::Help)
        );
    }

    #[test]
    fn test_parse_cli_args_invalid_returns_err_for_exit_1() {
        assert!(parse_cli_args(["rustbindctl"]).is_err());
        assert!(parse_cli_args(["rustbindctl", "unknown"]).is_err());
        assert!(parse_cli_args(["rustbindctl", "--unknown"]).is_err());
        assert!(parse_cli_args(["rustbindctl", "status", "extra"]).is_err());
        assert!(parse_cli_args(["rustbindctl", "-h", "extra"]).is_err());
    }

    #[test]
    fn test_format_ipc_outcome_status_exit_0() {
        let out_active = format_ipc_outcome(
            IpcCommand::GetStatus,
            Ok(IpcResponse::Status {
                state: IPC_STATE_ACTIVE,
                active_rules: 3,
            }),
        );
        assert_eq!(out_active.exit_code, EXIT_OK);
        assert_eq!(
            out_active.stdout.as_deref(),
            Some("status=active active_rules=3")
        );
        assert!(out_active.stderr.is_none());

        let out_paused = format_ipc_outcome(
            IpcCommand::GetStatus,
            Ok(IpcResponse::Status {
                state: IPC_STATE_PAUSED,
                active_rules: 0,
            }),
        );
        assert_eq!(out_paused.exit_code, EXIT_OK);
        assert_eq!(
            out_paused.stdout.as_deref(),
            Some("status=paused active_rules=0")
        );

        let out_err = format_ipc_outcome(
            IpcCommand::GetStatus,
            Ok(IpcResponse::Status {
                state: IPC_STATE_ERROR,
                active_rules: 0,
            }),
        );
        assert_eq!(out_err.exit_code, EXIT_OK);
        assert_eq!(
            out_err.stdout.as_deref(),
            Some("status=error active_rules=0")
        );
    }

    #[test]
    fn test_format_ipc_outcome_suspend_exit_0_and_4() {
        let ok_res = format_ipc_outcome(
            IpcCommand::Suspend,
            Ok(IpcResponse::Suspend {
                outcome: IPC_SUSPEND_SUCCESS_OR_PAUSED,
            }),
        );
        assert_eq!(ok_res.exit_code, EXIT_OK);
        assert_eq!(ok_res.stdout.as_deref(), Some("suspended"));
        assert!(ok_res.stderr.is_none());

        let err_state_res = format_ipc_outcome(
            IpcCommand::Suspend,
            Ok(IpcResponse::Suspend {
                outcome: IPC_SUSPEND_REJECTED_ERROR_STATE,
            }),
        );
        assert_eq!(err_state_res.exit_code, EXIT_IPC_OR_ERROR_STATE);
        assert!(err_state_res.stdout.is_none());
        assert!(err_state_res.stderr.is_some());
    }

    #[test]
    fn test_format_ipc_outcome_reload_and_resume_exit_0_and_4() {
        let reload_ok = format_ipc_outcome(
            IpcCommand::ReloadConfig,
            Ok(IpcResponse::Reload {
                outcome: IPC_RELOAD_ACCEPTED,
            }),
        );
        assert_eq!(reload_ok.exit_code, EXIT_OK);
        assert_eq!(reload_ok.stdout.as_deref(), Some("reloaded"));
        assert!(reload_ok.stderr.is_none());

        let reload_bad = format_ipc_outcome(
            IpcCommand::ReloadConfig,
            Ok(IpcResponse::Reload { outcome: 0xEE }),
        );
        assert_eq!(reload_bad.exit_code, EXIT_IPC_OR_ERROR_STATE);
        assert!(reload_bad.stderr.is_some());

        let resume_ok = format_ipc_outcome(
            IpcCommand::Resume,
            Ok(IpcResponse::Resume {
                outcome: IPC_RESUME_SUCCESS_OR_ACTIVE,
            }),
        );
        assert_eq!(resume_ok.exit_code, EXIT_OK);
        assert_eq!(resume_ok.stdout.as_deref(), Some("resumed"));
        assert!(resume_ok.stderr.is_none());

        let resume_err = format_ipc_outcome(
            IpcCommand::Resume,
            Ok(IpcResponse::Resume {
                outcome: IPC_RESUME_REJECTED_ERROR_STATE,
            }),
        );
        assert_eq!(resume_err.exit_code, EXIT_IPC_OR_ERROR_STATE);
        assert!(resume_err.stdout.is_none());
        assert!(resume_err.stderr.is_some());
    }

    #[test]
    fn test_format_ipc_outcome_daemon_unavailable_exit_3() {
        for err in [
            IpcClientError::DaemonNotRunning,
            IpcClientError::PipeBusyTimeout,
        ] {
            let outcome = format_ipc_outcome(IpcCommand::GetStatus, Err(err));
            assert_eq!(outcome.exit_code, EXIT_DAEMON_UNAVAILABLE);
            assert!(outcome.stdout.is_none());
            assert!(outcome.stderr.is_some());
        }
    }

    #[test]
    fn test_format_ipc_outcome_protocol_timeout_os_and_unexpected_exit_4() {
        let errs = [
            Err(IpcClientError::IoTimeout),
            Err(IpcClientError::Protocol(IpcProtocolError::TooLarge(99))),
            Err(IpcClientError::OsError("errore win32".into())),
            Ok(IpcResponse::Error),
            Ok(IpcResponse::Suspend {
                outcome: IPC_SUSPEND_SUCCESS_OR_PAUSED,
            }),
            Ok(IpcResponse::Status {
                state: 0xEE,
                active_rules: 1,
            }),
        ];

        for res in errs {
            let outcome = format_ipc_outcome(IpcCommand::GetStatus, res);
            assert_eq!(outcome.exit_code, EXIT_IPC_OR_ERROR_STATE);
            assert!(outcome.stdout.is_none());
            assert!(outcome.stderr.is_some());
        }
    }
}
