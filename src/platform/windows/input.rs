use std::collections::HashSet;
use std::mem::size_of;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MOUSE_EVENT_FLAGS,
    MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, MapVirtualKeyW, SendInput,
    VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL,
    VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
};

use crate::core::action::{InputAction, MouseButton, MouseWheelDirection};
use crate::core::key::VirtualKey;
use crate::core::modifiers::Modifiers;
use crate::platform::traits::{InputSender, PhysicalModifierKey, PlatformError};
use crate::platform::windows::conversion::{is_extended_virtual_key, to_win32_vk};

/// Valore Win32 standard (`WHEEL_DELTA`) corrispondente a una tacca della rotella del mouse.
const MOUSE_WHEEL_DELTA: i32 = 120;

/// Esecutore nativo Win32 per l'invio di input sintetico tramite `SendInput`.
///
/// Rispetta rigorosamente i punti 7-9 della Sezione 3.4 di `docs/ARCHITECTURE.md`:
/// - Controllo del valore di ritorno su ogni batch (fail-closed, nessun retry aggressivo).
/// - Invio di testo Unicode con gestione corretta di coppie surrogate UTF-16.
/// - Invio di input mouse tramite `INPUT_MOUSE` (click, rotella, spostamenti relativi).
/// - Tracciamento e rilascio esclusivo dei tasti e pulsanti mouse premuti dal programma.
/// - Attesa interrompibile del rilascio dei modificatori fisici con timeout.
///
/// REGOLA DI PRIVACY E SICUREZZA PER `GetAsyncKeyState`:
/// `GetAsyncKeyState` è interrogata ESCLUSIVAMENTE per i tasti modificatori fisici
/// (`Ctrl`, `Alt`, `Shift`, `Win`, sia varianti generiche che sinistra/destra) al solo scopo di
/// sincronizzare il rilascio del trigger dell'hotkey. È TASSATIVAMENTE VIETATO interrogare
/// altri tasti (alfanumerici, funzione, navigazione), escludendo alla radice qualsiasi
/// rischio o comportamento assimilabile a un keylogger.
pub struct WindowsInputSender {
    pressed_keys: Mutex<HashSet<VirtualKey>>,
    pressed_mouse_buttons: Mutex<HashSet<MouseButton>>,
}

// Verifica a compile-time: WindowsInputSender DEVE essere Send + Sync per l'uso nel thread di esecuzione.
static_assertions::assert_impl_all!(WindowsInputSender: Send, Sync);

impl Default for WindowsInputSender {
    fn default() -> Self {
        Self {
            pressed_keys: Mutex::new(HashSet::new()),
            pressed_mouse_buttons: Mutex::new(HashSet::new()),
        }
    }
}

impl WindowsInputSender {
    pub fn new() -> Self {
        Self::default()
    }

    /// Helper interno per inviare un array di `INPUT` tramite Win32 `SendInput`.
    ///
    /// Controlla sempre che tutti gli eventi siano stati inseriti con successo.
    fn send_inputs_batch(&self, inputs: &[INPUT]) -> Result<(), PlatformError> {
        if inputs.is_empty() {
            return Ok(());
        }

        // SAFETY: Invocazione documentata di SendInput con puntatore ad array di struct INPUT valide.
        let inserted = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };

        if inserted != inputs.len() as u32 {
            // Invio parziale o bloccato (es. isolamento UIPI o desktop protetto)
            return Err(PlatformError::InputSendFailed {
                expected: inputs.len(),
                inserted: inserted as usize,
            });
        }

