//! Settings window shell (Task 11) plus live preview (Task 12): a top-level, titled
//! `CcumConfig` window with a left section-nav list, a right content panel that renders a
//! WYSIWYG live preview of `draft` (via `window::paint_widget`, the same paint path the real
//! widget uses) with a running demo animation, and an (as yet empty) bottom button bar.
//! Section click switches the active section and repaints the highlight.
//!
//! Scope for this task is the shell plus preview: no controls wired to the draft settings
//! (Task 13; the preview currently occupies the whole content panel Task 11 left empty, and
//! Task 13 carves out real per-section control layout alongside it), no Save/Cancel/Reset
//! (Task 14), no menu/tray entry point (Task 15).
//!
//! # Threading / message-loop architecture
//!
//! This window is created on the *same* UI thread as the main widget window and is pumped
//! by the *same, single* `GetMessageW` loop in `window::run()` (that loop passes
//! `HWND::default()` for its `hWnd` filter, which means "any window owned by this thread's
//! queue" -- so a second top-level window on this thread is picked up automatically). This
//! module must therefore never call `GetMessageW`/`DispatchMessageW`/`TranslateMessage`
//! itself, and its `WM_DESTROY` handler must never call `PostQuitMessage` (that would tear
//! down the whole app's message loop, not just this window).

// The only public entry point, `open_config_window`, isn't called from anywhere yet -- Task
// 15 wires it up to a menu/tray item. Until then the whole module is unreachable from
// `main`, which would otherwise flag every item here as dead code (matching the
// `#[allow(dead_code)]` markers already used the same way throughout `controls.rs`).
#![allow(dead_code)]

use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::animation::{AnimationClock, AnimationFrame};
use crate::diagnose;
use crate::localization::{self, LanguageId};
use crate::native_interop::{self, Color, IDT_PREVIEW_ANIM};
use crate::settings::Settings;
use crate::theme;
use crate::window::{self, UsageData};

const CLASS_NAME: &str = "CcumConfig";
const WINDOW_TITLE: &str = "Settings";

/// Section labels. Plain hardcoded English placeholders for this task; localized strings
/// come in Task 18.
const SECTIONS: [&str; 6] = ["Appearance", "Font", "Size", "Animations", "Update", "Presets"];

// Layout constants, all in "baseline" (96 DPI) pixels; scaled at paint/hit-test time via
// `scale()` using this window's own live per-monitor DPI (see `effective_dpi`). This
// mirrors window.rs's `sc()` scaling formula, but is queried per-window rather than off the
// shared `CURRENT_DPI` global (which tracks the main widget window specifically) since the
// config window is a separate top-level window that can live on a different monitor.
const WINDOW_W: i32 = 640;
const WINDOW_H: i32 = 460;
const SIDEBAR_W: i32 = 170;
const SECTION_ITEM_H: i32 = 40;
const SECTION_TOP_PAD: i32 = 16;
const SECTION_TEXT_INSET: i32 = 20;
const BUTTON_BAR_H: i32 = 56;
const ACCENT_BAR_W: i32 = 3;

/// Per-window state for the config window. Only one instance can exist at a time (enforced
/// by `open_config_window`'s idempotency check), so this is held in a global rather than via
/// `GWLP_USERDATA` -- matching how `window.rs` holds its own single-instance `AppState`.
struct ConfigState {
    /// Working copy of settings edited by this window. Cloned from `current_settings()` when
    /// the window opens; the live preview (this task) renders it, Task 13 wires controls to
    /// edit it, Task 14 adds Save/Cancel.
    draft: Settings,
    active_section: usize,
    /// Drives the live preview's bar-fill/shimmer/glow/fade animation. Constructed fresh for
    /// this window and ticked by `IDT_PREVIEW_ANIM` -- entirely independent of the main
    /// widget's global `ANIM` clock in `window.rs` (see that module's `with_anim`).
    preview_clock: AnimationClock,
    /// Most recent frame produced by `preview_clock.tick`, consumed by `draw_preview` on the
    /// next `WM_PAINT`.
    preview_frame: AnimationFrame,
    /// Wall-clock timestamp of the previous preview tick, mirroring `window.rs`'s
    /// `LAST_ANIM_TICK`: `None` both before the first tick and whenever the preview timer has
    /// been stopped (idle), so the next tick after a gap assumes one frame's worth (16ms)
    /// rather than a huge `dt`.
    preview_last_tick: Option<Instant>,
}

