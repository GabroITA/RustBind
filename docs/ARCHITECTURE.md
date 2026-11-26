# RustBind Architecture

This document details the technical architecture, security model, module boundaries, and design decisions for **RustBind**: a Windows (10/11) desktop application dedicated to hotkey management and keyboard/mouse input automation, with primary focus on security, anticheat compatibility, and minimal resource consumption.

---

## 1. Philosophy and Guiding Principles

1. **Maximum Anticheat Compatibility**:
   - Exclusive use of documented, official Windows APIs (`RegisterHotKey`, `SendInput`, `Shell_NotifyIconW`).
   - Strict prohibition of kernel drivers, code/DLL injection, reading/writing memory of other processes, and low-level input hooks (`SetWindowsHookEx` / `WH_KEYBOARD_LL`).
   - Full transparency: no evasion techniques, no obfuscation, no jitter or synthetic timing randomization designed to mimic human behavior.
2. **Minimal Resource Footprint (goal: a few MB of idle daemon RAM, verified via Private Working Set measurement)**:
   - **Two-process architecture**: an ultra-lightweight resident daemon (`rustbind.exe`) and an ephemeral graphical user interface (`rustbind-ui.exe`) that terminates completely after use.
3. **Security & Least Privilege**:
   - Runs entirely in standard user space (no administrator privileges required).
   - Respects Windows UIPI (User Interface Privilege Isolation) boundaries.
   - Privacy protection: strict prohibition of keylogging and logging typed keys, `SendText` payloads, or configuration values.
4. **Unsafe Isolation & Testability**:
   - All `unsafe` code is strictly isolated in `src/platform/windows/`, each accompanied by a `// SAFETY:` justification.
   - The core logic communicates with the platform exclusively through safe Rust traits, enabling comprehensive automated tests with mock implementations (`MockPlatform`) without touching the OS.

---

## 2. Two-Process Architecture

To keep idle memory consumption minimal (spending 99% of its time minimized in the System Tray), RustBind is split into two distinct processes, alongside a lightweight console controller (`rustbindctl.exe`):

```
 +-------------------------------------------------------------------------+
 |                        PROCESS 1: DAEMON (rustbind.exe)                 |
 |                      (Resident, background, ~2-4 MB)                    |
 |                                                                         |
 |  +---------------------+   +---------------------+   +---------------+  |
 |  | Win32 Message Loop  |   |    Rule Engine      |   | Native Win32  |  |
 |  | (RegisterHotKey /   |-->| (Trigger -> Actions)|-->| Tray Icon     |  |
 |  |  WM_HOTKEY)         |   |                     |   | (Shell_Notify)|  |
 |  +---------------------+   +---------------------+   +---------------+  |
 |             ^                         |                                 |
 |             | Foreground Context      v                                 |
 |             | (WindowFilter)   +---------------+                        |
 |             |                  |   SendInput   |                        |
 |             |                  | (Keys/Text/Ms)|                        |
 |             v                  +---------------+                        |
 |     +-------------------------------------------------+                 |
 |     | Named Pipe Server (Local IPC with strict ACL)                     |
 +-----+-------------------------------------------------+-----------------+
                               ^
                               | Local IPC (Named Pipe)
                               v
 +-------------------------------------------------------------------------+
 |                         PROCESS 2: UI (rustbind-ui.exe)                 |
 |               (Ephemeral: launched on demand, exits on close)           |
 |                                                                         |
 |   +-----------------------------------------------------------------+   |
 |   | egui Graphical Interface (eframe + glow)                        |   |
 |   | - Visual TOML rules & groups editor + live input recorder       |   |
 |   | - Daemon status, hotkey conflicts, and recent log warnings      |   |
 |   +-----------------------------------------------------------------+   |
 +-------------------------------------------------------------------------+
```

### 2.1. Process 1 — Resident Daemon (`rustbind.exe`)
- **Role**: Continuous hotkey handling, foreground context switching (when `window_filter` rules exist), input sequence execution, tray icon management, runtime diagnostics persistence, and IPC server.
- **GUI Dependencies**: **Zero GUI toolkit loaded** (no egui, no slint, no winit).
- **Internal Components**:
  - *Invisible Top-Level Win32 Window*: Hidden window (`WS_POPUP`, `WS_EX_TOOLWINDOW`) dedicated to receiving `WM_HOTKEY` messages, tray icon callbacks, and system broadcasts.
  - *Native Tray Icon*: Created and managed directly via the Win32 `Shell_NotifyIconW` API.
  - *Rule Engine*: Reactive engine mapping hotkey bindings (and additive pass-through modifier variants) to action sequences, dynamically swapping per-app rules when active foreground `.exe` changes.
  - *Input Executor*: Safe wrapper around `SendInput` (`INPUT_KEYBOARD` and `INPUT_MOUSE`) with circuit breakers and sliding-window rate limiting.
  - *Execution Thread*: Action sequences NEVER run on the message loop thread, running instead on a dedicated cancellable thread (`SequenceExecutor`, see section 3.8).
  - *Named Pipe IPC Server*: Listening on `\\.\pipe\RustBind-IPC-<UserSID>` to communicate with `rustbind-ui.exe` and `rustbindctl.exe`.
- **Lifecycle & Single Instance**: Single active process enforced by a Windows Named Mutex (`Local\RustBind_Daemon_SingleInstance`). Concurrent startup exits immediately with exit code `2` before touching log files.

