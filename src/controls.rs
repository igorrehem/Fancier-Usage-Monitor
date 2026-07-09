//! Reusable GDI controls for the settings window (Task 7+). Each control owns its own
//! state and knows how to `draw` itself into a caller-provided rect and how to react to
//! raw `WM_*` mouse messages via `on_mouse`, translated to client-relative `x`/`y`. The
//! settings window (Tasks 11-15) composes these into sections; nothing here depends on
//! `window.rs`/`settings.rs`/`animation.rs`.

use windows::Win32::Foundation::{COLORREF, RECT};
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
/// Layout constant: vertical gap between the swatch/hex row and the first slider row.
const ROW_GAP: i32 = 6;
/// Layout constant: width reserved for the single-letter channel label left of each slider.
const LABEL_WIDTH: i32 = 14;
/// Layout constant: horizontal gap between the swatch and the hex readout text.
const HEX_GAP: i32 = 8;
/// Layout constant: checkerboard cell size used to render alpha transparency under the swatch.
const CHECKER_CELL: i32 = 6;

/// A composed color picker: four `Slider`s (R/G/B/A, each ranging over `0..255`) plus a
/// swatch (alpha rendered over a checkerboard so transparency is visible) and a
/// `#RRGGBBAA` hex readout. `on_mouse` dispatches to whichever channel row the cursor is
/// in and keeps dispatching to that same channel while the drag continues, even if the
/// cursor drifts outside that row's exact bounds mid-drag.
#[allow(dead_code)] // consumed by the settings window (Tasks 11-15), not yet wired up
pub struct RgbaPicker {
    pub value: Rgba,
    r_slider: Slider,
    g_slider: Slider,
    b_slider: Slider,
    a_slider: Slider,
    /// Channel index (0=R, 1=G, 2=B, 3=A) currently being dragged, if any.
    active: Option<usize>,
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

    /// Y position where the four channel rows begin (just below the swatch/hex row).
    fn rows_top(&self, rect: RECT) -> i32 {
        self.swatch_rect(rect).bottom + ROW_GAP
    }

    /// The full-width row (label + slider) for channel `index` (0=R, 1=G, 2=B, 3=A).
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

    /// The slider-only portion of a channel row (row rect minus the label column).
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
}

impl Control for RgbaPicker {
    fn draw(&self, hdc: HDC, rect: RECT, dark: bool) {
        unsafe {
            let text_color = if dark {
                Color::new(0xf0, 0xf0, 0xf0)
            } else {
                Color::new(0x20, 0x20, 0x20)
            };
            let _ = SetBkMode(hdc, TRANSPARENT);
            let _ = SetTextColor(hdc, COLORREF(text_color.to_colorref()));

            let swatch = self.swatch_rect(rect);
            draw_checkerboard_swatch(hdc, swatch, &self.value.to_color());

            let hex = self.value.to_color().to_hex();
            let mut hex_wide: Vec<u16> = hex.encode_utf16().collect();
            let mut hex_rect = RECT {
                left: swatch.right + HEX_GAP,
                top: swatch.top,
                right: rect.right,
                bottom: swatch.bottom,
            };
            let _ = DrawTextW(
                hdc,
                &mut hex_wide,
                &mut hex_rect,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );

            const LABELS: [&str; 4] = ["R", "G", "B", "A"];
            for (i, label) in LABELS.iter().enumerate() {
                let row = self.row_rect(i, rect);
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
                self.slider(i).draw(hdc, self.slider_rect(i, rect), dark);
            }
        }
    }

    fn on_mouse(&mut self, msg: u32, x: i32, y: i32, rect: RECT) -> Option<ControlEvent> {
        match msg {
            WM_LBUTTONDOWN => {
                let idx = self.row_at(x, y, rect)?;
                self.active = Some(idx);
                let sub_rect = self.slider_rect(idx, rect);
                let event = self.slider_mut(idx).on_mouse(msg, x, y, sub_rect);
                if event.is_some() {
                    self.sync_value();
                }
                event
            }
            WM_MOUSEMOVE => {
                let idx = self.active?;
                let sub_rect = self.slider_rect(idx, rect);
                let event = self.slider_mut(idx).on_mouse(msg, x, y, sub_rect);
                if event.is_some() {
                    self.sync_value();
                }
                event
            }
            WM_LBUTTONUP => {
                let idx = self.active.take()?;
                let sub_rect = self.slider_rect(idx, rect);
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
}
