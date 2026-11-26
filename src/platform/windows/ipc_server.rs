use std::collections::VecDeque;
use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_NO_DATA,
    ERROR_NOT_FOUND, ERROR_OPERATION_ABORTED, ERROR_PIPE_CONNECTED, GetLastError, HANDLE, HLOCAL,
    LocalFree, WAIT_EVENT, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, INFINITE, OpenProcessToken, SetEvent, WaitForMultipleObjects,
    WaitForSingleObject,
};
use windows::core::{PCWSTR, PWSTR};

use crate::engine::state::ServiceState;
pub use crate::ipc_protocol::{
    DEFAULT_IPC_IO_TIMEOUT, IPC_CMD_GET_STATUS, IPC_CMD_RELOAD_CONFIG, IPC_CMD_RESUME,
    IPC_CMD_SUSPEND, IPC_RELOAD_ACCEPTED, IPC_RESP_PROTOCOL_ERROR, IPC_RESP_STATUS_SUCCESS,
    IPC_RESUME_REJECTED_ERROR_STATE, IPC_RESUME_SUCCESS_OR_ACTIVE, IPC_STATE_ACTIVE,
    IPC_STATE_ERROR, IPC_STATE_PAUSED, IPC_SUSPEND_REJECTED_ERROR_STATE,
    IPC_SUSPEND_SUCCESS_OR_PAUSED, IpcCommand, IpcProtocolError, IpcResponse, MAX_IPC_FRAME_SIZE,
    decode_ipc_response, encode_ipc_response, get_pipe_name, parse_frame_header, parse_ipc_frame,
    parse_ipc_payload,
};
use crate::platform::traits::PlatformError;
use crate::platform::windows::handles::SafeHandle;
use crate::platform::windows::message_loop::WakeHandle;
use crate::platform::windows::worker_threads::WorkerThreads;

/// Azioni generate dal server IPC da trasmettere al thread del message loop Win32.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcServerAction {
    Suspend,
    ReloadConfig,
    Resume,
}

/// Applica la logica di transizione di stato per il comando IPC Suspend (funzione pura).
///
/// Regole:
/// - Se `Active`: transiziona a `Paused`, esito `0x01`.
/// - Se `Paused`: rimane `Paused`, esito `0x01`.
/// - Se `Error`: rimane `Error` (fail-closed), esito `0x02` (nessuna modifica).
pub fn apply_ipc_suspend(state: &ServiceState) -> (ServiceState, u8) {
    match state {
        ServiceState::Active => (ServiceState::Paused, IPC_SUSPEND_SUCCESS_OR_PAUSED),
        ServiceState::Paused => (ServiceState::Paused, IPC_SUSPEND_SUCCESS_OR_PAUSED),
        ServiceState::Error(msg) => (
            ServiceState::Error(msg.clone()),
            IPC_SUSPEND_REJECTED_ERROR_STATE,
        ),
    }
}

/// Applica la logica di transizione di stato per il comando IPC Resume (funzione pura).
///
/// Regole (fail-closed):
/// - Se `Active`: rimane `Active`, esito `0x01`.
/// - Se `Paused`: transiziona ad `Active`, esito `0x01`.
/// - Se `Error`: rimane `Error` (fail-closed), esito `0x02` (nessuna modifica).
pub fn apply_ipc_resume(state: &ServiceState) -> (ServiceState, u8) {
    match state {
        ServiceState::Active => (ServiceState::Active, IPC_RESUME_SUCCESS_OR_ACTIVE),
        ServiceState::Paused => (ServiceState::Active, IPC_RESUME_SUCCESS_OR_ACTIVE),
        ServiceState::Error(msg) => (
            ServiceState::Error(msg.clone()),
            IPC_RESUME_REJECTED_ERROR_STATE,
        ),
    }
}

/// Wrapper RAII per un descrittore di sicurezza `PSECURITY_DESCRIPTOR` allocato da Windows.
///
/// Rilascia la memoria allocata chiamando `LocalFree` nel `Drop`.
#[derive(Debug)]
pub struct SafeSecurityDescriptor {
    raw: PSECURITY_DESCRIPTOR,
}

impl SafeSecurityDescriptor {
    pub fn as_ptr(&self) -> PSECURITY_DESCRIPTOR {
        self.raw
    }
}

impl Drop for SafeSecurityDescriptor {
    fn drop(&mut self) {
        if !self.raw.0.is_null() {
            // SAFETY: Invocazione documentata di LocalFree sul descrittore di sicurezza allocato da
            // ConvertStringSecurityDescriptorToSecurityDescriptorW.
            unsafe {
                let _ = LocalFree(Some(HLOCAL(self.raw.0)));
            }
        }
    }
}

/// Estrae il SID dell'utente corrente (`TokenUser`) dal token di accesso del processo.
pub fn get_current_process_user_sid() -> Result<String, PlatformError> {
    let mut token_handle = HANDLE::default();
    // SAFETY: Apertura del token del processo corrente con diritto di lettura TOKEN_QUERY.
    unsafe {
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token_handle)
            .map_err(|e| PlatformError::OsError(format!("OpenProcessToken fallito: {}", e)))?;
    }
    let safe_token = SafeHandle::new(token_handle);

    // 1. Chiamata esplorativa per ricavare la dimensione necessaria del buffer
    let mut len = 0u32;
    // SAFETY: Prima invocazione per misurare la dimensione del buffer TOKEN_USER.
    unsafe {
        let _ = GetTokenInformation(safe_token.raw(), TokenUser, None, 0, &mut len);
    }
    if len == 0 {
        return Err(PlatformError::OsError(
            "GetTokenInformation non ha restituito la dimensione del buffer".into(),
        ));
    }

    const _: () = assert!(std::mem::align_of::<u64>() >= std::mem::align_of::<TOKEN_USER>());
    let word_count = (len as usize).div_ceil(std::mem::size_of::<u64>());
    let mut buffer = vec![0u64; word_count];
    let byte_len = (word_count * std::mem::size_of::<u64>()) as u32;
    let mut returned_len = byte_len;
    // SAFETY: Seconda invocazione per riempire il buffer allineato a 8 byte (>= align_of::<TOKEN_USER>())
    // con la struttura TOKEN_USER.
    unsafe {
        GetTokenInformation(
            safe_token.raw(),
            TokenUser,
            Some(buffer.as_mut_ptr() as *mut _),
            byte_len,
            &mut returned_len,
        )
        .map_err(|e| PlatformError::OsError(format!("GetTokenInformation fallito: {}", e)))?;
    }

    if (returned_len as usize) < std::mem::size_of::<TOKEN_USER>() {
        return Err(PlatformError::OsError(
            "Buffer TOKEN_USER restituito più piccolo della struttura attesa".into(),
        ));
    }

    // SAFETY: `buffer` è allocato come Vec<u64> (allineamento >= align_of::<TOKEN_USER>())
    // e GetTokenInformation ha popolato almeno size_of::<TOKEN_USER>() byte.
    let token_user = unsafe { &*(buffer.as_ptr() as *const TOKEN_USER) };
    let mut string_sid = PWSTR::null();

    // SAFETY: Converte il SID binario in una rappresentazione testuale SDDL allocata dal sistema.
    unsafe {
        ConvertSidToStringSidW(token_user.User.Sid, &mut string_sid).map_err(|e| {
            PlatformError::OsError(format!("ConvertSidToStringSidW fallito: {}", e))
        })?;
    }

    // Estrae la stringa in un tipo Rust sicuro e dealloca tempestivamente la memoria Win32
    let sid_str = unsafe {
        let s = string_sid
            .to_string()
            .map_err(|e| PlatformError::OsError(format!("Conversione string_sid fallita: {}", e)));
        let _ = LocalFree(Some(HLOCAL(string_sid.0 as _)));
        s?
    };

    Ok(sid_str)
}