        Ok(())
    }

    /// Crea una struct `INPUT` per un evento da tastiera popolando hardware scan code e flag estesi.
    fn make_key_input_vk(vk: VIRTUAL_KEY, is_extended: bool, is_up: bool) -> INPUT {
        // SAFETY: Lookup di solo valore VK -> scan code hardware sulla mappa corrente, senza puntatori.
        let scan_code = unsafe { MapVirtualKeyW(vk.0 as u32, MAPVK_VK_TO_VSC) } as u16;
        let mut flags = KEYBD_EVENT_FLAGS(0);
        if is_extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        if is_up {
            flags |= KEYEVENTF_KEYUP;
        }

        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: scan_code,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    /// Crea una struct `INPUT` per un `VirtualKey` astratto gestendo automaticamente i tasti estesi.
    fn make_virtual_key_input(key: VirtualKey, is_up: bool) -> INPUT {
        Self::make_key_input_vk(to_win32_vk(key), is_extended_virtual_key(key), is_up)
    }

    /// Restituisce `(VIRTUAL_KEY, is_extended)` per uno specifico modificatore fisico.
    fn physical_modifier_spec(modifier: PhysicalModifierKey) -> (VIRTUAL_KEY, bool) {
        match modifier {
            PhysicalModifierKey::LeftControl => (VK_LCONTROL, false),
            PhysicalModifierKey::RightControl => (VK_RCONTROL, true),
            PhysicalModifierKey::LeftAlt => (VK_LMENU, false),
            PhysicalModifierKey::RightAlt => (VK_RMENU, true),
            PhysicalModifierKey::LeftShift => (VK_LSHIFT, false),
            PhysicalModifierKey::RightShift => (VK_RSHIFT, false),
            PhysicalModifierKey::LeftWin => (VK_LWIN, true),
            PhysicalModifierKey::RightWin => (VK_RWIN, true),
        }
    }

    /// Indica se il modificatore usa `Alt` o `Win` (può attivare barra menu o Start al rilascio isolato).
    fn modifier_needs_menu_mask(modifier: PhysicalModifierKey) -> bool {
        matches!(
            modifier,
            PhysicalModifierKey::LeftAlt
                | PhysicalModifierKey::RightAlt
                | PhysicalModifierKey::LeftWin
                | PhysicalModifierKey::RightWin
        )
    }

    fn is_shift_modifier(modifier: PhysicalModifierKey) -> bool {
        matches!(
            modifier,
            PhysicalModifierKey::LeftShift | PhysicalModifierKey::RightShift
        )
    }

    fn is_ctrl_modifier(modifier: PhysicalModifierKey) -> bool {
        matches!(
            modifier,
            PhysicalModifierKey::LeftControl | PhysicalModifierKey::RightControl
        )
    }

    /// Costruisce il batch `SendInput` per rilasciare temporaneamente i modificatori fisici del trigger.
    ///
    /// Usa `VK_LCONTROL` come maschera riconosciuta da Win32, WinUI3 ed Electron/Chromium quando
    /// `Alt` o `Win` vanno rilasciati senza attivare la barra menu o il menu Start, evitando sempre
    /// la sovrapposizione `Ctrl+Shift` (anche quando `Shift` è premuto in modalità pass-through).
    fn build_suppress_modifier_inputs(
        held: &[PhysicalModifierKey],
        passthrough_shifts: &[PhysicalModifierKey],
        passthrough_ctrl_held: bool,
    ) -> Vec<INPUT> {
        let has_menu = held.iter().copied().any(Self::modifier_needs_menu_mask);
        let has_ctrl = passthrough_ctrl_held || held.iter().copied().any(Self::is_ctrl_modifier);
        let need_ctrl_mask = has_menu && !has_ctrl;
        let mut inputs = Vec::with_capacity(held.len() + passthrough_shifts.len() * 2 + 2);

        // 1. Rilascia prima eventuali Shift del trigger e (solo se serve maschera Ctrl) eventuali
        //    Shift pass-through, così Ctrl+Shift non si sovrappone mai.
        for modifier in held.iter().copied().filter(|m| Self::is_shift_modifier(*m)) {
            let (vk, is_extended) = Self::physical_modifier_spec(modifier);
            inputs.push(Self::make_key_input_vk(vk, is_extended, true));
        }
        if need_ctrl_mask {
            for modifier in passthrough_shifts {
                let (vk, is_extended) = Self::physical_modifier_spec(*modifier);
                inputs.push(Self::make_key_input_vk(vk, is_extended, true));
            }
            inputs.push(Self::make_key_input_vk(VK_LCONTROL, false, false));
        }

        for modifier in held
            .iter()
            .copied()
            .filter(|m| Self::modifier_needs_menu_mask(*m))
        {
            let (vk, is_extended) = Self::physical_modifier_spec(modifier);
            inputs.push(Self::make_key_input_vk(vk, is_extended, true));
        }

        if need_ctrl_mask {
            inputs.push(Self::make_key_input_vk(VK_LCONTROL, false, true));
            // Ripristina subito eventuali Shift pass-through prima di eseguire le azioni (es. Shift+End).
            for modifier in passthrough_shifts {
                let (vk, is_extended) = Self::physical_modifier_spec(*modifier);
                inputs.push(Self::make_key_input_vk(vk, is_extended, false));
            }
        }

        // 3. Rilascia eventuali Ctrl originali dopo Alt/Win (così Ctrl maschera naturalmente Alt/Win).
        for modifier in held.iter().copied().filter(|m| Self::is_ctrl_modifier(*m)) {
            let (vk, is_extended) = Self::physical_modifier_spec(modifier);
            inputs.push(Self::make_key_input_vk(vk, is_extended, true));
        }

        inputs
    }

    /// Costruisce il batch `SendInput` per ripristinare i modificatori tuttora premuti dall'utente.
    fn build_restore_modifier_inputs(
        still_held: &[PhysicalModifierKey],
        passthrough_shifts: &[PhysicalModifierKey],
        passthrough_ctrl_held: bool,
    ) -> Vec<INPUT> {
        let has_menu = still_held
            .iter()
            .copied()
            .any(Self::modifier_needs_menu_mask);
        let has_ctrl =
            passthrough_ctrl_held || still_held.iter().copied().any(Self::is_ctrl_modifier);
        let need_ctrl_mask = has_menu && !has_ctrl;
        let mut inputs = Vec::with_capacity(still_held.len() + passthrough_shifts.len() * 2 + 2);

        if need_ctrl_mask {
            for modifier in passthrough_shifts {
                let (vk, is_extended) = Self::physical_modifier_spec(*modifier);
                inputs.push(Self::make_key_input_vk(vk, is_extended, true));
            }
        }

        for modifier in still_held
            .iter()
            .copied()
            .filter(|m| !Self::is_shift_modifier(*m))
        {
            let (vk, is_extended) = Self::physical_modifier_spec(modifier);
            inputs.push(Self::make_key_input_vk(vk, is_extended, false));
        }

        // Dopo Alt/Win Down (senza Ctrl già presente) invia LControl Down/Up prima di eventuali Shift.
        if need_ctrl_mask {
            inputs.push(Self::make_key_input_vk(VK_LCONTROL, false, false));
            inputs.push(Self::make_key_input_vk(VK_LCONTROL, false, true));
            for modifier in passthrough_shifts {
                let (vk, is_extended) = Self::physical_modifier_spec(*modifier);
                inputs.push(Self::make_key_input_vk(vk, is_extended, false));
            }
        }

        for modifier in still_held
            .iter()
            .copied()
            .filter(|m| Self::is_shift_modifier(*m))
        {
            let (vk, is_extended) = Self::physical_modifier_spec(modifier);
            inputs.push(Self::make_key_input_vk(vk, is_extended, false));
        }

        inputs
    }

    /// Rileva eventuali tasti `Shift` tenuti fisicamente dall'utente al di fuori del trigger della regola.
    fn held_passthrough_shifts(trigger_modifiers: &Modifiers) -> Vec<PhysicalModifierKey> {
        if trigger_modifiers.shift {
            return Vec::new();
        }
        let mut shifts = Vec::with_capacity(2);
        for modifier in [
            PhysicalModifierKey::LeftShift,
            PhysicalModifierKey::RightShift,
        ] {
            let (vk, _) = Self::physical_modifier_spec(modifier);
            if Self::is_modifier_vk_down(vk) {
                shifts.push(modifier);
            }
        }
        shifts
    }

    /// Indica se l'utente tiene premuto `Ctrl` al di fuori dei modificatori propri del trigger.
    fn has_passthrough_ctrl(trigger_modifiers: &Modifiers) -> bool {
        !trigger_modifiers.ctrl
            && (Self::is_modifier_vk_down(VK_LCONTROL) || Self::is_modifier_vk_down(VK_RCONTROL))
    }

    /// Interroga esclusivamente lo stato fisico di un tasto modificatore tramite `GetAsyncKeyState`.
    fn is_modifier_vk_down(vk: VIRTUAL_KEY) -> bool {
        // SAFETY: Chiamata circoscritta ai soli tasti modificatori fisici (regola privacy/anticheat).
        let state = unsafe { GetAsyncKeyState(vk.0 as i32) };
        (state as u16 & 0x8000) != 0
    }

    /// Elenca i modificatori fisici richiesti da `modifiers` che risultano attualmente premuti.
    fn held_trigger_modifiers(modifiers: &Modifiers) -> Vec<PhysicalModifierKey> {
        let mut held = Vec::with_capacity(4);
        let candidates: [(bool, PhysicalModifierKey, PhysicalModifierKey); 4] = [
            (
                modifiers.ctrl,
                PhysicalModifierKey::LeftControl,
                PhysicalModifierKey::RightControl,
            ),
            (
                modifiers.alt,
                PhysicalModifierKey::LeftAlt,
                PhysicalModifierKey::RightAlt,
            ),
            (
                modifiers.shift,
                PhysicalModifierKey::LeftShift,
                PhysicalModifierKey::RightShift,
            ),
            (
                modifiers.win,
                PhysicalModifierKey::LeftWin,
                PhysicalModifierKey::RightWin,
            ),
        ];

        for (enabled, left_mod, right_mod) in candidates {
            if !enabled {
                continue;
            }
            let (left_vk, _) = Self::physical_modifier_spec(left_mod);
            let (right_vk, _) = Self::physical_modifier_spec(right_mod);
            if Self::is_modifier_vk_down(left_vk) {
                held.push(left_mod);
            }
            if Self::is_modifier_vk_down(right_vk) {
                held.push(right_mod);
            }
        }
        held
    }

    /// Crea una struct INPUT per un'unità di codice UTF-16 (Unicode).
    fn make_unicode_input(code_unit: u16, is_up: bool) -> INPUT {
        let mut flags = KEYEVENTF_UNICODE;
        if is_up {
            flags |= KEYEVENTF_KEYUP;
        }

        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: code_unit,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    /// Crea una struct `INPUT` per pressione/rilascio di un pulsante mouse (`INPUT_MOUSE`).
    fn make_mouse_button_input(button: MouseButton, is_up: bool) -> INPUT {
        let (flags, mouse_data) = match (button, is_up) {
            (MouseButton::Left, false) => (MOUSEEVENTF_LEFTDOWN, 0u32),
            (MouseButton::Left, true) => (MOUSEEVENTF_LEFTUP, 0u32),
            (MouseButton::Right, false) => (MOUSEEVENTF_RIGHTDOWN, 0u32),
            (MouseButton::Right, true) => (MOUSEEVENTF_RIGHTUP, 0u32),
            (MouseButton::Middle, false) => (MOUSEEVENTF_MIDDLEDOWN, 0u32),
            (MouseButton::Middle, true) => (MOUSEEVENTF_MIDDLEUP, 0u32),
            (MouseButton::X1, false) => (MOUSEEVENTF_XDOWN, 1u32),
            (MouseButton::X1, true) => (MOUSEEVENTF_XUP, 1u32),
            (MouseButton::X2, false) => (MOUSEEVENTF_XDOWN, 2u32),
            (MouseButton::X2, true) => (MOUSEEVENTF_XUP, 2u32),
        };

        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: mouse_data,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    /// Crea una struct `INPUT` per scorrimento rotella verticale od orizzontale (`INPUT_MOUSE`).
    fn make_mouse_wheel_input(direction: MouseWheelDirection, steps: i32) -> INPUT {
        let flags: MOUSE_EVENT_FLAGS = match direction {
            MouseWheelDirection::Vertical => MOUSEEVENTF_WHEEL,
            MouseWheelDirection::Horizontal => MOUSEEVENTF_HWHEEL,
        };
        let wheel_delta = steps.saturating_mul(MOUSE_WHEEL_DELTA);

        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: wheel_delta as u32,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    /// Crea una struct `INPUT` per spostamento relativo del puntatore (`MOUSEEVENTF_MOVE`).
    fn make_mouse_move_input(dx: i32, dy: i32) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx,
                    dy,
                    mouseData: 0,
                    dwFlags: MOUSEEVENTF_MOVE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }
}

impl InputSender for WindowsInputSender {
    fn send_action(
        &self,
        action: &InputAction,
        cancel_flag: &AtomicBool,
    ) -> Result<(), PlatformError> {
        // Controllo atomico preventivo di annullamento
        if cancel_flag.load(Ordering::Relaxed) {
            let _ = self.release_pressed_keys();
            return Err(PlatformError::ExecutionCancelled);
        }

        match action {
            InputAction::KeyClick { key } => {
                let inputs = [
                    Self::make_virtual_key_input(*key, false),
                    Self::make_virtual_key_input(*key, true),
                ];
                self.send_inputs_batch(&inputs)?;
            }
            InputAction::KeyDown { key } => {
                let inputs = [Self::make_virtual_key_input(*key, false)];
                self.send_inputs_batch(&inputs)?;
                // Traccia il tasto premuto solo dopo l'esito positivo
                let mut pressed = self
                    .pressed_keys
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                pressed.insert(*key);
            }
            InputAction::KeyUp { key } => {
                let inputs = [Self::make_virtual_key_input(*key, true)];
                self.send_inputs_batch(&inputs)?;
                let mut pressed = self
                    .pressed_keys
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                pressed.remove(key);
            }
            InputAction::SendText { text } => {
                // Conversione in UTF-16 (include nativamente coppie surrogate nell'ordine corretto)
                let mut inputs = Vec::with_capacity(text.len() * 2);
                for code_unit in text.encode_utf16() {
                    // Pressione (down)
                    inputs.push(Self::make_unicode_input(code_unit, false));
                    // Rilascio (up)
                    inputs.push(Self::make_unicode_input(code_unit, true));
                }
                self.send_inputs_batch(&inputs)?;
            }
            InputAction::Delay { milliseconds } => {
                // Attesa deterministica e interrompibile a fette di 5 millisecondi
                let deadline = Instant::now() + Duration::from_millis(*milliseconds);
                while Instant::now() < deadline {
                    if cancel_flag.load(Ordering::Relaxed) {
                        let _ = self.release_pressed_keys();
                        return Err(PlatformError::ExecutionCancelled);
                    }
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    std::thread::sleep(Duration::from_millis(5).min(remaining));
                }
            }
            InputAction::MouseClick { button } => {
                let inputs = [
                    Self::make_mouse_button_input(*button, false),
                    Self::make_mouse_button_input(*button, true),
                ];
                self.send_inputs_batch(&inputs)?;
            }
            InputAction::MouseDown { button } => {
                let inputs = [Self::make_mouse_button_input(*button, false)];
                self.send_inputs_batch(&inputs)?;
                let mut pressed = self
                    .pressed_mouse_buttons
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                pressed.insert(*button);
            }
            InputAction::MouseUp { button } => {
                let inputs = [Self::make_mouse_button_input(*button, true)];
                self.send_inputs_batch(&inputs)?;
                let mut pressed = self
                    .pressed_mouse_buttons
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                pressed.remove(button);
            }
            InputAction::MouseScroll { direction, steps } => {
                let inputs = [Self::make_mouse_wheel_input(*direction, *steps)];
                self.send_inputs_batch(&inputs)?;
            }
            InputAction::MouseMove { dx, dy } => {
                let inputs = [Self::make_mouse_move_input(*dx, *dy)];
                self.send_inputs_batch(&inputs)?;
            }
        }

        Ok(())
    }

    fn wait_for_physical_modifiers_release(
        &self,
        modifiers: &Modifiers,
        cancel_flag: &AtomicBool,
        timeout: Duration,
    ) -> Result<(), PlatformError> {
        let start = Instant::now();

        // REGOLA PRIVACY: GetAsyncKeyState è circoscritta ai soli modificatori fisici del trigger.
        // Nessun altro tasto dell'utente viene mai interrogato.
        loop {
            if cancel_flag.load(Ordering::Relaxed) {
                return Err(PlatformError::ExecutionCancelled);
            }

            let mut any_held = false;

            if modifiers.ctrl {
                // SAFETY: GetAsyncKeyState verifica lo stato hardware fisico del tasto.
                let state = unsafe { GetAsyncKeyState(VK_CONTROL.0 as i32) };
                if (state as u16 & 0x8000) != 0 {
                    any_held = true;
                }
            }

            if modifiers.alt {
                // SAFETY: GetAsyncKeyState verifica lo stato hardware di Alt (VK_MENU).
                let state = unsafe { GetAsyncKeyState(VK_MENU.0 as i32) };
                if (state as u16 & 0x8000) != 0 {
                    any_held = true;
                }
            }

            if modifiers.shift {
                // SAFETY: GetAsyncKeyState verifica lo stato hardware di Shift.
                let state = unsafe { GetAsyncKeyState(VK_SHIFT.0 as i32) };
                if (state as u16 & 0x8000) != 0 {
                    any_held = true;
                }
            }

            if modifiers.win {
                // SAFETY: GetAsyncKeyState verifica lo stato hardware dei tasti Windows.
                let lwin = unsafe { GetAsyncKeyState(VK_LWIN.0 as i32) };
                let rwin = unsafe { GetAsyncKeyState(VK_RWIN.0 as i32) };
                if (lwin as u16 & 0x8000) != 0 || (rwin as u16 & 0x8000) != 0 {
                    any_held = true;
                }
            }

            // Se nessun modificatore è premuto, l'utente li ha rilasciati tutti
            if !any_held {
                return Ok(());
            }

            if start.elapsed() >= timeout {
                return Err(PlatformError::PhysicalModifiersTimeout);
            }

            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn suppress_trigger_modifiers(
        &self,
        modifiers: &Modifiers,
        cancel_flag: &AtomicBool,
    ) -> Result<Vec<PhysicalModifierKey>, PlatformError> {
        if cancel_flag.load(Ordering::Relaxed) {
            return Err(PlatformError::ExecutionCancelled);
        }
        if !modifiers.any() {
            return Ok(Vec::new());
        }

        let held = Self::held_trigger_modifiers(modifiers);
        if held.is_empty() {
            return Ok(Vec::new());
        }

        let passthrough_shifts = Self::held_passthrough_shifts(modifiers);
        let passthrough_ctrl_held = Self::has_passthrough_ctrl(modifiers);
        let inputs =
            Self::build_suppress_modifier_inputs(&held, &passthrough_shifts, passthrough_ctrl_held);
        self.send_inputs_batch(&inputs)?;
        Ok(held)
    }

    fn restore_trigger_modifiers(
        &self,
        released_modifiers: &[PhysicalModifierKey],
        trigger_modifiers: &Modifiers,
        cancel_flag: &AtomicBool,
        force_restore: bool,
    ) -> Result<(), PlatformError> {
        if released_modifiers.is_empty() || cancel_flag.load(Ordering::Relaxed) {
            return Ok(());
        }

        // Nei remap istantanei (<40ms senza Delay) `SendInput(KEYUP)` ha azzerato GetAsyncKeyState,
        // quindi ripristiniamo subito i modificatori soppressi così che l'utente possa continuare a
        // tenere premuto Alt/Ctrl/Win e inviare altri trigger consecutivi.
        let to_restore: Vec<PhysicalModifierKey> = if force_restore {
            released_modifiers.to_vec()
        } else {
            released_modifiers
                .iter()
                .copied()
                .filter(|modifier| {
                    let (vk, _) = Self::physical_modifier_spec(*modifier);
                    Self::is_modifier_vk_down(vk)
                })
                .collect()
        };

        if to_restore.is_empty() {
            return Ok(());
        }

        let passthrough_shifts = Self::held_passthrough_shifts(trigger_modifiers);
        let passthrough_ctrl_held = Self::has_passthrough_ctrl(trigger_modifiers);
        let inputs = Self::build_restore_modifier_inputs(
            &to_restore,
            &passthrough_shifts,
            passthrough_ctrl_held,
        );
        self.send_inputs_batch(&inputs)?;
        Ok(())
    }

    fn release_pressed_keys(&self) -> Result<(), PlatformError> {
        let mut inputs = Vec::new();

        {
            let mut pressed = self
                .pressed_keys
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            for key in pressed.drain() {
                inputs.push(Self::make_virtual_key_input(key, true));
            }
        }

        {
            let mut pressed_mouse = self
                .pressed_mouse_buttons
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            for button in pressed_mouse.drain() {
                inputs.push(Self::make_mouse_button_input(button, true));
            }
        }

        if inputs.is_empty() {
            return Ok(());
        }

        self.send_inputs_batch(&inputs)?;
        Ok(())
    }
}

impl Drop for WindowsInputSender {
    fn drop(&mut self) {
        // Garantisce il rilascio dei tasti del programma alla caduta dell'esecutore
        let _ = self.release_pressed_keys();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use windows::Win32::Foundation::{GetLastError, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_F13;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, MSG, PM_REMOVE,
        PeekMessageW, RegisterClassW, SW_SHOW, SetForegroundWindow, ShowWindow, TranslateMessage,
        WINDOW_EX_STYLE, WM_CHAR, WM_KEYDOWN, WNDCLASSW, WS_OVERLAPPEDWINDOW,
    };
    use windows::core::{PCWSTR, w};

    use super::*;
    use crate::platform::windows::handles::SafeHwnd;

    thread_local! {
        static TEST_KEYDOWN_VKS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
        static TEST_UTF16_CHARS: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
    }

    unsafe extern "system" fn test_input_wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_KEYDOWN => {
                TEST_KEYDOWN_VKS.with(|v| v.borrow_mut().push((wparam.0 & 0xFFFF) as u32));
                LRESULT(0)
            }
            WM_CHAR => {
                TEST_UTF16_CHARS.with(|c| c.borrow_mut().push((wparam.0 & 0xFFFF) as u16));
                LRESULT(0)
            }
            _ => {
                // SAFETY: Inoltro standard al gestore Win32 di default nella WndProc di test.
                unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
            }
        }
    }

    fn pump_test_messages(hwnd: HWND) {
        let mut msg = MSG::default();
        // SAFETY: Svuota la coda messaggi della finestra di test con TranslateMessage per produrre WM_CHAR.
        unsafe {
            while PeekMessageW(&mut msg, Some(hwnd), 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    #[test]
    #[ignore = "Richiede interazione con il sistema Windows reale"]
    fn test_windows_send_input_real() {
        TEST_KEYDOWN_VKS.with(|v| v.borrow_mut().clear());
        TEST_UTF16_CHARS.with(|c| c.borrow_mut().clear());

        const CLASS_NAME: PCWSTR = w!("RustBind_SendInputTestWindow");
        // SAFETY: Recupera HINSTANCE del modulo corrente per registrare la classe finestra del test.
        let hinstance = unsafe { GetModuleHandleW(None) }.expect("GetModuleHandleW riuscito");

        let wnd_class = WNDCLASSW {
            lpfnWndProc: Some(test_input_wndproc),
            hInstance: hinstance.into(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };

        // SAFETY: Registra la classe finestra temporanea tollerando ERROR_CLASS_ALREADY_EXISTS (1410).
        unsafe {
            let atom = RegisterClassW(&wnd_class);
            if atom == 0 {
                let err = GetLastError();
                assert_eq!(err.0, 1410, "RegisterClassW fallito: {:?}", err);
            }
        }

        // SAFETY: Crea una finestra temporanea dedicata al test racchiusa in SafeHwnd (DestroyWindow al Drop).
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                CLASS_NAME,
                w!("RustBind SendInput Test"),
                WS_OVERLAPPEDWINDOW,
                100,
                100,
                300,
                200,
                None,
                None,
                Some(hinstance.into()),
                None,
            )
        }
        .expect("CreateWindowExW riuscito");
        let safe_hwnd = SafeHwnd::new(hwnd);

        // SAFETY: Mostra la finestra di test e richiede il focus di sistema prima di chiamare SendInput.
        unsafe {
            let _ = ShowWindow(safe_hwnd.raw(), SW_SHOW);
            let _ = SetForegroundWindow(safe_hwnd.raw());
        }
        pump_test_messages(safe_hwnd.raw());
        std::thread::sleep(Duration::from_millis(50));
        pump_test_messages(safe_hwnd.raw());

        // SAFETY: Verifica rigorosa del focus prima di inviare qualsiasi input sintetico reale.
        let foreground = unsafe { GetForegroundWindow() };
        assert_eq!(
            foreground,
            safe_hwnd.raw(),
            "La finestra di test non ha il focus (GetForegroundWindow != hwnd): annullato per non inviare tasti a finestre esterne"
        );

        let sender = WindowsInputSender::new();
        let cancel = AtomicBool::new(false);

        let sample_text = "okè\u{1F600}";
        let expected_utf16: Vec<u16> = sample_text.encode_utf16().collect();

        sender
            .send_action(
                &InputAction::KeyClick {
                    key: VirtualKey::F13,
                },
                &cancel,
            )
            .expect("KeyClick F13 riuscito");

        sender
            .send_action(
                &InputAction::SendText {
                    text: sample_text.to_string(),
                },
                &cancel,
            )
            .expect("SendText Unicode riuscito");

        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            pump_test_messages(safe_hwnd.raw());
            let has_key = TEST_KEYDOWN_VKS.with(|v| v.borrow().contains(&(VK_F13.0 as u32)));
            let chars_len = TEST_UTF16_CHARS.with(|c| c.borrow().len());
            if has_key && chars_len >= expected_utf16.len() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        let captured_keys = TEST_KEYDOWN_VKS.with(|v| v.borrow().clone());
        let captured_chars = TEST_UTF16_CHARS.with(|c| c.borrow().clone());

        assert!(
            captured_keys.contains(&(VK_F13.0 as u32)),
            "Atteso WM_KEYDOWN per VK_F13, ricevuti: {:?}",
            captured_keys
        );
        assert_eq!(
            captured_chars, expected_utf16,
            "La sequenza UTF-16 WM_CHAR ricevuta non corrisponde a '{}'",
            sample_text
        );

        drop(safe_hwnd);
    }

    #[test]
    fn test_make_virtual_key_input_extended_flags_and_scancodes() {
        for key in [VirtualKey::End, VirtualKey::Home, VirtualKey::NumpadEnter] {
            let down = WindowsInputSender::make_virtual_key_input(key, false);
            let up = WindowsInputSender::make_virtual_key_input(key, true);
            // SAFETY: lettura di campi valore della union INPUT appena costruita con tipo tastiera.
            unsafe {
                assert!(
                    down.Anonymous.ki.dwFlags.contains(KEYEVENTF_EXTENDEDKEY),
                    "{:?} down deve includere KEYEVENTF_EXTENDEDKEY",
                    key
                );
                assert!(
                    up.Anonymous
                        .ki
                        .dwFlags
                        .contains(KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP),
                    "{:?} up deve includere KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP",
                    key
                );
                assert_ne!(
                    down.Anonymous.ki.wScan, 0,
                    "{:?} deve popolare hardware scan code",
                    key
                );
            }
        }

        let enter_down = WindowsInputSender::make_virtual_key_input(VirtualKey::Enter, false);
        // SAFETY: lettura di campi valore della union INPUT appena costruita con tipo tastiera.
        unsafe {
            assert!(
                !enter_down
                    .Anonymous
                    .ki
                    .dwFlags
                    .contains(KEYEVENTF_EXTENDEDKEY),
                "Enter standard non deve avere KEYEVENTF_EXTENDEDKEY"
            );
            assert_ne!(enter_down.Anonymous.ki.wScan, 0);
        }
    }

    fn decode_input_events(inputs: &[INPUT]) -> Vec<(u16, bool)> {
        inputs
            .iter()
            .map(|inp| {
                // SAFETY: lettura di campi valore della union INPUT appena costruita con tipo tastiera.
                unsafe {
                    (
                        inp.Anonymous.ki.wVk.0,
                        inp.Anonymous.ki.dwFlags.contains(KEYEVENTF_KEYUP),
                    )
                }
            })
            .collect()
    }

    #[test]
    fn test_suppress_and_restore_modifier_mask_ordering() {
        let alt_suppress =
            decode_input_events(&WindowsInputSender::build_suppress_modifier_inputs(
                &[PhysicalModifierKey::LeftAlt],
                &[],
                false,
            ));
        assert_eq!(
            alt_suppress,
            vec![
                (VK_LCONTROL.0, false),
                (VK_LMENU.0, true),
                (VK_LCONTROL.0, true),
            ]
        );

        let alt_with_passthrough_shift_suppress =
            decode_input_events(&WindowsInputSender::build_suppress_modifier_inputs(
                &[PhysicalModifierKey::LeftAlt],
                &[PhysicalModifierKey::LeftShift],
                false,
            ));
        assert_eq!(
            alt_with_passthrough_shift_suppress,
            vec![
                (VK_LSHIFT.0, true),
                (VK_LCONTROL.0, false),
                (VK_LMENU.0, true),
                (VK_LCONTROL.0, true),
                (VK_LSHIFT.0, false),
            ]
        );

        let alt_with_passthrough_ctrl_and_shift_suppress =
            decode_input_events(&WindowsInputSender::build_suppress_modifier_inputs(
                &[PhysicalModifierKey::LeftAlt],
                &[PhysicalModifierKey::LeftShift],
                true,
            ));
        assert_eq!(
            alt_with_passthrough_ctrl_and_shift_suppress,
            vec![(VK_LMENU.0, true)]
        );

        let alt_shift_suppress =
            decode_input_events(&WindowsInputSender::build_suppress_modifier_inputs(
                &[PhysicalModifierKey::LeftAlt, PhysicalModifierKey::LeftShift],
                &[],
                false,
            ));
        assert_eq!(
            alt_shift_suppress,
            vec![
                (VK_LSHIFT.0, true),
                (VK_LCONTROL.0, false),
                (VK_LMENU.0, true),
                (VK_LCONTROL.0, true),
            ]
        );

        let ctrl_alt_suppress =
            decode_input_events(&WindowsInputSender::build_suppress_modifier_inputs(
                &[
                    PhysicalModifierKey::LeftControl,
                    PhysicalModifierKey::LeftAlt,
                ],
                &[],
                false,
            ));
        assert_eq!(
            ctrl_alt_suppress,
            vec![(VK_LMENU.0, true), (VK_LCONTROL.0, true)]
        );

        let alt_shift_restore =
            decode_input_events(&WindowsInputSender::build_restore_modifier_inputs(
                &[PhysicalModifierKey::LeftAlt, PhysicalModifierKey::LeftShift],
                &[],
                false,
            ));
        assert_eq!(
            alt_shift_restore,
            vec![
                (VK_LMENU.0, false),
                (VK_LCONTROL.0, false),
                (VK_LCONTROL.0, true),
                (VK_LSHIFT.0, false),
            ]
        );

        let alt_with_passthrough_shift_restore =
            decode_input_events(&WindowsInputSender::build_restore_modifier_inputs(
                &[PhysicalModifierKey::LeftAlt],
                &[PhysicalModifierKey::LeftShift],
                false,
            ));
        assert_eq!(
            alt_with_passthrough_shift_restore,
            vec![
                (VK_LSHIFT.0, true),
                (VK_LMENU.0, false),
                (VK_LCONTROL.0, false),
                (VK_LCONTROL.0, true),
                (VK_LSHIFT.0, false),
            ]
        );
    }

    #[test]
    fn test_make_mouse_inputs_flags_and_data() {
        let x2_down = WindowsInputSender::make_mouse_button_input(MouseButton::X2, false);
        assert_eq!(x2_down.r#type, INPUT_MOUSE);
        // SAFETY: lettura di unione `INPUT_MOUSE` costruita dal helper.
        let x2_mi = unsafe { x2_down.Anonymous.mi };
        assert_eq!(x2_mi.dwFlags, MOUSEEVENTF_XDOWN);
        assert_eq!(x2_mi.mouseData, 2);

        let wheel_down =
            WindowsInputSender::make_mouse_wheel_input(MouseWheelDirection::Vertical, -2);
        assert_eq!(wheel_down.r#type, INPUT_MOUSE);
        // SAFETY: lettura di unione `INPUT_MOUSE` costruita dal helper.
        let wheel_mi = unsafe { wheel_down.Anonymous.mi };
        assert_eq!(wheel_mi.dwFlags, MOUSEEVENTF_WHEEL);
        assert_eq!(wheel_mi.mouseData as i32, -240);

        let hwheel_right =
            WindowsInputSender::make_mouse_wheel_input(MouseWheelDirection::Horizontal, 3);
        // SAFETY: lettura di unione `INPUT_MOUSE` costruita dal helper.
        let hwheel_mi = unsafe { hwheel_right.Anonymous.mi };
        assert_eq!(hwheel_mi.dwFlags, MOUSEEVENTF_HWHEEL);
        assert_eq!(hwheel_mi.mouseData as i32, 360);

        let rel_move = WindowsInputSender::make_mouse_move_input(-120, 45);
        // SAFETY: lettura di unione `INPUT_MOUSE` costruita dal helper.
        let move_mi = unsafe { rel_move.Anonymous.mi };
        assert_eq!(move_mi.dwFlags, MOUSEEVENTF_MOVE);
        assert_eq!(move_mi.dx, -120);
        assert_eq!(move_mi.dy, 45);
    }
}
