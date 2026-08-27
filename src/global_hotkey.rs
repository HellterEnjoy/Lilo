//! System-wide global hotkey registration for background and unfocused Quick Capture.

use std::sync::mpsc::{self, Receiver};

#[cfg(target_os = "windows")]
use std::sync::mpsc::Sender;
#[cfg(target_os = "windows")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "windows")]
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub enum GlobalHotkeyEvent {
    QuickCapture,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub struct ParsedShortcut {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub key: String,
    pub virtual_key_code: u32,
}

/// Parses a shortcut string such as `"Ctrl+Shift+C"` or `"Ctrl+Alt+C"`.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn parse_shortcut(shortcut_str: &str) -> Option<ParsedShortcut> {
    let parts: Vec<&str> = shortcut_str
        .split('+')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();

    if parts.is_empty() {
        return None;
    }

    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut win = false;
    let mut key_str = None;

    for &part in &parts {
        if part.eq_ignore_ascii_case("ctrl") || part.eq_ignore_ascii_case("control") {
            ctrl = true;
        } else if part.eq_ignore_ascii_case("alt") {
            alt = true;
        } else if part.eq_ignore_ascii_case("shift") {
            shift = true;
        } else if part.eq_ignore_ascii_case("win")
            || part.eq_ignore_ascii_case("super")
            || part.eq_ignore_ascii_case("meta")
        {
            win = true;
        } else {
            key_str = Some(part);
        }
    }

    let key_name = key_str?.to_ascii_uppercase();
    let vk = match key_name.as_str() {
        "A" => 0x41,
        "B" => 0x42,
        "C" => 0x43,
        "D" => 0x44,
        "E" => 0x45,
        "F" => 0x46,
        "G" => 0x47,
        "H" => 0x48,
        "I" => 0x49,
        "J" => 0x4A,
        "K" => 0x4B,
        "L" => 0x4C,
        "M" => 0x4D,
        "N" => 0x4E,
        "O" => 0x4F,
        "P" => 0x50,
        "Q" => 0x51,
        "R" => 0x52,
        "S" => 0x53,
        "T" => 0x54,
        "U" => 0x55,
        "V" => 0x56,
        "W" => 0x57,
        "X" => 0x58,
        "Y" => 0x59,
        "Z" => 0x5A,
        "0" => 0x30,
        "1" => 0x31,
        "2" => 0x32,
        "3" => 0x33,
        "4" => 0x34,
        "5" => 0x35,
        "6" => 0x36,
        "7" => 0x37,
        "8" => 0x38,
        "9" => 0x39,
        "F1" => 0x70,
        "F2" => 0x71,
        "F3" => 0x72,
        "F4" => 0x73,
        "F5" => 0x74,
        "F6" => 0x75,
        "F7" => 0x76,
        "F8" => 0x77,
        "F9" => 0x78,
        "F10" => 0x79,
        "F11" => 0x7A,
        "F12" => 0x7B,
        "SPACE" => 0x20,
        _ => return None,
    };

    Some(ParsedShortcut {
        ctrl,
        alt,
        shift,
        win,
        key: key_name,
        virtual_key_code: vk,
    })
}

pub struct GlobalHotkeyManager {
    receiver: Receiver<GlobalHotkeyEvent>,
    #[cfg(target_os = "windows")]
    thread_id: Arc<Mutex<Option<u32>>>,
    #[cfg(target_os = "windows")]
    current_shortcut: Arc<Mutex<Option<ParsedShortcut>>>,
}

