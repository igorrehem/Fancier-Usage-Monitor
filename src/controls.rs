//! Reusable GDI controls for the settings window (Task 7+). Each control owns its own
//! state and knows how to `draw` itself into a caller-provided rect and how to react to
//! raw `WM_*` mouse messages via `on_mouse`, translated to client-relative `x`/`y`. The
//! settings window (Tasks 11-15) composes these into sections; nothing here depends on
//! `window.rs`/`settings.rs`/`animation.rs`.

use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::{WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE};

use crate::native_interop::Color;

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
}
