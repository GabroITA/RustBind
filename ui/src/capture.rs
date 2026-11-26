use eframe::egui;
use rustbind::core::action::{InputAction, MouseButton, MouseWheelDirection};
use rustbind::core::binding::KeyBinding;
use rustbind::core::key::VirtualKey;
use rustbind::core::modifiers::Modifiers;
use rustbind::core::{MAX_MOUSE_MOVE_DELTA, MAX_MOUSE_SCROLL_STEPS};

/// Indica se il tasto `egui::Key` rappresenta un modificatore fisico.
pub fn is_modifier_egui_key(key: egui::Key) -> bool {
    matches!(
        key,
        egui::Key::ControlLeft
            | egui::Key::ControlRight
            | egui::Key::AltLeft
            | egui::Key::AltRight
            | egui::Key::ShiftLeft
            | egui::Key::ShiftRight
            | egui::Key::SuperLeft
            | egui::Key::SuperRight
    )
}

/// Converte un `egui::Key` nel corrispondente `VirtualKey` di dominio.
pub fn egui_key_to_virtual_key(key: egui::Key) -> Option<VirtualKey> {
    Some(match key {
        egui::Key::A => VirtualKey::KeyA,
        egui::Key::B => VirtualKey::KeyB,
        egui::Key::C => VirtualKey::KeyC,
        egui::Key::D => VirtualKey::KeyD,
        egui::Key::E => VirtualKey::KeyE,
        egui::Key::F => VirtualKey::KeyF,
        egui::Key::G => VirtualKey::KeyG,
        egui::Key::H => VirtualKey::KeyH,
        egui::Key::I => VirtualKey::KeyI,
        egui::Key::J => VirtualKey::KeyJ,
        egui::Key::K => VirtualKey::KeyK,
        egui::Key::L => VirtualKey::KeyL,
        egui::Key::M => VirtualKey::KeyM,
        egui::Key::N => VirtualKey::KeyN,
        egui::Key::O => VirtualKey::KeyO,
        egui::Key::P => VirtualKey::KeyP,
        egui::Key::Q => VirtualKey::KeyQ,
        egui::Key::R => VirtualKey::KeyR,
        egui::Key::S => VirtualKey::KeyS,
        egui::Key::T => VirtualKey::KeyT,
        egui::Key::U => VirtualKey::KeyU,
        egui::Key::V => VirtualKey::KeyV,
        egui::Key::W => VirtualKey::KeyW,
        egui::Key::X => VirtualKey::KeyX,
        egui::Key::Y => VirtualKey::KeyY,
        egui::Key::Z => VirtualKey::KeyZ,

        egui::Key::Num0 => VirtualKey::Num0,
        egui::Key::Num1 => VirtualKey::Num1,
        egui::Key::Num2 => VirtualKey::Num2,
        egui::Key::Num3 => VirtualKey::Num3,
        egui::Key::Num4 => VirtualKey::Num4,
        egui::Key::Num5 => VirtualKey::Num5,
        egui::Key::Num6 => VirtualKey::Num6,
        egui::Key::Num7 => VirtualKey::Num7,
        egui::Key::Num8 => VirtualKey::Num8,
        egui::Key::Num9 => VirtualKey::Num9,

        egui::Key::F1 => VirtualKey::F1,
        egui::Key::F2 => VirtualKey::F2,
        egui::Key::F3 => VirtualKey::F3,
        egui::Key::F4 => VirtualKey::F4,
        egui::Key::F5 => VirtualKey::F5,
        egui::Key::F6 => VirtualKey::F6,
        egui::Key::F7 => VirtualKey::F7,
        egui::Key::F8 => VirtualKey::F8,
        egui::Key::F9 => VirtualKey::F9,
        egui::Key::F10 => VirtualKey::F10,
        egui::Key::F11 => VirtualKey::F11,
        egui::Key::F12 => VirtualKey::F12,
        egui::Key::F13 => VirtualKey::F13,
        egui::Key::F14 => VirtualKey::F14,
        egui::Key::F15 => VirtualKey::F15,
        egui::Key::F16 => VirtualKey::F16,
        egui::Key::F17 => VirtualKey::F17,
        egui::Key::F18 => VirtualKey::F18,
        egui::Key::F19 => VirtualKey::F19,
        egui::Key::F20 => VirtualKey::F20,
        egui::Key::F21 => VirtualKey::F21,
        egui::Key::F22 => VirtualKey::F22,
        egui::Key::F23 => VirtualKey::F23,
        egui::Key::F24 => VirtualKey::F24,

        egui::Key::ShiftLeft | egui::Key::ShiftRight => VirtualKey::Shift,
        egui::Key::ControlLeft | egui::Key::ControlRight => VirtualKey::Control,
        egui::Key::AltLeft | egui::Key::AltRight => VirtualKey::Alt,
        egui::Key::SuperLeft | egui::Key::SuperRight => VirtualKey::Windows,

        egui::Key::Enter => VirtualKey::Enter,
        egui::Key::Escape => VirtualKey::Escape,
        egui::Key::Space => VirtualKey::Space,
        egui::Key::Tab => VirtualKey::Tab,
        egui::Key::Backspace => VirtualKey::Backspace,
        egui::Key::Insert => VirtualKey::Insert,
        egui::Key::Delete => VirtualKey::Delete,
        egui::Key::Home => VirtualKey::Home,
        egui::Key::End => VirtualKey::End,
        egui::Key::PageUp => VirtualKey::PageUp,
        egui::Key::PageDown => VirtualKey::PageDown,
        egui::Key::ArrowUp => VirtualKey::Up,
        egui::Key::ArrowDown => VirtualKey::Down,
        egui::Key::ArrowLeft => VirtualKey::Left,
        egui::Key::ArrowRight => VirtualKey::Right,

        egui::Key::CapsLock => VirtualKey::CapsLock,
        egui::Key::ScrollLock => VirtualKey::ScrollLock,
        egui::Key::NumLock => VirtualKey::NumLock,
        egui::Key::PrintScreen => VirtualKey::PrintScreen,
        egui::Key::Pause => VirtualKey::Pause,

        egui::Key::Semicolon => VirtualKey::Semicolon,
        egui::Key::Equals | egui::Key::Plus => VirtualKey::Equal,
        egui::Key::Comma => VirtualKey::Comma,
        egui::Key::Minus => VirtualKey::Minus,
        egui::Key::Period => VirtualKey::Period,
        egui::Key::Slash => VirtualKey::Slash,
        egui::Key::Backtick => VirtualKey::Backquote,
        egui::Key::OpenBracket => VirtualKey::LeftBracket,
        egui::Key::Backslash => VirtualKey::Backslash,
        egui::Key::CloseBracket => VirtualKey::RightBracket,
        egui::Key::Quote => VirtualKey::Quote,

        egui::Key::AudioVolumeMute => VirtualKey::VolumeMute,
        egui::Key::AudioVolumeDown => VirtualKey::VolumeDown,
        egui::Key::AudioVolumeUp => VirtualKey::VolumeUp,
        egui::Key::MediaTrackNext => VirtualKey::MediaNext,
        egui::Key::MediaTrackPrevious => VirtualKey::MediaPrev,
        egui::Key::MediaStop => VirtualKey::MediaStop,
        egui::Key::MediaPlayPause => VirtualKey::MediaPlayPause,

        _ => return None,
    })
}