/// Crea un `SafeSecurityDescriptor` basato su SDDL che garantisce accesso esclusivo al SID fornito.
///
/// La DACL configurata (`D:P(A;;GRGW;;;{sid})`) imposta:
/// - `D:P`: DACL protetta (Protected), che impedisce l'ereditarietà di permessi terzi.
/// - `(A;;GRGW;;;{sid})`: Access Allowed (A), Generic Read e Generic Write (GRGW) unicamente per il SID dell'utente corrente.
pub fn create_restricted_security_descriptor(
    sid: &str,
) -> Result<SafeSecurityDescriptor, PlatformError> {
    let sddl = format!("D:P(A;;GRGW;;;{})", sid);
    let wide: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();

    let mut p_sd = PSECURITY_DESCRIPTOR::default();
    // SAFETY: Parsing della stringa SDDL e creazione del descrittore di sicurezza Win32.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(wide.as_ptr()),
            SDDL_REVISION_1,
            &mut p_sd,
            None,
        )
        .map_err(|e| {
            PlatformError::OsError(format!(
                "ConvertStringSecurityDescriptorToSecurityDescriptorW fallito: {}",
                e
            ))
        })?;
    }

    Ok(SafeSecurityDescriptor { raw: p_sd })
}

/// Server IPC locale su Named Pipe Windows con ACL restrittiva.
///
/// Opera su un thread worker dedicato registrato in `WorkerThreads`. Comunica con il thread
/// del message loop tramite `WakeHandle` e risponde alle richieste dei client locali autorizzati.
pub struct IpcServer {
    stop_event: Arc<SafeHandle>,
    stop_flag: Arc<AtomicBool>,
    action_queue: Arc<Mutex<VecDeque<IpcServerAction>>>,
}

impl IpcServer {
    /// Avvia il server IPC su un thread worker dedicato registrato in `WorkerThreads`.
    ///
    /// Se una pipe con il medesimo identificatore esiste già (tentativo di squatting),
    /// la creazione fallisce garantendo la sicurezza del processo.
    pub fn start(
        wake_handle: WakeHandle,
        status_state: Arc<Mutex<ServiceState>>,
        active_rules_count: Arc<AtomicUsize>,
        worker_threads: &mut WorkerThreads,
    ) -> Result<Arc<Self>, PlatformError> {
        let sid = get_current_process_user_sid()?;
        let pipe_name = get_pipe_name(&sid);
        Self::start_with_pipe_name(
            &pipe_name,
            &sid,
            wake_handle,
            status_state,
            active_rules_count,
            worker_threads,
        )
    }

    /// Avvia il server IPC specificando esplicitamente il nome della Named Pipe (utilizzato anche nei test).
    pub fn start_with_pipe_name(
        pipe_name: &str,
        sid: &str,
        wake_handle: WakeHandle,
        status_state: Arc<Mutex<ServiceState>>,
        active_rules_count: Arc<AtomicUsize>,
        worker_threads: &mut WorkerThreads,
    ) -> Result<Arc<Self>, PlatformError> {
        let sec_desc = create_restricted_security_descriptor(sid)?;
        let mut sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sec_desc.as_ptr().0,
            bInheritHandle: false.into(),
        };

        let pipe_wide: Vec<u16> = pipe_name.encode_utf16().chain(std::iter::once(0)).collect();

        // SAFETY: Creazione della Named Pipe con flag anti-squatting e rifiuto categorico dei client remoti.
        let pipe_handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(pipe_wide.as_ptr()),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,                             // Un solo client per volta
                MAX_IPC_FRAME_SIZE as u32 * 2, // Buffer output
                MAX_IPC_FRAME_SIZE as u32 * 2, // Buffer input
                2000,                          // Timeout predefinito (2s)
                Some(&mut sa as *const _),
            )
        };

        if pipe_handle.is_invalid() || pipe_handle.0 as usize == 0 {
            let last_error = unsafe { GetLastError() };
            let category = if last_error == ERROR_ACCESS_DENIED {
                "pipe già esistente (squatting prevenuto) o accesso negato"
            } else if last_error == ERROR_ALREADY_EXISTS {
                "pipe già esistente"
            } else {
                "errore generico Win32"
            };
            return Err(PlatformError::OsError(format!(
                "Creazione Named Pipe fallita [{} - codice {:?}]",
                category, last_error
            )));
        }

        let pipe = SafeHandle::new(pipe_handle);

        // Creazione dell'evento di stop (manual-reset, inizialmente non segnalato)
        // SAFETY: Creazione evento anonimo per il segnale di arresto del worker thread.
        let stop_event_handle = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }
            .map_err(|e| PlatformError::OsError(format!("CreateEventW fallito: {}", e)))?;
        let stop_event = Arc::new(SafeHandle::new(stop_event_handle));

        let stop_flag = Arc::new(AtomicBool::new(false));
        let action_queue = Arc::new(Mutex::new(VecDeque::new()));

        let server = Arc::new(Self {
            stop_event: stop_event.clone(),
            stop_flag: stop_flag.clone(),
            action_queue: action_queue.clone(),
        });

        let ctx = IpcWorkerContext {
            stop_event,
            stop_flag,
            wake_handle,
            action_queue,
            status_state,
            active_rules_count,
        };

        worker_threads.spawn(move || {
            server_worker_loop(pipe, ctx);
        });

        Ok(server)
    }

    /// Segnala l'evento di stop e imposta il flag atomico per arrestare immediatamente il worker thread.
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        // SAFETY: Segnala l'evento manual-reset di stop per sbloccare WaitForMultipleObjects.
        unsafe {
            let _ = SetEvent(self.stop_event.raw());
        }
    }

    /// Estrae la prossima azione IPC accodata da inoltrare al message loop.
    pub fn pop_action(&self) -> Option<IpcServerAction> {
        self.action_queue
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .pop_front()
    }
}

/// Contesto condiviso con il thread worker del server IPC.
struct IpcWorkerContext {
    stop_event: Arc<SafeHandle>,
    stop_flag: Arc<AtomicBool>,
    wake_handle: WakeHandle,
    action_queue: Arc<Mutex<VecDeque<IpcServerAction>>>,
    status_state: Arc<Mutex<ServiceState>>,
    active_rules_count: Arc<AtomicUsize>,
}

/// Limite massimo di attesa (500 ms) per la cancellazione dell'I/O asincrono da parte del kernel Windows.
pub const IO_CANCEL_TIMEOUT_MS: u32 = 500;
pub const IO_CANCEL_TIMEOUT: Duration = Duration::from_millis(500);