/// Hardcoded placeholder usage numbers for the live preview. The settings window has no live
/// poll data (it isn't polling), so the preview always shows the same plausible Claude Code
/// session/weekly figures regardless of what the real widget is currently displaying -- just
/// enough to prove `paint_widget` renders end to end with `draft`'s appearance/geometry/
/// typography/animation settings applied.
fn demo_usage_data() -> UsageData {
    UsageData {
        session_pct: 62.0,
        session_text: "62% \u{00b7} 3h".to_string(),
        weekly_pct: 34.0,
        weekly_text: "34% \u{00b7} 4d".to_string(),
        codex_session_pct: 0.0,
        codex_session_text: String::new(),
        codex_weekly_pct: 0.0,
        codex_weekly_text: String::new(),
        antigravity_session_pct: 0.0,
        antigravity_session_text: String::new(),
        antigravity_weekly_pct: 0.0,
        antigravity_weekly_text: String::new(),
        show_claude_code: true,
        show_codex: false,
        show_antigravity: false,
    }
}

/// Fill-animation targets (0.0..=1.0) for `demo_usage_data`'s two visible bars, in
/// `[claude.session, claude.weekly]` order. This mirrors `window.rs`'s private
/// `ordered_bar_slots` contract (Claude Code's session/weekly pair always comes first, and
/// the demo dataset never shows Codex/Antigravity, so this pair is always the complete
/// order) -- `paint_widget` maps `AnimationFrame::fill_pcts` back onto bars using that same
/// index contract.
const DEMO_TARGETS: [f32; 2] = [0.62, 0.34];

static CONFIG_STATE: Mutex<Option<ConfigState>> = Mutex::new(None);

/// Wrapper to make HWND sendable across threads/storable in a `static`, mirroring
/// `window.rs`'s `SendHwnd` (HWND itself is a raw pointer newtype and isn't `Send`).
#[derive(Clone, Copy)]
struct SendHwnd(isize);

unsafe impl Send for SendHwnd {}

impl SendHwnd {
    fn from_hwnd(hwnd: HWND) -> Self {
        Self(hwnd.0 as isize)
    }
    fn to_hwnd(self) -> HWND {
        HWND(self.0 as *mut _)
    }
}

/// Tracks the single live config window instance, if any, so `open_config_window` can be
/// idempotent (focus instead of creating a second window).
static CONFIG_HWND: Mutex<Option<SendHwnd>> = Mutex::new(None);

static CLASS_REGISTERED: Once = std::sync::Once::new();

/// Open the settings window, or focus it if it's already open. Safe to call repeatedly.
pub fn open_config_window() {
    unsafe {
        let existing = *CONFIG_HWND.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(h) = existing {
            let hwnd = h.to_hwnd();
            if IsWindow(hwnd).as_bool() {
                if IsIconic(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                }
                let _ = SetForegroundWindow(hwnd);
                return;
            }
            // Stale handle (window was destroyed without going through our WM_DESTROY
            // cleanup, e.g. killed externally) -- clear it and fall through to recreate.
            *CONFIG_HWND.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }

        create_window();
    }
}