### 2.2. Process 2 — Configurator UI (`rustbind-ui.exe`, workspace crate `ui/`)
- **Role**: Separate desktop configuration and monitoring application with its own dedicated binary (`ui/Cargo.toml`; `eframe`/`egui` dependency completely absent from the root `rustbind` crate).
- **Ephemeral Lifecycle & Single Instance**:
  - Launched explicitly by the user via **"Configure..."** in the tray context menu (double left-click on the tray icon remains reserved solely for precautionary suspension) or by running `rustbind-ui.exe` directly. In the latter case, if the daemon Named Mutex (`Local\RustBind_Daemon_SingleInstance`) is free, the UI launches the sibling `rustbind.exe` binary from the same directory as `std::env::current_exe()`.
  - Both companions resolve the fixed path of the sibling executable strictly inside the same directory as `std::env::current_exe()` (never from configuration, `PATH`, or a shell), distinct from the absolute ban on `LaunchProcess` in rules.
  - The UI process acquires its own Named Mutex `Local\RustBind_UI_SingleInstance`; if a window titled `"RustBind"` (`UI_WINDOW_TITLE`) is already open, it brings it to the foreground (`FindWindowW` + `ShowWindow(SW_RESTORE)` + `SetForegroundWindow`) and exits with code `0`.
  - Queries daemon state via local IPC (`GetStatus`, `Suspend`, `ReloadConfig`, `Resume`) on a non-blocking background thread (`mpsc` channel), automatically refreshing on window focus gain, silent 1-second periodic polling (`GetStatus`), and immediately after `Suspend`/`Resume`/`ReloadConfig` (waiting briefly to reflect message loop transition).
  - **Responsive 3-Tab Visual Editor**:
    - *Header & Daemon Menu*: Top bar with 3 tabs (`"Rules"`, `"Settings"`, `"Status & security"`), automatic reset of nested subtabs/editors to each tab's initial view (`"Rule list"`, `"General"`, closing the Theme Editor) when switching top-level tabs, color status indicator matching the tray icon painted via `ui.painter()` (green `Active`, amber `Paused`, red `Error`, neutral gray `Not running`), undo/redo history buttons (`Ctrl+Z` / `Ctrl+Y`), and `"Daemon"` dropdown menu (`Refresh status`, `Suspend daemon` / `Resume daemon` based on `can_suspend()`/`can_resume()`, `Reload in daemon`).
    - *"Rules" Tab*: Organized into subtabs via `selectable_value`, with side-by-side resizable panes using a draggable vertical splitter (isolated with `push_id`/`clip_rect` and guaranteed minimum widths), full vertical height utilization, uniform rounded cards (`10 px` default, customizable in Custom Theme), subtle `#b7410e` selection drawn internally without dimension shift, and click priority on inner widgets:
      - *"Rule list" Subtab*: Multi-select `Groups:` filter (`All groups`, `No group`, individual groups via checkboxes) above `Search:` aligned via `egui::Grid`, `"Active only"` / `"Disabled only"` checkboxes, `"Create"`, rule cards showing compact single-line `<Trigger>: <Actions>` preview with top-right `X` delete button (with confirmation modal; automatically removes the rule from all groups), `"Enabled"` toggle (disabled for groupless rules), `Up`/`Down`, `"Duplicate"`, drag-and-drop rule reordering, and drag-and-drop rule-to-actions import (showing a dimmed action preview overlay on hover), plus `egui::Grid`-aligned Rule Details pane (hidden internal ID, unique `name`, `Groups:` multi-select with `"(no group)"` and `"Create"` shortcut to `"Groups"`, `Trigger:` hotkey `Ctrl`/`Alt`/`Shift`/`Win` + `VirtualKey::ALL` + live `"Record"` capture modal, `Context:` filter `Everywhere (global)` / `Only in selected apps` / `Everywhere except` `.exe` with installed/running app picker and `.exe` file dialog, `"Wait for trigger keys release before running"`, live `"Record"` keyboard/mouse action capture modal, action toolbar (`Record`, `KeyClick`, `KeyDown`, `KeyUp`, `SendText`, `Delay`, `MouseClick`, `MouseDown`, `MouseUp`, `MouseScroll`, `MouseMove`), and column-aligned drag-and-drop sequence items with inline editing and hover chip deletion; never `LaunchProcess`).
      - *"Groups" Subtab*: `"Name:"` and `"Search:"` inputs aligned via `egui::Grid`, group cards with `"Active"` toggle, context badge, and top-right `X` delete confirmation (hidden on `"default"`, which cannot be deleted and renders a disabled `Context:` `"Everywhere (global)"` selector; rules left without a group stay disabled without falling back to `"default"`), plus per-group `Context:` (`Everywhere (global)` / `Only in selected apps` / `Everywhere except`) above the right pane split into `"Available rules"` and `"Rules in group"` with compact checkboxes, search filters, dynamic `"Select all"` / `"Deselect all"` buttons, width-aligned `"Open"` rule buttons, and `"Add"` / `"Remove"` actions.
      - *Conditional "Duplicate rules" Subtab*: Shown exclusively when two or more rules share an identical `actions` sequence; `"Duplicate rules"` cards with an `egui::Grid` table (`Keep`, `Status`, `Trigger`, `Groups`), per-card checkbox to inherit groups from deleted rules, and confirmation modal prior to removal.
    - *"Settings" Tab*: Organized into three borderless, divider-separated subtabs (`section_frame` + `render_section_divider`):
      - *"General"*: Displays the `"Check for updates"` button (anchored next to the current version solely on this subtab); UI language selector (English [default], Italian, French, German, Spanish) grouped with Windows sign-in autostart toggle on `HKCU\...\Run`; `"Hotkeys & input limits"` (`kill_switch`, optional `resume_hotkey`, `max_events_per_second` slider) grouped with `"Startup groups"` (`All groups enabled` + per-group checkboxes); `"Configuration file"` (`%APPDATA%\RustBind\configs\`, defaulting to `config.toml`, tracked via `%APPDATA%\RustBind\.active_config`, single active profile radio-button selection, native `.toml` import/export dialogs, drag-and-drop `.toml` import routing config profiles to immediate activation/daemon reload and custom theme files to immediate Custom Theme activation even if the active config is locked invalid, plus overwrite confirmation on colliding filename); and `"Updates & Project"` (automatic update check toggle, GitHub release changelog link, and GitHub repository link).
      - *"Themes"*: Three divider-separated sections: `"Custom theme"` (single-line truncated summary card with `"Edit"` button under the preview swatches that opens an in-tab `"Theme Editor"` without activating the theme, plus single-slot `.toml` theme import/export via native dialogs or window drag-and-drop, overwriting and activating on import); `"Dark themes"` (5 presets including *GTA VI — Vice Sunset* with `"True Black"` `#000000` toggle directly underneath the heading); and `"Light themes"` (5 presets). The in-tab `"Theme Editor"` provides a `"Back"` button, `"Reset defaults"`, dark/light base mode + `"True Black"` toggle, color pickers for all 9 palette roles (including base text), and 4 grouped border-radius sliders (`controls`, `inner_boxes`, `cards_menus`, `modals`), persisted alongside language and layout state in `%APPDATA%\RustBind\preferences.toml`.
      - *"Shortcuts"*: Borderless reference overview of keyboard shortcuts supported by the configurator (`Ctrl+Z`, `Ctrl+Y`, `Ctrl+N`, `Ctrl+S`, `Ctrl+F`, `Del`).
    - *"Status & security" Tab*: Divider-separated borderless sections showing Daemon connection status + 5 summary counters (`Registered rules`, `Active rules`, `Registered groups`, `Active groups`, `Active rules in active groups`), live daemon hotkey registration conflict diagnostics (`runtime_diagnostics.json`), recent `rustbind.log` warnings/errors viewer, fail-closed two-step backup+restore flow for invalid `.toml` files (confirmation checkbox + `"Backup and restore"` creating `.bak.<timestamp>`), and security/privacy guarantees.
    - *Responsive Layout, Stable Footer, Modals, Update Banner & Toasts*: Fixed-height bottom `Panel::bottom("editor_footer")` with live validation message on the left (no prefix or trailing period) and `"Discard changes"` + `"Save configuration"` on the right; uniform button heights and `"Daemon"` menu matching card corner radius; compact confirmation modals with dark blocking backdrop, customizable modal corner radius (`14 px` default), white titles on dark themes, tight uniform shadow, and right-aligned buttons; persistent centered update-available banner (`Close`, `Changelog`, `Update`) above the footer when a newer GitHub release is found; centered auto-dismissing toast notifications (`4s`); `ScrollArea::vertical().hscroll(false)`; and automatic vertical fallback below narrow width threshold.
  - **Independent Lifecycle**: Closing only the UI window terminates the GUI process (`0.00 MB` RAM) while leaving the daemon active in the tray; exiting the daemon from the tray (`"Exit"`) sends best-effort `PostMessageW(WM_CLOSE)` to the `"RustBind"` window.

---

## 3. Technical Decisions & Design Rationale

### 3.1. UI Graphics Framework
- **Decision**: `egui` + `eframe` (0.37, `glow` backend with `default_fonts` and `default-features = false`) in the separate ephemeral process.
- **Measured Windows 11 Memory Benchmark**:
  - `egui` (`glow` backend): Private Working Set = **65.87 MB** while open; **0.00 MB** after close (process exited).
  - `slint` (software backend): Private Working Set = **3.25 MB** while open; **0.00 MB** after close.
- **Rationale**:
  - Although `slint` uses less RAM while the window is open, its licensing (GPL-3.0 or constrained proprietary Royalty-Free) and external DSL introduce friction for a permissive open-source tool.
  - With the two-process architecture, memory usage during configuration has zero impact on idle background overhead: as soon as the user closes the window, `egui` is completely unloaded from RAM.
  - `egui` uses permissive MIT/Apache-2.0 licenses, is written in pure Rust, integrates cleanly, and is battle-tested.
- **Rejected Alternatives**:
  - *Slint*: Rejected due to licensing constraints (GPL-3.0) and separate DSL compiler requirement.
  - *Iced*: Rejected due to stricter Elm-style architecture being less ergonomic for isolated ephemeral config tooling.
  - *Single process with hidden GUI*: Rejected because keeping a GUI toolkit resident in background wastes tens of megabytes of RAM for an app meant to idle quietly.

### 3.2. Windows API Bindings
- **Decision**: Official Microsoft `windows` crate (`0.62.2`) on Rust Edition 2024, enabling strictly only required sub-modules in `Cargo.toml`.
- **Enabled Feature Flags & Justification**:
  - `Win32_Foundation`: Base types, `HWND`, `HANDLE`, `BOOL`, `CloseHandle`, `DestroyWindow`, `WIN32_ERROR` handling, `LocalFree`.
  - `Win32_UI_WindowsAndMessaging`: `GetMessageW`, `TranslateMessage`, `DispatchMessageW`, `PostMessageW`, `DefWindowProcW`, `RegisterClassW`, `CreateWindowExW`, `EnumWindows`, `GetForegroundWindow`, `WM_HOTKEY`, and tray/dialog messages.
  - `Win32_UI_Input_KeyboardAndMouse`: `RegisterHotKey`, `UnregisterHotKey`, `SendInput`, `GetAsyncKeyState`, `MapVirtualKeyW`, `INPUT`, `KEYBDINPUT`, `MOUSEINPUT`, virtual key constants, and modifiers (`MOD_CONTROL`, `MOD_ALT`, `MOD_SHIFT`, `MOD_WIN`).
  - `Win32_UI_Accessibility`: `SetWinEventHook` / `UnhookWinEvent` (`EVENT_SYSTEM_FOREGROUND`, `WINEVENT_OUTOFCONTEXT`) strictly to observe foreground application switches when contextual `WindowFilter` rules are enabled.
  - `Win32_System_LibraryLoader`: `GetModuleHandleW` to obtain the current module handle (`HINSTANCE`) needed to register the listener window class.
  - `Win32_Graphics_Gdi`: Required by `windows-rs` types for `WNDCLASSW` and `RegisterClassW`.
  - `Win32_UI_Shell`: `Shell_NotifyIconW`, `NOTIFYICONDATAW`, `NIM_ADD`, `NIM_MODIFY`, `NIM_DELETE`, `NIM_SETVERSION` for the native GUI-free tray icon, plus `ShellExecuteW` for opening verified GitHub HTTPS links in the user's browser.
  - `Win32_UI_Controls_Dialogs`: Native Win32 file open/save dialogs (`GetOpenFileNameW` / `GetSaveFileNameW`) for `.toml` import/export (configs and custom themes) and `.exe` process selection in `file_dialog.rs`.
  - `Win32_System_Pipes`: `CreateNamedPipeW`, `ConnectNamedPipe`, `DisconnectNamedPipe`, `WaitNamedPipeW` for the IPC server and client.
  - `Win32_Storage_FileSystem`: `ReadFile`, `WriteFile`, `CreateFileW` for Named Pipe IPC I/O.
  - `Win32_Security` & `Win32_Security_Authorization`: `SECURITY_ATTRIBUTES`, `OpenProcessToken`, `GetTokenInformation`, `ConvertStringSecurityDescriptorToSecurityDescriptorW`, and `ConvertSidToStringSidW` to restrict pipe access via SDDL ACL exclusively to the current user (`TokenUser`).
  - `Win32_System_Threading`: `CreateMutexW`, `CreateEventW`, `WaitForSingleObject`, `WaitForMultipleObjects`, `OpenProcess`, and `QueryFullProcessImageNameW` for single-instance guards, overlapped I/O sync, and limited foreground process executable lookup (`PROCESS_QUERY_LIMITED_INFORMATION`).
  - `Win32_System_IO`: `OVERLAPPED`, `CancelIoEx`, and `GetOverlappedResult` for cancellable asynchronous Named Pipe I/O.
  - `Win32_System_Memory`: `LocalFree` / memory helpers required by SDDL security descriptor cleanup.
  - `Win32_System_Registry`: `RegOpenKeyExW`, `RegQueryValueExW`, `RegSetValueExW`, `RegDeleteValueW`, `RegEnumKeyExW`, `RegCloseKey` on `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` for per-user autostart and read-only installed app enumeration.
  - `Win32_Networking_WinHttp`: `WinHttpOpen`, `WinHttpConnect`, `WinHttpOpenRequest`, `WinHttpSendRequest`, `WinHttpReceiveResponse`, `WinHttpQueryHeaders`, `WinHttpQueryDataAvailable`, `WinHttpReadData`, `WinHttpCloseHandle` for strict HTTPS-only GitHub Releases update checks and verified SHA-256 package downloads restricted to an explicit GitHub host allowlist.
- **`unsafe` Isolation & RAII**:
  - Zero direct Win32 calls outside `src/platform/windows/`.
  - All system handles implement RAII via dedicated wrapper types (`SafeHwnd`, `SafeHandle`, `SafeHicon`, `ForegroundWatcher`), ensuring automatic release in `Drop`.
  - Win32 Thread Affinity: window-bound types (`SafeHwnd`, `WindowsHotkeyManager`) are strictly `!Send` and `!Sync`, created, used, and destroyed exclusively on the Windows message loop thread. `WindowsInputSender` is `Send + Sync` (verified via `static_assertions::assert_impl_all!`).
- **Rejected Alternatives**:
  - *`windows-sys`*: Rejected because it exposes raw C FFI types without type safety, requiring manual checks on every handle and increasing memory management risk compared to `windows-rs` idiomatic abstractions.

### 3.3. Hotkey Handling & Foreground Context
- **Decision**: Exclusively `RegisterHotKey` for global triggers, paired with out-of-context `EVENT_SYSTEM_FOREGROUND` (`SetWinEventHook` + `WINEVENT_OUTOFCONTEXT`) when contextual `window_filter` rules are configured.
- **Mechanism**:
  - Registers combinations via the Window Manager API bound to an invisible top-level window (`WS_POPUP`, `WS_EX_TOOLWINDOW`, never shown on screen).
  - Receives triggers via `WM_HOTKEY(wparam = id, lparam = modifiers | vk)`.
  - *Why invisible top-level window instead of `HWND_MESSAGE`*: Message-only windows (`HWND_MESSAGE`) cannot receive system broadcasts like `TaskbarCreated`, do not properly support `SetForegroundWindow` (needed for reliable context menu behavior and focus release), and miss `WM_QUERYENDSESSION`/`WM_ENDSESSION` shutdown messages. An invisible top-level window (`WS_POPUP`, `WS_EX_TOOLWINDOW`) receives broadcasts and handles context menus cleanly, with the security trade-off that it is enumerable via `EnumWindows` and receives system broadcasts (handled safely).
  - *Contextual Foreground Filtering*: When one or more rules define `WindowFilter` (`Include` or `Exclude` process names), `ForegroundWatcher` listens to `EVENT_SYSTEM_FOREGROUND` (`WINEVENT_OUTOFCONTEXT`, zero DLL injection into external processes), coalesces focus changes into the message loop, queries the foreground process executable stem via `PROCESS_QUERY_LIMITED_INFORMATION` + `QueryFullProcessImageNameW`, and dynamically updates active rule registrations while keeping `kill_switch` and `resume_hotkey` permanently active.
- **Security & Anticheat Compatibility Analysis**:
  - *Why `RegisterHotKey`*: Standard, documented Windows interface for global shortcuts. Does not install keyboard hooks into the OS input processing pipeline and does not inspect keystrokes that do not match registered combinations. This lowers false-positive risk compared to global hooks, though it **does not guarantee compatibility with any specific anticheat or EDR**: each vendor enforces its own heuristics and policies, and many games prohibit input automation in their Terms of Service regardless of technique.
- **Rejected Alternatives**:
  - *`SetWindowsHookEx` (`WH_KEYBOARD_LL` / `WH_MOUSE_LL`)*: **Rejected and formally banned**. Global low-level input hooks intercept every keystroke/mouse event OS-wide (behavior indistinguishable from a keylogger) and are systematically flagged or blocked by anticheats (e.g., Vanguard, EAC, BattlEye). They can also freeze system input if callbacks exceed `LowLevelHooksTimeout`.
  - *Architectural consequence*: Banning low-level input hooks is a deliberate design choice: consequently **single-key remapping without `RegisterHotKey` or global input interception/suppression is not supported**; only shortcuts registered with `RegisterHotKey` can trigger rules and sequences.

### 3.4. Input Dispatch (`SendInput`) & Anticheat Constraints
- **Decision**: Keyboard (`VirtualKey` + `KEYEVENTF_UNICODE` text), bounded mouse actions (using `INPUT_MOUSE`), and deterministic fixed delays — with zero low-level input hooks and zero timing randomization.
- **Supported Input Perimeter & Safety Rationale**:
  - Keyboard actions (`KeyClick`, `KeyDown`, `KeyUp`), Unicode text (`SendText`), deterministic delays (`Delay` from `MIN_DELAY_MS = 1` to `MAX_DELAY_MS = 60_000` ms), and bounded mouse actions (`MouseClick`, `MouseDown`, `MouseUp`, `MouseScroll`, and relative `MouseMove`) all execute exclusively through documented Win32 `SendInput` batches.
  - Mouse scroll is bounded to `±MAX_MOUSE_SCROLL_STEPS` (`±50`, scaled by `WHEEL_DELTA = 120`), and relative mouse moves are bounded to `±MAX_MOUSE_MOVE_DELTA` (`±10,000`) per action, with zero randomization or synthetic smoothing.
- **Strict Anticheat Constraints & Operational Rules**:
  1. **Zero randomization or "humanization"**: Delays are strictly fixed constants set by the user in configuration. Introducing jitter or pseudo-random delays aimed at evading anticheat checks is formally banned.
  2. **Atomic per-action dispatch**: Keys and modifiers of a single action are submitted in batch via a single `INPUT` array to `SendInput` to avoid unexpected interleaving with physical events. Multi-action sequences execute action by action so they can be cancelled between actions (see 3.8).
  3. **Unicode text via `KEYEVENTF_UNICODE`**: Sends characters directly via Unicode scan codes (`wScan`), eliminating keyboard layout dependence (`VkKeyScanEx`).
  4. **Safety Limits (Circuit Breakers)**:
     - Maximum sequence length: `MAX_ACTIONS_PER_SEQUENCE = 256` actions per trigger.
     - Maximum text length (`SendText`): `MAX_SEND_TEXT_LENGTH = 1024` characters per single action. Because each UTF-16 code unit generates two input events (down + up), 1024 characters can produce up to 2048 events, corresponding to over 20 seconds at the max rate limit of 100 ev/s.
     - Rate limiter: `MIN_EVENTS_PER_SECOND = 1` .. `MAX_EVENTS_PER_SECOND = 100` synthetic input events per second to prevent accidental flooding. Excess events are paced via interruptible waits (`sleep_interruptible`) without dropping or truncating the sequence.
     - No infinite loops: all sequences are strictly finite.
  5. **Emergency Kill-Switch (Configurable, Default `Ctrl+Alt+Shift+F12`)**:
     - Registers a configurable global emergency hotkey in config (`Ctrl+Alt+Shift+F12` by default, avoiding keys like `Pause/Break` missing on many laptops and compact keyboards).
     - **Fail-Safe Security Requirement**: If registering the kill-switch fails at startup (e.g., due to conflict with another application), the daemon enters emergency lockdown: **zero configured rules run**, and critical error state is immediately reflected via the tray icon. Unlike the kill-switch, if registering a single user rule's hotkey conflicts or is shadowed by an earlier rule/system hotkey, the daemon uses **partial degradation**: logs a `WARN`, records the categorical reason (`SkippedRuleReason`) in runtime diagnostics, skips only the conflicting rule, and keeps the kill-switch and all other valid rules active.
     - Pressing the kill-switch or calling `pause()` always sets `cancel_flag = true` to immediately abort any running sequence and transitions `Active -> Paused`. **Fail-closed invariant**: neither the kill-switch nor `pause()` nor `Resume` can ever exit `ServiceState::Error` (state strictly remains `Error`; only reloading a valid config with a registrable kill-switch returns the service to `Active`). Selecting "Suspend" from the tray menu also disables handling of user `WM_HOTKEY` triggers.
     - **Architectural Requirement**: Because `WM_HOTKEY` is delivered to the message loop, reading the kill-switch must never be delayed by a running sequence. Sequences therefore run on a separate thread with an atomic cancellation flag (see 3.8).
  6. **Platform Boundaries (UIPI)**:
     - `SendInput` respects Windows UIPI privilege isolation. An unprivileged RustBind process cannot inject input into elevated windows (e.g., Task Manager). This boundary is a Windows security feature and **must not be bypassed** by requesting elevation at launch.
  7. **`SendInput` Return Value Verification**:
     - Always inspect the return value: if fewer events were inserted than requested (UIPI block, input blocked by another app, secure desktop), the action fails explicitly and the sequence aborts.
     - No aggressive retry loops and no bypass attempts.
  8. **Unicode Text**:
     - Strings are encoded into UTF-16 code units; characters outside the Basic Multilingual Plane are sent as surrogate pairs in order within the same batch.
  9. **Program-Pressed Keys/Buttons, Extended Keys, and Trigger Modifiers**:
     - The engine tracks both `pressed_keys` and `pressed_mouse_buttons` pressed by itself via `KeyDown`/`MouseDown` and not yet released; if a sequence aborts or terminates abnormally, corresponding release events (`KeyUp`/`MouseUp`) are emitted automatically. Sent keys always populate `wScan` (`MapVirtualKeyW`) and `KEYEVENTF_EXTENDEDKEY` when required (`Home`, `End`, arrows, `Insert`, `Delete`, `PageUp`/`PageDown`, `NumpadEnter`, etc.).
     - Trigger modifier handling and shortcut pass-through (enabled by default): all enabled rules additionally register additive modifier variants (`+Shift`, `+Ctrl`, `+Alt`, excluding `Win` and `Ctrl+Alt` on `AltGr`-sensitive keys), giving priority to kill-switch, `resume_hotkey`, and explicit rules without inflating `active_rules_count`. With `rule.wait_for_modifier_release = false` (macOS-style remapper default), before the sequence runs, only the trigger's own held modifiers are temporarily released while keeping extra user-held modifiers intact (e.g., `Shift` to form `Shift+Home`/`Shift+End`), masking `Alt`/`Win` menu/Start activation with `VK_LCONTROL` without colliding with `Shift`, and restoring them afterwards (unless cancelled): immediately on instant remaps (`< 40 ms` and without `Delay`), or only if still physically pressed on longer sequences. Only when explicitly enabled per rule (`rule.wait_for_modifier_release = true`) does the daemon wait (up to 500 ms, interruptibly) for the user to release trigger modifiers before executing; on timeout expiration, the sequence does not start and an error event is logged.
  10. **Rule Groups, Group Context & Startup Filtering**:
      - Every `Rule` has a unique name (case-insensitive) and belongs to zero, one, or more groups (`groups`, default `["default"]`, validated as ASCII alphanumeric/`_`/`-` in a privacy-safe way). Groupless rules must stay disabled (`ConfigError::EnabledRuleWithoutGroup`) until assigned a group. `GeneralSettings::active_groups` filters which enabled rules the runner registers (`Vec::new()` activates all existing groups); even if a rule belongs to multiple active groups, the runner registers it once without triggering hotkey conflicts. Custom groups (excluding `"default"`, which stays global) can optionally define a foreground process `WindowFilter` (`[group_filters]`), combined with any rule-level `window_filter` using `Groups (OR) && Rule (AND)` semantics. Optional `settings.resume_hotkey` enables resuming from `Paused` to `Active` while preserving the fail-closed invariant on `Error`.
      - **Exclusive Physical Modifier Query (`GetAsyncKeyState`)**: `GetAsyncKeyState` is strictly limited to querying physical modifier keys (`Ctrl`, `Alt`, `Shift`, `Win`, both generic and left/right variants) solely to synchronize trigger release. Querying any other key (alphanumeric, function, navigation) is categorically prohibited, guaranteeing zero keylogging capability.

### 3.5. Rules & Automation Model
- **Decision**: Hybrid approach with **purely declarative TOML** (`CURRENT_SCHEMA_VERSION = 1`, supporting keyboard, Unicode text, delays, mouse actions, and per-group/per-rule context filters) and trait-abstracted action dispatcher prepared for future extensions.
- **TOML Rule Structure**:
  - Strictly typed and validated TOML schema (`deny_unknown_fields` via Serde): unknown or malformed fields fail immediately and reject loading, preventing unpredictable behavior.
  - Example:
    ```toml
    schema_version = 1

    [[rules]]
    id = "paste_email"
    name = "Insert Email"
    groups = ["default"]
    enabled = true

    [rules.trigger]
    type = "hotkey"
    modifiers = ["ctrl", "alt"]
    key = "e"

    [[rules.actions]]
    type = "send_text"
    text = "user@example.com"

    [[rules.actions]]
    type = "delay"
    milliseconds = 50

    [[rules.actions]]
    type = "key_click"
    key = "tab"
    ```
- **Process Launch Actions (`LaunchProcess`)**:
  - **Completely excluded**. No `LaunchProcess` variant in `InputAction` and no `allow_process_launch` flag in config schema.
  - *Rationale*: Arbitrary process launching from a config file introduces arbitrary code execution risks if the TOML file is tampered with locally.
  - The trait abstraction (`ActionDispatcher`) remains ready for future phases subject to strict security review.
  - *Distinction from tray UI launch*: the daemon launches only its companion binary `rustbind-ui.exe` solely when the user selects `"Configure..."`, resolving the fixed path next to `std::env::current_exe()` and calling `std::process::Command::new` directly without shell invocation, user arguments, or reading any path from `.toml` configs or `PATH`.
- **Configuration Threat Model & Atomic Persistence (`ConfigManager::save_to_file_safe`)**:
  - Canonical folder: `%APPDATA%\RustBind\configs\` (starting with `config.toml`), with active profile filename stored in `%APPDATA%\RustBind\.active_config` (sanitized to ASCII alphanumeric/`_`/`-` + `.toml`).
  - Permissions: Resides in user profile and inherits standard Windows ACLs (writable only by owning user and Administrators).
  - Validation on startup & reload (`ReloadConfig`): if unreadable, invalid (even a single unknown field), or out of safety bounds, RustBind **never renames, moves, or overwrites it**.
  - **Fail-closed**: on invalid config the daemon enters error state, **executes zero rules** (neither prior nor defaults), displays Error state on the tray icon, and the UI displays the error category.
  - **Safe Atomic Save (`save_to_file_safe`)**: validates and serializes the new config first (`save_to_string`). If the target file exists and is invalid, refuses overwriting with `ConfigError::RefusedOverwriteInvalidFile` leaving the original untouched, unless the user explicitly authorizes backup + overwrite (`allow_backup_and_overwrite_invalid == true`), in which case a `.bak.<timestamp>` backup is created first. Writes to a temporary file in the same directory (`*.tmp.<pid>.<counter>`), calls `sync_all()`, and atomically replaces the target via `fs::rename` (cleaning up the temp file on failure).
- **Absent vs Invalid File Behavior**:
  - *Absent file*: if the active `.toml` file does not exist on disk, the daemon does not auto-create or write any file; it starts cleanly in `Active` state with 0 configured rules and logs an informational `INFO` message.
  - *Invalid existing file*: if the file exists with syntax errors, unknown fields, or out-of-bounds values, the daemon strictly applies **fail-closed** behavior: enters `Error` state, executes zero rules, leaves the file untouched, and displays Error on the tray icon.

### 3.6. IPC Protocol & Security
- **Mechanism**: Local Windows Named Pipe (`\\.\pipe\RustBind-IPC-<UserSID>`).
- **Access Control & Security Flags**:
  - `FILE_FLAG_FIRST_PIPE_INSTANCE`: guarantees the daemon is the first instance creating the pipe, preventing pipe-squatting attacks by preexisting processes.
  - `PIPE_REJECT_REMOTE_CLIENTS`: strictly rejects any network or remote client connections via RPC/SMB.
  - Pipe created with a restrictive DACL via SDDL (`D:P(A;;GRGW;;;{sid})`) granting solely Generic Read and Generic Write (`GRGW`) to the current user token (`TokenUser`), revoking `GENERIC_ALL` and permission modification rights.
- **Threat Model**:
  - Strictly protects against remote clients and other local users (even unprivileged) on the same machine.
  - **Windows Threat Model Boundary**: a user-SID DACL **does NOT protect** against other processes running under the exact same user account (sharing the same security token).
  - *Mitigation via Strict Whitelist*: because any same-user process can connect to the local pipe, RustBind minimizes attack surface by exposing strictly 4 non-arbitrary state/config commands:
    1. `GetStatus` (opcode `0x01`): read-only runtime status and active rule count (logged at `DEBUG` so polling never floods logs).
    2. `Suspend` (opcode `0x02`): immediate precautionary transition to `Paused` (equivalent to kill-switch; if daemon is in `Error`, no change and replies `0x02`).
    3. `ReloadConfig` (opcode `0x03`): reloads the currently active `.toml` profile under `%APPDATA%\RustBind\configs\` while strictly enforcing fail-closed validation.
    4. `Resume` (opcode `0x04`): controlled transition from `Paused` to `Active`; **fail-closed invariant**: if daemon is in `Error`, no change and replies `0x02`.
  - *Formally Excluded Commands*: zero keyboard/mouse injection and zero execution of sequences, macros, or external processes are allowed over IPC.
- **Pure Byte Wire Protocol (No JSON / `serde_json`)**:
  - Pure byte-level binary protocol with zero complex parser dependencies.
  - Request format: length-prefixed frame (4-byte little-endian) followed by 1-byte payload (`0x01` GetStatus, `0x02` Suspend, `0x03` ReloadConfig, `0x04` Resume).
  - Max payload size: 16 bytes (`MAX_IPC_FRAME_SIZE = 16`). Frames with length `0` (`Empty`), `>16` (`TooLarge`), or not equal to 1 byte (`BadLength`) are rejected right from the header, returning `0xFF`.
  - Response format: 4-byte little-endian length prefix + binary payload:
    - `GetStatus`: 6-byte payload (`0x00` ok, 1-byte state [`0` Active, `1` Paused, `2` Error], 4-byte `active_rules` as `u32` little-endian).
    - `Suspend`: 1-byte payload (`0x01` suspended or already paused, `0x02` in Error state without change).
    - `ReloadConfig`: 2-byte payload (`[0x03, outcome]` with `0x01` accepted/queued).
    - `Resume`: 2-byte payload (`[0x04, outcome]` with `0x01` resumed or already active, `0x02` in Error state without change).
    - Protocol error: 1-byte payload (`0xFF`).
  - Connection lifecycle: one request per connection; server reads frame, writes response, and immediately disconnects (`DisconnectNamedPipe`).
  - I/O timeout: 2 seconds for read and write operations (`DEFAULT_IPC_IO_TIMEOUT`) to prevent denial-of-service from slow/stalled clients.
  - Single client instance at a time (`nMaxInstances = 1`).
  - Log privacy: zero input payloads or text logged; only command events are logged (`GetStatus` at `DEBUG`, mutating commands at `INFO`).
- **`WorkerThread` Architecture & Clean Shutdown with Overlapped Guarantees**:
  - The IPC server runs on a dedicated worker thread registered with `WorkerThreads`.
  - Wakes the message loop via `WakeHandle` and a thread-safe action queue (`IpcServerAction::{Suspend, ReloadConfig, Resume}`).
  - Shutdown on "Exit": Win32 calls (`ConnectNamedPipe`, `ReadFile`, `WriteFile`) use asynchronous I/O (`FILE_FLAG_OVERLAPPED`) paired with a manual-reset `stop_event`; on shutdown, the event is signaled and pending I/O is cancelled via `CancelIoEx`.
  - Kernel memory invariant & `PendingIo`: async operations (`ConnectNamedPipe`, `ReadFile`, `WriteFile`, and final drain) are encapsulated in RAII `PendingIo`, owning its dedicated manual-reset `hEvent` (`SafeHandle`), heap `OVERLAPPED`, and buffer (`Box` inside `ManuallyDrop`, zero `unwrap`/`expect`). On `Drop`, if incomplete, it calls `CancelIoEx` (handling `ERROR_NOT_FOUND`), waits up to 500 ms via `WaitForSingleObject`, and calls `GetOverlappedResult(bWait = false)` handling `ERROR_OPERATION_ABORTED` and `ERROR_NOT_FOUND`. If the 500 ms limit expires before kernel release, `hEvent` is intentionally leaked alongside `OVERLAPPED` and the buffer (`Box::leak` / `std::mem::forget`) with a `tracing::warn`, guaranteeing the event is never closed or reset while the kernel may still reference `OVERLAPPED`.
  - Post-response drain (zero blocking `FlushFileBuffers`): after `WriteFile`, the server runs an overlapped 1-byte drain via `PendingIo` waiting on `[hEvent, stop_event]` with a short 500 ms timeout, immediately interruptible on shutdown (`stop_event`) or client disconnect before `DisconnectNamedPipe`.
  - Synchronization ensures worker join timeout (`WorkerThreads::join_all`, set to 1000 ms in `MessageLoop::drop`) strictly exceeds the I/O cancellation bound (500 ms), guaranteeing clean thread termination prior to window destruction.
  - I/O logging: client disconnect before reading response (`ERROR_BROKEN_PIPE` 109 and `ERROR_NO_DATA` 232) is logged at `DEBUG`, unexpected errors at `WARN`.
- **Shared Protocol Module, Named Pipe Client & `rustbindctl` CLI**:
  - `src/ipc_protocol.rs`: pure shared module (zero `unsafe`, zero Windows types, zero `unwrap`/`expect`) containing constants, `IpcCommand`, `IpcResponse`, `IpcProtocolError`, `get_pipe_name`, `encode_ipc_command`, header/payload/response validation, and `decode_ipc_response`.
  - `src/platform/windows/ipc_client.rs`: only `unsafe` client module (all blocks annotated with `// SAFETY:`, zero `unwrap`/`expect`). Implements `send_command` and `send_command_to_pipe`: opens the pipe with `CreateFileW(..., FILE_FLAG_OVERLAPPED, ...)`, retries `ERROR_PIPE_BUSY` via `WaitNamedPipeW`, sends/reads via `PendingIo` and `WaitForSingleObject` with `DEFAULT_IPC_IO_TIMEOUT`, distinguishing `DaemonNotRunning` (`ERROR_FILE_NOT_FOUND`), `PipeBusyTimeout`, `IoTimeout`, `Protocol(IpcProtocolError)`, and `OsError`.
  - `src/bin/rustbindctl.rs`: console CLI binary (zero `unsafe`, zero `unwrap`/`expect`, no `#![windows_subsystem = "windows"]`). Supports `status`, `suspend`, `resume`, `reload`, and `-h`/`--help` with exit codes:
    - `0`: success (`status=<active|paused|error> active_rules=<N>`, `suspended`, `resumed`, `reloaded`, or `-h`/`--help`).
    - `1`: invalid, missing, or extra arguments, or unrecognized command, printing usage to `stderr`.
    - `3`: daemon not running (`DaemonNotRunning`) or pipe busy timeout (`PipeBusyTimeout`).
    - `4`: protocol error, `IoTimeout`, `OsError`, unexpected response/`IpcResponse::Error`, or daemon in `Error` state on `suspend` / `resume` (`0x02`, no changes applied).

### 3.7. Tray Icon Behavior & Transparency
- **Constant Visibility**: The taskbar tray icon is **always visible**; the program offers no option to hide itself (transparency principle).
- **`NOTIFYICON_VERSION_4` Standard & Tooltips**:
  - Uses modern Shell APIs with `NOTIFYICON_VERSION_4`.
  - Mandatory `NIF_SHOWTIP` flag on both `NIM_ADD` and `NIM_MODIFY` so Windows displays standard `szTip` tooltips (suppressed in v4 otherwise).
  - Explicit `Shell_NotifyIconW(NIM_SETVERSION, NOTIFYICON_VERSION_4)` call after every `NIM_ADD`, including Explorer restart recovery (`TaskbarCreated`).
- **State Representation & Localization**:
  - Distinct geometric icon shapes (accessibility) + color, paired with localized tooltips and context menu entries synced with `language` in `%APPDATA%\RustBind\preferences.toml` (English default, plus Italian, French, German, and Spanish):
    - *Active*: solid circle (green), e.g. `"RustBind - Active"`.
    - *Paused*: square / double pause bars (amber/orange), e.g. `"RustBind - Paused"`.
    - *Error*: cross mark (red), e.g. `"RustBind - Error (No rules running)"`.
  - Icons generated entirely at runtime in memory via Win32 `CreateIcon` (no embedded binary assets or extra build tools needed for tray states).
- **User Interaction Semantics**:
  - *Single Left-Click*: No action (`NIN_SELECT` discarded to prevent accidental toggling).
  - *Keyboard (`Win+B`)*: Enter and Spacebar represent primary icon activation and, matching single left-click, intentionally take no action. Opening the context menu via keyboard uses `Shift+F10` or the Menu/Application key (generating `WM_CONTEXTMENU`).
  - *Context Menu*: Opened exclusively via `WM_CONTEXTMENU` (right-click, `Shift+F10`, or Menu key). While open, no further tray events are queued, and when `TrackPopupMenuEx` returns, pending tray events are drained to avoid accidental reopening.
  - *Double Left-Click*: May **only suspend** the daemon (`runner.pause()`); does nothing if already paused or in Error state. Resuming is allowed via the context menu item, `Resume` IPC command, or configured `resume_hotkey` (all strictly preserving fail-closed `ServiceState::Error`).
- **Diagnostic Flag `--debug-log`**:
  - CLI option elevating the `tracing` subscriber level to `DEBUG`.
  - Logs only tray callbacks in hex (solely the numeric notification code, with zero mouse/keyboard distinction), strictly excluding high-frequency `WM_MOUSEMOVE` (`0x0200`) and honoring full input privacy. Without this flag, the daemon runs at `INFO`.
- **Explorer Restart Resilience (`TaskbarCreated`)**:
  - The message loop listens for the registered system broadcast `RegisterWindowMessageW(w!("TaskbarCreated"))`.
  - On Windows taskbar (`explorer.exe`) restart/crash, the daemon immediately re-adds the icon via `Shell_NotifyIconW(NIM_ADD)` and reapplies version 4.
- **Minimal Context Menu**:
  - State: *Suspend* / *Resume* (disabled and labeled *Paused (Error)* when in `ServiceState::Error`)
  - *Active groups* (submenu listing all available groups with `MF_CHECKED`/`MF_UNCHECKED` state, disabled/`MF_GRAYED` in `ServiceState::Error`; toggling persists `active_groups` safely via `ConfigManager::save_to_file_safe`, preserves `"default"` fallback, reloads the runner, and keeps `Paused` state when paused)
  - *Configure...* (launches companion `rustbind-ui.exe` from `current_exe()`'s directory, or restores/focuses existing `"RustBind"` UI window)
  - *Reload Rules* (reloads active `.toml` config on demand, logging resulting state at `INFO`: Active with `active_rules=N`, or Error without file snippets or values)
  - *Start when Windows signs in* (checkable `MF_CHECKED`/`MF_UNCHECKED` menu item toggling `HKCU\...\Run\RustBind` via `autostart.rs`)
  - *Update available* (shown conditionally right above *Exit* when automatic update checking is enabled and `%LOCALAPPDATA%\RustBind\update_cache.toml` holds a newer release; launches `rustbind-ui.exe` where the update banner is presented)
  - *Exit* (stops sequences with timeout, deletes tray icon with `NIM_DELETE`, closes any `"RustBind"` UI window cleanly via `PostMessageW(WM_CLOSE)`, releases mutex, and exits)
- **Autostart**: Optional, off by default, managed via user registry `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (no admin rights) from either the tray menu or the UI Settings tab.

### 3.8. Concurrency and Sequence Cancellation
- **Message Loop Thread**: receives `WM_HOTKEY`, tray messages, foreground switch notifications, and IPC actions. Never executes sequences or blocking waits.
- **Execution Thread**: executes actions one sequence at a time. At most one sequence runs concurrently: any hotkey pressed while a sequence is active is ignored and logged as a status event.
- **Cancellation**: a shared atomic flag is set immediately by the message loop on kill-switch press, "Suspend", "Exit", or configuration state change. The executor checks the flag before each action and inside every `Delay`; delays use interruptible waits (short steps or timeout synchronization events), never long uninterruptible sleeps.
- **Cleanup on Cancellation**: only keys (`pressed_keys`) and mouse buttons (`pressed_mouse_buttons`) pressed by the program and not yet released are released (see 3.4, item 9).
- **Shared State**: service state (active, paused, error) is synchronized cleanly; the execution thread never holds locks while waiting.
- **`WakeHandle` & `WorkerThreads` Synchronization**:
  - `PostMessageW` at kernel level verifies only handle validity in the handle table, not ownership by the original window (risk of `HWND` handle reuse for third-party windows).
  - *Safety Invariant*: no thread holding a `WakeHandle` may stay active when `DestroyWindow` destroys the window.
  - `WorkerThreads` tracks and joins worker threads associated with the message loop before window drop. If timeout expires, the message loop logs `tracing::warn` and intentionally leaks the window handle (`SafeHwnd::leak()`) to prevent Windows from reassigning the `HWND` to another application.

---

## 4. Code and Module Layout

The repository follows a strict separation between pure domain abstractions, logic engine, native OS layer, UI configurator, and build tooling:

```
src/
├── lib.rs                      # Shared library crate (config, core, engine, ipc_protocol, logging, platform, updater)
├── main.rs                     # Windows daemon entry point (#![windows_subsystem = "windows"])
├── ipc_protocol.rs             # Pure shared binary IPC protocol (zero unsafe, zero Win32)
├── updater.rs                  # Pure GitHub Release parser, HTTPS host allowlist, semver & update cache
├── bin/
│   └── rustbindctl.rs          # Console CLI IPC control client (status, suspend, resume, reload)
├── core/                       # Pure domain types (safe, zero Win32 dependencies)
│   ├── mod.rs                  # Schema version and safety constants
│   ├── key.rs                  # VirtualKey
│   ├── modifiers.rs            # Modifiers
│   ├── binding.rs              # KeyBinding
│   ├── action.rs               # InputAction (Keyboard, Text, Delay, Mouse), MouseButton, MouseWheelDirection
│   ├── dispatcher.rs           # ActionDispatcher trait
│   └── rule.rs                 # Rule, Trigger, WindowFilter, WindowFilterMode
├── config/                     # TOML persistence and validation
│   ├── mod.rs
│   ├── error.rs                # Privacy-safe ConfigError
│   ├── schema.rs               # Serializable structs (deny_unknown_fields)
│   └── manager.rs              # Load, atomic save, error backups, configs/ & .active_config resolution
├── engine/                     # Reactive rule engine
│   ├── mod.rs
│   ├── clock.rs                # Abstract Clock & SystemClock
│   ├── state.rs                # ServiceState, SkippedRuleReason & runtime_diagnostics.json persistence
│   ├── runner.rs               # Coordinator: hotkey mapping, pass-throughs, context switching -> dispatch
│   ├── executor.rs             # Cancellable sequence executor thread (atomic flag)
│   └── circuit_breaker.rs      # Sliding-window rate limiting & action event counting
└── platform/                   # Platform abstractions and safe traits
    ├── mod.rs
    ├── traits.rs               # Traits: Platform, HotkeyManager, InputSender, TrayManager
    └── windows/                # ALL Win32 unsafe code isolated here
        ├── mod.rs              # Windows module & wrapper exports
        ├── handles.rs          # RAII wrappers: SafeHwnd, SafeHicon, SafeHandle
        ├── conversion.rs       # VirtualKey / Modifiers Win32 mapping & extended-key classification
        ├── message_loop.rs     # Invisible window and GetMessageW / DispatchMessageW loop
        ├── hotkey.rs           # unsafe RegisterHotKey / UnregisterHotKey calls
        ├── input.rs            # unsafe SendInput (keyboard & mouse) & modifier-only GetAsyncKeyState
        ├── tray.rs             # unsafe Shell_NotifyIconW calls & 5-language tray menu/tooltips
        ├── single_instance.rs  # Daemon/UI Named Mutexes & focus_window_by_title
        ├── autostart.rs        # Per-user HKCU\...\Run autostart & close_window_by_title_best_effort
        ├── file_dialog.rs      # Native Win32 Open/Save/Pick file dialogs
        ├── foreground.rs       # ForegroundWatcher (WinEvent) & running/installed app discovery
        ├── worker_threads.rs   # WorkerThreads tracking & bounded join
        ├── ipc_server.rs       # DACL-restricted Named Pipe server, WorkerThread & PendingIo
        ├── ipc_client.rs       # Overlapped Named Pipe client with timeout & PendingIo
        └── updater.rs          # WinHTTP HTTPS client, SHA-256 verification & staged binary update
ui/                             # Separate workspace crate for rustbind-ui.exe (eframe/egui, zero unsafe)
├── Cargo.toml
├── build.rs                    # Windows icon/version resource embedding
└── src/
    ├── main.rs                 # Ephemeral window, visual editor UI, Theme Editor, update notice, mpsc channels
    ├── state.rs                # Pure DaemonStatusView & tab/subtab view-models (unit-tested)
    ├── i18n.rs                 # 5-language localization tables
    ├── theme.rs                # 10 preset themes, CustomTheme + radii, True Black & preferences.toml persistence/TOML I/O
    ├── capture.rs              # Live trigger & keyboard/mouse action capture
    └── editor.rs               # Editor state, multi-group filters, live validation, atomic save & restore
xtask/                          # Workspace build/packaging/install CLI (`cargo xtask`)
├── Cargo.toml
└── src/
    ├── main.rs                 # `package` (portable .zip + SHA-256 + optional WiX MSI), `install`, `uninstall`
    ├── sha256.rs               # Pure Rust SHA-256 generator (FIPS 180-4 test-verified)
    └── win32.rs                # Native Shell/Process helpers for Start Menu shortcut & process checks
```

---

## 5. Testability and Testing Strategy

1. **Pure Domain Unit Tests (`src/core/`, `src/config/`)**:
   - Parsing valid rules (`schema_version = 1`) and strictly rejecting unknown or malformed fields.
   - Validating key combinations, mouse action limits, and parsing Unicode strings.
   - TOML serialization and deserialization round-trips plus atomic save safety checks.
2. **Mock Platform Integration Tests (`src/platform/traits.rs`, `src/engine/`)**:
   - `MockPlatform` struct implementing `HotkeyManager` and `InputSender`.
   - Tests verify hotkey triggers emit the exact expected `InputAction` sequence without touching the Windows OS or generating real input.
   - Emergency circuit verification (kill-switch) on long or simulated sequences, including cancellation mid-`Delay` and releasing only program-pressed keys/buttons.
   - Verification of foreground `WindowFilter` swapping, pass-through modifier priority, single-rule partial degradation, busy kill-switch fail-closed behavior, and invalid config error state (zero rules executed).
   - Tests interacting with the real OS (`RegisterHotKey`, `SendInput`, live Named Pipes) are marked `#[ignore]` and never type into windows outside test-owned windows.
3. **CI/CD Quality Gate**:
   - Pre-commit checks: `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `cargo audit` (plus portable bundle verification via `cargo xtask package --run-tests`).

---

## 6. Privacy and Logging Policy

- **No Keystroke Recording**: RustBind is not a keylogger and implements no global input buffer. `GetAsyncKeyState` is used solely to query physical modifier keys (`Ctrl`, `Alt`, `Shift`, `Win`) for trigger release synchronization and never inspects character or other keys.
- **Log & Diagnostics Destinations**:
  - Daemon logs are written to `%LOCALAPPDATA%\RustBind\logs\rustbind.log` with automatic rotation at max 5 MB keeping a limited history (`rustbind.log.1`, `rustbind.log.2`).
  - Non-sensitive runtime hotkey registration conflict summaries are persisted atomically to `%LOCALAPPDATA%\RustBind\runtime_diagnostics.json`.
  - GitHub release check metadata is cached locally in `%LOCALAPPDATA%\RustBind\update_cache.toml`. Update checks only send standard GET requests over HTTPS (`WinHTTP`) to `api.github.com` / GitHub release CDN hosts on the allowlist, with zero telemetry, analytics, or user configuration payloads transmitted.
- **Log Sanitization (`tracing`)**:
  - `INFO` level: Logs only lifecycle events and state transitions (e.g., daemon started, hotkey activated, rule completion timing).
  - `DEBUG`/`TRACE` levels: **NEVER** include `SendText` payloads or typed character sequences, preventing accidental logging of passwords, emails, or personal data.
  - **Configuration Error Privacy (`ConfigError`)**: Errors produced by TOML parsing and validation **NEVER** include file text snippets or config values (`SendText` payloads, tokens, or out-of-range numeric values). Only the generic error category (syntax error, unknown field, out-of-bounds value) and line/column coordinates are exposed so neither `format!("{err}")` nor `format!("{err:?}")` can leak sensitive data into logs or UI error states.
  - **Panic Hook Privacy & Sanitization**: The daemon installs a custom panic hook logging solely source location (`file:line`) and generic error category, excluding panic payloads/messages to prevent secret leakage, performs best-effort tray icon cleanup (`NIM_DELETE`), and terminates the process.
- **Pre-Logging Single-Instance Check**:
  - The Named Mutex check (`Local\RustBind_Daemon_SingleInstance`) runs before initializing the file logger. If another instance is already running, the process exits immediately with code `2` without opening or writing to the log file.
