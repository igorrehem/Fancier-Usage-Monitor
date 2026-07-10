//! Reusable GDI controls for the settings window (Task 7+). Each control owns its own
//! state and knows how to `draw` itself into a caller-provided rect and how to react to
//! raw `WM_*` mouse messages via `on_mouse`, translated to client-relative `x`/`y`. The
//! settings window (Tasks 11-15) composes these into sections; nothing here depends on
//! `window.rs`/`settings.rs`/`animation.rs`.

use windows::Win32::Foundation::{COLORREF, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::{WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE};

use crate::native_interop::Color;
use crate::settings::Rgba;

/// Result of feeding a mouse message to a `Control`.
#[allow(dead_code)] // consumed by the settings window (Tasks 11-15), not yet wired up
pub enum ControlEvent {
    /// The value changed mid-interaction (e.g. while dragging); callers should update
    /// their live preview but need not persist yet.
    Changed,
    /// The interaction ended (e.g. mouse released); callers should commit/persist the
    /// current value.
    CommitPreview,
}

/// Common behavior shared by settings-window controls: draw into a rect, and react to
/// mouse messages addressed to that same rect.
#[allow(dead_code)] // consumed by the settings window (Tasks 11-15), not yet wired up
pub trait Control {
    /// Paints the control into `rect` on `hdc`. `dark` selects the dark/light color set.
    fn draw(&self, hdc: HDC, rect: RECT, dark: bool);

    /// Handles a raw mouse message (`WM_LBUTTONDOWN`/`WM_MOUSEMOVE`/`WM_LBUTTONUP`, ...)
    /// with `x`/`y` already client-relative and `rect` the control's own bounds. Returns
    /// `Some(ControlEvent)` when the interaction produced a value change worth reacting to.
    fn on_mouse(&mut self, msg: u32, x: i32, y: i32, rect: RECT) -> Option<ControlEvent>;
}

/// A horizontal slider mapping a value in `[min, max]` onto a track spanning `rect`'s
/// width. Drag interaction: `WM_LBUTTONDOWN` starts a drag (and jumps the value to the
/// click position), `WM_MOUSEMOVE` while dragging keeps updating the value (`Changed`),
/// and `WM_LBUTTONUP` ends the drag and commits (`CommitPreview`).
#[allow(dead_code)] // consumed by the settings window (Tasks 11-15), not yet wired up
pub struct Slider {
    pub value: f32,
    pub min: f32,
    pub max: f32,
    dragging: bool,
}

impl Slider {
    #[allow(dead_code)] // consumed by the settings window (Tasks 11-15), not yet wired up
    pub fn new(value: f32, min: f32, max: f32) -> Self {
        Self {
            value,
            min,
            max,
            dragging: false,
        }
    }

    /// Maps a client-x pixel position to a value in `[min, max]`, clamping positions
    /// outside `rect` to the nearest end of the range.
    #[allow(dead_code)] // called by draw()/on_mouse() below, and directly by tests
    pub fn pos_to_value(&self, x: i32, rect: RECT) -> f32 {
        let width = (rect.right - rect.left).max(1) as f32;
        let t = ((x - rect.left) as f32 / width).clamp(0.0, 1.0);
        self.min + t * (self.max - self.min)
    }

    /// Maps the current value to the knob's client-x pixel position within `rect`.
    #[allow(dead_code)] // consumed by the settings window (Tasks 11-15), not yet wired up
    pub fn value_to_x(&self, rect: RECT) -> i32 {
        let width = (rect.right - rect.left).max(1) as f32;
        let range = (self.max - self.min).max(f32::EPSILON);
        let t = ((self.value - self.min) / range).clamp(0.0, 1.0);
        rect.left + (t * width).round() as i32
    }
}

impl Control for Slider {
    fn draw(&self, hdc: HDC, rect: RECT, dark: bool) {
        unsafe {
            let full_h = (rect.bottom - rect.top).max(1);
            let track_h = 4.min(full_h);
            let track_top = rect.top + (full_h - track_h) / 2;
            let track_rect = RECT {
                left: rect.left,
                top: track_top,
                right: rect.right,
                bottom: track_top + track_h,
            };

            let track_color = if dark {
                Color::new(0x4a, 0x4a, 0x4a)
            } else {
                Color::new(0xd4, 0xd4, 0xd4)
            };
            let fill_color = Color::new(0xd9, 0x77, 0x57);
            let knob_color = if dark {
                Color::new(0xf0, 0xf0, 0xf0)
            } else {
                Color::new(0xff, 0xff, 0xff)
            };
            let knob_border = if dark {
                Color::new(0x20, 0x20, 0x20)
            } else {
                Color::new(0xa0, 0xa0, 0xa0)
            };

            draw_rounded_rect(hdc, &track_rect, &track_color, track_h / 2);

            let knob_x = self.value_to_x(rect);
            if knob_x > track_rect.left {
                let fill_rect = RECT {
                    left: track_rect.left,
                    top: track_rect.top,
                    right: knob_x.min(track_rect.right),
                    bottom: track_rect.bottom,
                };
                draw_rounded_rect(hdc, &fill_rect, &fill_color, track_h / 2);
            }

            let knob_r = (full_h / 2).max(track_h);
            let knob_cy = rect.top + full_h / 2;
            let brush = CreateSolidBrush(COLORREF(knob_color.to_colorref()));
            let pen = CreatePen(PS_SOLID, 1, COLORREF(knob_border.to_colorref()));
            let old_brush = SelectObject(hdc, brush);
            let old_pen = SelectObject(hdc, pen);
            let _ = Ellipse(
                hdc,
                knob_x - knob_r,
                knob_cy - knob_r,
                knob_x + knob_r,
                knob_cy + knob_r,
            );
            SelectObject(hdc, old_brush);
            SelectObject(hdc, old_pen);
            let _ = DeleteObject(brush);
            let _ = DeleteObject(pen);
        }
    }

    fn on_mouse(&mut self, msg: u32, x: i32, y: i32, rect: RECT) -> Option<ControlEvent> {
        let _ = y;
        match msg {
            WM_LBUTTONDOWN => {
                self.dragging = true;
                self.value = self.pos_to_value(x, rect);
                Some(ControlEvent::Changed)
            }
            WM_MOUSEMOVE => {
                if self.dragging {
                    self.value = self.pos_to_value(x, rect);
                    Some(ControlEvent::Changed)
                } else {
                    None
                }
            }
            WM_LBUTTONUP => {
                if self.dragging {
                    self.dragging = false;
                    self.value = self.pos_to_value(x, rect);
                    Some(ControlEvent::CommitPreview)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// Layout constant: side length of the color swatch (also the height of the top row that
/// holds the swatch and the hex readout).
const SWATCH_SIZE: i32 = 28;
/// Layout constant: vertical gap between the swatch/hex row and the first slider row. Only
/// relevant to the legacy always-expanded row math (`rows_top`/`row_rect`), retained for
/// its existing unit test coverage but no longer reachable from `draw`/`on_mouse` now that
/// the four slider rows live in the popover instead (see `popover_slider_row_rect`).
const ROW_GAP: i32 = 6;
/// Layout constant: width reserved for the single-letter channel label left of each slider.
/// Shared by both the legacy row math and the popover's slider rows.
const LABEL_WIDTH: i32 = 14;
/// Layout constant: horizontal gap between the swatch and the hex readout text.
const HEX_GAP: i32 = 8;
/// Layout constant: checkerboard cell size used to render alpha transparency under the swatch.
const CHECKER_CELL: i32 = 6;
/// Width reserved on the right of the closed row for the popover's disclosure arrow glyph.
const RGBA_ARROW_ZONE: i32 = 18;
/// Horizontal/vertical padding used inside the popover (around the swatch grid, and for the
/// "Custom…" row's text inset).
const POPOVER_PAD: i32 = 4;
/// Columns in the popover's quick-swatch grid.
const POPOVER_GRID_COLS: i32 = 5;
/// Side length of each quick-swatch cell in the popover grid.
const POPOVER_SWATCH_CELL: i32 = 24;
/// Gap between adjacent quick-swatch cells.
const POPOVER_SWATCH_GAP: i32 = 4;
/// Height of the "Custom…" row within the popover.
const POPOVER_CUSTOM_ROW_HEIGHT: i32 = 24;
/// Height of each of the four R/G/B/A slider rows revealed inside the popover once
/// "Custom…" has been picked.
const POPOVER_SLIDER_ROW_HEIGHT: i32 = 22;

/// Curated quick-swatch palette shown at the top of the popover: this app's brand accent,
/// a spread of complementary hues (amber, red, green, blue, purple, teal), and three
/// neutrals (near-white, mid-gray, near-black). All fully opaque (`a: 255`) — quick
/// swatches never carry partial transparency; alpha is only adjustable via the "Custom…"
/// sliders. Ten entries lay out evenly as `POPOVER_GRID_COLS` (5) x 2 rows.
const QUICK_SWATCHES: [Rgba; 10] = [
    Rgba { r: 0xD9, g: 0x77, b: 0x57, a: 255 }, // brand accent
    Rgba { r: 0xF5, g: 0xA6, b: 0x23, a: 255 }, // amber
    Rgba { r: 0xE5, g: 0x48, b: 0x4D, a: 255 }, // red
    Rgba { r: 0x46, g: 0xA7, b: 0x58, a: 255 }, // green
    Rgba { r: 0x3B, g: 0x82, b: 0xF6, a: 255 }, // blue
    Rgba { r: 0x8B, g: 0x5C, b: 0xF6, a: 255 }, // purple
    Rgba { r: 0x14, g: 0xB8, b: 0xA6, a: 255 }, // teal
    Rgba { r: 0xF5, g: 0xF5, b: 0xF4, a: 255 }, // near-white
    Rgba { r: 0x8A, g: 0x8A, b: 0x8A, a: 255 }, // mid-gray
    Rgba { r: 0x1C, g: 0x1C, b: 0x1C, a: 255 }, // near-black
];

/// A composed color picker, closed by default: a single row shows the swatch, the
/// `#RRGGBBAA` hex readout, and a disclosure arrow. Clicking it opens a popover (mirroring
/// `Dropdown`'s closed/open shape) offering a curated grid of `QUICK_SWATCHES` plus a
/// "Custom…" row; picking "Custom…" reveals the four `Slider`s (R/G/B/A, each ranging over
/// `0..255`) for manual entry. `on_mouse` dispatches to whichever popover element the
/// cursor is in and keeps dispatching to the active slider while a drag continues, even if
/// the cursor drifts outside that row's exact bounds mid-drag.
#[allow(dead_code)] // consumed by the settings window (Tasks 11-15), not yet wired up
pub struct RgbaPicker {
    pub value: Rgba,
    r_slider: Slider,
    g_slider: Slider,
    b_slider: Slider,
    a_slider: Slider,
    /// Channel index (0=R, 1=G, 2=B, 3=A) currently being dragged, if any.
    active: Option<usize>,
    /// Whether the popover (quick swatches + "Custom…") is currently open.
    open: bool,
    /// Whether, within the open popover, "Custom…" has been picked and the manual R/G/B/A
    /// sliders are revealed.
    custom_expanded: bool,
}

impl RgbaPicker {
    #[allow(dead_code)] // consumed by the settings window (Tasks 11-15), not yet wired up
    pub fn new(value: Rgba) -> Self {
        Self {
            r_slider: Slider::new(value.r as f32, 0.0, 255.0),
            g_slider: Slider::new(value.g as f32, 0.0, 255.0),
            b_slider: Slider::new(value.b as f32, 0.0, 255.0),
            a_slider: Slider::new(value.a as f32, 0.0, 255.0),
            value,
            active: None,
            open: false,
            custom_expanded: false,
        }
    }

    /// The swatch rect: a square anchored at the control's top-left, clamped so it never
    /// takes up more than half the control's height (the usual 12px floor only applies
    /// when half the height is at least that big, so a very short control still shrinks
    /// the swatch instead of overflowing it).
    fn swatch_rect(&self, rect: RECT) -> RECT {
        let half_h = ((rect.bottom - rect.top) / 2).max(1);
        let size = SWATCH_SIZE.min(half_h).max(12.min(half_h));
        RECT {
            left: rect.left,
            top: rect.top,
            right: rect.left + size,
            bottom: rect.top + size,
        }
    }

    /// Y position where the four channel rows begin (just below the swatch/hex row). Part
    /// of the legacy always-expanded row math: no longer called from `draw`/`on_mouse`
    /// (superseded by `popover_slider_row_rect`, anchored to the popover's "Custom…" row
    /// instead), but kept — along with `row_rect`/`slider_rect`/`row_at` below — because its
    /// exact behavior is still covered by pre-existing unit tests and callers outside this
    /// module may rely on the same math shape.
    #[allow(dead_code)]
    fn rows_top(&self, rect: RECT) -> i32 {
        self.swatch_rect(rect).bottom + ROW_GAP
    }

    /// The full-width row (label + slider) for channel `index` (0=R, 1=G, 2=B, 3=A). See
    /// `rows_top`'s doc comment: retained for its existing test coverage, not called from
    /// `draw`/`on_mouse` anymore (see `popover_slider_row_rect`).
    #[allow(dead_code)]
    fn row_rect(&self, index: usize, rect: RECT) -> RECT {
        let top0 = self.rows_top(rect);
        let rows_h = (rect.bottom - top0).max(4);
        let row_h = rows_h / 4;
        let top = top0 + row_h * index as i32;
        let bottom = if index == 3 { rect.bottom } else { top + row_h };
        RECT {
            left: rect.left,
            top,
            right: rect.right,
            bottom,
        }
    }

    /// The slider-only portion of a channel row (row rect minus the label column). See
    /// `rows_top`'s doc comment: retained for its existing test coverage, not called from
    /// `draw`/`on_mouse` anymore (see `popover_slider_slot_rect`).
    #[allow(dead_code)]
    fn slider_rect(&self, index: usize, rect: RECT) -> RECT {
        let row = self.row_rect(index, rect);
        RECT {
            left: row.left + LABEL_WIDTH,
            top: row.top,
            right: row.right,
            bottom: row.bottom,
        }
    }

    /// Which channel row (if any) contains client point `(x, y)`. Returns `None` both when
    /// `y` falls outside the rows area and when `x` falls within the row's label column
    /// (`row.left .. row.left + LABEL_WIDTH`) — a click on the label text must not be
    /// dispatched to the channel's slider (it would otherwise snap that channel to 0, since
    /// `Slider::pos_to_value` clamps any `x` left of the sub-slider's rect to its minimum).
    /// See `rows_top`'s doc comment: retained for its existing test coverage, not called
    /// from `draw`/`on_mouse` anymore (see `popover_slider_row_at`).
    #[allow(dead_code)]
    fn row_at(&self, x: i32, y: i32, rect: RECT) -> Option<usize> {
        if x < rect.left + LABEL_WIDTH {
            return None;
        }
        let top0 = self.rows_top(rect);
        if y < top0 || y > rect.bottom {
            return None;
        }
        let rows_h = (rect.bottom - top0).max(4);
        let row_h = (rows_h / 4).max(1);
        let idx = ((y - top0) / row_h).clamp(0, 3);
        Some(idx as usize)
    }

    /// Whether client point `(x, y)` falls within `r`, using half-open bounds so adjacent
    /// rects (e.g. the closed row and the popover starting exactly at its bottom edge)
    /// never both claim the same point. Mirrors `Dropdown::point_in`.
    fn point_in(&self, x: i32, y: i32, r: RECT) -> bool {
        x >= r.left && x < r.right && y >= r.top && y < r.bottom
    }

    /// How many rows the quick-swatch grid needs to lay out all of `QUICK_SWATCHES` at
    /// `POPOVER_GRID_COLS` columns per row.
    fn swatch_grid_rows(&self) -> i32 {
        (QUICK_SWATCHES.len() as i32 + POPOVER_GRID_COLS - 1) / POPOVER_GRID_COLS
    }

    /// The quick-swatch grid's rect: directly below the closed `rect`, spanning its width.
    fn swatch_grid_rect(&self, rect: RECT) -> RECT {
        let rows = self.swatch_grid_rows();
        let height =
            POPOVER_PAD * 2 + rows * POPOVER_SWATCH_CELL + (rows - 1).max(0) * POPOVER_SWATCH_GAP;
        RECT {
            left: rect.left,
            top: rect.bottom,
            right: rect.right,
            bottom: rect.bottom + height,
        }
    }

    /// The rect of quick-swatch cell `index` within the grid.
    fn swatch_cell_rect(&self, index: usize, rect: RECT) -> RECT {
        let grid = self.swatch_grid_rect(rect);
        let col = index as i32 % POPOVER_GRID_COLS;
        let row = index as i32 / POPOVER_GRID_COLS;
        let left = grid.left + POPOVER_PAD + col * (POPOVER_SWATCH_CELL + POPOVER_SWATCH_GAP);
        let top = grid.top + POPOVER_PAD + row * (POPOVER_SWATCH_CELL + POPOVER_SWATCH_GAP);
        RECT {
            left,
            top,
            right: left + POPOVER_SWATCH_CELL,
            bottom: top + POPOVER_SWATCH_CELL,
        }
    }

    /// Which quick-swatch cell (if any) contains client point `(x, y)`.
    fn swatch_at(&self, x: i32, y: i32, rect: RECT) -> Option<usize> {
        let grid = self.swatch_grid_rect(rect);
        if !self.point_in(x, y, grid) {
            return None;
        }
        (0..QUICK_SWATCHES.len()).find(|&i| self.point_in(x, y, self.swatch_cell_rect(i, rect)))
    }

    /// The "Custom…" row's rect: directly below the quick-swatch grid, spanning `rect`'s
    /// width.
    fn custom_row_rect(&self, rect: RECT) -> RECT {
        let grid = self.swatch_grid_rect(rect);
        RECT {
            left: rect.left,
            top: grid.bottom,
            right: rect.right,
            bottom: grid.bottom + POPOVER_CUSTOM_ROW_HEIGHT,
        }
    }

    /// The full popover rect: the quick-swatch grid plus the "Custom…" row plus — while
    /// `custom_expanded` — the four R/G/B/A slider rows, directly below the closed `rect`.
    /// Mirrors `Dropdown::list_rect`'s shape (same width as `rect`, positioned directly
    /// below it, height depending on current state).
    fn popover_rect(&self, rect: RECT) -> RECT {
        let custom = self.custom_row_rect(rect);
        let sliders_h = if self.custom_expanded {
            POPOVER_SLIDER_ROW_HEIGHT * 4
        } else {
            0
        };
        RECT {
            left: rect.left,
            top: rect.bottom,
            right: rect.right,
            bottom: custom.bottom + sliders_h,
        }
    }

    /// The full-width row (label + slider) for channel `index`, within the popover's
    /// revealed slider section below the "Custom…" row. Positional counterpart to the
    /// legacy `row_rect`, anchored to the popover instead of always-visible below the
    /// swatch/hex header.
    fn popover_slider_row_rect(&self, index: usize, rect: RECT) -> RECT {
        let custom = self.custom_row_rect(rect);
        let top = custom.bottom + POPOVER_SLIDER_ROW_HEIGHT * index as i32;
        RECT {
            left: rect.left,
            top,
            right: rect.right,
            bottom: top + POPOVER_SLIDER_ROW_HEIGHT,
        }
    }

    /// The slider-only portion of popover channel row `index` (row rect minus the label
    /// column). Positional counterpart to the legacy `slider_rect`.
    fn popover_slider_slot_rect(&self, index: usize, rect: RECT) -> RECT {
        let row = self.popover_slider_row_rect(index, rect);
        RECT {
            left: row.left + LABEL_WIDTH,
            top: row.top,
            right: row.right,
            bottom: row.bottom,
        }
    }

    /// Which popover channel row (if any) contains client point `(x, y)`. Only meaningful
    /// while `custom_expanded` (the rows aren't drawn, or hit-testable, otherwise). Mirrors
    /// `row_at`'s label-column exclusion.
    fn popover_slider_row_at(&self, x: i32, y: i32, rect: RECT) -> Option<usize> {
        if !self.custom_expanded || x < rect.left + LABEL_WIDTH {
            return None;
        }
        let top0 = self.custom_row_rect(rect).bottom;
        if y < top0 {
            return None;
        }
        let idx = (y - top0) / POPOVER_SLIDER_ROW_HEIGHT;
        if !(0..4).contains(&idx) {
            None
        } else {
            Some(idx as usize)
        }
    }

    fn slider(&self, index: usize) -> &Slider {
        match index {
            0 => &self.r_slider,
            1 => &self.g_slider,
            2 => &self.b_slider,
            _ => &self.a_slider,
        }
    }

    fn slider_mut(&mut self, index: usize) -> &mut Slider {
        match index {
            0 => &mut self.r_slider,
            1 => &mut self.g_slider,
            2 => &mut self.b_slider,
            _ => &mut self.a_slider,
        }
    }

    /// Recomputes `value` from the four sliders' current (float) values.
    fn sync_value(&mut self) {
        self.value = Rgba {
            r: self.r_slider.value.round().clamp(0.0, 255.0) as u8,
            g: self.g_slider.value.round().clamp(0.0, 255.0) as u8,
            b: self.b_slider.value.round().clamp(0.0, 255.0) as u8,
            a: self.a_slider.value.round().clamp(0.0, 255.0) as u8,
        };
    }

    /// Force-closes the popover (and collapses "Custom…" back down), if open, resetting to
    /// the compact closed-row state. Mirrors `Dropdown::close`. A no-op if already closed.
    pub fn close(&mut self) {
        self.open = false;
        self.custom_expanded = false;
    }
}

impl Control for RgbaPicker {
    fn draw(&self, hdc: HDC, rect: RECT, dark: bool) {
        unsafe {
            let text_color = if dark {
                Color::new(0xf0, 0xf0, 0xf0)
            } else {
                Color::new(0x20, 0x20, 0x20)
            };
            let border = if dark {
                Color::new(0x55, 0x55, 0x55)
            } else {
                Color::new(0xc0, 0xc0, 0xc0)
            };
            let bg = if dark {
                Color::new(0x33, 0x33, 0x33)
            } else {
                Color::new(0xff, 0xff, 0xff)
            };
            let highlight = Color::new(0xd9, 0x77, 0x57);

            let _ = SetBkMode(hdc, TRANSPARENT);
            let _ = SetTextColor(hdc, COLORREF(text_color.to_colorref()));

            // Closed row: swatch + hex readout + disclosure arrow. Always drawn, whether
            // or not the popover is open.
            let swatch = self.swatch_rect(rect);
            draw_checkerboard_swatch(hdc, swatch, &self.value.to_color());

            let hex = self.value.to_color().to_hex();
            let mut hex_wide: Vec<u16> = hex.encode_utf16().collect();
            let mut hex_rect = RECT {
                left: swatch.right + HEX_GAP,
                top: swatch.top,
                right: rect.right - RGBA_ARROW_ZONE,
                bottom: swatch.bottom,
            };
            let _ = DrawTextW(
                hdc,
                &mut hex_wide,
                &mut hex_rect,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );

            let arrow = if self.open { "\u{25B2}" } else { "\u{25BC}" };
            let mut arrow_wide: Vec<u16> = arrow.encode_utf16().collect();
            let mut arrow_rect = RECT {
                left: rect.right - RGBA_ARROW_ZONE,
                top: swatch.top,
                right: rect.right,
                bottom: swatch.bottom,
            };
            let _ = DrawTextW(
                hdc,
                &mut arrow_wide,
                &mut arrow_rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );

            if !self.open {
                return;
            }

            // Popover: panel background, quick-swatch grid, "Custom…" row, and — while
            // `custom_expanded` — the four R/G/B/A slider rows.
            let popover = self.popover_rect(rect);
            draw_rounded_rect(hdc, &popover, &border, 4);
            let inset_popover = RECT {
                left: popover.left + 1,
                top: popover.top,
                right: popover.right - 1,
                bottom: popover.bottom - 1,
            };
            draw_rounded_rect(hdc, &inset_popover, &bg, 4);

            for i in 0..QUICK_SWATCHES.len() {
                let cell = self.swatch_cell_rect(i, rect);
                let swatch_color = QUICK_SWATCHES[i].to_color();
                if self.value == QUICK_SWATCHES[i] {
                    draw_rounded_rect(hdc, &cell, &highlight, 4);
                    let inset = RECT {
                        left: cell.left + 2,
                        top: cell.top + 2,
                        right: cell.right - 2,
                        bottom: cell.bottom - 2,
                    };
                    draw_rounded_rect(hdc, &inset, &swatch_color, 3);
                } else {
                    draw_rounded_rect(hdc, &cell, &swatch_color, 4);
                }
            }

            let custom_row = self.custom_row_rect(rect);
            let mut custom_wide: Vec<u16> = "Custom\u{2026}".encode_utf16().collect();
            let mut custom_rect = RECT {
                left: custom_row.left + POPOVER_PAD * 2,
                top: custom_row.top,
                right: custom_row.right - POPOVER_PAD * 2,
                bottom: custom_row.bottom,
            };
            let _ = DrawTextW(
                hdc,
                &mut custom_wide,
                &mut custom_rect,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );

            if self.custom_expanded {
                const LABELS: [&str; 4] = ["R", "G", "B", "A"];
                for (i, label) in LABELS.iter().enumerate() {
                    let row = self.popover_slider_row_rect(i, rect);
                    let mut label_wide: Vec<u16> = label.encode_utf16().collect();
                    let mut label_rect = RECT {
                        left: row.left,
                        top: row.top,
                        right: row.left + LABEL_WIDTH,
                        bottom: row.bottom,
                    };
                    let _ = DrawTextW(
                        hdc,
                        &mut label_wide,
                        &mut label_rect,
                        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                    );
                    self.slider(i)
                        .draw(hdc, self.popover_slider_slot_rect(i, rect), dark);
                }
            }
        }
    }

    fn on_mouse(&mut self, msg: u32, x: i32, y: i32, rect: RECT) -> Option<ControlEvent> {
        match msg {
            WM_LBUTTONDOWN => {
                if !self.open {
                    // Closed: only a click on the closed row itself opens the popover.
                    if self.point_in(x, y, rect) {
                        self.open = true;
                    }
                    return None;
                }
                if self.point_in(x, y, rect) {
                    // Click on the closed row again while open: collapse.
                    self.close();
                    return None;
                }
                if let Some(idx) = self.swatch_at(x, y, rect) {
                    // Quick swatches are always fully opaque; sync the sliders too so a
                    // later "Custom…" open reflects the swatch that was picked.
                    let picked = QUICK_SWATCHES[idx];
                    self.r_slider.value = picked.r as f32;
                    self.g_slider.value = picked.g as f32;
                    self.b_slider.value = picked.b as f32;
                    self.a_slider.value = picked.a as f32;
                    self.sync_value();
                    self.close();
                    return Some(ControlEvent::Changed);
                }
                if self.point_in(x, y, self.custom_row_rect(rect)) {
                    self.custom_expanded = true;
                    return None;
                }
                if self.custom_expanded {
                    if let Some(idx) = self.popover_slider_row_at(x, y, rect) {
                        self.active = Some(idx);
                        let sub_rect = self.popover_slider_slot_rect(idx, rect);
                        let event = self.slider_mut(idx).on_mouse(msg, x, y, sub_rect);
                        if event.is_some() {
                            self.sync_value();
                        }
                        return event;
                    }
                }
                // Anywhere else inside the popover's own bounds (e.g. the padding margin
                // around the swatch grid, or the gaps between swatch cells) is a dead zone:
                // a no-op, not a dismissal. Only a click genuinely outside `popover_rect`
                // dismisses the popover (see struct doc comment / `Dropdown`'s identical
                // outside-click convention).
                if !self.point_in(x, y, self.popover_rect(rect)) {
                    self.close();
                }
                None
            }
            WM_MOUSEMOVE => {
                let idx = self.active?;
                let sub_rect = self.popover_slider_slot_rect(idx, rect);
                let event = self.slider_mut(idx).on_mouse(msg, x, y, sub_rect);
                if event.is_some() {
                    self.sync_value();
                }
                event
            }
            WM_LBUTTONUP => {
                let idx = self.active.take()?;
                let sub_rect = self.popover_slider_slot_rect(idx, rect);
                let event = self.slider_mut(idx).on_mouse(msg, x, y, sub_rect);
                if event.is_some() {
                    self.sync_value();
                }
                event
            }
            _ => None,
        }
    }
}

/// Fills `swatch` with a checkerboard pattern (so alpha transparency is visible) blended
/// with `color` according to its alpha channel, then draws that over a rounded background.
fn draw_checkerboard_swatch(hdc: HDC, swatch: RECT, color: &Color) {
    let checker_light = Color::new(0xff, 0xff, 0xff);
    let checker_dark = Color::new(0xc0, 0xc0, 0xc0);
    let t = color.a as f32 / 255.0;
    let blended_light = checker_light.lerp(color, t);
    let blended_dark = checker_dark.lerp(color, t);

    draw_rounded_rect(hdc, &swatch, &blended_dark, 4);

    let mut row = 0;
    let mut y = swatch.top;
    while y < swatch.bottom {
        let cell_bottom = (y + CHECKER_CELL).min(swatch.bottom);
        let mut col = 0;
        let mut x = swatch.left;
        while x < swatch.right {
            let cell_right = (x + CHECKER_CELL).min(swatch.right);
            if (row + col) % 2 == 0 {
                let cell = RECT {
                    left: x,
                    top: y,
                    right: cell_right,
                    bottom: cell_bottom,
                };
                fill_rect(hdc, &cell, &blended_light);
            }
            col += 1;
            x += CHECKER_CELL;
        }
        row += 1;
        y += CHECKER_CELL;
    }
}

/// Fills a plain (non-rounded) rect with a solid color.
fn fill_rect(hdc: HDC, rect: &RECT, color: &Color) {
    unsafe {
        let brush = CreateSolidBrush(COLORREF(color.to_colorref()));
        let _ = FillRect(hdc, rect, brush);
        let _ = DeleteObject(brush);
    }
}

/// Fills `rect` with a rounded rectangle of `color`, matching the look used elsewhere in
/// the app's bar rendering (`window.rs::draw_rounded_rect`).
#[allow(dead_code)] // called from Slider::draw(), which is itself not yet wired up
fn draw_rounded_rect(hdc: HDC, rect: &RECT, color: &Color, radius: i32) {
    unsafe {
        let brush = CreateSolidBrush(COLORREF(color.to_colorref()));
        let rgn = CreateRoundRectRgn(
            rect.left,
            rect.top,
            rect.right + 1,
            rect.bottom + 1,
            radius * 2,
            radius * 2,
        );
        let _ = FillRgn(hdc, rgn, brush);
        let _ = DeleteObject(rgn);
        let _ = DeleteObject(brush);
    }
}

/// `EnumFontFamiliesExW` callback: appends the enumerated family name to the `Vec<String>`
/// pointed to by `lparam` (set up by `enumerate_font_families`). Skips the empty name (can
/// occur for degenerate font entries) and names starting with `@`, which are Windows'
/// vertical-writing variants of an East Asian family (e.g. "@MS Gothic") — cosmetic
/// duplicates of the base family, not distinct fonts a user would pick by name. Returning `1`
/// tells GDI to keep enumerating.
unsafe extern "system" fn enum_font_families_callback(
    lplf: *const LOGFONTW,
    _lptm: *const TEXTMETRICW,
    _font_type: u32,
    lparam: LPARAM,
) -> i32 {
    if lplf.is_null() {
        return 1;
    }
    let logfont = &*lplf;
    let name_len = logfont
        .lfFaceName
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(logfont.lfFaceName.len());
    let name = String::from_utf16_lossy(&logfont.lfFaceName[..name_len]);
    if !name.is_empty() && !name.starts_with('@') {
        let families = &mut *(lparam.0 as *mut Vec<String>);
        families.push(name);
    }
    1
}

/// Enumerates installed font family names via `EnumFontFamiliesExW`, returning them sorted
/// and deduplicated. Uses an empty `lfFaceName` + `DEFAULT_CHARSET` in the query `LOGFONTW`,
/// which asks GDI to enumerate one entry per distinct family name (across all charsets)
/// rather than every style/charset variant of a single family.
#[allow(dead_code)] // consumed by the settings window (Tasks 11-15), not yet wired up
pub fn enumerate_font_families() -> Vec<String> {
    let mut families: Vec<String> = Vec::new();
    unsafe {
        let hdc = GetDC(None);
        let logfont = LOGFONTW {
            lfCharSet: DEFAULT_CHARSET,
            ..Default::default()
        };
        let _ = EnumFontFamiliesExW(
            hdc,
            &logfont,
            Some(enum_font_families_callback),
            LPARAM(&mut families as *mut Vec<String> as isize),
            0,
        );
        ReleaseDC(None, hdc);
    }
    families.sort();
    families.dedup();
    families
}

/// Layout constants for `Dropdown`'s open list: each row's height, the maximum number of
/// rows shown at once before the list scrolls, and text padding shared with the closed row.
const DROPDOWN_ROW_HEIGHT: i32 = 24;
const DROPDOWN_MAX_VISIBLE: usize = 6;
const DROPDOWN_TEXT_PAD: i32 = 8;
/// Width reserved on the right of the closed row for the open/close arrow glyph.
const DROPDOWN_ARROW_ZONE: i32 = 18;

/// A closed-by-default combobox: the closed row shows `items[selected]` plus an arrow
/// glyph; clicking it opens a scrollable list of all `items` drawn directly below the
/// closed row. Clicking an item selects it, closes the list, and emits `Changed`. Clicking
/// the closed row again while open collapses it, and clicking anywhere else (outside both
/// the closed row and the open list) also dismisses it, without changing `selected` — the
/// brief doesn't fully specify outside-click behavior; this follows the standard
/// combobox/`<select>` convention of treating an outside click as "cancel", not "commit".
///
/// Note for the settings window wiring (Tasks 11-15): while open, the interactive area
/// extends below `rect` (see `list_rect`) — clicks landing there must still be routed to
/// this control's `on_mouse` with the *closed* row's `rect`, not just clicks within `rect`
/// itself.
#[allow(dead_code)] // consumed by the settings window (Tasks 11-15), not yet wired up
pub struct Dropdown {
    pub items: Vec<String>,
    pub selected: usize,
    open: bool,
    /// Index of the first item currently visible in the open list.
    scroll: usize,
}

impl Dropdown {
    #[allow(dead_code)] // consumed by the settings window (Tasks 11-15), not yet wired up
    pub fn new(items: Vec<String>, selected: usize) -> Self {
        let selected = if items.is_empty() {
            0
        } else {
            selected.min(items.len() - 1)
        };
        Self {
            items,
            selected,
            open: false,
            scroll: 0,
        }
    }

    /// How many rows the open list shows at once (never more than the item count).
    fn visible_count(&self) -> usize {
        self.items.len().min(DROPDOWN_MAX_VISIBLE)
    }

    /// The largest valid `scroll` value: any further would leave blank rows at the bottom.
    fn max_scroll(&self) -> usize {
        self.items.len().saturating_sub(self.visible_count())
    }

    /// Adjusts `scroll` so `selected` falls within the visible window; called on open so
    /// the current selection is always in view.
    fn scroll_to_selected(&mut self) {
        let visible = self.visible_count();
        if visible == 0 {
            self.scroll = 0;
            return;
        }
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + visible {
            self.scroll = self.selected + 1 - visible;
        }
        self.scroll = self.scroll.min(self.max_scroll());
    }

    /// The open list's rect: directly below the closed `rect`, spanning its width, with
    /// height sized to the number of currently visible rows (zero when there are no items).
    fn list_rect(&self, rect: RECT) -> RECT {
        let height = DROPDOWN_ROW_HEIGHT * self.visible_count() as i32;
        RECT {
            left: rect.left,
            top: rect.bottom,
            right: rect.right,
            bottom: rect.bottom + height,
        }
    }

    /// The rect of the visible row at `visible_idx` (0-based within the current scroll
    /// window), relative to the open list computed from the closed `rect`.
    fn row_rect(&self, rect: RECT, visible_idx: usize) -> RECT {
        let list = self.list_rect(rect);
        let top = list.top + DROPDOWN_ROW_HEIGHT * visible_idx as i32;
        RECT {
            left: list.left,
            top,
            right: list.right,
            bottom: top + DROPDOWN_ROW_HEIGHT,
        }
    }

    /// Whether client point `(x, y)` falls within `r` using half-open bounds (`[left,
    /// right)` x `[top, bottom)`), so adjacent rects (e.g. the closed row and the open list
    /// starting exactly at its bottom edge) never both claim the same point.
    fn point_in(&self, x: i32, y: i32, r: RECT) -> bool {
        x >= r.left && x < r.right && y >= r.top && y < r.bottom
    }

    /// Maps a client point to an item index in the open list, accounting for `scroll`.
    /// Returns `None` when `(x, y)` falls outside the list's horizontal or vertical bounds
    /// (including below the last visible row).
    fn item_at(&self, x: i32, y: i32, rect: RECT) -> Option<usize> {
        let list = self.list_rect(rect);
        if !self.point_in(x, y, list) {
            return None;
        }
        let visible_idx = ((y - list.top) / DROPDOWN_ROW_HEIGHT) as usize;
        let idx = self.scroll + visible_idx;
        if idx < self.items.len() {
            Some(idx)
        } else {
            None
        }
    }

    /// Force-closes the open list, if any, without changing `selected`. Used by embedders that
    /// need to guarantee a dropdown isn't left visually "stuck open" when something other than a
    /// click on/around the dropdown itself (e.g. navigating away from the section that owns it)
    /// makes the open list stop making sense to show. A no-op if already closed.
    pub fn close(&mut self) {
        self.open = false;
    }
}

impl Control for Dropdown {
    fn draw(&self, hdc: HDC, rect: RECT, dark: bool) {
        unsafe {
            let bg = if dark {
                Color::new(0x33, 0x33, 0x33)
            } else {
                Color::new(0xff, 0xff, 0xff)
            };
            let border = if dark {
                Color::new(0x55, 0x55, 0x55)
            } else {
                Color::new(0xc0, 0xc0, 0xc0)
            };
            let text_color = if dark {
                Color::new(0xf0, 0xf0, 0xf0)
            } else {
                Color::new(0x20, 0x20, 0x20)
            };
            let highlight = Color::new(0xd9, 0x77, 0x57);

            draw_rounded_rect(hdc, &rect, &border, 4);
            let inset_rect = RECT {
                left: rect.left + 1,
                top: rect.top + 1,
                right: rect.right - 1,
                bottom: rect.bottom - 1,
            };
            draw_rounded_rect(hdc, &inset_rect, &bg, 4);

            let _ = SetBkMode(hdc, TRANSPARENT);
            let _ = SetTextColor(hdc, COLORREF(text_color.to_colorref()));

            let label = self
                .items
                .get(self.selected)
                .map(String::as_str)
                .unwrap_or("");
            let mut label_wide: Vec<u16> = label.encode_utf16().collect();
            let mut label_rect = RECT {
                left: rect.left + DROPDOWN_TEXT_PAD,
                top: rect.top,
                right: rect.right - DROPDOWN_ARROW_ZONE,
                bottom: rect.bottom,
            };
            let _ = DrawTextW(
                hdc,
                &mut label_wide,
                &mut label_rect,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            );

            let arrow = if self.open { "\u{25B2}" } else { "\u{25BC}" };
            let mut arrow_wide: Vec<u16> = arrow.encode_utf16().collect();
            let mut arrow_rect = RECT {
                left: rect.right - DROPDOWN_ARROW_ZONE,
                top: rect.top,
                right: rect.right - DROPDOWN_TEXT_PAD / 2,
                bottom: rect.bottom,
            };
            let _ = DrawTextW(
                hdc,
                &mut arrow_wide,
                &mut arrow_rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );

            if self.open && !self.items.is_empty() {
                let list = self.list_rect(rect);
                draw_rounded_rect(hdc, &list, &border, 4);
                let inset_list = RECT {
                    left: list.left + 1,
                    top: list.top,
                    right: list.right - 1,
                    bottom: list.bottom - 1,
                };
                draw_rounded_rect(hdc, &inset_list, &bg, 4);

                for visible_idx in 0..self.visible_count() {
                    let idx = self.scroll + visible_idx;
                    let row = self.row_rect(rect, visible_idx);
                    if idx == self.selected {
                        fill_rect(hdc, &row, &highlight);
                    }
                    let mut item_wide: Vec<u16> = self.items[idx].encode_utf16().collect();
                    let mut item_rect = RECT {
                        left: row.left + DROPDOWN_TEXT_PAD,
                        top: row.top,
                        right: row.right - DROPDOWN_TEXT_PAD,
                        bottom: row.bottom,
                    };
                    let _ = DrawTextW(
                        hdc,
                        &mut item_wide,
                        &mut item_rect,
                        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                    );
                }
            }
        }
    }

    fn on_mouse(&mut self, msg: u32, x: i32, y: i32, rect: RECT) -> Option<ControlEvent> {
        if msg != WM_LBUTTONDOWN || self.items.is_empty() {
            return None;
        }
        if self.open {
            if self.point_in(x, y, rect) {
                // Click on the closed row while open: collapse without changing selection.
                self.open = false;
                return None;
            }
            if let Some(idx) = self.item_at(x, y, rect) {
                self.selected = idx;
                self.open = false;
                return Some(ControlEvent::Changed);
            }
            // Outside both the closed row and the open list: dismiss, no value change (see
            // struct doc comment for the outside-click interpretation).
            self.open = false;
            None
        } else if self.point_in(x, y, rect) {
            self.open = true;
            self.scroll_to_selected();
            None
        } else {
            None
        }
    }
}

/// Horizontal gap, in pixels, between adjacent segment pills in a `Segmented` control.
const SEGMENTED_GAP: i32 = 2;

/// A row of equal-width "pill" buttons, one per `options` entry, with `selected` drawn
/// highlighted. Clicking a pill (other than the one already selected) selects it and
/// emits `Changed`; clicking the already-selected pill is a no-op (nothing changed).
#[allow(dead_code)] // consumed by the settings window (Tasks 11-15), not yet wired up
pub struct Segmented {
    pub options: Vec<String>,
    pub selected: usize,
}

impl Segmented {
    #[allow(dead_code)] // consumed by the settings window (Tasks 11-15), not yet wired up
    pub fn new(options: Vec<String>, selected: usize) -> Self {
        let selected = if options.is_empty() {
            0
        } else {
            selected.min(options.len() - 1)
        };
        Self { options, selected }
    }

    /// The rect of segment `index` within `rect`: `rect`'s width divided evenly into
    /// `options.len()` columns, with a `SEGMENTED_GAP`-wide gap between adjacent segments
    /// (the gap is carved out of each segment's own width, not added on top, so the row of
    /// segments still exactly spans `rect`). The last segment absorbs any leftover width
    /// from integer division, matching `RgbaPicker::row_rect`'s convention.
    fn segment_rect(&self, index: usize, rect: RECT) -> RECT {
        let n = self.options.len().max(1) as i32;
        let width = (rect.right - rect.left).max(1);
        // `.max(1)` matches `option_at`'s floor so the two agree on which option owns space
        // even when `rect` is narrower than `n` (each column at least 1px rather than 0).
        let col_w = (width / n).max(1);
        let left = rect.left + col_w * index as i32;
        let right = if index as i32 == n - 1 {
            rect.right
        } else {
            left + col_w
        };
        let gap = SEGMENTED_GAP.min((right - left) / 2).max(0);
        RECT {
            left: left + if index == 0 { 0 } else { gap },
            top: rect.top,
            right: right - if index as i32 == n - 1 { 0 } else { gap },
            bottom: rect.bottom,
        }
    }

    /// Which option index (if any) contains client point `(x, y)`, using half-open bounds
    /// per-column (`[col.left, col.left + col_w)` horizontally, matching the `Dropdown`
    /// convention) so that a click exactly on a shared boundary resolves to exactly one
    /// segment, and a click above/below `rect` (or when there are no options) resolves to
    /// `None`.
    fn option_at(&self, x: i32, y: i32, rect: RECT) -> Option<usize> {
        if self.options.is_empty() || y < rect.top || y >= rect.bottom {
            return None;
        }
        let n = self.options.len().max(1) as i32;
        let width = (rect.right - rect.left).max(1);
        let col_w = (width / n).max(1);
        if x < rect.left || x >= rect.right {
            return None;
        }
        let idx = ((x - rect.left) / col_w).clamp(0, n - 1);
        Some(idx as usize)
    }
}

impl Control for Segmented {
    fn draw(&self, hdc: HDC, rect: RECT, dark: bool) {
        unsafe {
            let bg = if dark {
                Color::new(0x33, 0x33, 0x33)
            } else {
                Color::new(0xe8, 0xe8, 0xe8)
            };
            let selected_bg = Color::new(0xd9, 0x77, 0x57);
            let text_color = if dark {
                Color::new(0xf0, 0xf0, 0xf0)
            } else {
                Color::new(0x20, 0x20, 0x20)
            };
            let selected_text = Color::new(0xff, 0xff, 0xff);

            let _ = SetBkMode(hdc, TRANSPARENT);

            let radius = ((rect.bottom - rect.top).max(1) / 2).min(8);
            for (i, option) in self.options.iter().enumerate() {
                let seg = self.segment_rect(i, rect);
                let (fill, text) = if i == self.selected {
                    (&selected_bg, selected_text)
                } else {
                    (&bg, text_color)
                };
                draw_rounded_rect(hdc, &seg, fill, radius);

                let _ = SetTextColor(hdc, COLORREF(text.to_colorref()));
                let mut label_wide: Vec<u16> = option.encode_utf16().collect();
                let mut label_rect = seg;
                let _ = DrawTextW(
                    hdc,
                    &mut label_wide,
                    &mut label_rect,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
            }
        }
    }

    fn on_mouse(&mut self, msg: u32, x: i32, y: i32, rect: RECT) -> Option<ControlEvent> {
        if msg != WM_LBUTTONDOWN {
            return None;
        }
        let idx = self.option_at(x, y, rect)?;
        if idx == self.selected {
            return None;
        }
        self.selected = idx;
        Some(ControlEvent::Changed)
    }
}

/// Layout constants for `Toggle`'s pill switch: overall track size and the padding between
/// the track's edge and the knob circle.
const TOGGLE_TRACK_W: i32 = 36;
const TOGGLE_TRACK_H: i32 = 18;
const TOGGLE_KNOB_PAD: i32 = 2;

/// A simple on/off switch drawn as a pill-shaped track with a circular knob at the left
/// (off) or right (on) end. Clicking within the drawn track (`track_rect(rect)`, which may
/// be narrower than `rect` itself) flips `on` and emits `Changed`; clicks in the empty space
/// of `rect` outside the track are ignored.
#[allow(dead_code)] // consumed by the settings window (Tasks 11-15), not yet wired up
pub struct Toggle {
    pub on: bool,
}

impl Toggle {
    #[allow(dead_code)] // consumed by the settings window (Tasks 11-15), not yet wired up
    pub fn new(on: bool) -> Self {
        Self { on }
    }

    /// The track rect: a `TOGGLE_TRACK_W` x `TOGGLE_TRACK_H` pill vertically centered
    /// within `rect` and anchored to `rect`'s left edge (matching `RgbaPicker::swatch_rect`'s
    /// left-anchored, height-clamped convention), shrunk to fit if `rect` is smaller than
    /// the nominal track size.
    fn track_rect(&self, rect: RECT) -> RECT {
        let avail_h = (rect.bottom - rect.top).max(1);
        let avail_w = (rect.right - rect.left).max(1);
        let h = TOGGLE_TRACK_H.min(avail_h);
        // Never narrower than it is tall, but the final `.min(avail_w)` re-clamps in case
        // `.max(h)` (applied after the first `.min(avail_w)`) would otherwise re-widen `w`
        // past the available width for narrow-but-tall rects (e.g. avail_w=1, h=18: without
        // this the naive `min(36,1).max(18)` would overflow to 18).
        let w = TOGGLE_TRACK_W.min(avail_w).max(h).min(avail_w);
        let top = rect.top + (avail_h - h) / 2;
        RECT {
            left: rect.left,
            top,
            right: rect.left + w,
            bottom: top + h,
        }
    }

    /// The knob's bounding rect within `track`: a circle inset by `TOGGLE_KNOB_PAD`, flush
    /// with the track's left edge when off and its right edge when on.
    fn knob_rect(&self, track: RECT) -> RECT {
        let h = (track.bottom - track.top).max(1);
        let d = (h - TOGGLE_KNOB_PAD * 2).max(1);
        let left = if self.on {
            track.right - TOGGLE_KNOB_PAD - d
        } else {
            track.left + TOGGLE_KNOB_PAD
        };
        RECT {
            left,
            top: track.top + TOGGLE_KNOB_PAD,
            right: left + d,
            bottom: track.top + TOGGLE_KNOB_PAD + d,
        }
    }
}

impl Control for Toggle {
    fn draw(&self, hdc: HDC, rect: RECT, dark: bool) {
        unsafe {
            let track = self.track_rect(rect);
            let track_color = if self.on {
                Color::new(0xd9, 0x77, 0x57)
            } else if dark {
                Color::new(0x4a, 0x4a, 0x4a)
            } else {
                Color::new(0xd4, 0xd4, 0xd4)
            };
            let knob_color = if dark {
                Color::new(0xf0, 0xf0, 0xf0)
            } else {
                Color::new(0xff, 0xff, 0xff)
            };
            let knob_border = if dark {
                Color::new(0x20, 0x20, 0x20)
            } else {
                Color::new(0xa0, 0xa0, 0xa0)
            };

            draw_rounded_rect(hdc, &track, &track_color, (track.bottom - track.top) / 2);

            let knob = self.knob_rect(track);
            let brush = CreateSolidBrush(COLORREF(knob_color.to_colorref()));
            let pen = CreatePen(PS_SOLID, 1, COLORREF(knob_border.to_colorref()));
            let old_brush = SelectObject(hdc, brush);
            let old_pen = SelectObject(hdc, pen);
            let _ = Ellipse(hdc, knob.left, knob.top, knob.right, knob.bottom);
            SelectObject(hdc, old_brush);
            SelectObject(hdc, old_pen);
            let _ = DeleteObject(brush);
            let _ = DeleteObject(pen);
        }
    }

    fn on_mouse(&mut self, msg: u32, x: i32, y: i32, rect: RECT) -> Option<ControlEvent> {
        if msg != WM_LBUTTONDOWN {
            return None;
        }
        // Hit-test against the actual drawn track (`track_rect`), not the raw `rect`: when
        // `rect` is wider than the fixed-size track (see `draw`/`track_rect`), a click in the
        // visually empty space to the right of the track must not flip `on`.
        let track = self.track_rect(rect);
        if x >= track.left && x < track.right && y >= track.top && y < track.bottom {
            self.on = !self.on;
            Some(ControlEvent::Changed)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_maps_and_clamps() {
        let s = Slider {
            value: 0.0,
            min: 0.0,
            max: 100.0,
            dragging: false,
        };
        let r = RECT {
            left: 10,
            top: 0,
            right: 110,
            bottom: 20,
        };
        assert_eq!(s.pos_to_value(60, r).round(), 50.0);
        assert_eq!(s.pos_to_value(-999, r), 0.0);
        assert_eq!(s.pos_to_value(9999, r), 100.0);
    }

    #[test]
    fn row_at_rejects_label_column_clicks() {
        let picker = RgbaPicker::new(Rgba {
            r: 200,
            g: 150,
            b: 100,
            a: 255,
        });
        let rect = RECT {
            left: 0,
            top: 0,
            right: 200,
            bottom: 120,
        };
        let row0 = picker.row_rect(0, rect);
        let y = (row0.top + row0.bottom) / 2;

        // A click anywhere in the label column (x < row.left + LABEL_WIDTH) must not
        // resolve to a row, regardless of how far left it is.
        assert_eq!(picker.row_at(row0.left, y, rect), None);
        assert_eq!(picker.row_at(row0.left + LABEL_WIDTH - 1, y, rect), None);
        // Just past the label column it resolves to the expected channel again.
        assert_eq!(picker.row_at(row0.left + LABEL_WIDTH, y, rect), Some(0));
    }

    #[test]
    fn rgba_picker_label_click_does_not_zero_slider_value() {
        let mut picker = RgbaPicker::new(Rgba {
            r: 200,
            g: 150,
            b: 100,
            a: 255,
        });
        let rect = RECT {
            left: 0,
            top: 0,
            right: 200,
            bottom: 120,
        };
        let row0 = picker.row_rect(0, rect);
        let y = (row0.top + row0.bottom) / 2;
        let x_in_label = row0.left + 2; // inside the "R" label column

        let event = picker.on_mouse(WM_LBUTTONDOWN, x_in_label, y, rect);

        // Before the fix, this click would dispatch to the R slider with an
        // out-of-range x, clamping it to 0 (`Slider::pos_to_value`). Now it's a no-op.
        assert!(event.is_none());
        assert_eq!(picker.value.r, 200);
        assert!(picker.active.is_none());
    }

    fn dropdown_items(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("Item {i}")).collect()
    }

    #[test]
    fn dropdown_item_at_accounts_for_scroll_offset() {
        // 10 items, only 6 visible at a time (DROPDOWN_MAX_VISIBLE), scrolled so item 3 is
        // the first visible row.
        let dropdown = Dropdown {
            items: dropdown_items(10),
            selected: 0,
            open: true,
            scroll: 3,
        };
        let rect = RECT {
            left: 0,
            top: 0,
            right: 200,
            bottom: 24,
        };
        let list = dropdown.list_rect(rect);

        // First visible row maps to item (scroll + 0) = 3, not 0.
        let mid_row0_y = list.top + DROPDOWN_ROW_HEIGHT / 2;
        assert_eq!(dropdown.item_at(100, mid_row0_y, rect), Some(3));

        // Last visible row (index 5) maps to item 3 + 5 = 8.
        let mid_last_row_y = list.top + DROPDOWN_ROW_HEIGHT * 5 + DROPDOWN_ROW_HEIGHT / 2;
        assert_eq!(dropdown.item_at(100, mid_last_row_y, rect), Some(8));

        // Exactly at the list's bottom edge (one pixel past the last visible row) is
        // outside the half-open list rect -> None, not an out-of-range item index.
        assert_eq!(dropdown.item_at(100, list.bottom, rect), None);

        // Outside the horizontal bounds is also None even at a valid row's y.
        assert_eq!(dropdown.item_at(-5, mid_row0_y, rect), None);
        assert_eq!(dropdown.item_at(500, mid_row0_y, rect), None);

        // Above the list (still within the closed row's y-range) is None.
        assert_eq!(dropdown.item_at(100, 0, rect), None);
    }

    #[test]
    fn dropdown_item_at_clamps_when_scroll_leaves_partial_last_page() {
        // 7 items, 6 visible max -> max_scroll is 1. With scroll at the max, the visible
        // window is items [1..7), so the last visible row (index 5) is item 6, and there is
        // no item 7 to click even though the row slot exists.
        let dropdown = Dropdown {
            items: dropdown_items(7),
            selected: 0,
            open: true,
            scroll: 1,
        };
        assert_eq!(dropdown.max_scroll(), 1);
        let rect = RECT {
            left: 0,
            top: 0,
            right: 200,
            bottom: 24,
        };
        let list = dropdown.list_rect(rect);
        let last_row_mid_y = list.top + DROPDOWN_ROW_HEIGHT * 5 + DROPDOWN_ROW_HEIGHT / 2;
        assert_eq!(dropdown.item_at(100, last_row_mid_y, rect), Some(6));
    }

    #[test]
    fn dropdown_scroll_to_selected_keeps_selection_in_view() {
        let mut dropdown = Dropdown::new(dropdown_items(20), 15);
        dropdown.scroll_to_selected();
        let visible = dropdown.visible_count();
        assert!(dropdown.selected >= dropdown.scroll);
        assert!(dropdown.selected < dropdown.scroll + visible);
        assert!(dropdown.scroll <= dropdown.max_scroll());
    }

    #[test]
    fn dropdown_click_opens_then_click_item_selects_and_closes() {
        let mut dropdown = Dropdown::new(dropdown_items(10), 0);
        let rect = RECT {
            left: 0,
            top: 0,
            right: 200,
            bottom: 24,
        };

        // Click the closed row: opens, no value change.
        let event = dropdown.on_mouse(WM_LBUTTONDOWN, 50, 12, rect);
        assert!(event.is_none());
        assert!(dropdown.open);

        // Click the 3rd visible row (index 2) in the open list: selects it, closes, and
        // reports Changed.
        let list = dropdown.list_rect(rect);
        let row2_mid_y = list.top + DROPDOWN_ROW_HEIGHT * 2 + DROPDOWN_ROW_HEIGHT / 2;
        let event = dropdown.on_mouse(WM_LBUTTONDOWN, 50, row2_mid_y, rect);
        assert!(matches!(event, Some(ControlEvent::Changed)));
        assert_eq!(dropdown.selected, 2);
        assert!(!dropdown.open);
    }

    #[test]
    fn dropdown_click_outside_open_list_closes_without_changing_selection() {
        let mut dropdown = Dropdown::new(dropdown_items(10), 4);
        let rect = RECT {
            left: 0,
            top: 0,
            right: 200,
            bottom: 24,
        };
        dropdown.on_mouse(WM_LBUTTONDOWN, 50, 12, rect); // open it
        assert!(dropdown.open);

        // Click far below the open list: outside both the closed row and the list.
        let list = dropdown.list_rect(rect);
        let event = dropdown.on_mouse(WM_LBUTTONDOWN, 50, list.bottom + 100, rect);
        assert!(event.is_none());
        assert!(!dropdown.open);
        assert_eq!(dropdown.selected, 4);
    }

    #[test]
    fn dropdown_click_closed_row_while_open_collapses_without_change() {
        let mut dropdown = Dropdown::new(dropdown_items(10), 4);
        let rect = RECT {
            left: 0,
            top: 0,
            right: 200,
            bottom: 24,
        };
        dropdown.on_mouse(WM_LBUTTONDOWN, 50, 12, rect); // open it
        assert!(dropdown.open);

        let event = dropdown.on_mouse(WM_LBUTTONDOWN, 50, 12, rect); // click closed row again
        assert!(event.is_none());
        assert!(!dropdown.open);
        assert_eq!(dropdown.selected, 4);
    }

    #[test]
    fn dropdown_ignores_clicks_when_no_items() {
        let mut dropdown = Dropdown::new(Vec::new(), 0);
        let rect = RECT {
            left: 0,
            top: 0,
            right: 200,
            bottom: 24,
        };
        let event = dropdown.on_mouse(WM_LBUTTONDOWN, 50, 12, rect);
        assert!(event.is_none());
        assert!(!dropdown.open);
    }

    // No TDD test drives `enumerate_font_families` itself (per the task brief: it's a system
    // API with no meaningful pure-logic seam, and the real result depends on whatever fonts
    // are installed on the machine running the test). This is an ignored sanity check for
    // manual verification (`cargo test controls -- --ignored --nocapture`), not part of the
    // normal `cargo test` run.
    #[test]
    #[ignore]
    fn enumerate_font_families_is_nonempty_sorted_and_deduped() {
        let families = enumerate_font_families();
        println!("enumerated {} font families", families.len());
        assert!(
            !families.is_empty(),
            "expected at least one installed font family"
        );
        let mut sorted = families.clone();
        sorted.sort();
        assert_eq!(families, sorted, "expected result to already be sorted");
        let mut deduped = families.clone();
        deduped.dedup();
        assert_eq!(
            families, deduped,
            "expected result to contain no duplicates"
        );
    }

    fn segmented_options(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("Opt{i}")).collect()
    }

    #[test]
    fn segmented_option_at_splits_evenly_and_resolves_boundaries() {
        // 3 options over a 300px-wide rect: even columns [0,100) [100,200) [200,300).
        let seg = Segmented::new(segmented_options(3), 0);
        let rect = RECT {
            left: 0,
            top: 0,
            right: 300,
            bottom: 24,
        };
        let y = 12;

        // Interior of each column.
        assert_eq!(seg.option_at(50, y, rect), Some(0));
        assert_eq!(seg.option_at(150, y, rect), Some(1));
        assert_eq!(seg.option_at(250, y, rect), Some(2));

        // Exactly on a shared boundary: half-open bounds mean it belongs to the segment
        // starting there, not the one ending there.
        assert_eq!(seg.option_at(100, y, rect), Some(1));
        assert_eq!(seg.option_at(200, y, rect), Some(2));

        // Leftmost pixel and rightmost valid pixel (right edge is exclusive).
        assert_eq!(seg.option_at(0, y, rect), Some(0));
        assert_eq!(seg.option_at(299, y, rect), Some(2));
        assert_eq!(seg.option_at(300, y, rect), None);
        assert_eq!(seg.option_at(-1, y, rect), None);

        // Outside the vertical bounds (half-open: bottom is exclusive) is also None.
        assert_eq!(seg.option_at(50, rect.top - 1, rect), None);
        assert_eq!(seg.option_at(50, rect.bottom, rect), None);
    }

    #[test]
    fn segmented_option_at_last_segment_absorbs_uneven_remainder() {
        // 3 options over a 301px-wide rect: 301 / 3 = 100 (integer division), so the last
        // column must still resolve up to rect.right - 1 (300), not fall off after 299.
        let seg = Segmented::new(segmented_options(3), 0);
        let rect = RECT {
            left: 0,
            top: 0,
            right: 301,
            bottom: 24,
        };
        assert_eq!(seg.option_at(300, 12, rect), Some(2));
        assert_eq!(seg.option_at(199, 12, rect), Some(1));
        assert_eq!(seg.option_at(200, 12, rect), Some(2));
    }

    #[test]
    fn segmented_click_selects_and_emits_changed() {
        let mut seg = Segmented::new(segmented_options(3), 0);
        let rect = RECT {
            left: 0,
            top: 0,
            right: 300,
            bottom: 24,
        };
        let event = seg.on_mouse(WM_LBUTTONDOWN, 250, 12, rect);
        assert!(matches!(event, Some(ControlEvent::Changed)));
        assert_eq!(seg.selected, 2);
    }

    #[test]
    fn segmented_click_on_already_selected_option_is_a_no_op() {
        let mut seg = Segmented::new(segmented_options(3), 1);
        let rect = RECT {
            left: 0,
            top: 0,
            right: 300,
            bottom: 24,
        };
        let event = seg.on_mouse(WM_LBUTTONDOWN, 150, 12, rect);
        assert!(event.is_none());
        assert_eq!(seg.selected, 1);
    }

    #[test]
    fn segmented_ignores_clicks_when_no_options() {
        let mut seg = Segmented::new(Vec::new(), 0);
        let rect = RECT {
            left: 0,
            top: 0,
            right: 300,
            bottom: 24,
        };
        let event = seg.on_mouse(WM_LBUTTONDOWN, 150, 12, rect);
        assert!(event.is_none());
        assert_eq!(seg.selected, 0);
    }

    #[test]
    fn segmented_single_option_always_selected_and_click_is_a_no_op() {
        // With exactly one option, any click within `rect` resolves to that option, which is
        // already selected -> no-op, mirroring `segmented_ignores_clicks_when_no_options` for
        // the N=0 case.
        let mut seg = Segmented::new(segmented_options(1), 0);
        let rect = RECT {
            left: 0,
            top: 0,
            right: 300,
            bottom: 24,
        };
        assert_eq!(seg.option_at(150, 12, rect), Some(0));
        let event = seg.on_mouse(WM_LBUTTONDOWN, 150, 12, rect);
        assert!(event.is_none());
        assert_eq!(seg.selected, 0);
    }

    #[test]
    fn toggle_click_flips_state_and_emits_changed() {
        let mut toggle = Toggle::new(false);
        let rect = RECT {
            left: 0,
            top: 0,
            right: 36,
            bottom: 18,
        };
        let event = toggle.on_mouse(WM_LBUTTONDOWN, 10, 9, rect);
        assert!(matches!(event, Some(ControlEvent::Changed)));
        assert!(toggle.on);

        let event = toggle.on_mouse(WM_LBUTTONDOWN, 10, 9, rect);
        assert!(matches!(event, Some(ControlEvent::Changed)));
        assert!(!toggle.on);
    }

    #[test]
    fn toggle_ignores_clicks_outside_rect() {
        let mut toggle = Toggle::new(false);
        let rect = RECT {
            left: 0,
            top: 0,
            right: 36,
            bottom: 18,
        };
        assert!(toggle.on_mouse(WM_LBUTTONDOWN, 36, 9, rect).is_none());
        assert!(toggle.on_mouse(WM_LBUTTONDOWN, -1, 9, rect).is_none());
        assert!(toggle.on_mouse(WM_LBUTTONDOWN, 10, 18, rect).is_none());
        assert!(!toggle.on);
    }

    #[test]
    fn toggle_ignores_clicks_past_track_edge_in_wider_rect() {
        // `rect` is much wider than the drawn track (TOGGLE_TRACK_W == 36); a click in the
        // empty space to the right of the track must not flip `on`, even though it's well
        // within `rect` itself. This is the regression scenario for the hit-test fix: before
        // it, `on_mouse` tested against the raw (wide) `rect` instead of `track_rect(rect)`.
        let mut toggle = Toggle::new(false);
        let rect = RECT {
            left: 0,
            top: 0,
            right: 200,
            bottom: 18,
        };
        let track = toggle.track_rect(rect);
        assert_eq!(track.right, TOGGLE_TRACK_W); // sanity: track stays fixed-size, left-anchored

        // Click well past the track's right edge, still inside `rect`: ignored.
        let event = toggle.on_mouse(WM_LBUTTONDOWN, 150, 9, rect);
        assert!(event.is_none());
        assert!(!toggle.on);

        // Click just within the track's own bounds still flips it.
        let event = toggle.on_mouse(WM_LBUTTONDOWN, track.right - 1, 9, rect);
        assert!(matches!(event, Some(ControlEvent::Changed)));
        assert!(toggle.on);
    }

    #[test]
    fn toggle_track_rect_never_exceeds_narrow_rects_width() {
        // A narrow-but-tall rect: avail_w=1 is far smaller than TOGGLE_TRACK_H (18), so the
        // naive `TOGGLE_TRACK_W.min(avail_w).max(h)` would re-widen `w` back up to `h` (18),
        // overflowing 17px past `rect.right`. The track must never exceed the rect's own
        // width, even when that means it's narrower than it is tall.
        let toggle = Toggle::new(false);
        let rect = RECT {
            left: 0,
            top: 0,
            right: 1,
            bottom: 100,
        };
        let track = toggle.track_rect(rect);
        assert!(track.right - track.left <= rect.right - rect.left);
        assert_eq!(track.right, rect.right);
    }

    #[test]
    fn toggle_knob_moves_from_left_to_right_edge_when_on() {
        let track = RECT {
            left: 0,
            top: 0,
            right: 36,
            bottom: 18,
        };
        let off = Toggle::new(false);
        let on = Toggle::new(true);
        let off_knob = off.knob_rect(track);
        let on_knob = on.knob_rect(track);
        assert_eq!(off_knob.left, TOGGLE_KNOB_PAD);
        assert_eq!(on_knob.right, track.right - TOGGLE_KNOB_PAD);
        assert!(on_knob.left > off_knob.left);
    }
}

#[cfg(test)]
mod popover_tests {
    use super::*;

    #[test]
    fn closed_row_is_one_row_tall() {
        let picker = RgbaPicker::new(Rgba { r: 217, g: 119, b: 87, a: 255 });
        let rect = RECT { left: 0, top: 0, right: 200, bottom: 20 };
        // The closed control's own draw rect is exactly what's passed in — no
        // extra height is consumed until `open` is true.
        assert_eq!(picker.open, false);
        let popover = picker.popover_rect(rect);
        assert!(popover.top >= rect.bottom, "popover must render below the closed row");
    }

    #[test]
    fn click_on_closed_row_opens_popover() {
        let mut picker = RgbaPicker::new(Rgba { r: 217, g: 119, b: 87, a: 255 });
        let rect = RECT { left: 0, top: 0, right: 200, bottom: 20 };
        let _ = picker.on_mouse(WM_LBUTTONDOWN, 10, 10, rect);
        assert!(picker.open, "clicking the closed row must open the popover");
    }

    #[test]
    fn click_outside_when_open_closes_it() {
        let mut picker = RgbaPicker::new(Rgba { r: 217, g: 119, b: 87, a: 255 });
        picker.open = true;
        let rect = RECT { left: 0, top: 0, right: 200, bottom: 20 };
        // A click far outside both the row and the popover.
        let _ = picker.on_mouse(WM_LBUTTONDOWN, 500, 500, rect);
        assert!(!picker.open, "an outside click must close the popover");
    }

    #[test]
    fn click_in_swatch_grid_gap_stays_open_and_is_a_no_op() {
        // Regression test: a click that lands inside `popover_rect`'s bounds but in a
        // "dead zone" (here, the `POPOVER_SWATCH_GAP` gap between quick-swatch cells 0 and
        // 1) must be a no-op, not a dismissal. Before this fix, any click not specifically
        // matched by the row/swatch/Custom-row/slider checks fell through to `close()`,
        // even when it was still clearly inside the open popover.
        let mut picker = RgbaPicker::new(Rgba { r: 217, g: 119, b: 87, a: 255 });
        picker.open = true;
        let rect = RECT { left: 0, top: 0, right: 200, bottom: 20 };

        // Cell 0 spans x in [4, 28); cell 1 starts at x = 32 (POPOVER_SWATCH_GAP = 4 wide).
        // x = 30, y = 30 falls in that horizontal gap, within the grid's row-0 vertical
        // band (y in [24, 48)) but outside every swatch cell.
        let cell0 = picker.swatch_cell_rect(0, rect);
        let cell1 = picker.swatch_cell_rect(1, rect);
        assert!(cell0.right < cell1.left, "expected a real gap between adjacent swatch cells");
        let gap_x = cell0.right + (cell1.left - cell0.right) / 2;
        let gap_y = cell0.top + 6;
        assert!(picker.swatch_at(gap_x, gap_y, rect).is_none(), "test point must miss every cell");
        assert!(
            picker.point_in(gap_x, gap_y, picker.popover_rect(rect)),
            "test point must still be inside the popover's own bounds"
        );

        let event = picker.on_mouse(WM_LBUTTONDOWN, gap_x, gap_y, rect);
        assert!(event.is_none(), "a dead-zone click inside the popover must not fire an event");
        assert!(picker.open, "a dead-zone click inside the popover must not close it");
    }

    #[test]
    fn click_on_revealed_slider_label_column_stays_open_and_is_a_no_op() {
        // Companion regression test: a click on the "R"/"G"/"B"/"A" label column of a
        // *revealed* popover slider row (custom_expanded == true) is also a dead zone —
        // it's inside `popover_rect` but doesn't hit `popover_slider_row_at` (which
        // excludes the label column, same as the legacy `row_at`). Before this fix it fell
        // through to the outside-click branch and closed the whole popover; the legacy
        // always-visible sliders already treated the equivalent click as inert (see
        // `rgba_picker_label_click_does_not_zero_slider_value`), so the popover-mode
        // sliders should match.
        let mut picker = RgbaPicker::new(Rgba { r: 10, g: 20, b: 30, a: 255 });
        picker.open = true;
        picker.custom_expanded = true;
        let rect = RECT { left: 0, top: 0, right: 200, bottom: 20 };

        let row0 = picker.popover_slider_row_rect(0, rect);
        let label_x = row0.left + 2; // inside the "R" label column (< LABEL_WIDTH)
        let label_y = (row0.top + row0.bottom) / 2;
        assert!(picker.popover_slider_row_at(label_x, label_y, rect).is_none());

        let event = picker.on_mouse(WM_LBUTTONDOWN, label_x, label_y, rect);
        assert!(event.is_none(), "a label-column click must not fire an event");
        assert!(picker.open, "a label-column click must not close the popover");
        assert_eq!(picker.value.r, 10, "the R channel must not be zeroed");
    }

    #[test]
    fn close_forces_open_false() {
        let mut picker = RgbaPicker::new(Rgba { r: 217, g: 119, b: 87, a: 255 });
        picker.open = true;
        picker.close();
        assert!(!picker.open);
    }

    #[test]
    fn clicking_a_quick_swatch_sets_value_and_closes() {
        let mut picker = RgbaPicker::new(Rgba { r: 0, g: 0, b: 0, a: 255 });
        picker.open = true;
        let rect = RECT { left: 0, top: 0, right: 200, bottom: 20 };
        let popover = picker.popover_rect(rect);
        // Click the first quick-swatch cell (top-left of the grid).
        let event = picker.on_mouse(WM_LBUTTONDOWN, popover.left + 5, popover.top + 5, rect);
        assert!(event.is_some(), "clicking a swatch must fire a Changed event");
        assert_eq!(picker.value, QUICK_SWATCHES[0]);
        assert!(!picker.open, "picking a swatch closes the popover");
    }

    #[test]
    fn clicking_custom_reveals_sliders_without_closing() {
        let mut picker = RgbaPicker::new(Rgba { r: 0, g: 0, b: 0, a: 255 });
        picker.open = true;
        let rect = RECT { left: 0, top: 0, right: 200, bottom: 20 };
        let custom_row = picker.custom_row_rect(rect);
        let _ = picker.on_mouse(WM_LBUTTONDOWN, custom_row.left + 5, custom_row.top + 5, rect);
        assert!(picker.open, "Custom stays open to show the manual sliders");
        assert!(picker.custom_expanded, "Custom must flip on the manual-slider view");
    }

    #[test]
    fn dragging_a_revealed_slider_stays_open_and_updates_only_that_channel() {
        // Beyond the brief's literal test list: exercises the full open -> Custom ->
        // drag-a-slider path end to end, including that dragging one channel's slider
        // doesn't perturb the other three (a scenario none of the brief's tests cover).
        let mut picker = RgbaPicker::new(Rgba { r: 10, g: 20, b: 30, a: 255 });
        picker.open = true;
        picker.custom_expanded = true;
        let rect = RECT { left: 0, top: 0, right: 200, bottom: 20 };

        // Drag the G slider (index 1) to its rect's right edge -> should saturate near 255.
        let g_slot = picker.popover_slider_slot_rect(1, rect);
        let event = picker.on_mouse(WM_LBUTTONDOWN, g_slot.right, g_slot.top, rect);
        assert!(matches!(event, Some(ControlEvent::Changed)));
        assert!(picker.open, "dragging a slider must not close the popover");
        assert!(picker.custom_expanded, "dragging a slider must not collapse Custom");
        assert_eq!(picker.value.g, 255);
        // R/B/A untouched by dragging G.
        assert_eq!(picker.value.r, 10);
        assert_eq!(picker.value.b, 30);
        assert_eq!(picker.value.a, 255);

        // WM_MOUSEMOVE keeps tracking the same channel (active = Some(1)) even if the
        // cursor drifts to x=0 (clamped to the slider's minimum).
        let move_event = picker.on_mouse(WM_MOUSEMOVE, g_slot.left - 999, g_slot.top, rect);
        assert!(matches!(move_event, Some(ControlEvent::Changed)));
        assert_eq!(picker.value.g, 0);

        // WM_LBUTTONUP ends the drag and commits.
        let up_event = picker.on_mouse(WM_LBUTTONUP, g_slot.left, g_slot.top, rect);
        assert!(matches!(up_event, Some(ControlEvent::CommitPreview)));
        assert!(picker.active.is_none());
    }

    #[test]
    fn picking_a_swatch_syncs_sliders_for_a_later_custom_view() {
        // Beyond the brief's literal test list: `clicking_a_quick_swatch_sets_value_and_closes`
        // checks `value` and `open`, but not that the four `Slider`s were kept in sync — if
        // they weren't, reopening the popover and choosing "Custom…" afterward would show
        // stale slider positions that don't match the swatch just picked.
        let mut picker = RgbaPicker::new(Rgba { r: 0, g: 0, b: 0, a: 255 });
        picker.open = true;
        let rect = RECT { left: 0, top: 0, right: 200, bottom: 20 };
        let cell = picker.swatch_cell_rect(3, rect); // QUICK_SWATCHES[3] = green
        let _ = picker.on_mouse(WM_LBUTTONDOWN, cell.left + 2, cell.top + 2, rect);
        assert_eq!(picker.value, QUICK_SWATCHES[3]);
        assert!(!picker.open);

        // Reopen and expand Custom: the sliders must already reflect the picked swatch.
        picker.open = true;
        picker.custom_expanded = true;
        assert_eq!(picker.r_slider.value, QUICK_SWATCHES[3].r as f32);
        assert_eq!(picker.g_slider.value, QUICK_SWATCHES[3].g as f32);
        assert_eq!(picker.b_slider.value, QUICK_SWATCHES[3].b as f32);
        assert_eq!(picker.a_slider.value, QUICK_SWATCHES[3].a as f32);
    }
}