/// Attesa breve interrompibile (500 ms) dopo l'invio della risposta prima di `DisconnectNamedPipe`.
pub const IPC_DRAIN_TIMEOUT_MS: u32 = 500;

/// Wrapper RAII per operazioni di I/O asincrone Windows (Overlapped I/O).
///
/// Possiede il proprio evento manual-reset (`SafeHandle` per `hEvent`), la struttura `OVERLAPPED`
/// e il buffer dati sull'heap (`Box`), incapsulati in `ManuallyDrop` per evitare qualsiasi `unwrap`/`expect`
/// e garantire indirizzi stabili durante l'I/O kernel.
///
/// # Invariante di Sicurezza e Drop
/// Quando `PendingIo` viene distrutto:
/// - Se l'operazione è stata completata con successo o è fallita subito senza restare pendente
///   (`completed = true`), `OVERLAPPED`, buffer e `hEvent` (`SafeHandle`) vengono rilasciati normalmente.
/// - Se l'operazione è ancora pendente (`completed = false`), `Drop` richiede l'annullamento
///   tramite `CancelIoEx`, attende fino a 500 ms con `WaitForSingleObject` che il kernel Windows
///   segnali il rilascio, ed estrae lo stato con `GetOverlappedResult(bWait = false)`.
/// - Qualora il timeout di 500 ms scada prima che il kernel rilasci l'operazione, TUTTE le risorse
///   riferibili dal kernel (`OVERLAPPED`, buffer e handle `hEvent` tramite `std::mem::forget`) vengono
///   intenzionalmente lasciate orfane (leak) invece di essere deallocate o chiuse, con un warning di log.
///   In questo modo `hEvent` non viene mai resettato né chiuso e la memoria non viene mai liberata mentre
///   il kernel può ancora accedere a `OVERLAPPED` (prevenzione assoluta di kernel use-after-free).
pub struct PendingIo {
    pipe: HANDLE,
    event: ManuallyDrop<SafeHandle>,
    overlapped: ManuallyDrop<Box<OVERLAPPED>>,
    buffer: ManuallyDrop<Box<[u8]>>,
    completed: bool,
}

impl std::fmt::Debug for PendingIo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingIo")
            .field("pipe", &self.pipe)
            .field("event", &self.event.raw())
            .field("buffer_len", &self.buffer.len())
            .field("completed", &self.completed)
            .finish()
    }
}

impl PendingIo {
    fn create_event() -> Result<SafeHandle, IpcProtocolError> {
        // SAFETY: Creazione evento anonimo manual-reset inizialmente non segnalato dedicato a questa operazione I/O.
        let handle = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.map_err(|e| {
            IpcProtocolError::SystemError(format!("CreateEventW fallito per PendingIo: {}", e))
        })?;
        Ok(SafeHandle::new(handle))
    }

    /// Inizializza un `PendingIo` per un'operazione di connessione (`ConnectNamedPipe`).
    pub fn new_connect(pipe: HANDLE) -> Result<Self, IpcProtocolError> {
        let event = Self::create_event()?;
        let ov = Box::new(OVERLAPPED {
            hEvent: event.raw(),
            ..Default::default()
        });
        Ok(Self {
            pipe,
            event: ManuallyDrop::new(event),
            overlapped: ManuallyDrop::new(ov),
            buffer: ManuallyDrop::new(Box::from([])),
            completed: false,
        })
    }

    /// Inizializza un `PendingIo` per un'operazione di lettura (`ReadFile`) con buffer heap dedicato.
    pub fn new_read(pipe: HANDLE, len: usize) -> Result<Self, IpcProtocolError> {
        let event = Self::create_event()?;
        let ov = Box::new(OVERLAPPED {
            hEvent: event.raw(),
            ..Default::default()
        });
        let buf = vec![0u8; len].into_boxed_slice();
        Ok(Self {
            pipe,
            event: ManuallyDrop::new(event),
            overlapped: ManuallyDrop::new(ov),
            buffer: ManuallyDrop::new(buf),
            completed: false,
        })
    }

    /// Inizializza un `PendingIo` per un'operazione di scrittura (`WriteFile`) con payload heap dedicato.
    pub fn new_write(pipe: HANDLE, data: &[u8]) -> Result<Self, IpcProtocolError> {
        let event = Self::create_event()?;
        let ov = Box::new(OVERLAPPED {
            hEvent: event.raw(),
            ..Default::default()
        });
        let buf = data.to_vec().into_boxed_slice();
        Ok(Self {
            pipe,
            event: ManuallyDrop::new(event),
            overlapped: ManuallyDrop::new(ov),
            buffer: ManuallyDrop::new(buf),
            completed: false,
        })
    }

    /// Restituisce l'handle Win32 grezzo dell'evento manual-reset dedicato a questa operazione I/O.
    pub fn event_raw(&self) -> HANDLE {
        self.event.raw()
    }

    /// Segnala che l'operazione di I/O non è più pendente nel kernel,
    /// disarmando la cancellazione e consentendo il normale rilascio di tutte le risorse al drop.
    pub fn mark_completed(&mut self) {
        self.completed = true;
    }

    /// Indica se l'operazione è stata marcata come completata.
    pub fn is_completed(&self) -> bool {
        self.completed
    }

    /// Restituisce un riferimento mutabile alla struttura `OVERLAPPED` allocata sull'heap.
    pub fn overlapped_mut(&mut self) -> &mut OVERLAPPED {
        &mut self.overlapped
    }

    /// Restituisce un riferimento al buffer dati allocato sull'heap.
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    /// Restituisce contemporaneamente il buffer di lettura mutabile e la struttura `OVERLAPPED`.
    pub fn read_targets(&mut self) -> (&mut [u8], &mut OVERLAPPED) {
        (&mut self.buffer, &mut self.overlapped)
    }

    /// Restituisce contemporaneamente il buffer di scrittura immutabile e la struttura `OVERLAPPED`.
    pub fn write_targets(&mut self) -> (&[u8], &mut OVERLAPPED) {
        (&self.buffer, &mut self.overlapped)
    }
}

