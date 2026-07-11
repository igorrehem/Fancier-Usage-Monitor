use windows::core::PCWSTR;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Shell::{SHAppBarMessage, ABM_GETTASKBARPOS, APPBARDATA};
use windows::Win32::UI::WindowsAndMessaging::*;

// Window style constants
pub const WS_POPUP_STYLE: u32 = 0x80000000;
pub const WS_CHILD_STYLE: u32 = 0x40000000;
pub const WS_CLIPSIBLINGS_STYLE: u32 = 0x04000000;

// Win event constants
pub const EVENT_OBJECT_LOCATIONCHANGE: u32 = 0x800B;
pub const WINEVENT_OUTOFCONTEXT: u32 = 0x0000;

// Timer IDs
pub const TIMER_POLL: usize = 1;
pub const TIMER_COUNTDOWN: usize = 2;
pub const TIMER_RESET_POLL: usize = 3;
pub const TIMER_UPDATE_CHECK: usize = 4;
/// Drives repaints while the animation clock has active work (fill/shimmer/glow/fade).
/// Started when a render kicks off an animation and stopped once `AnimationClock::tick`
/// reports `active == false`, so idle CPU returns to ~0%.
pub const IDT_ANIM: usize = 0xA0;
/// Same purpose as `IDT_ANIM` but for the settings window's live preview animation clock
/// (`config_window.rs`). Deliberately a distinct ID/value from `IDT_ANIM`: the two clocks are
/// wholly independent (the preview never shares the main widget's global `ANIM`), and this
/// keeps their timers unambiguous even though in practice Win32 already scopes timer IDs per
/// HWND, so the two windows' timers could never actually collide.
pub const IDT_PREVIEW_ANIM: usize = 0xA1;
/// One-shot debounce timer for tray icon clicks. Windows always delivers `WM_LBUTTONUP` for
/// the first click of a double-click before `WM_LBUTTONDBLCLK` fires for the second, so a
/// naive handler would toggle widget visibility on every double-click just before opening
/// settings. Instead, `WM_LBUTTONUP` starts this timer (duration `GetDoubleClickTime()`)
/// rather than acting immediately; if `WM_LBUTTONDBLCLK` arrives first, the timer is killed
/// and settings opens without the toggle ever firing. Distinct from `IDT_ANIM`/
/// `IDT_PREVIEW_ANIM` (Win32 scopes timer IDs per HWND anyway, so collision isn't possible,
/// but a distinct ID keeps the `WM_TIMER` dispatch unambiguous).
pub const IDT_TRAY_CLICK_DEBOUNCE: usize = 0xA2;

// Custom messages
pub const WM_APP: u32 = 0x8000;
pub const WM_APP_USAGE_UPDATED: u32 = WM_APP + 1;
pub const WM_APP_TRAY: u32 = WM_APP + 3;

#[derive(Clone, Copy, Debug)]
pub struct TaskbarWindow {
    pub hwnd: HWND,
    pub rect: RECT,
}

pub fn find_taskbars() -> Vec<TaskbarWindow> {
    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let taskbars = &mut *(lparam.0 as *mut Vec<TaskbarWindow>);
        let mut class_name = [0u16; 64];
        let len = unsafe { GetClassNameW(hwnd, &mut class_name) };
        if len > 0 {
            let class_name = String::from_utf16_lossy(&class_name[..len as usize]);
            if class_name == "Shell_TrayWnd" || class_name == "Shell_SecondaryTrayWnd" {
                if let Some(rect) = get_taskbar_rect(hwnd).or_else(|| get_window_rect_safe(hwnd)) {
                    taskbars.push(TaskbarWindow { hwnd, rect });
                }
            }
        }
        BOOL(1)
    }

    let mut taskbars: Vec<TaskbarWindow> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut taskbars as *mut _ as isize));
    }
    taskbars.sort_by_key(|taskbar| {
        (
            taskbar.rect.top,
            taskbar.rect.left,
            taskbar.rect.bottom,
            taskbar.rect.right,
        )
    });
    taskbars
}

/// Find a child window by class name
pub fn find_child_window(parent: HWND, class_name: &str) -> Option<HWND> {
    unsafe {
        let class = wide_str(class_name);
        match FindWindowExW(
            parent,
            HWND::default(),
            PCWSTR::from_raw(class.as_ptr()),
            PCWSTR::null(),
        ) {
            Ok(h) if h != HWND::default() => Some(h),
            _ => None,
        }
    }
}

/// Get taskbar position via SHAppBarMessage
pub fn get_taskbar_rect(taskbar_hwnd: HWND) -> Option<RECT> {
    unsafe {
        let mut class_name = [0u16; 64];
        let len = GetClassNameW(taskbar_hwnd, &mut class_name);
        if len > 0 {
            let class_name = String::from_utf16_lossy(&class_name[..len as usize]);
            if class_name == "Shell_SecondaryTrayWnd" {
                return get_window_rect_safe(taskbar_hwnd);
            }
        }

        let mut abd = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: taskbar_hwnd,
            ..Default::default()
        };
        let result = SHAppBarMessage(ABM_GETTASKBARPOS, &mut abd);
        if result == 0 {
            return None;
        }
        Some(abd.rc)
    }
}

/// Get the bounding rectangle of a window
pub fn get_window_rect_safe(hwnd: HWND) -> Option<RECT> {
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_ok() {
            Some(rect)
        } else {
            None
        }
    }
}