fn clipboard_event_virtual_key(event: &egui::Event, mods: egui::Modifiers) -> Option<VirtualKey> {
    match event {
        egui::Event::Copy => Some(if mods.ctrl {
            VirtualKey::KeyC
        } else {
            VirtualKey::Insert
        }),
        egui::Event::Cut => Some(if mods.ctrl {
            VirtualKey::KeyX
        } else {
            VirtualKey::Delete
        }),
        egui::Event::Paste(_) | egui::Event::PasteImage(_) => Some(if mods.ctrl {
            VirtualKey::KeyV
        } else {
            VirtualKey::Insert
        }),
        _ => None,
    }
}

/// Stato di acquisizione diretta da tastiera per il trigger della regola.
#[derive(Debug, Clone)]
pub struct TriggerCaptureState {
    pub rule_idx: usize,
    pub live_modifiers: Modifiers,
    pub captured: Option<KeyBinding>,
    win_down: bool,
}

impl TriggerCaptureState {
    pub fn new(rule_idx: usize) -> Self {
        Self {
            rule_idx,
            live_modifiers: Modifiers::NONE,
            captured: None,
            win_down: false,
        }
    }

    pub fn reset(&mut self) {
        self.live_modifiers = Modifiers::NONE;
        self.captured = None;
        self.win_down = false;
    }