impl Drop for PendingIo {
    fn drop(&mut self) {
        if self.completed {
            // SAFETY: L'operazione I/O è conclusa; né OVERLAPPED, né buffer, né hEvent sono più referenziati dal kernel.
            unsafe {
                ManuallyDrop::drop(&mut self.overlapped);
                ManuallyDrop::drop(&mut self.buffer);
                ManuallyDrop::drop(&mut self.event);
            }
            return;
        }

        // SAFETY: Invariante di memoria kernel: né la struttura OVERLAPPED, né il buffer dati,
        // né l'handle dell'evento hEvent (SafeHandle) vengono mai liberati, resettati o chiusi
        // finché il kernel Windows può scriverci, leggerli o segnalarli. Se la cancellazione
        // va in timeout (500 ms), OVERLAPPED, buffer ed handle hEvent vengono intenzionalmente
        // lasciati orfani (Box::leak / std::mem::forget) per prevenire qualsiasi kernel use-after-free.
        let signaled = cancel_io_and_wait(self.pipe, &mut self.overlapped);
        if signaled {
            // SAFETY: Il kernel ha segnalato hEvent confermando il rilascio dell'I/O: è sicuro liberare tutto.
            unsafe {
                ManuallyDrop::drop(&mut self.overlapped);
                ManuallyDrop::drop(&mut self.buffer);
                ManuallyDrop::drop(&mut self.event);
            }
        } else {
            tracing::warn!(
                "Timeout attesa cancellazione I/O kernel scaduto (500 ms): OVERLAPPED, buffer e handle hEvent persi intenzionalmente (leak) per prevenire kernel use-after-free"
            );
            // SAFETY: Nessuna risorsa viene più usata dopo Drop; trasferiamo ownership per leak esplicito.
            unsafe {
                let ov = ManuallyDrop::take(&mut self.overlapped);
                let buf = ManuallyDrop::take(&mut self.buffer);
                let ev = ManuallyDrop::take(&mut self.event);
                let _ = Box::leak(ov);
                let _ = Box::leak(buf);
                std::mem::forget(ev);
            }
        }
    }
}

/// Annulla l'operazione di I/O asincrona associata a `overlapped` e attende
/// che il kernel Windows segnali il completamento entro il limite di timeout (500 ms).
///
/// Restituisce `true` se il kernel ha confermato il completamento/annullamento dell'I/O (evento segnalato);
/// restituisce `false` se il timeout è scaduto prima che il kernel rilasciasse la richiesta.
pub fn cancel_io_and_wait(pipe: HANDLE, overlapped: &mut OVERLAPPED) -> bool {
    // SAFETY: Richiesta di interruzione dell'operazione asincrona al driver del kernel.
    let cancel_res = unsafe { CancelIoEx(pipe, Some(overlapped)) };
    if let Err(_err) = cancel_res {
        let last = unsafe { GetLastError() };
        // ERROR_NOT_FOUND (1168) indica che l'operazione non era pendente o era già completata.
        if last != ERROR_NOT_FOUND {
            tracing::warn!(
                error_code = last.0,
                "CancelIoEx ha restituito un codice imprevisto durante l'annullamento"
            );
        }
    }

    // SAFETY: Attesa con limite prefissato (500 ms) sul completamento dell'evento OVERLAPPED.
    let wait_res = unsafe { WaitForSingleObject(overlapped.hEvent, IO_CANCEL_TIMEOUT_MS) };
    if wait_res == WAIT_OBJECT_0 {
        let mut transferred = 0u32;
        // SAFETY: Verifica non bloccante (bWait = false) dello stato finale post-completamento.
        let res = unsafe { GetOverlappedResult(pipe, overlapped, &mut transferred, false) };
        if let Err(_err) = res {
            let last = unsafe { GetLastError() };
            // ERROR_OPERATION_ABORTED (995) e ERROR_NOT_FOUND (1168) sono esiti attesi per cancellazione riuscita.
            // ERROR_BROKEN_PIPE (109) ed ERROR_NO_DATA (232) indicano disconnessione del client e vanno registrati a debug.
            if last == ERROR_BROKEN_PIPE || last == ERROR_NO_DATA {
                tracing::debug!(
                    error_code = last.0,
                    "Client disconnesso durante l'annullamento I/O (BROKEN_PIPE o NO_DATA)"
                );
            } else if last != ERROR_OPERATION_ABORTED && last != ERROR_NOT_FOUND {
                tracing::warn!(
                    error_code = last.0,
                    "GetOverlappedResult dopo CancelIoEx ha restituito un codice imprevisto"
                );
            }
        }
        true
    } else {
        false
    }
}

/// Loop di ascolto e gestione della named pipe eseguito sul thread worker dedicato.
fn server_worker_loop(pipe: SafeHandle, ctx: IpcWorkerContext) {
    let stop_event_raw = ctx.stop_event.raw();
    while !ctx.stop_flag.load(Ordering::SeqCst) {
        let mut pending_connect = match PendingIo::new_connect(pipe.raw()) {
            Ok(p) => p,
            Err(err) => {
                tracing::warn!(error = %err, "Impossibile allocare PendingIo per ConnectNamedPipe");
                break;
            }
        };

        // SAFETY: ConnectNamedPipe in modalità asincrona (overlapped).
        let connect_res =
            unsafe { ConnectNamedPipe(pipe.raw(), Some(pending_connect.overlapped_mut())) };
        let mut connected = false;

        let is_pending = if connect_res.is_ok() {
            connected = true;
            pending_connect.mark_completed();
            false
        } else {
            let err = unsafe { GetLastError() };
            if err == ERROR_PIPE_CONNECTED {
                connected = true;
                pending_connect.mark_completed();
                false
            } else if err == ERROR_IO_PENDING {
                true
            } else {
                // Errore immediato diverso da ERROR_IO_PENDING: l'operazione non è pendente
                pending_connect.mark_completed();
                tracing::warn!(
                    error_code = err.0,
                    "ConnectNamedPipe fallito immediatamente"
                );
                false
            }
        };

        if is_pending {
            // Attesa congiunta: evento di I/O (connessione client) o evento di stop (arresto daemon)
            let wait_handles = [pending_connect.event_raw(), stop_event_raw];
            // SAFETY: Attesa a tempo indeterminato su handles locali del processo.
            let wait_res = unsafe { WaitForMultipleObjects(&wait_handles, false, INFINITE) };
            if wait_res == WAIT_OBJECT_0 {
                let mut transferred = 0u32;
                // SAFETY: Verifica completamento operazione di connessione asincrona.
                let get_res = unsafe {
                    GetOverlappedResult(
                        pipe.raw(),
                        pending_connect.overlapped_mut(),
                        &mut transferred,
                        false,
                    )
                };
                pending_connect.mark_completed();
                if get_res.is_ok() {
                    connected = true;
                }
            } else if wait_res == WAIT_EVENT(WAIT_OBJECT_0.0 + 1) {
                // Segnale di stop ricevuto: pending_connect.drop() annulla I/O e attende il completamento kernel
                break;
            } else {
                // Errore o evento inatteso
                break;
            }
        }

        drop(pending_connect);

        if connected && !ctx.stop_flag.load(Ordering::SeqCst) {
            handle_connected_client(pipe.raw(), &ctx);
        }

        // SAFETY: Disconnette il client per rendere la pipe disponibile a una nuova connessione.
        unsafe {
            let _ = DisconnectNamedPipe(pipe.raw());
        }
    }
}