unsafe fn create_window() {
    let hinstance = match GetModuleHandleW(PCWSTR::null()) {
        Ok(h) => h,
        Err(error) => {
            diagnose::log_error("config_window: GetModuleHandleW failed", error);
            return;
        }
    };

    let class_wide = native_interop::wide_str(CLASS_NAME);

    CLASS_REGISTERED.call_once(|| {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(config_wnd_proc),
            hInstance: HINSTANCE(hinstance.0),
            hCursor: LoadCursorW(HINSTANCE::default(), IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH(std::ptr::null_mut()),
            lpszClassName: PCWSTR::from_raw(class_wide.as_ptr()),
            ..Default::default()
        };
        let atom = RegisterClassExW(&wc);
        if atom == 0 {
            diagnose::log("config_window: RegisterClassExW returned 0");
        }
    });

    // Seed the draft state *before* the window exists so a synchronous WM_PAINT arriving
    // during/just after CreateWindowExW always finds state populated.
    let draft = crate::window::current_settings();
    let mut preview_clock = AnimationClock::new(&draft.animation);
    // Seed at zero, then set the real demo targets, so the first few preview ticks animate
    // the bars growing in from empty -- a visible "this is live" cue when the settings
    // window opens, mirroring how the real widget looks on its first poll.
    preview_clock.set_targets(&[0.0; DEMO_TARGETS.len()]);
    preview_clock.set_targets(&DEMO_TARGETS);
    let usage_max = DEMO_TARGETS.iter().cloned().fold(0.0f32, f32::max);
    let (preview_frame, _) = preview_clock.tick(Duration::ZERO, usage_max);

    *CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner()) = Some(ConfigState {
        draft,
        active_section: 0,
        preview_clock,
        preview_frame,
        preview_last_tick: None,
    });

    // Size/position using the primary monitor's current DPI; refined per-monitor via
    // GetDpiForWindow once the window exists (and kept in sync via WM_DPICHANGED).
    let dpi = GetDpiForSystem();
    let width = scale(WINDOW_W, dpi);
    let height = scale(WINDOW_H, dpi);
    let screen_w = GetSystemMetrics(SM_CXSCREEN);
    let screen_h = GetSystemMetrics(SM_CYSCREEN);
    let x = ((screen_w - width) / 2).max(0);
    let y = ((screen_h - height) / 2).max(0);

    let title_wide = native_interop::wide_str(WINDOW_TITLE);
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        PCWSTR::from_raw(class_wide.as_ptr()),
        PCWSTR::from_raw(title_wide.as_ptr()),
        WS_OVERLAPPEDWINDOW,
        x,
        y,
        width,
        height,
        HWND::default(),
        HMENU::default(),
        hinstance,
        None,
    );

    let hwnd = match hwnd {
        Ok(h) => h,
        Err(error) => {
            diagnose::log_error("config_window: CreateWindowExW failed", error);
            *CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner()) = None;
            return;
        }
    };

    apply_dark_titlebar(hwnd, theme::is_dark_mode());

    *CONFIG_HWND.lock().unwrap_or_else(|e| e.into_inner()) = Some(SendHwnd::from_hwnd(hwnd));

    let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
    let _ = SetForegroundWindow(hwnd);
    let _ = UpdateWindow(hwnd);

    // Drives the live preview's animation while it has active work (fill grow-in, and
    // shimmer/glow if enabled in `draft.animation`); stopped by `tick_preview` once settled,
    // matching window.rs's IDT_ANIM idle-stop pattern.
    let _ = SetTimer(hwnd, IDT_PREVIEW_ANIM, 16, None);

    diagnose::log(format!("config window created hwnd={:?}", hwnd));
}

/// Applies (or removes) the native dark titlebar/frame via DWM. Best-effort: on older
/// Windows builds without `DWMWA_USE_IMMERSIVE_DARK_MODE` support this is a harmless no-op
/// (the call fails and is ignored), leaving a light titlebar over our dark client area.
unsafe fn apply_dark_titlebar(hwnd: HWND, dark: bool) {
    let value: BOOL = if dark { BOOL(1) } else { BOOL(0) };
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
        &value as *const BOOL as *const std::ffi::c_void,
        std::mem::size_of::<BOOL>() as u32,
    );
}

/// This window's own live per-monitor DPI (falls back to 96 = 100% if the query fails).
fn effective_dpi(hwnd: HWND) -> u32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi > 0 {
        dpi
    } else {
        96
    }
}

/// Scale a baseline (96 DPI) pixel value to `dpi`, mirroring window.rs's `sc()` formula.
fn scale(px: i32, dpi: u32) -> i32 {
    (px as f64 * dpi as f64 / 96.0).round() as i32
}

/// Client rects for each section-nav item, top to bottom, spanning the sidebar's width.
fn section_rects(dpi: u32) -> Vec<RECT> {
    let item_h = scale(SECTION_ITEM_H, dpi);
    let top_pad = scale(SECTION_TOP_PAD, dpi);
    let sidebar_w = scale(SIDEBAR_W, dpi);
    (0..SECTIONS.len())
        .map(|i| {
            let top = top_pad + i as i32 * item_h;
            RECT {
                left: 0,
                top,
                right: sidebar_w,
                bottom: top + item_h,
            }
        })
        .collect()
}

/// Index of the section-nav item containing client point `(x, y)`, if any.
fn section_at(x: i32, y: i32, dpi: u32) -> Option<usize> {
    section_rects(dpi)
        .into_iter()
        .position(|r| x >= r.left && x < r.right && y >= r.top && y < r.bottom)
}

fn fill_rect(hdc: HDC, rect: RECT, color: &Color) {
    unsafe {
        let brush = CreateSolidBrush(COLORREF(color.to_colorref()));
        FillRect(hdc, &rect, brush);
        let _ = DeleteObject(brush);
    }
}