    /// Elabora gli eventi tastiera di `egui` aggiornando i modificatori live e l'eventuale trigger rilevato.
    pub fn process_events(&mut self, events: &[egui::Event], frame_mods: egui::Modifiers) {
        for event in events {
            match event {
                egui::Event::Key {
                    key,
                    pressed,
                    repeat,
                    modifiers,
                    ..
                } => {
                    if matches!(key, egui::Key::SuperLeft | egui::Key::SuperRight) {
                        self.win_down = *pressed;
                    }
                    if !is_modifier_egui_key(*key)
                        && *pressed
                        && !*repeat
                        && let Some(vk) = egui_key_to_virtual_key(*key)
                    {
                        self.captured = Some(KeyBinding::new(
                            Modifiers {
                                ctrl: modifiers.ctrl,
                                alt: modifiers.alt,
                                shift: modifiers.shift,
                                win: self.win_down,
                            },
                            vk,
                        ));
                    }
                }
                other => {
                    if let Some(vk) = clipboard_event_virtual_key(other, frame_mods) {
                        self.captured = Some(KeyBinding::new(
                            Modifiers {
                                ctrl: frame_mods.ctrl,
                                alt: frame_mods.alt,
                                shift: frame_mods.shift,
                                win: self.win_down,
                            },
                            vk,
                        ));
                    }
                }
            }
        }

        self.live_modifiers = Modifiers {
            ctrl: frame_mods.ctrl,
            alt: frame_mods.alt,
            shift: frame_mods.shift,
            win: self.win_down,
        };
    }
}

/// Converte un pulsante `egui::PointerButton` nel corrispondente `MouseButton` di dominio.
pub fn egui_pointer_button_to_mouse_button(button: egui::PointerButton) -> MouseButton {
    match button {
        egui::PointerButton::Primary => MouseButton::Left,
        egui::PointerButton::Secondary => MouseButton::Right,
        egui::PointerButton::Middle => MouseButton::Middle,
        egui::PointerButton::Extra1 => MouseButton::X1,
        egui::PointerButton::Extra2 => MouseButton::X2,
    }
}

fn wheel_delta_to_steps(delta: f32, unit: egui::MouseWheelUnit) -> i32 {
    if delta.abs() <= f32::EPSILON {
        return 0;
    }
    let dir = if delta > 0.0 { 1 } else { -1 };
    match unit {
        egui::MouseWheelUnit::Line => dir,
        egui::MouseWheelUnit::Page => dir * 3,
        egui::MouseWheelUnit::Point => {
            if delta.abs() >= 1.0 {
                dir
            } else {
                0
            }
        }
    }
}

const MOUSE_MOVE_DEADZONE_PX: i32 = 4;

/// Bozza di modifica inline per un'azione `MouseScroll` o `MouseMove` registrata nella modale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditingActionDraft {
    MouseScroll {
        action_idx: usize,
        direction: MouseWheelDirection,
        steps: i32,
    },
    MouseMove {
        action_idx: usize,
        dx: i32,
        dy: i32,
    },
}

/// Stato di acquisizione diretta da tastiera e mouse per una sequenza di azioni.
#[derive(Debug, Clone)]
pub struct ActionCaptureState {
    pub rule_idx: usize,
    pub held_keys: Vec<VirtualKey>,
    pub held_mouse_buttons: Vec<MouseButton>,
    pub recorded_actions: Vec<InputAction>,
    pub capture_rect: Option<egui::Rect>,
    pub scrollbar_rect: Option<egui::Rect>,
    pub editable_chip_rects: Vec<(usize, egui::Rect)>,
    pub delete_chip_rects: Vec<(usize, egui::Rect)>,
    pub editing_action: Option<EditingActionDraft>,
    pub editing_anchor_rect: Option<egui::Rect>,
    pub record_mouse_move: bool,
    last_pointer_pos: Option<egui::Pos2>,
    pending_move_dx: i32,
    pending_move_dy: i32,
    ignoring_scrollbar_drag: bool,
    ignoring_chip_click: bool,
}

impl ActionCaptureState {
    pub fn new(rule_idx: usize) -> Self {
        Self {
            rule_idx,
            held_keys: Vec::new(),
            held_mouse_buttons: Vec::new(),
            recorded_actions: Vec::new(),
            capture_rect: None,
            scrollbar_rect: None,
            editable_chip_rects: Vec::new(),
            delete_chip_rects: Vec::new(),
            editing_action: None,
            editing_anchor_rect: None,
            record_mouse_move: false,
            last_pointer_pos: None,
            pending_move_dx: 0,
            pending_move_dy: 0,
            ignoring_scrollbar_drag: false,
            ignoring_chip_click: false,
        }
    }

    pub fn reset(&mut self) {
        self.held_keys.clear();
        self.held_mouse_buttons.clear();
        self.recorded_actions.clear();
        self.editable_chip_rects.clear();
        self.delete_chip_rects.clear();
        self.editing_action = None;
        self.editing_anchor_rect = None;
        self.last_pointer_pos = None;
        self.pending_move_dx = 0;
        self.pending_move_dy = 0;
        self.ignoring_scrollbar_drag = false;
        self.ignoring_chip_click = false;
    }