/// Gestisce una singola richiesta dal client connesso, invia la risposta e disconnette.
fn handle_connected_client(pipe: HANDLE, ctx: &IpcWorkerContext) {
    let stop_event = ctx.stop_event.raw();

    // 1. Lettura dei 4 byte dell'header (lunghezza payload LE)
    let mut len_header = [0u8; 4];
    if let Err(err) =
        read_exact_overlapped(pipe, stop_event, &mut len_header, DEFAULT_IPC_IO_TIMEOUT)
    {
        if err == IpcProtocolError::ClientDisconnected {
            tracing::debug!("Client IPC disconnesso durante la lettura dell'header");
        } else if err != IpcProtocolError::ServerStopped {
            tracing::debug!(error = %err, "Chiusura o timeout connessione client IPC su lettura header");
        }
        return;
    }

    // 2. Decodifica e validazione immediata dell'header di lunghezza (rifiuta >16 senza leggere il corpo)
    let payload_len = match parse_frame_header(&len_header) {
        Ok(len) => len,
        Err(err) => {
            tracing::warn!(error = %err, "Header frame IPC non valido");
            let _ = send_response_overlapped(pipe, stop_event, &IpcResponse::Error);
            return;
        }
    };

    // 3. Lettura del payload (esattamente 1 byte)
    let mut payload = vec![0u8; payload_len];
    if let Err(err) = read_exact_overlapped(pipe, stop_event, &mut payload, DEFAULT_IPC_IO_TIMEOUT)
    {
        if err == IpcProtocolError::ClientDisconnected {
            tracing::debug!("Client IPC disconnesso durante la lettura del payload");
        } else if err == IpcProtocolError::ServerStopped {
            tracing::debug!("Lettura payload IPC interrotta da arresto server");
        } else {
            tracing::warn!(error = %err, "Lettura payload IPC fallita");
        }
        return;
    }

    // 4. Parsing del comando (funzione pura)
    let command = match parse_ipc_payload(&payload) {
        Ok(cmd) => cmd,
        Err(err) => {
            tracing::warn!(error = %err, "Opcode comando IPC non valido");
            let _ = send_response_overlapped(pipe, stop_event, &IpcResponse::Error);
            return;
        }
    };

    // 5. Elaborazione del comando autorizzato e risposta a byte
    match command {
        IpcCommand::GetStatus => {
            tracing::debug!("Ricevuto comando IPC autorizzato: GetStatus");
            let (state_byte, rules) = {
                let s = ctx.status_state.lock().unwrap_or_else(|p| p.into_inner());
                let st = match *s {
                    ServiceState::Active => IPC_STATE_ACTIVE,
                    ServiceState::Paused => IPC_STATE_PAUSED,
                    ServiceState::Error(_) => IPC_STATE_ERROR,
                };
                let r = ctx.active_rules_count.load(Ordering::SeqCst) as u32;
                (st, r)
            };
            let response = IpcResponse::Status {
                state: state_byte,
                active_rules: rules,
            };
            send_and_log_response(pipe, stop_event, &response, "Status");
        }
        IpcCommand::Suspend => {
            tracing::info!("Ricevuto comando IPC autorizzato: Suspend");
            let current_state = ctx
                .status_state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone();
            let (_next_state, outcome) = apply_ipc_suspend(&current_state);

            // Accoda l'azione solo se il servizio è attualmente attivo
            if outcome == IPC_SUSPEND_SUCCESS_OR_PAUSED && current_state.is_active() {
                queue_action_and_wake(ctx, IpcServerAction::Suspend);
            }

            let response = IpcResponse::Suspend { outcome };
            send_and_log_response(pipe, stop_event, &response, "Suspend");
        }
        IpcCommand::ReloadConfig => {
            tracing::info!("Ricevuto comando IPC autorizzato: ReloadConfig");
            queue_action_and_wake(ctx, IpcServerAction::ReloadConfig);
            let response = IpcResponse::Reload {
                outcome: IPC_RELOAD_ACCEPTED,
            };
            send_and_log_response(pipe, stop_event, &response, "ReloadConfig");
        }
        IpcCommand::Resume => {
            tracing::info!("Ricevuto comando IPC autorizzato: Resume");
            let current_state = ctx
                .status_state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone();
            let (_next_state, outcome) = apply_ipc_resume(&current_state);

            // Invariante fail-closed: accoda la ripresa solo se il servizio è attualmente in Pausa (mai in Error)
            if outcome == IPC_RESUME_SUCCESS_OR_ACTIVE && current_state == ServiceState::Paused {
                queue_action_and_wake(ctx, IpcServerAction::Resume);
            }

            let response = IpcResponse::Resume { outcome };
            send_and_log_response(pipe, stop_event, &response, "Resume");
        }
    }
}

/// Accoda un'azione IPC per il message loop Win32 e lo risveglia via `WakeHandle`.
fn queue_action_and_wake(ctx: &IpcWorkerContext, action: IpcServerAction) {
    ctx.action_queue
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .push_back(action);
    if let Err(err) = ctx.wake_handle.wake() {
        tracing::warn!(error = %err, "WakeHandle::wake fallito dall'IPC server");
    }
}

/// Invia la risposta IPC su pipe overlapped registrando gli eventuali errori in base alla gravità.
fn send_and_log_response(
    pipe: HANDLE,
    stop_event: HANDLE,
    response: &IpcResponse,
    label: &'static str,
) {
    if let Err(err) = send_response_overlapped(pipe, stop_event, response) {
        if err == IpcProtocolError::ClientDisconnected {
            tracing::debug!(
                command = label,
                "Client IPC disconnesso prima di ricevere la risposta (broken pipe o no data)"
            );
        } else if err == IpcProtocolError::ServerStopped {
            tracing::debug!(
                command = label,
                "Invio o drain risposta IPC interrotto da stop server"
            );
        } else {
            tracing::warn!(command = label, error = %err, "Invio risposta IPC fallito");
        }
    }
}

/// Esegue una lettura esatta di `buf.len()` byte con supporto a overlapped I/O, timeout e stop.
fn read_exact_overlapped(
    pipe: HANDLE,
    stop_event: HANDLE,
    buf: &mut [u8],
    timeout: Duration,
) -> Result<(), IpcProtocolError> {
    let mut total_read = 0;
    let timeout_ms = timeout.as_millis().min(u32::MAX as u128) as u32;

    while total_read < buf.len() {
        let needed = buf.len() - total_read;
        let mut pending_io = PendingIo::new_read(pipe, needed)?;
        let (io_buf, io_ov) = pending_io.read_targets();

        // SAFETY: ReadFile asincrona tramite OVERLAPPED su pipe aperta con FILE_FLAG_OVERLAPPED.
        let read_res = unsafe { ReadFile(pipe, Some(io_buf), None, Some(io_ov)) };

        if read_res.is_err() {
            let err = unsafe { GetLastError() };
            if err == ERROR_BROKEN_PIPE || err == ERROR_NO_DATA {
                pending_io.mark_completed();
                return Err(IpcProtocolError::ClientDisconnected);
            }
            if err != ERROR_IO_PENDING {
                pending_io.mark_completed();
                return Err(IpcProtocolError::SystemError(format!(
                    "ReadFile fallito: {:?}",
                    err
                )));
            }
        }

        // Attesa: evento dedicato di PendingIo o stop_event con timeout
        let handles = [pending_io.event_raw(), stop_event];
        // SAFETY: WaitForMultipleObjects su handles verificati.
        let wait_res = unsafe { WaitForMultipleObjects(&handles, false, timeout_ms) };

        if wait_res == WAIT_OBJECT_0 {
            let mut transferred = 0u32;
            // SAFETY: Recupera il risultato dell'operazione asincrona completata.
            let res = unsafe {
                GetOverlappedResult(pipe, pending_io.overlapped_mut(), &mut transferred, false)
            };
            pending_io.mark_completed();
            if res.is_err() {
                let last = unsafe { GetLastError() };
                if last == ERROR_BROKEN_PIPE || last == ERROR_NO_DATA {
                    return Err(IpcProtocolError::ClientDisconnected);
                }
                return Err(IpcProtocolError::SystemError(format!(
                    "GetOverlappedResult fallito: {:?}",
                    last
                )));
            }
            if transferred == 0 {
                return Err(IpcProtocolError::ClientDisconnected);
            }
            buf[total_read..total_read + transferred as usize]
                .copy_from_slice(&pending_io.buffer()[..transferred as usize]);
            total_read += transferred as usize;
        } else if wait_res == WAIT_EVENT(WAIT_OBJECT_0.0 + 1) {
            // Drop per pending_io annulla e attende
            return Err(IpcProtocolError::ServerStopped);
        } else if wait_res == WAIT_TIMEOUT {
            // Drop per pending_io annulla e attende
            return Err(IpcProtocolError::IoTimeout);
        } else {
            return Err(IpcProtocolError::SystemError(
                "WaitForMultipleObjects fallito".into(),
            ));
        }
    }

    Ok(())
}

