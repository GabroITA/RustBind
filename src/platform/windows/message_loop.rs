use std::cell::RefCell;
use std::collections::VecDeque;
use std::marker::PhantomData;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MSG, PostMessageW,
    PostQuitMessage, RegisterClassW, RegisterWindowMessageW, TranslateMessage, WINDOW_EX_STYLE,
    WM_APP, WM_CLOSE, WM_DESTROY, WM_ENDSESSION, WM_HOTKEY, WM_MOUSEMOVE, WM_QUERYENDSESSION,
    WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::{PCWSTR, w};

use crate::platform::traits::{HotkeyId, PlatformError};
use crate::platform::windows::handles::SafeHwnd;
use crate::platform::windows::tray::{TrayEvent, parse_tray_event};
use crate::platform::windows::worker_threads::WorkerThreads;

thread_local! {
    static PENDING_EVENTS: RefCell<VecDeque<MessageLoopEvent>> = const { RefCell::new(VecDeque::new()) };
    static TASKBAR_CREATED_MSG: RefCell<u32> = const { RefCell::new(0) };
    static MENU_ACTIVE: RefCell<bool> = const { RefCell::new(false) };
}

const MESSAGE_LOOP_CLASS_NAME: PCWSTR = w!("RustBind_MessageLoopWindow");

/// Messaggio Win32 custom inviato dalla tray icon per le notifiche del mouse.
pub const WM_TRAY_CALLBACK: u32 = WM_APP + 1;

/// Messaggio Win32 custom inviato per risvegliare il message loop da un altro thread.
pub const WM_WAKEUP: u32 = WM_APP + 2;

/// Eventi smistati dall'unico message pump Win32 dell'applicazione.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageLoopEvent {
    /// Hotkey globale premuta, identificata dal relativo `HotkeyId`.
    Hotkey(HotkeyId),
    /// Evento di interazione con l'icona tray (tasto destro, doppio click o tastiera).
    Tray(TrayEvent),
    /// Notifica di riavvio della barra delle applicazioni di sistema (`explorer.exe`).
    TaskbarCreated,
    /// Risveglio esplicito del thread del loop inviato tramite `WakeHandle`.
    Wakeup,
    /// Cambio della finestra in primo piano o del suo titolo (per regole contestuali).
    ForegroundChanged,
    /// Ricezione di `WM_QUIT`, `WM_QUERYENDSESSION` o richiesta di chiusura del loop.
    Quit,
}