    /// Rimuove una singola azione registrata mantenendo coerente lo stato `Down` pendente.
    pub fn remove_recorded_action(&mut self, idx: usize) -> bool {
        if idx >= self.recorded_actions.len() {
            return false;
        }
        let removed = self.recorded_actions.remove(idx);
        match removed {
            InputAction::KeyDown { key }
                if !self
                    .recorded_actions
                    .iter()
                    .any(|a| matches!(a, InputAction::KeyDown { key: k } if *k == key)) =>
            {
                self.held_keys.retain(|&k| k != key);
            }
            InputAction::MouseDown { button }
                if !self
                    .recorded_actions
                    .iter()
                    .any(|a| matches!(a, InputAction::MouseDown { button: b } if *b == button)) =>
            {
                self.held_mouse_buttons.retain(|&b| b != button);
            }
            _ => {}
        }
        true
    }

    fn reset_pending_move(&mut self) {
        self.pending_move_dx = 0;
        self.pending_move_dy = 0;
    }

    fn push_press(&mut self, vk: VirtualKey, max_actions: usize) {
        if self.held_keys.contains(&vk) || self.recorded_actions.len() >= max_actions {
            return;
        }
        self.held_keys.push(vk);
        self.recorded_actions.push(InputAction::KeyDown { key: vk });
    }

    fn push_release(&mut self, vk: VirtualKey, max_actions: usize) {
        let Some(pos) = self.held_keys.iter().position(|&k| k == vk) else {
            return;
        };
        self.held_keys.remove(pos);

        if matches!(
            self.recorded_actions.last(),
            Some(InputAction::KeyDown { key }) if *key == vk
        ) {
            self.recorded_actions.pop();
            self.recorded_actions
                .push(InputAction::KeyClick { key: vk });
        } else if self.recorded_actions.len() < max_actions {
            self.recorded_actions.push(InputAction::KeyUp { key: vk });
        }
    }

    fn push_mouse_press(&mut self, button: MouseButton, max_actions: usize) {
        if self.held_mouse_buttons.contains(&button) || self.recorded_actions.len() >= max_actions {
            return;
        }
        self.held_mouse_buttons.push(button);
        self.recorded_actions
            .push(InputAction::MouseDown { button });
    }

    fn push_mouse_release(&mut self, button: MouseButton, max_actions: usize) {
        let Some(pos) = self.held_mouse_buttons.iter().position(|&b| b == button) else {
            return;
        };
        self.held_mouse_buttons.remove(pos);

        if matches!(
            self.recorded_actions.last(),
            Some(InputAction::MouseDown { button: prev }) if *prev == button
        ) {
            self.recorded_actions.pop();
            self.recorded_actions
                .push(InputAction::MouseClick { button });
        } else if self.recorded_actions.len() < max_actions {
            self.recorded_actions.push(InputAction::MouseUp { button });
        }
    }

    fn push_mouse_scroll(
        &mut self,
        direction: MouseWheelDirection,
        delta_steps: i32,
        max_actions: usize,
    ) {
        if delta_steps == 0 {
            return;
        }
        if let Some(InputAction::MouseScroll {
            direction: prev_dir,
            steps,
        }) = self.recorded_actions.last_mut()
            && *prev_dir == direction
            && steps.signum() == delta_steps.signum()
        {
            *steps = (*steps + delta_steps).clamp(-MAX_MOUSE_SCROLL_STEPS, MAX_MOUSE_SCROLL_STEPS);
            return;
        }
        if self.recorded_actions.len() < max_actions {
            self.recorded_actions.push(InputAction::MouseScroll {
                direction,
                steps: delta_steps.clamp(-MAX_MOUSE_SCROLL_STEPS, MAX_MOUSE_SCROLL_STEPS),
            });
        }
    }

    fn push_mouse_move(&mut self, dx: i32, dy: i32, max_actions: usize) {
        if dx == 0 && dy == 0 {
            return;
        }
        if let Some(InputAction::MouseMove {
            dx: prev_dx,
            dy: prev_dy,
        }) = self.recorded_actions.last_mut()
        {
            let next_dx = (*prev_dx + dx).clamp(-MAX_MOUSE_MOVE_DELTA, MAX_MOUSE_MOVE_DELTA);
            let next_dy = (*prev_dy + dy).clamp(-MAX_MOUSE_MOVE_DELTA, MAX_MOUSE_MOVE_DELTA);
            if next_dx == 0 && next_dy == 0 {
                self.recorded_actions.pop();
            } else {
                *prev_dx = next_dx;
                *prev_dy = next_dy;
            }
            self.reset_pending_move();
            return;
        }

        self.pending_move_dx =
            (self.pending_move_dx + dx).clamp(-MAX_MOUSE_MOVE_DELTA, MAX_MOUSE_MOVE_DELTA);
        self.pending_move_dy =
            (self.pending_move_dy + dy).clamp(-MAX_MOUSE_MOVE_DELTA, MAX_MOUSE_MOVE_DELTA);
        if self.pending_move_dx.abs() < MOUSE_MOVE_DEADZONE_PX
            && self.pending_move_dy.abs() < MOUSE_MOVE_DEADZONE_PX
        {
            return;
        }

        if self.recorded_actions.len() < max_actions {
            let committed_dx = self.pending_move_dx;
            let committed_dy = self.pending_move_dy;
            self.reset_pending_move();
            self.recorded_actions.push(InputAction::MouseMove {
                dx: committed_dx,
                dy: committed_dy,
            });
        }
    }