unsafe fn paint(hdc: HDC, hwnd: HWND) {
    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);
    let client_w = client.right - client.left;
    let client_h = client.bottom - client.top;
    let dpi = effective_dpi(hwnd);
    let dark = theme::is_dark_mode();

    let active_section = {
        let guard = CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().map(|s| s.active_section).unwrap_or(0)
    };

    let bg = if dark {
        Color::new(0x1e, 0x1e, 0x1e)
    } else {
        Color::new(0xf5, 0xf5, 0xf5)
    };
    let sidebar_bg = if dark {
        Color::new(0x25, 0x25, 0x26)
    } else {
        Color::new(0xea, 0xea, 0xea)
    };
    let divider = if dark {
        Color::new(0x3a, 0x3a, 0x3a)
    } else {
        Color::new(0xd0, 0xd0, 0xd0)
    };
    let text_color = if dark {
        Color::new(0xf0, 0xf0, 0xf0)
    } else {
        Color::new(0x20, 0x20, 0x20)
    };
    let muted_text = if dark {
        Color::new(0xb0, 0xb0, 0xb0)
    } else {
        Color::new(0x50, 0x50, 0x50)
    };
    let active_tint = if dark {
        Color::new(0x33, 0x2a, 0x26)
    } else {
        Color::new(0xf3, 0xe2, 0xdb)
    };
    let accent = Color::new(0xd9, 0x77, 0x57);

    let sidebar_w = scale(SIDEBAR_W, dpi);
    let button_bar_h = scale(BUTTON_BAR_H, dpi);
    let body_bottom = client_h - button_bar_h;

    // Whole-window background (covers the content panel and button bar; sidebar painted
    // over next).
    fill_rect(hdc, client, &bg);

    // Sidebar.
    let sidebar_rect = RECT {
        left: 0,
        top: 0,
        right: sidebar_w,
        bottom: body_bottom,
    };
    fill_rect(hdc, sidebar_rect, &sidebar_bg);

    // Divider between sidebar and content panel.
    fill_rect(
        hdc,
        RECT {
            left: sidebar_w,
            top: 0,
            right: sidebar_w + 1,
            bottom: body_bottom,
        },
        &divider,
    );

    // Divider above the (currently empty) button bar.
    fill_rect(
        hdc,
        RECT {
            left: 0,
            top: body_bottom,
            right: client_w,
            bottom: body_bottom + 1,
        },
        &divider,
    );

    let _ = SetBkMode(hdc, TRANSPARENT);
    let font_name = native_interop::wide_str("Segoe UI");
    let font = CreateFontW(
        -scale(14, dpi),
        0,
        0,
        0,
        FW_MEDIUM.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_TT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
        PCWSTR::from_raw(font_name.as_ptr()),
    );
    let old_font = SelectObject(hdc, font);

    for (i, item_rect) in section_rects(dpi).into_iter().enumerate() {
        if i == active_section {
            fill_rect(hdc, item_rect, &active_tint);
            fill_rect(
                hdc,
                RECT {
                    left: 0,
                    top: item_rect.top,
                    right: scale(ACCENT_BAR_W, dpi),
                    bottom: item_rect.bottom,
                },
                &accent,
            );
            let _ = SetTextColor(hdc, COLORREF(text_color.to_colorref()));
        } else {
            let _ = SetTextColor(hdc, COLORREF(muted_text.to_colorref()));
        }

        let mut label_rect = RECT {
            left: item_rect.left + scale(SECTION_TEXT_INSET, dpi),
            top: item_rect.top,
            right: item_rect.right - scale(8, dpi),
            bottom: item_rect.bottom,
        };
        let mut label_wide: Vec<u16> = SECTIONS[i].encode_utf16().collect();
        let _ = DrawTextW(
            hdc,
            &mut label_wide,
            &mut label_rect,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
    }

    SelectObject(hdc, old_font);
    let _ = DeleteObject(font);

    // Live preview: the whole content panel (right of the sidebar divider, above the button
    // bar) is Task 11's still-empty area. This task just proves pixels flow end to end into
    // it; Task 13 carves out real per-section control layout alongside it.
    let content_rect = RECT {
        left: sidebar_w + 1,
        top: 0,
        right: client_w,
        bottom: body_bottom,
    };
    draw_preview(hdc, content_rect, dpi);
}