impl GlobalHotkeyManager {
    pub fn new(enabled: bool, shortcut_str: &str) -> Self {
        let (tx, rx) = mpsc::channel();

        #[cfg(target_os = "windows")]
        {
            let parsed = if enabled {
                parse_shortcut(shortcut_str)
            } else {
                None
            };
            let current_shortcut = Arc::new(Mutex::new(parsed));
            let thread_id = Arc::new(Mutex::new(None));

            let thread_shortcut = current_shortcut.clone();
            let thread_id_clone = thread_id.clone();

            thread::spawn(move || {
                win32_hotkey_loop(tx, thread_shortcut, thread_id_clone);
            });

            Self {
                receiver: rx,
                thread_id,
                current_shortcut,
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = (enabled, shortcut_str, tx);
            Self { receiver: rx }
        }
    }

    pub fn try_recv(&self) -> Option<GlobalHotkeyEvent> {
        self.receiver.try_recv().ok()
    }

    pub fn update_shortcut(&self, enabled: bool, shortcut_str: &str) {
        #[cfg(target_os = "windows")]
        {
            let parsed = if enabled {
                parse_shortcut(shortcut_str)
            } else {
                None
            };
            if let Ok(mut current) = self.current_shortcut.lock() {
                *current = parsed;
            }
            if let Ok(tid_guard) = self.thread_id.lock()
                && let Some(tid) = *tid_guard
            {
                unsafe {
                    win32::PostThreadMessageW(tid, win32::WM_UPDATE_HOTKEY, 0, 0);
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (enabled, shortcut_str);
        }
    }
}

#[cfg(target_os = "windows")]
mod win32 {
    #![allow(clippy::upper_case_acronyms)]

    use std::ffi::c_void;
    pub type HWND = *mut c_void;
    pub type BOOL = i32;
    pub type UINT = u32;
    pub type WPARAM = usize;
    pub type LPARAM = isize;
    pub type DWORD = u32;

    #[repr(C)]
    pub struct POINT {
        pub x: i32,
        pub y: i32,
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    pub struct MSG {
        pub hwnd: HWND,
        pub message: UINT,
        pub wParam: WPARAM,
        pub lParam: LPARAM,
        pub time: DWORD,
        pub pt: POINT,
    }

    pub const WM_HOTKEY: UINT = 0x0312;
    pub const WM_QUIT: UINT = 0x0012;
    pub const WM_USER: UINT = 0x0400;
    pub const WM_UPDATE_HOTKEY: UINT = WM_USER + 1;

    pub const MOD_ALT: UINT = 0x0001;
    pub const MOD_CONTROL: UINT = 0x0002;
    pub const MOD_SHIFT: UINT = 0x0004;
    pub const MOD_WIN: UINT = 0x0008;
    pub const MOD_NOREPEAT: UINT = 0x4000;

    pub const SW_RESTORE: i32 = 9;

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn RegisterHotKey(hWnd: HWND, id: i32, fsModifiers: UINT, vk: UINT) -> BOOL;
        pub fn UnregisterHotKey(hWnd: HWND, id: i32) -> BOOL;
        pub fn GetMessageW(
            lpMsg: *mut MSG,
            hWnd: HWND,
            wMsgFilterMin: UINT,
            wMsgFilterMax: UINT,
        ) -> BOOL;
        pub fn PostThreadMessageW(
            idThread: DWORD,
            Msg: UINT,
            wParam: WPARAM,
            lParam: LPARAM,
        ) -> BOOL;
        pub fn GetCurrentThreadId() -> DWORD;
        pub fn GetCurrentProcessId() -> DWORD;
        pub fn EnumWindows(
            lpEnumFunc: unsafe extern "system" fn(HWND, LPARAM) -> BOOL,
            lParam: LPARAM,
        ) -> BOOL;
        pub fn GetWindowThreadProcessId(hWnd: HWND, lpdwProcessId: *mut DWORD) -> DWORD;
        pub fn IsWindowVisible(hWnd: HWND) -> BOOL;
        pub fn SetForegroundWindow(hWnd: HWND) -> BOOL;
        pub fn ShowWindow(hWnd: HWND, nCmdShow: i32) -> BOOL;
    }

    pub unsafe fn bring_app_window_to_foreground() {
        let current_pid = unsafe { GetCurrentProcessId() };

        unsafe extern "system" fn enum_window_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let target_pid = lparam as DWORD;
            let mut window_pid: DWORD = 0;
            unsafe {
                GetWindowThreadProcessId(hwnd, &mut window_pid);
                if window_pid == target_pid && IsWindowVisible(hwnd) != 0 {
                    ShowWindow(hwnd, SW_RESTORE);
                    SetForegroundWindow(hwnd);
                    return 0; // stop enumeration
                }
            }
            1 // continue
        }

        unsafe {
            EnumWindows(enum_window_callback, current_pid as LPARAM);
        }
    }
}

#[cfg(target_os = "windows")]
fn win32_hotkey_loop(
    tx: Sender<GlobalHotkeyEvent>,
    shortcut_holder: Arc<Mutex<Option<ParsedShortcut>>>,
    thread_id_holder: Arc<Mutex<Option<u32>>>,
) {
    let tid = unsafe { win32::GetCurrentThreadId() };
    if let Ok(mut guard) = thread_id_holder.lock() {
        *guard = Some(tid);
    }

    let hotkey_id = 0x4C49; // "LI" in hex
    let mut is_registered = false;

    let register = |shortcut: &ParsedShortcut| -> bool {
        let mut modifiers = win32::MOD_NOREPEAT;
        if shortcut.alt {
            modifiers |= win32::MOD_ALT;
        }
        if shortcut.ctrl {
            modifiers |= win32::MOD_CONTROL;
        }
        if shortcut.shift {
            modifiers |= win32::MOD_SHIFT;
        }
        if shortcut.win {
            modifiers |= win32::MOD_WIN;
        }

        unsafe {
            win32::RegisterHotKey(
                std::ptr::null_mut(),
                hotkey_id,
                modifiers,
                shortcut.virtual_key_code,
            ) != 0
        }
    };

    let unregister = || unsafe {
        win32::UnregisterHotKey(std::ptr::null_mut(), hotkey_id);
    };

    // Initial registration
    if let Ok(guard) = shortcut_holder.lock()
        && let Some(ref sc) = *guard
    {
        is_registered = register(sc);
    }

    let mut msg: win32::MSG = unsafe { std::mem::zeroed() };
    while unsafe { win32::GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
        if msg.message == win32::WM_HOTKEY && msg.wParam == hotkey_id as usize {
            unsafe {
                win32::bring_app_window_to_foreground();
            }
            let _ = tx.send(GlobalHotkeyEvent::QuickCapture);
        } else if msg.message == win32::WM_UPDATE_HOTKEY {
            if is_registered {
                unregister();
                is_registered = false;
            }
            if let Ok(guard) = shortcut_holder.lock()
                && let Some(ref sc) = *guard
            {
                is_registered = register(sc);
            }
        } else if msg.message == win32::WM_QUIT {
            break;
        }
    }

    if is_registered {
        unregister();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ctrl_shift_c() {
        let sc = parse_shortcut("Ctrl+Shift+C").unwrap();
        assert!(sc.ctrl);
        assert!(sc.shift);
        assert!(!sc.alt);
        assert!(!sc.win);
        assert_eq!(sc.key, "C");
        assert_eq!(sc.virtual_key_code, 0x43);
    }

    #[test]
    fn parses_ctrl_alt_c() {
        let sc = parse_shortcut("Ctrl+Alt+C").unwrap();
        assert!(sc.ctrl);
        assert!(sc.alt);
        assert!(!sc.shift);
        assert!(!sc.win);
        assert_eq!(sc.key, "C");
    }

    #[test]
    fn parses_function_keys_and_digits() {
        let sc = parse_shortcut("Alt+F11").unwrap();
        assert!(sc.alt);
        assert_eq!(sc.key, "F11");
        assert_eq!(sc.virtual_key_code, 0x7A);

        let sc_digit = parse_shortcut("Ctrl+Shift+1").unwrap();
        assert!(sc_digit.ctrl);
        assert!(sc_digit.shift);
        assert_eq!(sc_digit.key, "1");
        assert_eq!(sc_digit.virtual_key_code, 0x31);
    }

    #[test]
    fn rejects_invalid_shortcut() {
        assert!(parse_shortcut("").is_none());
        assert!(parse_shortcut("Ctrl+").is_none());
        assert!(parse_shortcut("UnknownKey").is_none());
    }
}