    /// Elabora gli eventi tastiera e mouse di `egui` comprimendo automaticamente le coppie down+up consecutive in Click.
    pub fn process_events(
        &mut self,
        events: &[egui::Event],
        frame_mods: egui::Modifiers,
        pointer_pos: Option<egui::Pos2>,
        pixels_per_point: f32,
        max_actions: usize,
    ) {
        let scale = if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
            pixels_per_point
        } else {
            1.0
        };
        if self.editing_action.is_some() {
            for event in events {
                if let egui::Event::PointerButton {
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    ..
                } = event
                {
                    self.ignoring_scrollbar_drag = false;
                    self.ignoring_chip_click = false;
                }
            }
            self.last_pointer_pos = None;
            self.reset_pending_move();
            return;
        }
        if !self.record_mouse_move {
            self.last_pointer_pos = None;
            self.reset_pending_move();
        }

        for event in events {
            match event {
                egui::Event::Key {
                    key,
                    pressed,
                    repeat,
                    ..
                } => {
                    let Some(vk) = egui_key_to_virtual_key(*key) else {
                        continue;
                    };
                    self.reset_pending_move();
                    if *pressed {
                        if !*repeat {
                            self.push_press(vk, max_actions);
                        }
                    } else {
                        self.push_release(vk, max_actions);
                    }
                }
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    ..
                } => {
                    let mb = egui_pointer_button_to_mouse_button(*button);
                    if *pressed {
                        if *button == egui::PointerButton::Primary
                            && (self.delete_chip_rects.iter().any(|(_, r)| r.contains(*pos))
                                || self
                                    .editable_chip_rects
                                    .iter()
                                    .any(|(_, r)| r.contains(*pos)))
                        {
                            self.ignoring_chip_click = true;
                            self.last_pointer_pos = None;
                            self.reset_pending_move();
                        } else if *button == egui::PointerButton::Primary
                            && self.scrollbar_rect.is_some_and(|r| r.contains(*pos))
                        {
                            self.ignoring_scrollbar_drag = true;
                            self.last_pointer_pos = None;
                            self.reset_pending_move();
                        } else if self.capture_rect.is_some_and(|r| r.contains(*pos))
                            && !self.scrollbar_rect.is_some_and(|r| r.contains(*pos))
                        {
                            self.reset_pending_move();
                            self.push_mouse_press(mb, max_actions);
                            if self.record_mouse_move {
                                self.last_pointer_pos = Some(*pos);
                            }
                        }
                    } else {
                        if *button == egui::PointerButton::Primary {
                            self.ignoring_scrollbar_drag = false;
                            if self.ignoring_chip_click {
                                self.ignoring_chip_click = false;
                                self.reset_pending_move();
                                continue;
                            }
                        }
                        self.reset_pending_move();
                        self.push_mouse_release(mb, max_actions);
                    }
                }
                egui::Event::MouseWheel { unit, delta, .. } => {
                    let in_capture = self
                        .capture_rect
                        .is_some_and(|r| pointer_pos.is_some_and(|p| r.contains(p)));
                    if !in_capture {
                        continue;
                    }
                    self.reset_pending_move();
                    if delta.y.abs() >= delta.x.abs() && delta.y.abs() > f32::EPSILON {
                        let steps = wheel_delta_to_steps(delta.y, *unit);
                        self.push_mouse_scroll(MouseWheelDirection::Vertical, steps, max_actions);
                    } else if delta.x.abs() > f32::EPSILON {
                        // In egui `delta.x > 0` sposta il contenuto a destra (scroll vista a sinistra);
                        // Win32 `MOUSEEVENTF_HWHEEL` usa positivo = destra.
                        let steps = wheel_delta_to_steps(-delta.x, *unit);
                        self.push_mouse_scroll(MouseWheelDirection::Horizontal, steps, max_actions);
                    }
                }
                egui::Event::PointerMoved(pos) => {
                    if !self.record_mouse_move
                        || self.ignoring_scrollbar_drag
                        || self.ignoring_chip_click
                    {
                        self.last_pointer_pos = None;
                        self.reset_pending_move();
                        continue;
                    }
                    let in_capture = self.capture_rect.is_some_and(|r| r.contains(*pos))
                        && !self.scrollbar_rect.is_some_and(|r| r.contains(*pos));
                    if in_capture {
                        if let Some(prev) = self.last_pointer_pos {
                            let dx = ((pos.x - prev.x) * scale).round() as i32;
                            let dy = ((pos.y - prev.y) * scale).round() as i32;
                            if dx != 0 || dy != 0 {
                                self.push_mouse_move(dx, dy, max_actions);
                                self.last_pointer_pos = Some(*pos);
                            }
                        } else {
                            self.last_pointer_pos = Some(*pos);
                        }
                    } else {
                        self.last_pointer_pos = None;
                        self.reset_pending_move();
                    }
                }
                egui::Event::PointerGone => {
                    self.last_pointer_pos = None;
                    self.reset_pending_move();
                }
                other => {
                    if let Some(vk) = clipboard_event_virtual_key(other, frame_mods) {
                        if frame_mods.ctrl && !self.held_keys.contains(&VirtualKey::Control) {
                            self.push_press(VirtualKey::Control, max_actions);
                        }
                        if frame_mods.shift && !self.held_keys.contains(&VirtualKey::Shift) {
                            self.push_press(VirtualKey::Shift, max_actions);
                        }
                        self.push_press(vk, max_actions);
                    }
                }
            }
        }
    }

    /// Restituisce le azioni registrate bilanciando eventuali tasti o pulsanti mouse ancora premuti.
    pub fn finalize_actions(&self, max_actions: usize) -> Vec<InputAction> {
        let mut out: Vec<InputAction> = self
            .recorded_actions
            .iter()
            .filter(|a| !matches!(a, InputAction::MouseMove { dx: 0, dy: 0 }))
            .cloned()
            .collect();
        for &button in self.held_mouse_buttons.iter().rev() {
            if out.len() >= max_actions {
                break;
            }
            if matches!(out.last(), Some(InputAction::MouseDown { button: b }) if *b == button) {
                out.pop();
                out.push(InputAction::MouseClick { button });
            } else {
                out.push(InputAction::MouseUp { button });
            }
        }
        for &vk in self.held_keys.iter().rev() {
            if out.len() >= max_actions {
                break;
            }
            if matches!(out.last(), Some(InputAction::KeyDown { key }) if *key == vk) {
                out.pop();
                out.push(InputAction::KeyClick { key: vk });
            } else {
                out.push(InputAction::KeyUp { key: vk });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_event(
        key: egui::Key,
        pressed: bool,
        repeat: bool,
        mods: egui::Modifiers,
    ) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat,
            modifiers: mods,
        }
    }

    #[test]
    fn test_trigger_capture_records_modifiers_win_clipboard_and_resets() {
        let mut cap = TriggerCaptureState::new(0);
        let alt_shift = egui::Modifiers {
            alt: true,
            shift: true,
            ..Default::default()
        };

        // Modificatori tenuti da soli: aggiornano live_modifiers ma non impostano ancora captured
        cap.process_events(
            &[key_event(egui::Key::AltLeft, true, false, alt_shift)],
            alt_shift,
        );
        assert!(cap.captured.is_none());
        assert_eq!(cap.live_modifiers, Modifiers::ALT | Modifiers::SHIFT);

        // Pressione tasto principale (ripetizione ignorata, pressione valida catturata)
        cap.process_events(
            &[
                key_event(egui::Key::ArrowRight, true, true, alt_shift),
                key_event(egui::Key::ArrowRight, true, false, alt_shift),
            ],
            alt_shift,
        );
        assert_eq!(
            cap.captured,
            Some(KeyBinding::new(
                Modifiers::ALT | Modifiers::SHIFT,
                VirtualKey::Right
            ))
        );

        // Evento clipboard Ctrl+C catturato come trigger valido
        let ctrl = egui::Modifiers::CTRL;
        cap.process_events(&[egui::Event::Copy], ctrl);
        assert_eq!(
            cap.captured,
            Some(KeyBinding::new(Modifiers::CTRL, VirtualKey::KeyC))
        );

        // Supporto tasto Windows (SuperLeft)
        cap.process_events(
            &[
                key_event(egui::Key::SuperLeft, true, false, egui::Modifiers::NONE),
                key_event(egui::Key::Tab, true, false, egui::Modifiers::NONE),
            ],
            egui::Modifiers::NONE,
        );
        assert_eq!(
            cap.captured,
            Some(KeyBinding::new(Modifiers::WIN, VirtualKey::Tab))
        );

        cap.reset();
        assert!(cap.captured.is_none());
        assert_eq!(cap.live_modifiers, Modifiers::NONE);
    }

    #[test]
    fn test_action_capture_collapses_clicks_handles_combos_and_balances_on_confirm() {
        let mut cap = ActionCaptureState::new(0);
        let none = egui::Modifiers::NONE;
        let ctrl = egui::Modifiers::CTRL;

        // 1. Tasto singolo premuto e rilasciato -> KeyClick(End)
        cap.process_events(
            &[
                key_event(egui::Key::End, true, false, none),
                key_event(egui::Key::End, true, true, none),
                key_event(egui::Key::End, false, false, none),
            ],
            none,
            None,
            1.0,
            256,
        );
        assert_eq!(
            cap.recorded_actions,
            vec![InputAction::KeyClick {
                key: VirtualKey::End
            }]
        );

        // 2. Reset svuota tutto
        cap.reset();
        assert!(cap.recorded_actions.is_empty());
        assert!(cap.held_keys.is_empty());

        // 3. Combinazione Ctrl+C (con Event::Copy + KeyUp C/Control) -> KeyDown(Control), KeyClick(KeyC), KeyUp(Control)
        cap.process_events(
            &[key_event(egui::Key::ControlLeft, true, false, ctrl)],
            ctrl,
            None,
            1.0,
            256,
        );
        cap.process_events(&[egui::Event::Copy], ctrl, None, 1.0, 256);
        cap.process_events(
            &[
                key_event(egui::Key::C, false, false, ctrl),
                key_event(egui::Key::ControlLeft, false, false, none),
            ],
            none,
            None,
            1.0,
            256,
        );
        assert_eq!(
            cap.finalize_actions(256),
            vec![
                InputAction::KeyDown {
                    key: VirtualKey::Control
                },
                InputAction::KeyClick {
                    key: VirtualKey::KeyC
                },
                InputAction::KeyUp {
                    key: VirtualKey::Control
                },
            ]
        );

        // 4. Se alla conferma un modificatore è ancora premuto, finalize_actions chiude con KeyUp
        cap.reset();
        cap.process_events(
            &[
                key_event(egui::Key::ShiftLeft, true, false, egui::Modifiers::SHIFT),
                key_event(egui::Key::Home, true, false, egui::Modifiers::SHIFT),
                key_event(egui::Key::Home, false, false, egui::Modifiers::SHIFT),
            ],
            egui::Modifiers::SHIFT,
            None,
            1.0,
            256,
        );
        assert_eq!(
            cap.finalize_actions(256),
            vec![
                InputAction::KeyDown {
                    key: VirtualKey::Shift
                },
                InputAction::KeyClick {
                    key: VirtualKey::Home
                },
                InputAction::KeyUp {
                    key: VirtualKey::Shift
                },
            ]
        );
    }

    #[test]
    fn test_action_capture_mouse_clicks_scroll_bounds_and_optional_move() {
        let mut cap = ActionCaptureState::new(0);
        let box_rect = egui::Rect::from_min_max(egui::pos2(10.0, 10.0), egui::pos2(200.0, 150.0));
        cap.capture_rect = Some(box_rect);

        let inside = egui::pos2(80.0, 60.0);
        let outside = egui::pos2(250.0, 250.0);

        // Click fuori dalla capture_rect (es. sui pulsanti Conferma/Annulla) ignorato
        cap.process_events(
            &[
                egui::Event::PointerButton {
                    pos: outside,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: outside,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
            Some(outside),
            1.0,
            256,
        );
        assert!(cap.recorded_actions.is_empty());

        // Click dentro capture_rect -> MouseClick(Right), poi rotella verticale -2, poi X1 Down
        cap.process_events(
            &[
                egui::Event::PointerButton {
                    pos: inside,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: inside,
                    button: egui::PointerButton::Secondary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, -3.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, -3.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: inside,
                    button: egui::PointerButton::Extra1,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
            Some(inside),
            1.0,
            256,
        );

        // Spostamenti senza record_mouse_move ignorati
        cap.process_events(
            &[
                egui::Event::PointerMoved(inside),
                egui::Event::PointerMoved(egui::pos2(120.0, 90.0)),
            ],
            egui::Modifiers::NONE,
            Some(egui::pos2(120.0, 90.0)),
            1.0,
            256,
        );
        assert_eq!(cap.recorded_actions.len(), 3);

        // Con record_mouse_move abilitato: gli spostamenti contigui si aggregano con pixels_per_point
        cap.record_mouse_move = true;
        cap.process_events(
            &[
                egui::Event::PointerMoved(egui::pos2(50.0, 50.0)),
                egui::Event::PointerMoved(egui::pos2(60.0, 55.0)),
                egui::Event::PointerMoved(egui::pos2(70.0, 45.0)),
            ],
            egui::Modifiers::NONE,
            Some(egui::pos2(70.0, 45.0)),
            2.0,
            256,
        );

        // finalize_actions chiude anche MouseDown(X1) -> MouseUp(X1) perché ci sono azioni dopo
        let finalized = cap.finalize_actions(256);
        assert_eq!(
            finalized,
            vec![
                InputAction::MouseClick {
                    button: MouseButton::Right
                },
                InputAction::MouseScroll {
                    direction: MouseWheelDirection::Vertical,
                    steps: -2
                },
                InputAction::MouseDown {
                    button: MouseButton::X1
                },
                InputAction::MouseMove { dx: 40, dy: -10 },
                InputAction::MouseUp {
                    button: MouseButton::X1
                },
            ]
        );

        // 5. Click e trascinamento partiti sulla scrollbar (anche rientrando nell'area cattura) vengono ignorati
        cap.reset();
        let sb_rect = egui::Rect::from_min_max(egui::pos2(201.0, 10.0), egui::pos2(225.0, 150.0));
        cap.scrollbar_rect = Some(sb_rect);
        let on_scrollbar = egui::pos2(210.0, 40.0);
        cap.process_events(
            &[
                egui::Event::PointerButton {
                    pos: on_scrollbar,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerMoved(inside),
                egui::Event::PointerMoved(egui::pos2(120.0, 100.0)),
                egui::Event::PointerButton {
                    pos: egui::pos2(120.0, 100.0),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
            Some(egui::pos2(120.0, 100.0)),
            1.0,
            256,
        );
        assert!(cap.recorded_actions.is_empty());

        // 6. Micro-tremore da 1px del sensore ottico prima della rotella viene filtrato dalla deadzone
        cap.process_events(
            &[
                egui::Event::PointerMoved(egui::pos2(80.0, 60.0)),
                egui::Event::PointerMoved(egui::pos2(81.0, 61.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, -3.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
            Some(egui::pos2(81.0, 61.0)),
            1.0,
            256,
        );
        assert_eq!(
            cap.recorded_actions,
            vec![InputAction::MouseScroll {
                direction: MouseWheelDirection::Vertical,
                steps: -1
            }]
        );

        // 7. Click su chip modificabile ignorato e registrazione sospesa durante editing_action
        let chip_rect = egui::Rect::from_min_max(egui::pos2(20.0, 20.0), egui::pos2(90.0, 45.0));
        cap.editable_chip_rects = vec![(0, chip_rect)];
        let on_chip = chip_rect.center();
        cap.process_events(
            &[
                egui::Event::PointerButton {
                    pos: on_chip,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: on_chip,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
            Some(on_chip),
            1.0,
            256,
        );
        assert_eq!(cap.recorded_actions.len(), 1);

        cap.editing_action = Some(EditingActionDraft::MouseScroll {
            action_idx: 0,
            direction: MouseWheelDirection::Vertical,
            steps: -1,
        });
        cap.process_events(
            &[
                key_event(egui::Key::A, true, false, egui::Modifiers::NONE),
                key_event(egui::Key::A, false, false, egui::Modifiers::NONE),
            ],
            egui::Modifiers::NONE,
            Some(inside),
            1.0,
            256,
        );
        assert_eq!(cap.recorded_actions.len(), 1);

        // Dopo la chiusura del menu (Salva/Annulla), i tasti tornano ad essere registrati
        cap.editing_action = None;
        cap.process_events(
            &[
                key_event(egui::Key::A, true, false, egui::Modifiers::NONE),
                key_event(egui::Key::A, false, false, egui::Modifiers::NONE),
            ],
            egui::Modifiers::NONE,
            Some(inside),
            1.0,
            256,
        );
        assert_eq!(
            cap.recorded_actions,
            vec![
                InputAction::MouseScroll {
                    direction: MouseWheelDirection::Vertical,
                    steps: -1
                },
                InputAction::KeyClick {
                    key: VirtualKey::KeyA
                },
            ]
        );

        // 8. Click sulla X di rimozione di un chip rimuove l'azione senza registrare Mouse Left
        let del_rect = egui::Rect::from_center_size(egui::pos2(88.0, 22.0), egui::vec2(14.0, 14.0));
        cap.delete_chip_rects = vec![(0, del_rect)];
        let on_del = del_rect.center();
        cap.process_events(
            &[
                egui::Event::PointerButton {
                    pos: on_del,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: on_del,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
            Some(on_del),
            1.0,
            256,
        );
        assert!(cap.remove_recorded_action(0));
        assert_eq!(
            cap.recorded_actions,
            vec![InputAction::KeyClick {
                key: VirtualKey::KeyA
            }]
        );
    }
}