/// Render `draft` via the same `paint_widget` the real widget uses (with a small hardcoded
/// demo `UsageData`, since this window has no live poll data) into an off-screen bitmap sized
/// to the widget's natural (unclamped) dimensions, then blit it into the top-left corner of
/// `content_rect` with a small margin. If the panel is too small to fit the natural size, the
/// preview is clamped down to whatever room is available rather than overflowing into the
/// sidebar or button bar.
unsafe fn draw_preview(hdc: HDC, content_rect: RECT, dpi: u32) {
    let (draft, frame) = {
        let guard = CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_ref() {
            Some(s) => (s.draft.clone(), s.preview_frame.clone()),
            None => return,
        }
    };
    let is_dark = theme::is_dark_mode();
    let usage = demo_usage_data();
    let strings = localization::resolve_language(
        draft.language.as_deref().and_then(LanguageId::from_code),
    )
    .strings();

    // Natural widget size, computed with window.rs's own layout math (`sc`/
    // `active_model_count`/`total_widget_width_for`) so the preview's internal proportions
    // (which `paint_content` lays out entirely in terms of that same `sc()`) stay consistent
    // with the width/height we pass it.
    let active_models = window::active_model_count(
        usage.show_claude_code,
        usage.show_codex,
        usage.show_antigravity,
    );
    let natural_w = window::total_widget_width_for(active_models, &draft.geometry);
    let natural_h = window::sc(draft.geometry.height);

    let pad = scale(16, dpi);
    let avail_w = (content_rect.right - content_rect.left - pad * 2).max(1);
    let avail_h = (content_rect.bottom - content_rect.top - pad * 2).max(1);
    let w = natural_w.clamp(1, avail_w);
    let h = natural_h.clamp(1, avail_h);

    let mem_dc = CreateCompatibleDC(hdc);
    let bmp = CreateCompatibleBitmap(hdc, w, h);
    if bmp.is_invalid() {
        let _ = DeleteDC(mem_dc);
        return;
    }
    let old_bmp = SelectObject(mem_dc, bmp);

    window::paint_widget(mem_dc, w, h, &draft, &frame, &usage, is_dark, strings);

    let _ = BitBlt(
        hdc,
        content_rect.left + pad,
        content_rect.top + pad,
        w,
        h,
        mem_dc,
        0,
        0,
        SRCCOPY,
    );

    SelectObject(mem_dc, old_bmp);
    let _ = DeleteObject(bmp);
    let _ = DeleteDC(mem_dc);
}

/// Advance the preview's local animation clock by one tick and repaint. Mirrors
/// `window.rs`'s `render_layered`/`IDT_ANIM` dt-measurement and idle-stop pattern, but against
/// this window's own `ConfigState.preview_clock` -- entirely independent of the main widget's
/// global `ANIM` clock.
unsafe fn tick_preview(hwnd: HWND) {
    let active = {
        let mut guard = CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner());
        let Some(state) = guard.as_mut() else {
            return;
        };
        let now = Instant::now();
        let dt = match state.preview_last_tick {
            Some(prev) => now.duration_since(prev),
            None => Duration::from_millis(16),
        };
        state.preview_last_tick = Some(now);
        let usage_max = DEMO_TARGETS.iter().cloned().fold(0.0f32, f32::max);
        let (frame, active) = state.preview_clock.tick(dt, usage_max);
        state.preview_frame = frame;
        active
    };

    let _ = InvalidateRect(hwnd, None, false);

    // The clock has nothing left to animate (fill settled, no shimmer/glow pulsing, fade
    // complete): stop the timer so idle CPU returns to ~0% instead of repainting every 16ms
    // forever, and clear the last-tick timestamp so a future kick (e.g. Task 13 changing
    // `draft.animation`) starts clean.
    if !active {
        let _ = KillTimer(hwnd, IDT_PREVIEW_ANIM);
        if let Some(state) = CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            state.preview_last_tick = None;
        }
    }
}

unsafe extern "system" fn config_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            paint(hdc, hwnd);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            let dpi = effective_dpi(hwnd);
            if let Some(idx) = section_at(x, y, dpi) {
                let changed = {
                    let mut guard = CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner());
                    match guard.as_mut() {
                        Some(state) if state.active_section != idx => {
                            state.active_section = idx;
                            true
                        }
                        _ => false,
                    }
                };
                if changed {
                    let _ = InvalidateRect(hwnd, None, false);
                }
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            // Standard DPI-change handling: lParam points at the RECT the system suggests
            // for the new monitor's DPI; resize/reposition to it so content stays crisp
            // instead of just scaled-up.
            let suggested = &*(lparam.0 as *const RECT);
            let _ = SetWindowPos(
                hwnd,
                HWND::default(),
                suggested.left,
                suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            apply_dark_titlebar(hwnd, theme::is_dark_mode());
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == IDT_PREVIEW_ANIM {
                tick_preview(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            // Deliberately does NOT call PostQuitMessage: this window shares the process's
            // single `GetMessageW` loop (window::run) with the main widget window, so
            // posting WM_QUIT here would tear down the whole app, not just this window.
            let _ = KillTimer(hwnd, IDT_PREVIEW_ANIM);
            *CONFIG_HWND.lock().unwrap_or_else(|e| e.into_inner()) = None;
            *CONFIG_STATE.lock().unwrap_or_else(|e| e.into_inner()) = None;
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