/// Embed our window as a child of the taskbar
pub fn embed_in_taskbar(hwnd: HWND, taskbar_hwnd: HWND) {
    unsafe {
        // Preserve existing extended style, add tool window + no activate
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let _ = SetWindowLongW(
            hwnd,
            GWL_EXSTYLE,
            ex_style | WS_EX_TOOLWINDOW.0 as i32 | WS_EX_NOACTIVATE.0 as i32,
        );

        // Change from popup to child
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let new_style = (style & !WS_POPUP_STYLE) | WS_CHILD_STYLE | WS_CLIPSIBLINGS_STYLE;
        let _ = SetWindowLongW(hwnd, GWL_STYLE, new_style as i32);

        let _ = SetParent(hwnd, taskbar_hwnd);
    }
}

/// Move the window
pub fn move_window(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) {
    unsafe {
        let _ = MoveWindow(hwnd, x, y, w, h, true);
    }
}

/// Set up a WinEvent hook for tray location changes
pub fn set_tray_event_hook(
    thread_id: u32,
    callback: unsafe extern "system" fn(HWINEVENTHOOK, u32, HWND, i32, i32, u32, u32),
) -> Option<HWINEVENTHOOK> {
    unsafe {
        let hook = SetWinEventHook(
            EVENT_OBJECT_LOCATIONCHANGE,
            EVENT_OBJECT_LOCATIONCHANGE,
            None,
            Some(callback),
            0,
            thread_id,
            WINEVENT_OUTOFCONTEXT,
        );
        if hook.is_invalid() {
            None
        } else {
            Some(hook)
        }
    }
}

/// Get the thread ID that owns a window
pub fn get_window_thread_id(hwnd: HWND) -> u32 {
    unsafe { GetWindowThreadProcessId(hwnd, None) }
}

/// Unhook a WinEvent hook
pub fn unhook_win_event(hook: HWINEVENTHOOK) {
    unsafe {
        let _ = UnhookWinEvent(hook);
    }
}

/// Convert a Rust string to a null-terminated wide string
pub fn wide_str(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// COLORREF wrapper (RGB packed into u32)
pub fn colorref(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

/// Color helper
#[derive(Clone, Copy, Debug)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    #[allow(dead_code)]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 0xff }
    }

    /// Parse `#RGB`, `#RRGGBB`, or `#RRGGBBAA`. For the 3/6-digit forms, alpha
    /// defaults to `0xff` (fully opaque).
    pub fn from_hex(hex: &str) -> Self {
        let hex = hex.trim_start_matches('#');
        if hex.len() == 3 {
            let r = u8::from_str_radix(&hex[0..1], 16).unwrap_or(0);
            let g = u8::from_str_radix(&hex[1..2], 16).unwrap_or(0);
            let b = u8::from_str_radix(&hex[2..3], 16).unwrap_or(0);
            return Self {
                r: r * 17,
                g: g * 17,
                b: b * 17,
                a: 0xff,
            };
        }

        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
        let a = if hex.len() >= 8 {
            u8::from_str_radix(&hex[6..8], 16).unwrap_or(0xff)
        } else {
            0xff
        };
        Self { r, g, b, a }
    }

    /// Format as uppercase `#RRGGBBAA`.
    #[allow(dead_code)]
    pub fn to_hex(&self) -> String {
        format!("#{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
    }

    /// GDI COLORREF (`0x00BBGGRR`). Alpha is ignored, matching existing GDI usage.
    pub fn to_colorref(&self) -> u32 {
        colorref(self.r, self.g, self.b)
    }

    #[allow(dead_code)]
    pub fn with_alpha(&self, a: u8) -> Color {
        Color { a, ..*self }
    }

    /// Per-channel linear interpolation (including alpha) toward `other`. `t`
    /// is clamped to `[0, 1]`.
    #[allow(dead_code)]
    pub fn lerp(&self, other: &Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let lerp_channel = |a: u8, b: u8| -> u8 {
            (a as f32 + (b as f32 - a as f32) * t) as u8
        };
        Color {
            r: lerp_channel(self.r, other.r),
            g: lerp_channel(self.g, other.g),
            b: lerp_channel(self.b, other.b),
            a: lerp_channel(self.a, other.a),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_rrggbbaa() {
        let c = Color::from_hex("#1C2A3Bff");
        assert_eq!((c.r, c.g, c.b, c.a), (0x1C, 0x2A, 0x3B, 0xff));
    }
    #[test]
    fn parses_rrggbb_defaults_opaque() {
        let c = Color::from_hex("#1C2A3B");
        assert_eq!(c.a, 0xff);
    }
    #[test]
    fn to_colorref_is_bgr_without_alpha() {
        let c = Color { r: 0x12, g: 0x34, b: 0x56, a: 0x80 };
        assert_eq!(c.to_colorref(), 0x00_56_34_12);
    }
    #[test]
    fn roundtrips_hex() {
        assert_eq!(Color::from_hex("#aabbccdd").to_hex(), "#AABBCCDD");
    }
    #[test]
    fn lerp_midpoint() {
        let a = Color { r: 0, g: 0, b: 0, a: 0 };
        let b = Color { r: 100, g: 200, b: 50, a: 255 };
        let m = a.lerp(&b, 0.5);
        assert_eq!((m.r, m.g, m.b, m.a), (50, 100, 25, 127));
    }
}
