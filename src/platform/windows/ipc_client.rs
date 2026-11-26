use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_IO_PENDING, ERROR_NO_DATA, ERROR_PIPE_BUSY,
    GetLastError, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_OVERLAPPED, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_NONE,
    OPEN_EXISTING, ReadFile, WriteFile,
};
use windows::Win32::System::IO::GetOverlappedResult;
use windows::Win32::System::Pipes::WaitNamedPipeW;
use windows::Win32::System::Threading::WaitForSingleObject;
use windows::core::PCWSTR;

use crate::ipc_protocol::{
    DEFAULT_IPC_IO_TIMEOUT, IpcCommand, IpcProtocolError, IpcResponse, decode_ipc_response,
    encode_ipc_command, get_pipe_name, parse_response_header,
};
use crate::platform::windows::handles::SafeHandle;
use crate::platform::windows::ipc_server::{PendingIo, get_current_process_user_sid};

/// Errori specifici del client IPC Named Pipe.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IpcClientError {
    #[error("Daemon RustBind non in esecuzione (named pipe non trovata)")]
    DaemonNotRunning,
    #[error("Timeout attesa disponibilità named pipe (ERROR_PIPE_BUSY)")]
    PipeBusyTimeout,
    #[error("Timeout operazione I/O su named pipe")]
    IoTimeout,
    #[error("Errore protocollo IPC: {0}")]
    Protocol(#[from] IpcProtocolError),
    #[error("Errore del sottosistema Windows: {0}")]
    OsError(String),
}

/// Invia un comando IPC al daemon RustBind dell'utente corrente usando la named pipe canonica.
pub fn send_command(cmd: IpcCommand) -> Result<IpcResponse, IpcClientError> {
    let sid = get_current_process_user_sid().map_err(|e| IpcClientError::OsError(e.to_string()))?;
    let pipe_name = get_pipe_name(&sid);
    send_command_to_pipe(&pipe_name, cmd, DEFAULT_IPC_IO_TIMEOUT)
}

/// Invia un comando IPC a una named pipe esplicita con timeout globale (usato anche nei test).
pub fn send_command_to_pipe(
    pipe_name: &str,
    cmd: IpcCommand,
    timeout: Duration,
) -> Result<IpcResponse, IpcClientError> {
    let deadline = Instant::now() + timeout;
    let wide_name: Vec<u16> = pipe_name.encode_utf16().chain(std::iter::once(0)).collect();

    let pipe = connect_overlapped_pipe(&wide_name, deadline, timeout)?;

    let req = encode_ipc_command(cmd);
    write_all_overlapped(pipe.raw(), &req, deadline)?;

    let mut header = [0u8; 4];
    read_exact_overlapped(pipe.raw(), &mut header, deadline)?;

    let payload_len = parse_response_header(&header)?;
    let mut payload = vec![0u8; payload_len];
    read_exact_overlapped(pipe.raw(), &mut payload, deadline)?;

    let mut frame = Vec::with_capacity(4 + payload_len);
    frame.extend_from_slice(&header);
    frame.extend_from_slice(&payload);

    let response = decode_ipc_response(&frame)?;
    Ok(response)
}

/// Apre una connessione overlapped verso la named pipe, gestendo `ERROR_PIPE_BUSY` con `WaitNamedPipeW`.
fn connect_overlapped_pipe(
    wide_name: &[u16],
    deadline: Instant,
    timeout: Duration,
) -> Result<SafeHandle, IpcClientError> {
    loop {
        // SAFETY: `wide_name` è una stringa UTF-16 terminata da NUL valida per tutta la chiamata Win32.
        let res = unsafe {
            CreateFileW(
                PCWSTR(wide_name.as_ptr()),
                FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                None,
            )
        };

        match res {
            Ok(handle) => return Ok(SafeHandle::new(handle)),
            Err(err) => {
                // SAFETY: Recupero del codice di errore Win32 dell'ultimo fallimento su questo thread.
                let code = unsafe { GetLastError() };
                if code == ERROR_FILE_NOT_FOUND {
                    return Err(IpcClientError::DaemonNotRunning);
                }
                if code != ERROR_PIPE_BUSY {
                    return Err(IpcClientError::OsError(format!(
                        "CreateFileW fallito ({}): {:?}",
                        err, code
                    )));
                }

                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(IpcClientError::PipeBusyTimeout);
                }

                let wait_ms = remaining.as_millis().min(u32::MAX as u128).max(1) as u32;
                // SAFETY: `wide_name` è UTF-16 NUL-terminated; attesa limitata a `wait_ms` millisecondi.
                let waited = unsafe { WaitNamedPipeW(PCWSTR(wide_name.as_ptr()), wait_ms) };
                if !waited.as_bool() {
                    // SAFETY: Verifica del motivo del fallimento di WaitNamedPipeW su questo thread.
                    let wait_err = unsafe { GetLastError() };
                    if wait_err == ERROR_FILE_NOT_FOUND {
                        return Err(IpcClientError::DaemonNotRunning);
                    }
                    let post_remaining = deadline.saturating_duration_since(Instant::now());
                    if post_remaining.is_zero() {
                        return Err(IpcClientError::PipeBusyTimeout);
                    }
                    let backoff = Duration::from_millis(5).min(post_remaining);
                    if timeout > Duration::ZERO {
                        std::thread::sleep(backoff);
                    }
                }
            }
        }
    }
}