/// Esegue un drain interrompibile dopo l'invio della risposta per evitare che `DisconnectNamedPipe`
/// scarti i dati prima che il client li legga, senza bloccare su `FlushFileBuffers`.
fn drain_pipe_before_disconnect(pipe: HANDLE, stop_event: HANDLE) -> Result<(), IpcProtocolError> {
    let mut pending_io = PendingIo::new_read(pipe, 1)?;
    let (io_buf, io_ov) = pending_io.read_targets();

    // SAFETY: ReadFile asincrona di 1 byte su pipe overlapped per attendere chiusura client in modo
    // interrompibile da stop_event o timeout breve (500 ms).
    let read_res = unsafe { ReadFile(pipe, Some(io_buf), None, Some(io_ov)) };
    if read_res.is_err() {
        let err = unsafe { GetLastError() };
        if err == ERROR_BROKEN_PIPE || err == ERROR_NO_DATA {
            pending_io.mark_completed();
            return Ok(());
        }
        if err != ERROR_IO_PENDING {
            pending_io.mark_completed();
            tracing::debug!(
                error_code = err.0,
                "ReadFile di drain terminata con codice non pendente"
            );
            return Ok(());
        }
    }

    let handles = [pending_io.event_raw(), stop_event];
    // SAFETY: WaitForMultipleObjects su handles verificati con timeout breve (500 ms).
    let wait_res = unsafe { WaitForMultipleObjects(&handles, false, IPC_DRAIN_TIMEOUT_MS) };

    if wait_res == WAIT_OBJECT_0 {
        let mut transferred = 0u32;
        // SAFETY: Recupera lo stato di completamento non bloccante dopo WAIT_OBJECT_0.
        let res = unsafe {
            GetOverlappedResult(pipe, pending_io.overlapped_mut(), &mut transferred, false)
        };
        pending_io.mark_completed();
        if res.is_err() {
            let last = unsafe { GetLastError() };
            if last != ERROR_BROKEN_PIPE && last != ERROR_NO_DATA {
                tracing::debug!(
                    error_code = last.0,
                    "GetOverlappedResult in drain terminato con codice"
                );
            }
        }
        Ok(())
    } else if wait_res == WAIT_EVENT(WAIT_OBJECT_0.0 + 1) {
        Err(IpcProtocolError::ServerStopped)
    } else {
        Ok(())
    }
}

/// Invia una risposta IPC codificata con supporto a overlapped I/O, timeout e stop.
fn send_response_overlapped(
    pipe: HANDLE,
    stop_event: HANDLE,
    response: &IpcResponse,
) -> Result<(), IpcProtocolError> {
    let frame_bytes = encode_ipc_response(response);
    let mut total_written = 0;
    let timeout_ms = DEFAULT_IPC_IO_TIMEOUT.as_millis().min(u32::MAX as u128) as u32;

    while total_written < frame_bytes.len() {
        let mut pending_io = PendingIo::new_write(pipe, &frame_bytes[total_written..])?;
        let (io_buf, io_ov) = pending_io.write_targets();

        // SAFETY: WriteFile asincrona tramite OVERLAPPED su pipe aperta con FILE_FLAG_OVERLAPPED.
        let write_res = unsafe { WriteFile(pipe, Some(io_buf), None, Some(io_ov)) };

        if write_res.is_err() {
            let err = unsafe { GetLastError() };
            if err == ERROR_BROKEN_PIPE || err == ERROR_NO_DATA {
                pending_io.mark_completed();
                return Err(IpcProtocolError::ClientDisconnected);
            }
            if err != ERROR_IO_PENDING {
                pending_io.mark_completed();
                return Err(IpcProtocolError::SystemError(format!(
                    "WriteFile fallito: {:?}",
                    err
                )));
            }
        }

        // Attesa completamento o segnale di stop
        let handles = [pending_io.event_raw(), stop_event];
        // SAFETY: WaitForMultipleObjects su handles verificati.
        let wait_res = unsafe { WaitForMultipleObjects(&handles, false, timeout_ms) };

        if wait_res == WAIT_OBJECT_0 {
            let mut transferred = 0u32;
            // SAFETY: Recupero del risultato di scrittura asincrono.
            let res = unsafe {
                GetOverlappedResult(pipe, pending_io.overlapped_mut(), &mut transferred, false)
            };
            pending_io.mark_completed();
            if res.is_err() {
                let last = unsafe { GetLastError() };
                if last == ERROR_BROKEN_PIPE || last == ERROR_NO_DATA {
                    return Err(IpcProtocolError::ClientDisconnected);
                }
                return Err(IpcProtocolError::SystemError(format!(
                    "GetOverlappedResult fallito: {:?}",
                    last
                )));
            }
            total_written += transferred as usize;
        } else if wait_res == WAIT_EVENT(WAIT_OBJECT_0.0 + 1) {
            return Err(IpcProtocolError::ServerStopped);
        } else if wait_res == WAIT_TIMEOUT {
            return Err(IpcProtocolError::IoTimeout);
        } else {
            return Err(IpcProtocolError::SystemError(
                "WaitForMultipleObjects fallito".into(),
            ));
        }
    }

    drain_pipe_before_disconnect(pipe, stop_event)
}

#[cfg(test)]
static TEST_PIPE_COUNTER: AtomicUsize = AtomicUsize::new(1);