unsafe extern "system" fn message_loop_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_HOTKEY => {
            PENDING_EVENTS.with(|q| {
                q.borrow_mut()
                    .push_back(MessageLoopEvent::Hotkey(HotkeyId(wparam.0 as u32)))
            });
            // SAFETY: Invia WM_WAKEUP per risvegliare GetMessageW se in attesa nel pump.
            unsafe {
                if let Err(err) = PostMessageW(Some(hwnd), WM_WAKEUP, WPARAM(0), LPARAM(0)) {
                    tracing::warn!(error = %err, "PostMessageW(WM_WAKEUP) fallito in wndproc su WM_HOTKEY");
                }
            }
            LRESULT(0)
        }
        WM_TRAY_CALLBACK => {
            let code = (lparam.0 & 0xFFFF) as u32;

            // Con --debug-log abilitato: logga a livello DEBUG i soli callback tray, escludendo WM_MOUSEMOVE
            if code != WM_MOUSEMOVE {
                tracing::debug!(
                    notification_code = format_args!("{:#06x}", code),
                    "Callback tray ricevuto"
                );
            }

            // Mentre il menu è aperto non si processano né accodano altri eventi tray
            if !is_menu_active() {
                if let Some(tray_event) = parse_tray_event(msg, lparam.0 as usize) {
                    PENDING_EVENTS
                        .with(|q| q.borrow_mut().push_back(MessageLoopEvent::Tray(tray_event)));
                    // SAFETY: I messaggi della shell possono arrivare tramite SendMessage sincrono
                    // durante l'attesa di GetMessageW. PostMessageW assicura che GetMessageW si sblocchi immediatamente.
                    unsafe {
                        if let Err(err) = PostMessageW(Some(hwnd), WM_WAKEUP, WPARAM(0), LPARAM(0))
                        {
                            tracing::warn!(error = %err, "PostMessageW(WM_WAKEUP) fallito in wndproc su WM_TRAY_CALLBACK");
                        }
                    }
                }
            }
            LRESULT(0)
        }
        msg if TASKBAR_CREATED_MSG.with(|m| *m.borrow() != 0 && msg == *m.borrow()) => {
            PENDING_EVENTS.with(|q| q.borrow_mut().push_back(MessageLoopEvent::TaskbarCreated));
            // SAFETY: TaskbarCreated è un broadcast inviato; sveglia GetMessageW.
            unsafe {
                if let Err(err) = PostMessageW(Some(hwnd), WM_WAKEUP, WPARAM(0), LPARAM(0)) {
                    tracing::warn!(error = %err, "PostMessageW(WM_WAKEUP) fallito in wndproc su TaskbarCreated");
                }
            }
            LRESULT(0)
        }
        WM_WAKEUP => {
            // Risveglio asincrono del message pump (generato internamente per sbloccare GetMessageW
            // o inviato da WakeHandle). Nessun evento accodato qui: lo svuotamento avviene in pump_one_event.
            LRESULT(0)
        }
        WM_CLOSE => {
            // Consente a Windows Installer / Restart Manager o chiusure pulite via WM_CLOSE
            // di arrestare ordinatamente il daemon senza passare da DefWindowProcW (che distruggerebbe subito l'HWND).
            PENDING_EVENTS.with(|q| q.borrow_mut().push_back(MessageLoopEvent::Quit));
            unsafe {
                if let Err(err) = PostMessageW(Some(hwnd), WM_WAKEUP, WPARAM(0), LPARAM(0)) {
                    tracing::warn!(error = %err, "PostMessageW(WM_WAKEUP) fallito in wndproc su WM_CLOSE");
                }
            }
            LRESULT(0)
        }
        WM_QUERYENDSESSION => {
            // Windows notifica l'imminente chiusura della sessione utente o del sistema.
            // Accoda l'uscita pulita ed esprime parere favorevole (1 / TRUE) alla chiusura.
            PENDING_EVENTS.with(|q| q.borrow_mut().push_back(MessageLoopEvent::Quit));
            unsafe {
                if let Err(err) = PostMessageW(Some(hwnd), WM_WAKEUP, WPARAM(0), LPARAM(0)) {
                    tracing::warn!(error = %err, "PostMessageW(WM_WAKEUP) fallito in wndproc su WM_QUERYENDSESSION");
                }
            }
            LRESULT(1)
        }
        WM_ENDSESSION => {
            if wparam.0 != 0 {
                PENDING_EVENTS.with(|q| q.borrow_mut().push_back(MessageLoopEvent::Quit));
                unsafe {
                    if let Err(err) = PostMessageW(Some(hwnd), WM_WAKEUP, WPARAM(0), LPARAM(0)) {
                        tracing::warn!(error = %err, "PostMessageW(WM_WAKEUP) fallito in wndproc su WM_ENDSESSION");
                    }
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            // SAFETY: Invia WM_QUIT al message pump del thread corrente.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => {
            // SAFETY: Inoltra i messaggi non consumati al gestore di default di Windows.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }
}

/// Handle leggero e thread-safe (`Send + Sync`) per risvegliare il message loop principale.
///
/// Espone unicamente il metodo `wake()`, garantendo a compile-time che nessun'altra
/// operazione sulla finestra possa essere eseguita da thread secondari.
#[derive(Debug, Clone, Copy)]
pub struct WakeHandle {
    raw_hwnd: HWND,
}

// SAFETY: `WakeHandle` racchiude un `HWND` nativo ed espone unicamente l'invocazione di `PostMessageW`.
// `PostMessageW` è una system call Win32 thread-safe che accoda messaggi nella coda asincrona del thread
// proprietario senza accedere a memoria condivisa né allo stato interno della finestra.
// ATTENZIONE: PostMessageW a livello kernel verifica unicamente che l'handle esista nel sistema, non che
// appartenga ancora alla nostra specifica finestra; se la finestra viene distrutta e il sistema operativo
// rialloca lo stesso valore di HWND a una nuova finestra (riuso dell'handle), PostMessageW invierebbe
// il messaggio a una finestra estranea.
// INVARIANTE DA RISPETTARE: Nessun thread possessore di un `WakeHandle` deve essere attivo quando la
// finestra Win32 sottostante viene distrutta. Il ciclo di vita di tutti i thread worker che detengono un
// `WakeHandle` deve essere rigidamente sincronizzato (tramite WorkerThreads e join con timeout prima del drop).
unsafe impl Send for WakeHandle {}
unsafe impl Sync for WakeHandle {}

static_assertions::assert_impl_all!(WakeHandle: Send, Sync);

impl WakeHandle {
    /// Inizializza un `WakeHandle` privo di finestra associata (utilizzato nei soli test unitari disaccoppiati).
    #[cfg(test)]
    pub fn new_null() -> Self {
        Self {
            raw_hwnd: HWND::default(),
        }
    }

    /// Invia un messaggio asincrono `WM_WAKEUP` al message loop Win32.
    pub fn wake(&self) -> Result<(), PlatformError> {
        // SAFETY: `PostMessageW` accoda in modo asincrono un messaggio non bloccante.
        let res = unsafe { PostMessageW(Some(self.raw_hwnd), WM_WAKEUP, WPARAM(0), LPARAM(0)) };

        if let Err(err) = res {
            tracing::warn!(error = %err, "PostMessageW(WM_WAKEUP) fallito durante il risveglio da WakeHandle");
            return Err(PlatformError::OsError(
                "PostMessageW fallito durante il risveglio del message loop".to_string(),
            ));
        }

        Ok(())
    }
}

/// Finestra Win32 invisibile (top-level) e gestore dell'unico message pump del daemon.
///
/// Creata con stile `WS_POPUP` e `WS_EX_TOOLWINDOW`, senza genitore e mai mostrata a video
/// (nessuna chiamata a `ShowWindow`). Questa configurazione è necessaria perché:
/// 1. `RegisterHotKey` e `WM_HOTKEY` richiedono una finestra appartenente alla gerarchia desktop.
/// 2. I broadcast di sistema (come `TaskbarCreated`) non vengono consegnati a `HWND_MESSAGE`.
/// 3. `SetForegroundWindow` per i menu contestuali richiede una finestra top-level.
/// 4. I messaggi di shutdown `WM_QUERYENDSESSION` e `WM_ENDSESSION` sono inviati solo a top-level.
///
/// Rispetta rigorosamente la thread-affinity Win32: è `!Send` e `!Sync`, posseduta e gestita
/// unicamente dal thread principale del daemon.
pub struct MessageLoop {
    hwnd: SafeHwnd,
    workers: WorkerThreads,
    _not_send_sync: PhantomData<*const ()>,
}

// Verifica a compile-time: MessageLoop NON deve implementare Send né Sync.
static_assertions::assert_not_impl_any!(MessageLoop: Send, Sync);

impl MessageLoop {
    /// Inizializza la classe finestra, registra il messaggio broadcast `TaskbarCreated`
    /// e crea la finestra invisibile top-level.
    pub fn new() -> Result<Self, PlatformError> {
        // SAFETY: Ottiene l'handle del modulo corrente per la registrazione della classe finestra.
        let hinstance = unsafe { GetModuleHandleW(None) }
            .map_err(|e| PlatformError::WindowCreationFailed(e.to_string()))?;

        let wnd_class = WNDCLASSW {
            lpfnWndProc: Some(message_loop_wndproc),
            hInstance: hinstance.into(),
            lpszClassName: MESSAGE_LOOP_CLASS_NAME,
            ..Default::default()
        };

        // SAFETY: Registrazione documentata della classe finestra con WNDCLASSW.
        unsafe {
            let atom = RegisterClassW(&wnd_class);
            if atom == 0 {
                let err = windows::Win32::Foundation::GetLastError();
                // 1410 = ERROR_CLASS_ALREADY_EXISTS (ammissibile se registrata in precedenza nello stesso processo)
                if err.0 != 1410 {
                    return Err(PlatformError::WindowCreationFailed(format!(
                        "RegisterClassW fallito: {:?}",
                        err
                    )));
                }
            }
        }

        // SAFETY: Registrazione del messaggio di broadcast inviato da Explorer al riavvio della taskbar.
        let taskbar_created_msg = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
        TASKBAR_CREATED_MSG.with(|m| *m.borrow_mut() = taskbar_created_msg);

        // SAFETY: Creazione di una finestra top-level invisibile (WS_POPUP | WS_EX_TOOLWINDOW).
        // Dimensioni 0x0, genitore None, mai mostrata (nessuna ShowWindow).
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default() | WS_EX_TOOLWINDOW,
                MESSAGE_LOOP_CLASS_NAME,
                w!("RustBind_MessageLoop"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(hinstance.into()),
                None,
            )
        }
        .map_err(|e| PlatformError::WindowCreationFailed(e.to_string()))?;

        Ok(Self {
            hwnd: SafeHwnd::new(hwnd),
            workers: WorkerThreads::new(),
            _not_send_sync: PhantomData,
        })
    }

    /// Restituisce l'handle `HWND` nativo della finestra invisibile.
    pub fn hwnd(&self) -> HWND {
        self.hwnd.raw()
    }

    /// Genera un `WakeHandle` thread-safe (`Send + Sync`) per risvegliare questo loop.
    pub fn wake_handle(&self) -> WakeHandle {
        WakeHandle {
            raw_hwnd: self.hwnd.raw(),
        }
    }

    /// Fornisce accesso mutabile al gestore dei worker thread associati al loop.
    pub fn worker_threads_mut(&mut self) -> &mut WorkerThreads {
        &mut self.workers
    }

    /// Fornisce accesso in sola lettura al gestore dei worker thread.
    pub fn worker_threads(&self) -> &WorkerThreads {
        &self.workers
    }

    /// Estrae e smista il prossimo evento Win32 bloccando il thread finché non arriva un messaggio.
    pub fn pump_one_event(&self) -> MessageLoopEvent {
        loop {
            // 1. Svuota la coda thread-local prima di invocare GetMessageW
            if let Some(event) = PENDING_EVENTS.with(|q| q.borrow_mut().pop_front()) {
                return event;
            }

            // 2. Blocca ed estrae il prossimo messaggio dalla message queue del thread
            let mut msg = MSG::default();
            // SAFETY: GetMessageW estrae il prossimo messaggio bloccando il thread finché non è disponibile.
            let ret = unsafe { GetMessageW(&mut msg, None, 0, 0) };
            if ret.0 <= 0 {
                return MessageLoopEvent::Quit;
            }

            // SAFETY: Traduzione e dispatching al wndproc (che popola la coda thread-local ed invia WM_WAKEUP).
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            // 3. Svuota la coda thread-local dopo ogni DispatchMessageW
            if let Some(event) = PENDING_EVENTS.with(|q| q.borrow_mut().pop_front()) {
                return event;
            }

            // 4. Se msg era un WM_WAKEUP esterno e nessun evento applicativo era in coda
            if msg.message == WM_WAKEUP {
                return MessageLoopEvent::Wakeup;
            }
        }
    }
}

impl Drop for MessageLoop {
    fn drop(&mut self) {
        let timeout = std::time::Duration::from_millis(1000);
        if !self.workers.join_all(timeout) {
            tracing::warn!(
                "Uno o più worker thread con WakeHandle non sono terminati entro il timeout: \
                 preservo la finestra Win32 per prevenire il riuso dell'handle (HWND reuse)"
            );
            // Preserva la finestra disarmando SafeHwnd per NON invocare DestroyWindow
            let empty_hwnd = SafeHwnd::new(HWND::default());
            let prev = std::mem::replace(&mut self.hwnd, empty_hwnd);
            let _ = prev.leak();
        }
    }
}

/// Imposta lo stato di apertura del menu contestuale.
pub fn set_menu_active(active: bool) {
    MENU_ACTIVE.with(|m| *m.borrow_mut() = active);
}

/// Indica se il menu contestuale è attualmente aperto.
pub fn is_menu_active() -> bool {
    MENU_ACTIVE.with(|m| *m.borrow())
}

/// Rimuove tutti gli eventi tray pendenti dalla coda thread-local.
///
/// Invocata subito dopo la chiusura di `TrackPopupMenuEx` per garantire che notifiche multiple
/// o clic effettuati sul menu non provochino la riapertura involontaria del menu.
pub fn clear_pending_tray_events() {
    PENDING_EVENTS.with(|q| {
        q.borrow_mut()
            .retain(|ev| !matches!(ev, MessageLoopEvent::Tray(_)));
    });
}

/// Accoda un evento nella coda thread-local (usato per simulazioni e test).
pub fn enqueue_event(event: MessageLoopEvent) {
    PENDING_EVENTS.with(|q| q.borrow_mut().push_back(event));
}

/// Accoda `ForegroundChanged` in modo coalescente evitando duplicati in coda.
///
/// Restituisce `true` se l'evento è stato appena accodato.
pub fn enqueue_foreground_changed_coalesced() -> bool {
    PENDING_EVENTS.with(|q| {
        let mut queue = q.borrow_mut();
        if queue
            .iter()
            .any(|ev| matches!(ev, MessageLoopEvent::ForegroundChanged))
        {
            return false;
        }
        queue.push_back(MessageLoopEvent::ForegroundChanged);
        true
    })
}

/// Estrae il prossimo evento dalla coda thread-local (usato per verifiche e test).
pub fn dequeue_event() -> Option<MessageLoopEvent> {
    PENDING_EVENTS.with(|q| q.borrow_mut().pop_front())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pending_events_queue_pure_fifo() {
        // Verifica accodamento e svuotamento FIFO puramente in memoria (senza chiamate OS)
        enqueue_event(MessageLoopEvent::Hotkey(HotkeyId(101)));
        enqueue_event(MessageLoopEvent::Tray(TrayEvent::ContextMenu));
        enqueue_event(MessageLoopEvent::TaskbarCreated);

        assert_eq!(
            dequeue_event(),
            Some(MessageLoopEvent::Hotkey(HotkeyId(101)))
        );
        assert_eq!(
            dequeue_event(),
            Some(MessageLoopEvent::Tray(TrayEvent::ContextMenu))
        );
        assert_eq!(dequeue_event(), Some(MessageLoopEvent::TaskbarCreated));
        assert_eq!(dequeue_event(), None);
    }

    #[test]
    fn test_clear_pending_tray_events_pure() {
        enqueue_event(MessageLoopEvent::Hotkey(HotkeyId(1)));
        enqueue_event(MessageLoopEvent::Tray(TrayEvent::ContextMenu));
        enqueue_event(MessageLoopEvent::Tray(TrayEvent::DoubleClick));
        enqueue_event(MessageLoopEvent::TaskbarCreated);

        clear_pending_tray_events();

        // Devono rimanere solo gli eventi non-tray in ordine FIFO
        assert_eq!(dequeue_event(), Some(MessageLoopEvent::Hotkey(HotkeyId(1))));
        assert_eq!(dequeue_event(), Some(MessageLoopEvent::TaskbarCreated));
        assert_eq!(dequeue_event(), None);
    }

    #[test]
    fn test_menu_active_flag_pure() {
        assert!(!is_menu_active());
        set_menu_active(true);
        assert!(is_menu_active());
        set_menu_active(false);
        assert!(!is_menu_active());
    }

    #[test]
    #[ignore = "Richiede interazione con il sistema Windows reale"]
    fn test_message_loop_wake_handle_real() {
        let loop_win = MessageLoop::new().expect("Creazione message loop");
        let wake_handle = loop_win.wake_handle();

        // Invia il risveglio asincrono
        assert!(wake_handle.wake().is_ok());

        // Estrae l'evento dal loop
        let event = loop_win.pump_one_event();
        assert_eq!(event, MessageLoopEvent::Wakeup);
    }

    #[test]
    #[ignore = "Richiede interazione con il sistema Windows reale"]
    fn test_sent_message_wakes_pump_real() {
        use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_CONTEXTMENU};

        let loop_win = MessageLoop::new().expect("Creazione message loop");

        // Simula un messaggio inviato direttamente con SendMessageW (come fa Explorer per la tray)
        // SendMessageW invoca direttamente message_loop_wndproc, che accoda TrayEvent::ContextMenu
        // e posta WM_WAKEUP alla finestra per sbloccare GetMessageW.
        unsafe {
            let _ = SendMessageW(
                loop_win.hwnd(),
                WM_TRAY_CALLBACK,
                Some(WPARAM(0)),
                Some(LPARAM(WM_CONTEXTMENU as isize)),
            );
        }

        // Il pump deve estrarre immediatamente l'evento senza rimanere bloccato
        let event = loop_win.pump_one_event();
        assert_eq!(event, MessageLoopEvent::Tray(TrayEvent::ContextMenu));
    }
}