/// Calcola i millisecondi residui entro la `deadline`, restituendo `IoTimeout` se già scaduta.
fn remaining_ms(deadline: Instant) -> Result<u32, IpcClientError> {
    let rem = deadline.saturating_duration_since(Instant::now());
    if rem.is_zero() {
        return Err(IpcClientError::IoTimeout);
    }
    Ok(rem.as_millis().min(u32::MAX as u128).max(1) as u32)
}

/// Scrive l'intero buffer `data` in modalità overlapped tramite `PendingIo` con timeout complessivo.
fn write_all_overlapped(
    pipe: HANDLE,
    data: &[u8],
    deadline: Instant,
) -> Result<(), IpcClientError> {
    let mut total_written = 0;

    while total_written < data.len() {
        let timeout_ms = remaining_ms(deadline)?;
        let mut pending_io = PendingIo::new_write(pipe, &data[total_written..])?;
        let (io_buf, io_ov) = pending_io.write_targets();

        // SAFETY: `io_buf` e `io_ov` appartengono a `PendingIo` su heap e restano validi finché
        // l'operazione non è completata o annullata/orfana nel Drop.
        let write_res = unsafe { WriteFile(pipe, Some(io_buf), None, Some(io_ov)) };

        if write_res.is_err() {
            // SAFETY: Lettura codice errore Win32 immediato della WriteFile sul thread corrente.
            let err = unsafe { GetLastError() };
            if err == ERROR_BROKEN_PIPE || err == ERROR_NO_DATA {
                pending_io.mark_completed();
                return Err(IpcClientError::Protocol(
                    IpcProtocolError::ClientDisconnected,
                ));
            }
            if err != ERROR_IO_PENDING {
                pending_io.mark_completed();
                return Err(IpcClientError::OsError(format!(
                    "WriteFile fallito: {:?}",
                    err
                )));
            }
        }

        // SAFETY: Attesa limitata sull'evento dedicato di PendingIo.
        let wait_res = unsafe { WaitForSingleObject(pending_io.event_raw(), timeout_ms) };
        if wait_res == WAIT_OBJECT_0 {
            let mut transferred = 0u32;
            // SAFETY: Evento segnalato; verifica non bloccante (bWait = false) dell'esito I/O.
            let res = unsafe {
                GetOverlappedResult(pipe, pending_io.overlapped_mut(), &mut transferred, false)
            };
            pending_io.mark_completed();
            if res.is_err() {
                // SAFETY: Lettura codice errore Win32 di GetOverlappedResult.
                let last = unsafe { GetLastError() };
                if last == ERROR_BROKEN_PIPE || last == ERROR_NO_DATA {
                    return Err(IpcClientError::Protocol(
                        IpcProtocolError::ClientDisconnected,
                    ));
                }
                return Err(IpcClientError::OsError(format!(
                    "GetOverlappedResult fallito su scrittura: {:?}",
                    last
                )));
            }
            if transferred == 0 {
                return Err(IpcClientError::Protocol(
                    IpcProtocolError::ClientDisconnected,
                ));
            }
            total_written += transferred as usize;
        } else if wait_res == WAIT_TIMEOUT {
            // Drop di `pending_io` cancella e attende (o esegue leak se il kernel non rilascia).
            return Err(IpcClientError::IoTimeout);
        } else {
            return Err(IpcClientError::OsError(
                "WaitForSingleObject fallito su scrittura IPC".into(),
            ));
        }
    }

    Ok(())
}