/// Genera un nome pipe univoco e isolato per i test unitari (PID + contatore atomico).
#[cfg(test)]
pub fn generate_test_pipe_name() -> String {
    let count = TEST_PIPE_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!(r"\\.\pipe\rustbind_test_{}_{}", std::process::id(), count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_ipc_suspend_pure() {
        // Attivo -> Pausa (esito 0x01)
        let (st, outcome) = apply_ipc_suspend(&ServiceState::Active);
        assert_eq!(st, ServiceState::Paused);
        assert_eq!(outcome, IPC_SUSPEND_SUCCESS_OR_PAUSED);

        // Pausa -> Pausa (esito 0x01)
        let (st, outcome) = apply_ipc_suspend(&ServiceState::Paused);
        assert_eq!(st, ServiceState::Paused);
        assert_eq!(outcome, IPC_SUSPEND_SUCCESS_OR_PAUSED);

        // Errore -> Errore (esito 0x02, fail-closed: nessuna modifica)
        let err_state = ServiceState::Error("Config non valida".into());
        let (st, outcome) = apply_ipc_suspend(&err_state);
        assert_eq!(st, err_state);
        assert_eq!(outcome, IPC_SUSPEND_REJECTED_ERROR_STATE);
    }

    #[test]
    fn test_apply_ipc_resume_rules_fail_closed() {
        // Pausa -> Attivo (esito 0x01)
        let (st, outcome) = apply_ipc_resume(&ServiceState::Paused);
        assert_eq!(st, ServiceState::Active);
        assert_eq!(outcome, IPC_RESUME_SUCCESS_OR_ACTIVE);

        // Attivo -> Attivo (esito 0x01)
        let (st, outcome) = apply_ipc_resume(&ServiceState::Active);
        assert_eq!(st, ServiceState::Active);
        assert_eq!(outcome, IPC_RESUME_SUCCESS_OR_ACTIVE);

        // Errore -> Errore (esito 0x02, fail-closed: impossibile uscire da Error via Resume)
        let err_state = ServiceState::Error("Kill-switch non registrabile".into());
        let (st, outcome) = apply_ipc_resume(&err_state);
        assert_eq!(st, err_state);
        assert_eq!(outcome, IPC_RESUME_REJECTED_ERROR_STATE);
    }

    #[test]
    fn test_pending_io_cancel_timeout_and_leak_on_unsignaled_event() {
        let start = std::time::Instant::now();
        {
            let pending = PendingIo::new_read(HANDLE::default(), 16).expect("Creazione PendingIo");
            assert!(!pending.is_completed());
            assert_eq!(pending.buffer().len(), 16);
            // Non chiamiamo mark_completed(): l'evento interno manual-reset non è segnalato;
            // il drop eseguirà cancel_io_and_wait, attenderà 500 ms e perderà OVERLAPPED, buffer e hEvent.
        } // Drop di pending avviene qui

        let elapsed = start.elapsed();
        // Verifica che l'attesa sia stata limitata a circa 500 ms (IO_CANCEL_TIMEOUT_MS) e non si sia bloccata all'infinito
        assert!(
            elapsed >= Duration::from_millis(450),
            "L'attesa doveva durare almeno ~500 ms, trascorsi: {:?}",
            elapsed
        );
        assert!(
            elapsed < Duration::from_millis(1500),
            "L'attesa non doveva superare 1.5s, trascorsi: {:?}",
            elapsed
        );
    }

    #[test]
    fn test_pending_io_signaled_event_no_leak() {
        let start = std::time::Instant::now();
        {
            let pending =
                PendingIo::new_write(HANDLE::default(), &[1, 2, 3]).expect("Creazione PendingIo");
            assert!(!pending.is_completed());
            assert_eq!(pending.buffer(), &[1, 2, 3]);
            // Segnala l'evento posseduto da PendingIo per verificare il rilascio immediato senza leak
            // SAFETY: SetEvent sull'handle evento valido posseduto da PendingIo nel test.
            unsafe {
                SetEvent(pending.event_raw()).expect("SetEvent su evento PendingIo");
            }
        } // Drop di pending: l'evento è segnalato, cancel_io_and_wait rilascia subito

        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_millis(200),
            "Con evento segnalato il rilascio doveva essere immediato, trascorsi: {:?}",
            elapsed
        );
    }

    #[test]
    #[ignore = "richiede named pipe reale Win32"]
    fn test_real_pipe_stop_no_client() {
        let mut workers = WorkerThreads::new();
        let wake_handle = WakeHandle::new_null();
        let status = Arc::new(Mutex::new(ServiceState::Active));
        let active_rules = Arc::new(AtomicUsize::new(0));

        let sid = get_current_process_user_sid().expect("SID utente recuperato");
        let test_pipe_name = generate_test_pipe_name();

        let server = IpcServer::start_with_pipe_name(
            &test_pipe_name,
            &sid,
            wake_handle,
            status,
            active_rules,
            &mut workers,
        )
        .expect("Avvio server IPC test");

        server.stop();
        let joined = workers.join_all(Duration::from_secs(1));
        assert!(
            joined,
            "Il thread worker del server IPC doveva completare il join entro 1 secondo senza client"
        );
    }

    #[test]
    #[ignore = "richiede named pipe reale Win32"]
    fn test_real_pipe_stop_silent_client() {
        use std::fs::OpenOptions;

        let mut workers = WorkerThreads::new();
        let wake_handle = WakeHandle::new_null();
        let status = Arc::new(Mutex::new(ServiceState::Active));
        let active_rules = Arc::new(AtomicUsize::new(0));

        let sid = get_current_process_user_sid().expect("SID utente recuperato");
        let test_pipe_name = generate_test_pipe_name();

        let server = IpcServer::start_with_pipe_name(
            &test_pipe_name,
            &sid,
            wake_handle,
            status,
            active_rules,
            &mut workers,
        )
        .expect("Avvio server IPC test");

        let _client = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&test_pipe_name)
            .expect("Connessione client silenzioso");

        server.stop();
        let joined = workers.join_all(Duration::from_secs(1));
        assert!(
            joined,
            "Il thread worker del server IPC doveva completare il join entro 1 secondo con client silenzioso"
        );
    }

    #[test]
    #[ignore = "richiede named pipe reale Win32"]
    fn test_real_pipe_stop_partial_header_client() {
        use std::fs::OpenOptions;
        use std::io::Write;

        let mut workers = WorkerThreads::new();
        let wake_handle = WakeHandle::new_null();
        let status = Arc::new(Mutex::new(ServiceState::Active));
        let active_rules = Arc::new(AtomicUsize::new(0));

        let sid = get_current_process_user_sid().expect("SID utente recuperato");
        let test_pipe_name = generate_test_pipe_name();

        let server = IpcServer::start_with_pipe_name(
            &test_pipe_name,
            &sid,
            wake_handle,
            status,
            active_rules,
            &mut workers,
        )
        .expect("Avvio server IPC test");

        let mut client = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&test_pipe_name)
            .expect("Connessione client");

        // Scrive header parziale (< 4 byte, es. 2 byte)
        client
            .write_all(&[0x01, 0x00])
            .expect("Scrittura header parziale");
        client.flush().expect("Flush header parziale");

        server.stop();
        let joined = workers.join_all(Duration::from_secs(1));
        assert!(
            joined,
            "Il thread worker del server IPC doveva completare il join entro 1 secondo con header parziale"
        );
    }

    #[test]
    #[ignore = "richiede named pipe reale Win32"]
    fn test_real_pipe_stop_missing_payload_client() {
        use std::fs::OpenOptions;
        use std::io::Write;

        let mut workers = WorkerThreads::new();
        let wake_handle = WakeHandle::new_null();
        let status = Arc::new(Mutex::new(ServiceState::Active));
        let active_rules = Arc::new(AtomicUsize::new(0));

        let sid = get_current_process_user_sid().expect("SID utente recuperato");
        let test_pipe_name = generate_test_pipe_name();

        let server = IpcServer::start_with_pipe_name(
            &test_pipe_name,
            &sid,
            wake_handle,
            status,
            active_rules,
            &mut workers,
        )
        .expect("Avvio server IPC test");

        let mut client = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&test_pipe_name)
            .expect("Connessione client");

        // Scrive 4 byte di header (lunghezza = 1) ma nessun payload
        client
            .write_all(&1u32.to_le_bytes())
            .expect("Scrittura header");
        client.flush().expect("Flush header");

        server.stop();
        let joined = workers.join_all(Duration::from_secs(1));
        assert!(
            joined,
            "Il thread worker del server IPC doveva completare il join entro 1 secondo con payload mancante"
        );
    }

    #[test]
    #[ignore = "richiede named pipe reale Win32"]
    fn test_real_pipe_stop_client_not_reading_response() {
        use std::fs::OpenOptions;
        use std::io::Write;

        let mut workers = WorkerThreads::new();
        let wake_handle = WakeHandle::new_null();
        let status = Arc::new(Mutex::new(ServiceState::Active));
        let active_rules = Arc::new(AtomicUsize::new(0));

        let sid = get_current_process_user_sid().expect("SID utente recuperato");
        let test_pipe_name = generate_test_pipe_name();

        let server = IpcServer::start_with_pipe_name(
            &test_pipe_name,
            &sid,
            wake_handle,
            status,
            active_rules,
            &mut workers,
        )
        .expect("Avvio server IPC test");

        let mut client = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&test_pipe_name)
            .expect("Connessione client");

        // Scrive comando completo (GetStatus)
        client
            .write_all(&1u32.to_le_bytes())
            .expect("Scrittura header");
        client
            .write_all(&[IPC_CMD_GET_STATUS])
            .expect("Scrittura comando");
        client.flush().expect("Flush comando");

        // Attende brevemente (~50 ms) che il server scriva la risposta senza leggere né chiudere client
        std::thread::sleep(Duration::from_millis(50));

        server.stop();
        let joined = workers.join_all(Duration::from_secs(1));
        assert!(
            joined,
            "Il thread worker del server IPC doveva completare il join entro 1 secondo con client aperto che non legge la risposta"
        );

        drop(client);
    }

    fn connect_test_pipe(wide_name: &[u16], timeout: Duration) -> SafeHandle {
        use windows::Win32::Foundation::ERROR_PIPE_BUSY;
        use windows::Win32::Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
            FILE_SHARE_NONE, OPEN_EXISTING,
        };
        use windows::Win32::System::Pipes::WaitNamedPipeW;

        let deadline = std::time::Instant::now() + timeout;
        loop {
            // SAFETY: Apertura client locale verso named pipe Win32 di test con nome terminato da NUL.
            let res = unsafe {
                CreateFileW(
                    PCWSTR(wide_name.as_ptr()),
                    FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0,
                    FILE_SHARE_NONE,
                    None,
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL,
                    None,
                )
            };
            match res {
                Ok(handle) => return SafeHandle::new(handle),
                Err(err) => {
                    let code = unsafe { GetLastError() };
                    if code != ERROR_PIPE_BUSY {
                        panic!("Connessione client alla pipe fallita: {}", err);
                    }
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    if remaining.is_zero() {
                        panic!(
                            "Timeout connessione client alla pipe (ERROR_PIPE_BUSY oltre {:?})",
                            timeout
                        );
                    }
                    let wait_ms = remaining.as_millis().min(u32::MAX as u128).max(1) as u32;
                    // SAFETY: Attesa che un'istanza della named pipe diventi disponibile per il tempo residuo.
                    let waited = unsafe { WaitNamedPipeW(PCWSTR(wide_name.as_ptr()), wait_ms) };
                    if !waited.as_bool() {
                        let wait_err = unsafe { GetLastError() };
                        if std::time::Instant::now() >= deadline {
                            panic!(
                                "WaitNamedPipeW fallito alla scadenza del timeout (codice {:?})",
                                wait_err
                            );
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "Richiede comunicazione IPC su Named Pipe Windows reale"]
    fn test_real_pipe_roundtrip() {
        let mut workers = WorkerThreads::new();
        let wake_handle = WakeHandle::new_null();
        let status = Arc::new(Mutex::new(ServiceState::Active));
        let active_rules = Arc::new(AtomicUsize::new(3));

        let sid = get_current_process_user_sid().expect("SID utente");
        let test_pipe_name = generate_test_pipe_name();

        let server = IpcServer::start_with_pipe_name(
            &test_pipe_name,
            &sid,
            wake_handle,
            status,
            active_rules,
            &mut workers,
        )
        .expect("Avvio server test reale");

        let wide_name: Vec<u16> = test_pipe_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        // Connessione client 1: GetStatus
        let safe_client = connect_test_pipe(&wide_name, Duration::from_secs(1));

        // Invia GetStatus (4 byte len = 1, payload = 0x01)
        let mut req = Vec::new();
        req.extend_from_slice(&1u32.to_le_bytes());
        req.push(IPC_CMD_GET_STATUS);

        unsafe {
            WriteFile(safe_client.raw(), Some(&req), None, None).expect("Scrittura GetStatus");
        }

        // Legge risposta (10 byte)
        let mut resp_buf = [0u8; 10];
        unsafe {
            ReadFile(safe_client.raw(), Some(&mut resp_buf), None, None)
                .expect("Lettura risposta GetStatus");
        }

        let resp = decode_ipc_response(&resp_buf).expect("Decodifica risposta Status");
        assert_eq!(
            resp,
            IpcResponse::Status {
                state: IPC_STATE_ACTIVE,
                active_rules: 3
            }
        );

        drop(safe_client);

        // Connessione client 2: Suspend
        let safe_client_2 = connect_test_pipe(&wide_name, Duration::from_secs(1));

        let mut req_suspend = Vec::new();
        req_suspend.extend_from_slice(&1u32.to_le_bytes());
        req_suspend.push(IPC_CMD_SUSPEND);

        unsafe {
            WriteFile(safe_client_2.raw(), Some(&req_suspend), None, None)
                .expect("Scrittura Suspend");
        }

        let mut resp_suspend_buf = [0u8; 5];
        unsafe {
            ReadFile(safe_client_2.raw(), Some(&mut resp_suspend_buf), None, None)
                .expect("Lettura risposta Suspend");
        }

        let resp_sus = decode_ipc_response(&resp_suspend_buf).expect("Decodifica risposta Suspend");
        assert_eq!(
            resp_sus,
            IpcResponse::Suspend {
                outcome: IPC_SUSPEND_SUCCESS_OR_PAUSED
            }
        );

        drop(safe_client_2);

        server.stop();
        assert!(
            workers.join_all(Duration::from_secs(1)),
            "Il thread worker del server IPC doveva completare il join entro 1 secondo al termine del roundtrip"
        );
    }
}