/// Legge esattamente `buf.len()` byte in modalità overlapped tramite `PendingIo` con timeout complessivo.
fn read_exact_overlapped(
    pipe: HANDLE,
    buf: &mut [u8],
    deadline: Instant,
) -> Result<(), IpcClientError> {
    let mut total_read = 0;

    while total_read < buf.len() {
        let timeout_ms = remaining_ms(deadline)?;
        let needed = buf.len() - total_read;
        let mut pending_io = PendingIo::new_read(pipe, needed)?;
        let (io_buf, io_ov) = pending_io.read_targets();

        // SAFETY: `io_buf` e `io_ov` appartengono a `PendingIo` su heap e restano validi finché
        // l'operazione non è completata o annullata/orfana nel Drop.
        let read_res = unsafe { ReadFile(pipe, Some(io_buf), None, Some(io_ov)) };

        if read_res.is_err() {
            // SAFETY: Lettura codice errore Win32 immediato della ReadFile sul thread corrente.
            let err = unsafe { GetLastError() };
            if err == ERROR_BROKEN_PIPE || err == ERROR_NO_DATA {
                pending_io.mark_completed();
                return Err(IpcClientError::Protocol(
                    IpcProtocolError::ClientDisconnected,
                ));
            }
            if err != ERROR_IO_PENDING {
                pending_io.mark_completed();
                return Err(IpcClientError::OsError(format!(
                    "ReadFile fallito: {:?}",
                    err
                )));
            }
        }

        // SAFETY: Attesa limitata sull'evento dedicato di PendingIo.
        let wait_res = unsafe { WaitForSingleObject(pending_io.event_raw(), timeout_ms) };
        if wait_res == WAIT_OBJECT_0 {
            let mut transferred = 0u32;
            // SAFETY: Evento segnalato; verifica non bloccante (bWait = false) dell'esito I/O.
            let res = unsafe {
                GetOverlappedResult(pipe, pending_io.overlapped_mut(), &mut transferred, false)
            };
            pending_io.mark_completed();
            if res.is_err() {
                // SAFETY: Lettura codice errore Win32 di GetOverlappedResult.
                let last = unsafe { GetLastError() };
                if last == ERROR_BROKEN_PIPE || last == ERROR_NO_DATA {
                    return Err(IpcClientError::Protocol(
                        IpcProtocolError::ClientDisconnected,
                    ));
                }
                return Err(IpcClientError::OsError(format!(
                    "GetOverlappedResult fallito su lettura: {:?}",
                    last
                )));
            }
            if transferred == 0 {
                return Err(IpcClientError::Protocol(
                    IpcProtocolError::ClientDisconnected,
                ));
            }
            buf[total_read..total_read + transferred as usize]
                .copy_from_slice(&pending_io.buffer()[..transferred as usize]);
            total_read += transferred as usize;
        } else if wait_res == WAIT_TIMEOUT {
            // Drop di `pending_io` cancella e attende (o esegue leak se il kernel non rilascia).
            return Err(IpcClientError::IoTimeout);
        } else {
            return Err(IpcClientError::OsError(
                "WaitForSingleObject fallito su lettura IPC".into(),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::engine::state::ServiceState;
    use crate::ipc_protocol::{
        IPC_STATE_ACTIVE, IPC_SUSPEND_REJECTED_ERROR_STATE, IPC_SUSPEND_SUCCESS_OR_PAUSED,
    };
    use crate::platform::windows::ipc_server::{
        IpcServer, IpcServerAction, generate_test_pipe_name, get_current_process_user_sid,
    };
    use crate::platform::windows::message_loop::WakeHandle;
    use crate::platform::windows::worker_threads::WorkerThreads;

    #[test]
    #[ignore = "Richiede comunicazione IPC su Named Pipe Windows reale"]
    fn test_real_pipe_client_status_suspend_and_error_state() {
        let mut workers = WorkerThreads::new();
        let wake_handle = WakeHandle::new_null();
        let status = Arc::new(Mutex::new(ServiceState::Active));
        let active_rules = Arc::new(AtomicUsize::new(2));

        let sid = get_current_process_user_sid().expect("SID utente");
        let pipe_name = generate_test_pipe_name();

        let server = IpcServer::start_with_pipe_name(
            &pipe_name,
            &sid,
            wake_handle,
            status.clone(),
            active_rules,
            &mut workers,
        )
        .expect("Avvio server IPC test");

        // 1. GetStatus su servizio Active
        let resp_status =
            send_command_to_pipe(&pipe_name, IpcCommand::GetStatus, DEFAULT_IPC_IO_TIMEOUT)
                .expect("GetStatus riuscito");
        assert_eq!(
            resp_status,
            IpcResponse::Status {
                state: IPC_STATE_ACTIVE,
                active_rules: 2
            }
        );

        // 2. Suspend su servizio Active -> esito 0x01 + azione IpcServerAction::Suspend accodata
        let resp_suspend =
            send_command_to_pipe(&pipe_name, IpcCommand::Suspend, DEFAULT_IPC_IO_TIMEOUT)
                .expect("Suspend su Active riuscito");
        assert_eq!(
            resp_suspend,
            IpcResponse::Suspend {
                outcome: IPC_SUSPEND_SUCCESS_OR_PAUSED
            }
        );
        assert_eq!(server.pop_action(), Some(IpcServerAction::Suspend));

        // 3. Suspend su servizio in stato Error -> esito 0x02 (nessuna modifica)
        {
            let mut guard = status.lock().unwrap_or_else(|p| p.into_inner());
            *guard = ServiceState::Error("Configurazione non valida".into());
        }
        let resp_suspend_err =
            send_command_to_pipe(&pipe_name, IpcCommand::Suspend, DEFAULT_IPC_IO_TIMEOUT)
                .expect("Suspend su Error riuscito");
        assert_eq!(
            resp_suspend_err,
            IpcResponse::Suspend {
                outcome: IPC_SUSPEND_REJECTED_ERROR_STATE
            }
        );
        assert_eq!(server.pop_action(), None);

        server.stop();
        assert!(
            workers.join_all(Duration::from_secs(1)),
            "Il worker thread del server IPC doveva terminare entro 1 secondo"
        );
    }
}
