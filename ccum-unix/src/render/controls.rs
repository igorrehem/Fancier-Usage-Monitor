//! Reusable `tiny-skia`-drawn controls for the popup settings panel (Task 10+), a direct port
//! of `ccum-windows/src/controls.rs`'s `Slider`/`RgbaPicker` (read in full, including the
//! outside-click-bounds fix that file's own review cycle found -- see `RgbaPicker::on_mouse`'s
//! `MouseMsg::Down` arm below, which preserves that exact fix). Only the two controls Task 10's
//! Appearance section actually needs are ported here; `Dropdown`/`Segmented`/`Toggle` are Task
//! 11's job when Font/Size/Animations/Update get ported.
//!
//! # Porting notes (`ccum-windows/src/controls.rs` is the reference)
//!
//! - **Rect type**: `ccum-windows`'s `RECT` (Win32) is a plain four-`i32`-field struct with no
//!   validity invariant -- `RgbaPicker::popover_height()` even builds one with all-zero
//!   coordinates as a throwaway anchor. `tiny_skia::Rect` (this crate's own `render::Rect`) is
//!   NOT a drop-in replacement for that: `Rect::from_ltrb`/`from_xywh` return `Option<Self>` and
//!   refuse degenerate (zero-or-negative-size) rects, which the `RECT`-based layout math here
//!   relies on being able to construct freely mid-calculation. So this module defines its own
//!   `LRect` (four plain `f32` fields, same shape as `RECT`) for every layout/hit-test
//!   computation, converting to `render::Rect` only at the point a shape actually gets filled
//!   (`LRect::to_skia`, `Option`-checked). This is a coordinate-type change only, per the Task
//!   10 brief -- the actual math (`swatch_rect`/`popover_rect`/`swatch_cell_rect`/etc.) is an
//!   unchanged, mechanical translation of the original `i32` arithmetic to `f32`.
//! - **Mouse message model**: Win32 delivers `WM_LBUTTONDOWN`/`WM_MOUSEMOVE`/`WM_LBUTTONUP` as a
//!   raw `u32` message code; `winit` 0.30 delivers `WindowEvent::CursorMoved`/`MouseInput`
//!   instead (see `panel.rs`'s `handle_window_event`). `MouseMsg` (`Down`/`Move`/`Up`) is the
//!   translation: `panel.rs` maps `MouseInput{state: Pressed, button: Left}` to `Down`,
//!   `MouseInput{state: Released, button: Left}` to `Up`, and every `CursorMoved` to `Move`
//!   (unconditionally -- exactly like `config_window.rs`'s own `WM_MOUSEMOVE` handler, which
//!   "broadcasts unconditionally" because only a control mid-drag reacts to it at all; see that
//!   handler's own doc comment). The actual dispatch logic inside `Slider`/`RgbaPicker` below is
//!   otherwise unchanged from the `WM_*` original.
//! - **GDI drawing calls**: `DrawTextW`/`Ellipse`/`CreateRoundRectRgn`+`FillRgn` become
//!   `TextRenderer::draw_text`/`Canvas::fill_circle`/`Canvas::fill_rounded_rect` respectively
//!   (`fill_circle` is a new, small `Canvas` primitive added by this task -- see its doc comment
//!   in `render::mod` -- mirroring `fill_rounded_rect`'s existing role for GDI's rounded-rect
//!   calls). No color-blending/checkerboard math changed; `draw_checkerboard_swatch`/`lerp_color`
//!   below are the same per-channel arithmetic as the Windows original, just against
//!   `tiny_skia::Color`'s `f32` (0.0..=1.0) channels instead of `native_interop::Color`'s `u8`
//!   ones (same adaptation `render::bars::lerp_color` already made for this crate).

use ccum_core::settings::Rgba;

use super::text::TextRenderer;
use super::{Canvas, Color, Rect as SkiaRect};

/// Translation of Win32's `WM_LBUTTONDOWN`/`WM_MOUSEMOVE`/`WM_LBUTTONUP` into `winit`'s pointer
/// event model -- see this module's doc comment ("Mouse message model").
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MouseMsg {
    Down,
    Move,
    Up,
}

/// Result of feeding a mouse message to a `Control`. Mirrors
/// `ccum-windows/src/controls.rs::ControlEvent` exactly.
pub enum ControlEvent {
    /// The value changed mid-interaction (e.g. while dragging); callers should update their
    /// live preview but need not persist yet.
    Changed,
    /// The interaction ended (e.g. mouse released); callers should commit/persist the current
    /// value.
    CommitPreview,
}

/// A plain four-`f32`-field rect with `RECT`'s exact semantics (no validity invariant) -- see
/// this module's doc comment ("Rect type") for why this exists instead of using `render::Rect`
/// directly for layout/hit-test math.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl LRect {
    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }

    /// Converts to a `render::Rect` for actually drawing a shape, or `None` if this `LRect` is
    /// degenerate (zero-or-negative width/height) -- `tiny_skia::Rect::from_ltrb`'s own
    /// contract. Every draw call below is written to just skip drawing on `None` (matching how
    /// GDI's `FillRect`/`CreateRoundRectRgn` on a degenerate `RECT` would simply paint nothing
    /// visible either).
    fn to_skia(self) -> Option<SkiaRect> {
        SkiaRect::from_ltrb(self.left, self.top, self.right, self.bottom)
    }
}

/// Common behavior shared by settings-panel controls: draw into a rect, and react to a mouse
/// event addressed to that same rect. Mirrors `ccum-windows/src/controls.rs::Control` exactly,
/// adapted to this crate's `Canvas`/`TextRenderer`/`MouseMsg`/`LRect` types.
pub trait Control {
    fn draw(&self, canvas: &mut Canvas, text: &mut TextRenderer, rect: LRect, dark: bool);
    fn on_mouse(&mut self, msg: MouseMsg, x: f32, y: f32, rect: LRect) -> Option<ControlEvent>;
}

/// A horizontal slider mapping a value in `[min, max]` onto a track spanning `rect`'s width.
/// Direct port of `ccum-windows/src/controls.rs::Slider`. Drag interaction: `MouseMsg::Down`
/// starts a drag (and jumps the value to the click position), `MouseMsg::Move` while dragging
/// keeps updating the value (`Changed`), and `MouseMsg::Up` ends the drag and commits
/// (`CommitPreview`).
pub struct Slider {
    pub value: f32,
    pub min: f32,
    pub max: f32,
    dragging: bool,
}

impl Slider {
    pub fn new(value: f32, min: f32, max: f32) -> Self {
        Self { value, min, max, dragging: false }
    }

    /// Maps a client-x pixel position to a value in `[min, max]`, clamping positions outside
    /// `rect` to the nearest end of the range.
    pub fn pos_to_value(&self, x: f32, rect: LRect) -> f32 {
        let width = rect.width().max(1.0);
        let t = ((x - rect.left) / width).clamp(0.0, 1.0);
        self.min + t * (self.max - self.min)
    }

    /// Maps the current value to the knob's client-x pixel position within `rect`.
    pub fn value_to_x(&self, rect: LRect) -> f32 {
        let width = rect.width().max(1.0);
        let range = (self.max - self.min).max(f32::EPSILON);
        let t = ((self.value - self.min) / range).clamp(0.0, 1.0);
        rect.left + t * width
    }
}

impl Control for Slider {
    fn draw(&self, canvas: &mut Canvas, _text: &mut TextRenderer, rect: LRect, dark: bool) {
        let full_h = rect.height().max(1.0);
        let track_h = 4.0_f32.min(full_h);
        let track_top = rect.top + (full_h - track_h) / 2.0;
        let track_rect = LRect { left: rect.left, top: track_top, right: rect.right, bottom: track_top + track_h };

        let track_color = if dark { Color::from_rgba8(0x4a, 0x4a, 0x4a, 0xff) } else { Color::from_rgba8(0xd4, 0xd4, 0xd4, 0xff) };
        let fill_color = Color::from_rgba8(0xd9, 0x77, 0x57, 0xff);
        let knob_color = if dark { Color::from_rgba8(0xf0, 0xf0, 0xf0, 0xff) } else { Color::from_rgba8(0xff, 0xff, 0xff, 0xff) };
        let knob_border = if dark { Color::from_rgba8(0x20, 0x20, 0x20, 0xff) } else { Color::from_rgba8(0xa0, 0xa0, 0xa0, 0xff) };

        if let Some(r) = track_rect.to_skia() {
            canvas.fill_rounded_rect(r, track_h / 2.0, track_color);
        }

        let knob_x = self.value_to_x(rect);
        if knob_x > track_rect.left {
            let fill_rect = LRect {
                left: track_rect.left,
                top: track_rect.top,
                right: knob_x.min(track_rect.right),
                bottom: track_rect.bottom,
            };
            if let Some(r) = fill_rect.to_skia() {
                canvas.fill_rounded_rect(r, track_h / 2.0, fill_color);
            }
        }

        let knob_r = (full_h / 2.0).max(track_h);
        let knob_cy = rect.top + full_h / 2.0;
        // GDI drew the knob as a bordered ellipse (a 1px pen stroke around a filled brush); the
        // same look here is a slightly larger border-colored circle behind a smaller
        // knob-colored one on top -- `Canvas::fill_circle` has no stroke-only mode of its own.
        canvas.fill_circle(knob_x, knob_cy, knob_r, knob_border);
        canvas.fill_circle(knob_x, knob_cy, (knob_r - 1.0).max(0.0), knob_color);
    }

    fn on_mouse(&mut self, msg: MouseMsg, x: f32, y: f32, rect: LRect) -> Option<ControlEvent> {
        let _ = y;
        match msg {
            MouseMsg::Down => {
                self.dragging = true;
                self.value = self.pos_to_value(x, rect);
                Some(ControlEvent::Changed)
            }
            MouseMsg::Move => {
                if self.dragging {
                    self.value = self.pos_to_value(x, rect);
                    Some(ControlEvent::Changed)
                } else {
                    None
                }
            }
            MouseMsg::Up => {
                if self.dragging {
                    self.dragging = false;
                    self.value = self.pos_to_value(x, rect);
                    Some(ControlEvent::CommitPreview)
                } else {
                    None
                }
            }
        }
    }
}

// --- RgbaPicker layout constants -- exact values ported from
// `ccum-windows/src/controls.rs`'s own constants of the same name. ---

const SWATCH_SIZE: f32 = 28.0;
const LABEL_WIDTH: f32 = 14.0;
const HEX_GAP: f32 = 8.0;
const CHECKER_CELL: f32 = 6.0;
const RGBA_ARROW_ZONE: f32 = 18.0;
const POPOVER_PAD: f32 = 4.0;
const POPOVER_GRID_COLS: i32 = 5;
const POPOVER_SWATCH_CELL: f32 = 24.0;
const POPOVER_SWATCH_GAP: f32 = 4.0;
const POPOVER_CUSTOM_ROW_HEIGHT: f32 = 24.0;
const POPOVER_SLIDER_ROW_HEIGHT: f32 = 22.0;

/// Curated quick-swatch palette -- byte-for-byte the same 10 entries (and the same
/// brand-accent/amber/red/green/blue/purple/teal/near-white/mid-gray/near-black ordering) as
/// `ccum-windows/src/controls.rs::QUICK_SWATCHES`.
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

fn rgba_to_color(c: Rgba) -> Color {
    Color::from_rgba8(c.r, c.g, c.b, c.a)
}

/// Per-channel linear interpolation (including alpha) from `a` toward `b`. Same role as
/// `render::bars::lerp_color` (not reused directly since it's private to that module) --
/// `tiny_skia::Color` has no built-in `lerp`, unlike `native_interop::Color::lerp` on Windows.
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: f32, y: f32| x + (y - x) * t;
    Color::from_rgba(lerp(a.red(), b.red()), lerp(a.green(), b.green()), lerp(a.blue(), b.blue()), lerp(a.alpha(), b.alpha())).unwrap_or(b)
}

/// Formats an `Rgba` as uppercase `#RRGGBBAA`, matching
/// `ccum-windows/src/native_interop::Color::to_hex`.
fn to_hex(c: Rgba) -> String {
    format!("#{:02X}{:02X}{:02X}{:02X}", c.r, c.g, c.b, c.a)
}

/// A composed color picker, closed by default: a single row shows the swatch, the
/// `#RRGGBBAA` hex readout, and a disclosure arrow. Clicking it opens a popover (below the
/// closed row, extending outside the control's own bounding rect -- same pattern
/// `ccum-windows`'s `Dropdown` uses) offering a curated grid of `QUICK_SWATCHES` plus a
/// "Custom…" row; picking "Custom…" reveals four `Slider`s (R/G/B/A, each ranging over
/// `0..255`) for manual entry. Direct port of `ccum-windows/src/controls.rs::RgbaPicker` --
/// see this module's doc comment for the coordinate-type/mouse-event-model translation, and
/// `on_mouse`'s `MouseMsg::Down` arm below for the outside-click-bounds fix this control's own
/// review cycle found on Windows, preserved here unchanged.
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
    /// The "Custom…" row label. Hardcoded English for now, mirroring `panel.rs`'s own
    /// `SECTIONS` hardcoding -- real i18n (`Strings::rgba_custom`) is a later task, same
    /// phasing `ccum-windows` itself followed (its Task 11 shell also hardcoded English, with
    /// i18n added in its own Task 18).
    custom_label: String,
}

impl RgbaPicker {
    pub fn new(value: Rgba, custom_label: String) -> Self {
        Self {
            r_slider: Slider::new(value.r as f32, 0.0, 255.0),
            g_slider: Slider::new(value.g as f32, 0.0, 255.0),
            b_slider: Slider::new(value.b as f32, 0.0, 255.0),
            a_slider: Slider::new(value.a as f32, 0.0, 255.0),
            value,
            active: None,
            open: false,
            custom_expanded: false,
            custom_label,
        }
    }

    /// The swatch rect: a square anchored at the control's top-left, clamped so it never takes
    /// up more than half the control's height.
    fn swatch_rect(&self, rect: LRect) -> LRect {
        let half_h = (rect.height() / 2.0).max(1.0);
        let size = SWATCH_SIZE.min(half_h).max(12.0_f32.min(half_h));
        LRect { left: rect.left, top: rect.top, right: rect.left + size, bottom: rect.top + size }
    }

    /// Whether client point `(x, y)` falls within `r`, using half-open bounds so adjacent rects
    /// (e.g. the closed row and the popover starting exactly at its bottom edge) never both
    /// claim the same point. Mirrors `Dropdown::point_in`.
    fn point_in(&self, x: f32, y: f32, r: LRect) -> bool {
        x >= r.left && x < r.right && y >= r.top && y < r.bottom
    }

    /// How many rows the quick-swatch grid needs to lay out all of `QUICK_SWATCHES` at
    /// `POPOVER_GRID_COLS` columns per row.
    fn swatch_grid_rows(&self) -> i32 {
        (QUICK_SWATCHES.len() as i32 + POPOVER_GRID_COLS - 1) / POPOVER_GRID_COLS
    }

    /// The quick-swatch grid's rect: directly below the closed `rect`, spanning its width.
    fn swatch_grid_rect(&self, rect: LRect) -> LRect {
        let rows = self.swatch_grid_rows();
        let height = POPOVER_PAD * 2.0 + rows as f32 * POPOVER_SWATCH_CELL + (rows - 1).max(0) as f32 * POPOVER_SWATCH_GAP;
        LRect { left: rect.left, top: rect.bottom, right: rect.right, bottom: rect.bottom + height }
    }

    /// The rect of quick-swatch cell `index` within the grid.
    fn swatch_cell_rect(&self, index: usize, rect: LRect) -> LRect {
        let grid = self.swatch_grid_rect(rect);
        let col = index as i32 % POPOVER_GRID_COLS;
        let row = index as i32 / POPOVER_GRID_COLS;
        let left = grid.left + POPOVER_PAD + col as f32 * (POPOVER_SWATCH_CELL + POPOVER_SWATCH_GAP);
        let top = grid.top + POPOVER_PAD + row as f32 * (POPOVER_SWATCH_CELL + POPOVER_SWATCH_GAP);
        LRect { left, top, right: left + POPOVER_SWATCH_CELL, bottom: top + POPOVER_SWATCH_CELL }
    }

    /// Which quick-swatch cell (if any) contains client point `(x, y)`.
    fn swatch_at(&self, x: f32, y: f32, rect: LRect) -> Option<usize> {
        let grid = self.swatch_grid_rect(rect);
        if !self.point_in(x, y, grid) {
            return None;
        }
        (0..QUICK_SWATCHES.len()).find(|&i| self.point_in(x, y, self.swatch_cell_rect(i, rect)))
    }

    /// The "Custom…" row's rect: directly below the quick-swatch grid, spanning `rect`'s width.
    fn custom_row_rect(&self, rect: LRect) -> LRect {
        let grid = self.swatch_grid_rect(rect);
        LRect { left: rect.left, top: grid.bottom, right: rect.right, bottom: grid.bottom + POPOVER_CUSTOM_ROW_HEIGHT }
    }

    /// The full popover rect: the quick-swatch grid plus the "Custom…" row plus -- while
    /// `custom_expanded` -- the four R/G/B/A slider rows, directly below the closed `rect`.
    /// Public: the Appearance section layout (`render::appearance`) needs this to reflow rows
    /// below an open picker.
    pub fn popover_rect(&self, rect: LRect) -> LRect {
        let custom = self.custom_row_rect(rect);
        let sliders_h = if self.custom_expanded { POPOVER_SLIDER_ROW_HEIGHT * 4.0 } else { 0.0 };
        LRect { left: rect.left, top: rect.bottom, right: rect.right, bottom: custom.bottom + sliders_h }
    }

    /// The popover's height alone (independent of `rect`'s position/width). Lets a caller doing
    /// vertical layout ask "how much extra space does this picker's open popover need?" without
    /// constructing a throwaway anchor rect itself.
    pub fn popover_height(&self) -> f32 {
        let r = self.popover_rect(LRect { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 });
        r.bottom - r.top
    }

    /// The full-width row (label + slider) for channel `index`, within the popover's revealed
    /// slider section below the "Custom…" row.
    fn popover_slider_row_rect(&self, index: usize, rect: LRect) -> LRect {
        let custom = self.custom_row_rect(rect);
        let top = custom.bottom + POPOVER_SLIDER_ROW_HEIGHT * index as f32;
        LRect { left: rect.left, top, right: rect.right, bottom: top + POPOVER_SLIDER_ROW_HEIGHT }
    }

    /// The slider-only portion of popover channel row `index` (row rect minus the label
    /// column).
    fn popover_slider_slot_rect(&self, index: usize, rect: LRect) -> LRect {
        let row = self.popover_slider_row_rect(index, rect);
        LRect { left: row.left + LABEL_WIDTH, top: row.top, right: row.right, bottom: row.bottom }
    }

    /// Which popover channel row (if any) contains client point `(x, y)`. Only meaningful
    /// while `custom_expanded` (the rows aren't drawn, or hit-testable, otherwise). Mirrors
    /// the label-column exclusion the legacy `row_at` used on Windows (a click on the label
    /// text must not be dispatched to the channel's slider, since `Slider::pos_to_value` would
    /// clamp any `x` left of the sub-slider's rect to its minimum, silently zeroing the
    /// channel).
    fn popover_slider_row_at(&self, x: f32, y: f32, rect: LRect) -> Option<usize> {
        if !self.custom_expanded || x < rect.left + LABEL_WIDTH {
            return None;
        }
        let top0 = self.custom_row_rect(rect).bottom;
        if y < top0 {
            return None;
        }
        let idx = ((y - top0) / POPOVER_SLIDER_ROW_HEIGHT).floor() as i32;
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

    /// Force-closes the popover (and collapses "Custom…" back down), if open, resetting to the
    /// compact closed-row state. Mirrors `Dropdown::close`. A no-op if already closed. Called
    /// by `render::appearance::dispatch_appearance`'s single-open-at-a-time enforcement and by
    /// `panel.rs`'s section-switch handling.
    pub fn close(&mut self) {
        self.open = false;
        self.custom_expanded = false;
    }

    /// Whether the popover is currently open -- the read-only view the Appearance section
    /// layout uses to find which picker (if any) needs reflow room, and to detect open/closed
    /// transitions for single-open-at-a-time enforcement.
    pub fn is_open(&self) -> bool {
        self.open
    }
}

impl Control for RgbaPicker {
    fn draw(&self, canvas: &mut Canvas, text: &mut TextRenderer, rect: LRect, dark: bool) {
        let text_color = if dark { Color::from_rgba8(0xf0, 0xf0, 0xf0, 0xff) } else { Color::from_rgba8(0x20, 0x20, 0x20, 0xff) };
        let border = if dark { Color::from_rgba8(0x55, 0x55, 0x55, 0xff) } else { Color::from_rgba8(0xc0, 0xc0, 0xc0, 0xff) };
        let bg = if dark { Color::from_rgba8(0x33, 0x33, 0x33, 0xff) } else { Color::from_rgba8(0xff, 0xff, 0xff, 0xff) };
        let highlight = Color::from_rgba8(0xd9, 0x77, 0x57, 0xff);

        // Closed row: swatch + hex readout + disclosure arrow. Always drawn, whether or not
        // the popover is open.
        let swatch = self.swatch_rect(rect);
        draw_checkerboard_swatch(canvas, swatch, rgba_to_color(self.value));

        let hex = to_hex(self.value);
        let text_y = swatch.top + (swatch.height() - 12.0) / 2.0;
        text.draw_text(canvas, swatch.right + HEX_GAP, text_y, &hex, 12.0, text_color);

        let arrow = if self.open { "\u{25B2}" } else { "\u{25BC}" };
        let arrow_x = rect.right - RGBA_ARROW_ZONE + (RGBA_ARROW_ZONE - 8.0) / 2.0;
        text.draw_text(canvas, arrow_x, text_y, arrow, 12.0, text_color);

        if !self.open {
            return;
        }

        // Popover: panel background, quick-swatch grid, "Custom…" row, and -- while
        // `custom_expanded` -- the four R/G/B/A slider rows.
        let popover = self.popover_rect(rect);
        if let Some(r) = popover.to_skia() {
            canvas.fill_rounded_rect(r, 4.0, border);
        }
        let inset_popover = LRect { left: popover.left + 1.0, top: popover.top, right: popover.right - 1.0, bottom: popover.bottom - 1.0 };
        if let Some(r) = inset_popover.to_skia() {
            canvas.fill_rounded_rect(r, 4.0, bg);
        }

        for i in 0..QUICK_SWATCHES.len() {
            let cell = self.swatch_cell_rect(i, rect);
            let swatch_color = rgba_to_color(QUICK_SWATCHES[i]);
            if self.value == QUICK_SWATCHES[i] {
                if let Some(r) = cell.to_skia() {
                    canvas.fill_rounded_rect(r, 4.0, highlight);
                }
                let inset = LRect { left: cell.left + 2.0, top: cell.top + 2.0, right: cell.right - 2.0, bottom: cell.bottom - 2.0 };
                if let Some(r) = inset.to_skia() {
                    canvas.fill_rounded_rect(r, 3.0, swatch_color);
                }
            } else if let Some(r) = cell.to_skia() {
                canvas.fill_rounded_rect(r, 4.0, swatch_color);
            }
        }

        let custom_row = self.custom_row_rect(rect);
        let custom_text_y = custom_row.top + (custom_row.height() - 12.0) / 2.0;
        text.draw_text(canvas, custom_row.left + POPOVER_PAD * 2.0, custom_text_y, &self.custom_label, 12.0, text_color);

        if self.custom_expanded {
            const LABELS: [&str; 4] = ["R", "G", "B", "A"];
            for (i, label) in LABELS.iter().enumerate() {
                let row = self.popover_slider_row_rect(i, rect);
                let label_y = row.top + (row.height() - 12.0) / 2.0;
                text.draw_text(canvas, row.left, label_y, label, 12.0, text_color);
                self.slider(i).draw(canvas, text, self.popover_slider_slot_rect(i, rect), dark);
            }
        }
    }

    fn on_mouse(&mut self, msg: MouseMsg, x: f32, y: f32, rect: LRect) -> Option<ControlEvent> {
        match msg {
            MouseMsg::Down => {
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
                    // Quick swatches are always fully opaque; sync the sliders too so a later
                    // "Custom…" open reflects the swatch that was picked.
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
                // around the swatch grid, or the gaps between swatch cells) is a dead zone: a
                // no-op, not a dismissal. Only a click genuinely outside `popover_rect`
                // dismisses the popover -- this is the exact outside-click-bounds fix
                // `ccum-windows/src/controls.rs::RgbaPicker`'s own review cycle found and
                // fixed (an earlier version dismissed on ANY click that didn't hit a specific
                // sub-region, including padding/gaps inside the popover itself); preserved
                // here unchanged.
                if !self.point_in(x, y, self.popover_rect(rect)) {
                    self.close();
                }
                None
            }
            MouseMsg::Move => {
                let idx = self.active?;
                let sub_rect = self.popover_slider_slot_rect(idx, rect);
                let event = self.slider_mut(idx).on_mouse(msg, x, y, sub_rect);
                if event.is_some() {
                    self.sync_value();
                }
                event
            }
            MouseMsg::Up => {
                let idx = self.active.take()?;
                let sub_rect = self.popover_slider_slot_rect(idx, rect);
                let event = self.slider_mut(idx).on_mouse(msg, x, y, sub_rect);
                if event.is_some() {
                    self.sync_value();
                }
                event
            }
        }
    }
}

/// Fills `swatch` with a checkerboard pattern (so alpha transparency is visible) blended with
/// `color` according to its alpha channel, then draws that over a rounded background. Direct
/// port of `ccum-windows/src/controls.rs::draw_checkerboard_swatch`.
fn draw_checkerboard_swatch(canvas: &mut Canvas, swatch: LRect, color: Color) {
    let checker_light = Color::from_rgba8(0xff, 0xff, 0xff, 0xff);
    let checker_dark = Color::from_rgba8(0xc0, 0xc0, 0xc0, 0xff);
    let t = color.alpha();
    let blended_light = lerp_color(checker_light, color, t);
    let blended_dark = lerp_color(checker_dark, color, t);

    if let Some(r) = swatch.to_skia() {
        canvas.fill_rounded_rect(r, 4.0, blended_dark);
    }

    let mut row = 0;
    let mut y = swatch.top;
    while y < swatch.bottom {
        let cell_bottom = (y + CHECKER_CELL).min(swatch.bottom);
        let mut col = 0;
        let mut x = swatch.left;
        while x < swatch.right {
            let cell_right = (x + CHECKER_CELL).min(swatch.right);
            if (row + col) % 2 == 0 {
                let cell = LRect { left: x, top: y, right: cell_right, bottom: cell_bottom };
                if let Some(r) = cell.to_skia() {
                    canvas.fill_rect(r, blended_light);
                }
            }
            col += 1;
            x += CHECKER_CELL;
        }
        row += 1;
        y += CHECKER_CELL;
    }
}

// --- Dropdown layout constants -- exact values ported from `ccum-windows/src/controls.rs`'s
// own constants of the same name. `DROPDOWN_ROW_HEIGHT` is `pub(crate)` (not private like the
// others) so `render::font`'s own tests can compute an open list's row positions without
// duplicating this value -- `Dropdown`'s other layout internals stay private since callers only
// ever need to click through `Control::on_mouse`, but the row height is the one number a test
// genuinely cannot derive any other way (list_rect/row_rect themselves stay private).
pub(crate) const DROPDOWN_ROW_HEIGHT: f32 = 24.0;
const DROPDOWN_MAX_VISIBLE: usize = 6;
const DROPDOWN_TEXT_PAD: f32 = 8.0;
const DROPDOWN_ARROW_ZONE: f32 = 18.0;

/// A closed-by-default combobox: the closed row shows `items[selected]` plus an arrow glyph;
/// clicking it opens a scrollable list of all `items` drawn directly below the closed row.
/// Clicking an item selects it, closes the list, and emits `Changed`. Clicking the closed row
/// again while open collapses it, and clicking anywhere else (outside both the closed row and
/// the open list) also dismisses it without changing `selected` (the standard combobox/`<select>`
/// "outside click cancels" convention). Direct port of `ccum-windows/src/controls.rs::Dropdown`.
///
/// # Outside-click handling (Task 11's own verification concern)
///
/// Like `RgbaPicker`, `Dropdown`'s valid interactive area extends *below* its own `rect` while
/// open -- the dropped-down item list, via `list_rect`. `ccum-windows/src/config_window.rs`'s
/// `dispatch_font` explicitly documents (in its own comment) that it skips the section-level
/// `dispatch_hit` gate for `Dropdown` specifically, for exactly that reason: gating a click on
/// `rect`'s own closed-row bounds would wrongly reject a click on an open list item below it.
/// `render::font::dispatch_font` (Task 11) replicates that same skip, with the same reasoning
/// preserved in its own comment -- see that function. This works because `Dropdown::on_mouse`
/// below already fully self-validates every hit via `point_in`/`item_at`, exactly mirroring
/// `RgbaPicker::on_mouse`'s identical self-validating contract (see this module's `RgbaPicker`
/// doc comment) -- routing every message to it unconditionally is safe.
pub struct Dropdown {
    pub items: Vec<String>,
    pub selected: usize,
    open: bool,
    /// Index of the first item currently visible in the open list.
    scroll: usize,
}

impl Dropdown {
    pub fn new(items: Vec<String>, selected: usize) -> Self {
        let selected = if items.is_empty() { 0 } else { selected.min(items.len() - 1) };
        Self { items, selected, open: false, scroll: 0 }
    }

    /// How many rows the open list shows at once (never more than the item count).
    fn visible_count(&self) -> usize {
        self.items.len().min(DROPDOWN_MAX_VISIBLE)
    }

    /// The largest valid `scroll` value: any further would leave blank rows at the bottom.
    fn max_scroll(&self) -> usize {
        self.items.len().saturating_sub(self.visible_count())
    }

    /// Adjusts `scroll` so `selected` falls within the visible window; called on open so the
    /// current selection is always in view.
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

    /// The open list's rect: directly below the closed `rect`, spanning its width, with height
    /// sized to the number of currently visible rows (zero when there are no items).
    fn list_rect(&self, rect: LRect) -> LRect {
        let height = DROPDOWN_ROW_HEIGHT * self.visible_count() as f32;
        LRect { left: rect.left, top: rect.bottom, right: rect.right, bottom: rect.bottom + height }
    }

    /// The rect of the visible row at `visible_idx` (0-based within the current scroll window),
    /// relative to the open list computed from the closed `rect`.
    fn row_rect(&self, rect: LRect, visible_idx: usize) -> LRect {
        let list = self.list_rect(rect);
        let top = list.top + DROPDOWN_ROW_HEIGHT * visible_idx as f32;
        LRect { left: list.left, top, right: list.right, bottom: top + DROPDOWN_ROW_HEIGHT }
    }

    /// Whether client point `(x, y)` falls within `r` using half-open bounds, so adjacent rects
    /// (e.g. the closed row and the open list starting exactly at its bottom edge) never both
    /// claim the same point.
    fn point_in(&self, x: f32, y: f32, r: LRect) -> bool {
        x >= r.left && x < r.right && y >= r.top && y < r.bottom
    }

    /// Maps a client point to an item index in the open list, accounting for `scroll`. Returns
    /// `None` when `(x, y)` falls outside the list's horizontal or vertical bounds (including
    /// below the last visible row).
    fn item_at(&self, x: f32, y: f32, rect: LRect) -> Option<usize> {
        let list = self.list_rect(rect);
        if !self.point_in(x, y, list) {
            return None;
        }
        let visible_idx = ((y - list.top) / DROPDOWN_ROW_HEIGHT) as usize;
        let idx = self.scroll + visible_idx;
        if idx < self.items.len() { Some(idx) } else { None }
    }

    /// Force-closes the open list, if any, without changing `selected`. Used by embedders that
    /// need to guarantee a dropdown isn't left visually "stuck open" when something other than
    /// a click on/around the dropdown itself (e.g. navigating away from the section that owns
    /// it) makes the open list stop making sense to show. A no-op if already closed.
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Whether the open list is currently showing. Unlike `RgbaPicker::is_open` (which
    /// `render::appearance::dispatch_appearance` reads to enforce "at most one popover open at
    /// a time" across six sibling pickers), the Font section has only ever one `Dropdown`, so
    /// nothing in `panel.rs`/`render::font` needs to query this outside a test -- `close_dropdown`
    /// closes unconditionally on section-switch, the same way `close_all_popovers` does. Kept
    /// as a public read-only accessor (mirroring `RgbaPicker`'s identical shape) since it's a
    /// natural part of this control's API surface and is exercised directly by this module's
    /// own tests.
    #[allow(dead_code)]
    pub fn is_open(&self) -> bool {
        self.open
    }
}

impl Control for Dropdown {
    fn draw(&self, canvas: &mut Canvas, text: &mut TextRenderer, rect: LRect, dark: bool) {
        let bg = if dark { Color::from_rgba8(0x33, 0x33, 0x33, 0xff) } else { Color::from_rgba8(0xff, 0xff, 0xff, 0xff) };
        let border = if dark { Color::from_rgba8(0x55, 0x55, 0x55, 0xff) } else { Color::from_rgba8(0xc0, 0xc0, 0xc0, 0xff) };
        let text_color = if dark { Color::from_rgba8(0xf0, 0xf0, 0xf0, 0xff) } else { Color::from_rgba8(0x20, 0x20, 0x20, 0xff) };
        let highlight = Color::from_rgba8(0xd9, 0x77, 0x57, 0xff);

        if let Some(r) = rect.to_skia() {
            canvas.fill_rounded_rect(r, 4.0, border);
        }
        let inset_rect = LRect { left: rect.left + 1.0, top: rect.top + 1.0, right: rect.right - 1.0, bottom: rect.bottom - 1.0 };
        if let Some(r) = inset_rect.to_skia() {
            canvas.fill_rounded_rect(r, 4.0, bg);
        }

        let label = self.items.get(self.selected).map(String::as_str).unwrap_or("");
        let label_y = rect.top + (rect.height() - 12.0) / 2.0;
        text.draw_text(canvas, rect.left + DROPDOWN_TEXT_PAD, label_y, label, 12.0, text_color);

        let arrow = if self.open { "\u{25B2}" } else { "\u{25BC}" };
        let arrow_w = text.text_width(arrow, 12.0);
        let arrow_zone_left = rect.right - DROPDOWN_ARROW_ZONE;
        let arrow_x = arrow_zone_left + ((DROPDOWN_ARROW_ZONE - arrow_w) / 2.0).max(0.0);
        text.draw_text(canvas, arrow_x, label_y, arrow, 12.0, text_color);

        if self.open && !self.items.is_empty() {
            let list = self.list_rect(rect);
            if let Some(r) = list.to_skia() {
                canvas.fill_rounded_rect(r, 4.0, border);
            }
            let inset_list = LRect { left: list.left + 1.0, top: list.top, right: list.right - 1.0, bottom: list.bottom - 1.0 };
            if let Some(r) = inset_list.to_skia() {
                canvas.fill_rounded_rect(r, 4.0, bg);
            }

            for visible_idx in 0..self.visible_count() {
                let idx = self.scroll + visible_idx;
                let row = self.row_rect(rect, visible_idx);
                if idx == self.selected {
                    if let Some(r) = row.to_skia() {
                        canvas.fill_rect(r, highlight);
                    }
                }
                let row_y = row.top + (row.height() - 12.0) / 2.0;
                text.draw_text(canvas, row.left + DROPDOWN_TEXT_PAD, row_y, &self.items[idx], 12.0, text_color);
            }
        }
    }

    fn on_mouse(&mut self, msg: MouseMsg, x: f32, y: f32, rect: LRect) -> Option<ControlEvent> {
        if msg != MouseMsg::Down || self.items.is_empty() {
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
            // Outside both the closed row and the open list: dismiss, no value change.
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
const SEGMENTED_GAP: f32 = 2.0;

/// A row of equal-width "pill" buttons, one per `options` entry, with `selected` drawn
/// highlighted. Clicking a pill (other than the one already selected) selects it and emits
/// `Changed`; clicking the already-selected pill is a no-op. Direct port of
/// `ccum-windows/src/controls.rs::Segmented`.
pub struct Segmented {
    pub options: Vec<String>,
    pub selected: usize,
}

impl Segmented {
    pub fn new(options: Vec<String>, selected: usize) -> Self {
        let selected = if options.is_empty() { 0 } else { selected.min(options.len() - 1) };
        Self { options, selected }
    }

    /// The rect of segment `index` within `rect`: `rect`'s width divided evenly into
    /// `options.len()` columns (floored, matching the original's integer-division column
    /// width, so `segment_rect`/`option_at` never disagree on which option owns a given pixel),
    /// with a `SEGMENTED_GAP`-wide gap carved out of each segment's own width (not added on
    /// top), so the row of segments still exactly spans `rect`. The last segment absorbs any
    /// leftover width from the floor division.
    fn segment_rect(&self, index: usize, rect: LRect) -> LRect {
        let n = (self.options.len() as i32).max(1);
        let width = rect.width().max(1.0);
        let col_w = (width / n as f32).floor().max(1.0);
        let left = rect.left + col_w * index as f32;
        let right = if index as i32 == n - 1 { rect.right } else { left + col_w };
        let gap = SEGMENTED_GAP.min((right - left) / 2.0).max(0.0);
        LRect {
            left: left + if index == 0 { 0.0 } else { gap },
            top: rect.top,
            right: right - if index as i32 == n - 1 { 0.0 } else { gap },
            bottom: rect.bottom,
        }
    }

    /// Which option index (if any) contains client point `(x, y)`, using half-open bounds
    /// per-column so a click exactly on a shared boundary resolves to exactly one segment, and
    /// a click above/below `rect` (or when there are no options) resolves to `None`.
    fn option_at(&self, x: f32, y: f32, rect: LRect) -> Option<usize> {
        if self.options.is_empty() || y < rect.top || y >= rect.bottom {
            return None;
        }
        let n = (self.options.len() as i32).max(1);
        let width = rect.width().max(1.0);
        let col_w = (width / n as f32).floor().max(1.0);
        if x < rect.left || x >= rect.right {
            return None;
        }
        let idx = (((x - rect.left) / col_w) as i32).clamp(0, n - 1);
        Some(idx as usize)
    }
}

impl Control for Segmented {
    fn draw(&self, canvas: &mut Canvas, text: &mut TextRenderer, rect: LRect, dark: bool) {
        let bg = if dark { Color::from_rgba8(0x33, 0x33, 0x33, 0xff) } else { Color::from_rgba8(0xe8, 0xe8, 0xe8, 0xff) };
        let selected_bg = Color::from_rgba8(0xd9, 0x77, 0x57, 0xff);
        let text_color = if dark { Color::from_rgba8(0xf0, 0xf0, 0xf0, 0xff) } else { Color::from_rgba8(0x20, 0x20, 0x20, 0xff) };
        let selected_text = Color::from_rgba8(0xff, 0xff, 0xff, 0xff);

        let radius = (rect.height().max(1.0) / 2.0).min(8.0);
        for (i, option) in self.options.iter().enumerate() {
            let seg = self.segment_rect(i, rect);
            let (fill, txt_color) = if i == self.selected { (selected_bg, selected_text) } else { (bg, text_color) };
            if let Some(r) = seg.to_skia() {
                canvas.fill_rounded_rect(r, radius, fill);
            }

            // `DT_CENTER` had no `cosmic-text` equivalent (see `TextRenderer::text_width`'s doc
            // comment) -- measure the label first, then compute a left-aligned offset that
            // centers it within the segment.
            let label_w = text.text_width(option, 12.0);
            let label_x = seg.left + ((seg.width() - label_w) / 2.0).max(0.0);
            let label_y = seg.top + (seg.height() - 12.0) / 2.0;
            text.draw_text(canvas, label_x, label_y, option, 12.0, txt_color);
        }
    }

    fn on_mouse(&mut self, msg: MouseMsg, x: f32, y: f32, rect: LRect) -> Option<ControlEvent> {
        if msg != MouseMsg::Down {
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

/// Layout constants for `Toggle`'s pill switch: overall track size and the padding between the
/// track's edge and the knob circle.
const TOGGLE_TRACK_W: f32 = 36.0;
const TOGGLE_TRACK_H: f32 = 18.0;
const TOGGLE_KNOB_PAD: f32 = 2.0;

/// A simple on/off switch drawn as a pill-shaped track with a circular knob at the left (off)
/// or right (on) end. Clicking within the drawn track (`track_rect(rect)`, which may be
/// narrower than `rect` itself) flips `on` and emits `Changed`; clicks in the empty space of
/// `rect` outside the track are ignored. Direct port of `ccum-windows/src/controls.rs::Toggle`,
/// including the hit-test-against-drawn-track fix that file's own review cycle found (see
/// `on_mouse` below).
pub struct Toggle {
    pub on: bool,
}

impl Toggle {
    pub fn new(on: bool) -> Self {
        Self { on }
    }

    /// The track rect: a `TOGGLE_TRACK_W` x `TOGGLE_TRACK_H` pill vertically centered within
    /// `rect` and anchored to `rect`'s left edge (matching `RgbaPicker::swatch_rect`'s
    /// left-anchored, height-clamped convention), shrunk to fit if `rect` is smaller than the
    /// nominal track size.
    fn track_rect(&self, rect: LRect) -> LRect {
        let avail_h = rect.height().max(1.0);
        let avail_w = rect.width().max(1.0);
        let h = TOGGLE_TRACK_H.min(avail_h);
        // Never narrower than it is tall, but the final `.min(avail_w)` re-clamps in case
        // `.max(h)` (applied after the first `.min(avail_w)`) would otherwise re-widen `w` past
        // the available width for narrow-but-tall rects (e.g. avail_w=1, h=18: without this the
        // naive `min(36,1).max(18)` would overflow to 18).
        let w = TOGGLE_TRACK_W.min(avail_w).max(h).min(avail_w);
        let top = rect.top + (avail_h - h) / 2.0;
        LRect { left: rect.left, top, right: rect.left + w, bottom: top + h }
    }

    /// The knob's bounding rect within `track`: a circle inset by `TOGGLE_KNOB_PAD`, flush with
    /// the track's left edge when off and its right edge when on.
    fn knob_rect(&self, track: LRect) -> LRect {
        let h = track.height().max(1.0);
        let d = (h - TOGGLE_KNOB_PAD * 2.0).max(1.0);
        let left = if self.on { track.right - TOGGLE_KNOB_PAD - d } else { track.left + TOGGLE_KNOB_PAD };
        LRect { left, top: track.top + TOGGLE_KNOB_PAD, right: left + d, bottom: track.top + TOGGLE_KNOB_PAD + d }
    }
}

impl Control for Toggle {
    fn draw(&self, canvas: &mut Canvas, _text: &mut TextRenderer, rect: LRect, dark: bool) {
        let track = self.track_rect(rect);
        let track_color = if self.on {
            Color::from_rgba8(0xd9, 0x77, 0x57, 0xff)
        } else if dark {
            Color::from_rgba8(0x4a, 0x4a, 0x4a, 0xff)
        } else {
            Color::from_rgba8(0xd4, 0xd4, 0xd4, 0xff)
        };
        let knob_color = if dark { Color::from_rgba8(0xf0, 0xf0, 0xf0, 0xff) } else { Color::from_rgba8(0xff, 0xff, 0xff, 0xff) };
        let knob_border = if dark { Color::from_rgba8(0x20, 0x20, 0x20, 0xff) } else { Color::from_rgba8(0xa0, 0xa0, 0xa0, 0xff) };

        if let Some(r) = track.to_skia() {
            canvas.fill_rounded_rect(r, track.height() / 2.0, track_color);
        }

        let knob = self.knob_rect(track);
        // GDI drew the knob as a bordered ellipse; same "bordered circle" look here as
        // `Slider::draw`'s knob -- a slightly larger border-colored circle behind a smaller
        // knob-colored one.
        let kr = (knob.width().max(0.0)) / 2.0;
        let kcx = (knob.left + knob.right) / 2.0;
        let kcy = (knob.top + knob.bottom) / 2.0;
        canvas.fill_circle(kcx, kcy, kr, knob_border);
        canvas.fill_circle(kcx, kcy, (kr - 1.0).max(0.0), knob_color);
    }

    fn on_mouse(&mut self, msg: MouseMsg, x: f32, y: f32, rect: LRect) -> Option<ControlEvent> {
        if msg != MouseMsg::Down {
            return None;
        }
        // Hit-test against the actual drawn track (`track_rect`), not the raw `rect`: when
        // `rect` is wider than the fixed-size track, a click in the visually empty space to the
        // right of the track must not flip `on`. This is the exact hit-test fix
        // `ccum-windows/src/controls.rs::Toggle`'s own review cycle found and fixed, preserved
        // here unchanged.
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

    fn rect(left: f32, top: f32, right: f32, bottom: f32) -> LRect {
        LRect { left, top, right, bottom }
    }

    #[test]
    fn slider_pos_to_value_clamps_to_range() {
        let s = Slider::new(0.0, 0.0, 100.0);
        let r = rect(0.0, 0.0, 200.0, 20.0);
        assert_eq!(s.pos_to_value(-50.0, r), 0.0);
        assert_eq!(s.pos_to_value(250.0, r), 100.0);
        assert_eq!(s.pos_to_value(100.0, r), 50.0);
    }

    #[test]
    fn slider_drag_lifecycle() {
        let mut s = Slider::new(0.0, 0.0, 100.0);
        let r = rect(0.0, 0.0, 200.0, 20.0);
        assert!(matches!(s.on_mouse(MouseMsg::Move, 100.0, 0.0, r), None), "hover without a prior Down must be a no-op");
        assert!(matches!(s.on_mouse(MouseMsg::Down, 0.0, 0.0, r), Some(ControlEvent::Changed)));
        assert_eq!(s.value, 0.0);
        assert!(matches!(s.on_mouse(MouseMsg::Move, 200.0, 0.0, r), Some(ControlEvent::Changed)));
        assert_eq!(s.value, 100.0);
        assert!(matches!(s.on_mouse(MouseMsg::Up, 100.0, 0.0, r), Some(ControlEvent::CommitPreview)));
        assert_eq!(s.value, 50.0);
        // Once released, further Move is a no-op again.
        assert!(matches!(s.on_mouse(MouseMsg::Move, 0.0, 0.0, r), None));
        assert_eq!(s.value, 50.0);
    }

    fn closed_row() -> LRect {
        rect(0.0, 0.0, 200.0, 22.0)
    }

    #[test]
    fn picker_starts_closed() {
        let p = RgbaPicker::new(Rgba { r: 10, g: 20, b: 30, a: 255 }, "Custom…".into());
        assert!(!p.is_open());
    }

    #[test]
    fn click_on_closed_row_opens_popover() {
        let mut p = RgbaPicker::new(Rgba { r: 10, g: 20, b: 30, a: 255 }, "Custom…".into());
        let row = closed_row();
        p.on_mouse(MouseMsg::Down, 5.0, 5.0, row);
        assert!(p.is_open());
    }

    #[test]
    fn click_on_closed_row_again_while_open_closes_it() {
        let mut p = RgbaPicker::new(Rgba { r: 10, g: 20, b: 30, a: 255 }, "Custom…".into());
        let row = closed_row();
        p.on_mouse(MouseMsg::Down, 5.0, 5.0, row);
        assert!(p.is_open());
        p.on_mouse(MouseMsg::Down, 5.0, 5.0, row);
        assert!(!p.is_open());
    }

    #[test]
    fn clicking_a_quick_swatch_sets_value_and_closes() {
        let mut p = RgbaPicker::new(Rgba { r: 10, g: 20, b: 30, a: 255 }, "Custom…".into());
        let row = closed_row();
        p.on_mouse(MouseMsg::Down, 5.0, 5.0, row);
        assert!(p.is_open());
        let cell = p.swatch_cell_rect(0, row);
        let cx = (cell.left + cell.right) / 2.0;
        let cy = (cell.top + cell.bottom) / 2.0;
        let event = p.on_mouse(MouseMsg::Down, cx, cy, row);
        assert!(matches!(event, Some(ControlEvent::Changed)));
        assert_eq!(p.value, QUICK_SWATCHES[0]);
        assert!(!p.is_open(), "picking a quick swatch must close the popover");
    }

    #[test]
    fn clicking_custom_reveals_sliders_without_closing() {
        let mut p = RgbaPicker::new(Rgba { r: 10, g: 20, b: 30, a: 255 }, "Custom…".into());
        let row = closed_row();
        p.on_mouse(MouseMsg::Down, 5.0, 5.0, row);
        let custom = p.custom_row_rect(row);
        let cx = (custom.left + custom.right) / 2.0;
        let cy = (custom.top + custom.bottom) / 2.0;
        p.on_mouse(MouseMsg::Down, cx, cy, row);
        assert!(p.is_open(), "Custom… must NOT close the popover");
        assert!(p.custom_expanded);
    }

    #[test]
    fn dragging_a_revealed_slider_updates_value_and_keeps_popover_open() {
        let mut p = RgbaPicker::new(Rgba { r: 10, g: 20, b: 30, a: 255 }, "Custom…".into());
        let row = closed_row();
        p.on_mouse(MouseMsg::Down, 5.0, 5.0, row);
        let custom = p.custom_row_rect(row);
        p.on_mouse(MouseMsg::Down, (custom.left + custom.right) / 2.0, (custom.top + custom.bottom) / 2.0, row);
        assert!(p.custom_expanded);

        let r_row = p.popover_slider_row_rect(0, row);
        let ry = (r_row.top + r_row.bottom) / 2.0;
        let slot = p.popover_slider_slot_rect(0, row);
        p.on_mouse(MouseMsg::Down, slot.left, ry, row);
        assert!(p.is_open());
        p.on_mouse(MouseMsg::Move, slot.right, ry, row);
        assert!(p.is_open(), "dragging a slider must not close the popover");
        assert_eq!(p.value.r, 255, "dragging the R slider to the far right must saturate red to 255");
        p.on_mouse(MouseMsg::Up, slot.right, ry, row);
        assert!(p.is_open());
    }

    #[test]
    fn click_in_popover_dead_zone_is_a_no_op_not_a_close() {
        // Reproduces the exact bug ccum-windows's RgbaPicker review cycle found and fixed: a
        // click landing inside the popover's own bounds but not on any specific sub-region
        // (here, the padding margin just inside the swatch grid's top-left corner) must be a
        // no-op, not a dismissal.
        let mut p = RgbaPicker::new(Rgba { r: 10, g: 20, b: 30, a: 255 }, "Custom…".into());
        let row = closed_row();
        p.on_mouse(MouseMsg::Down, 5.0, 5.0, row);
        assert!(p.is_open());

        let grid = p.swatch_grid_rect(row);
        // A point just inside the grid's own bounds but inside the POPOVER_PAD margin, not
        // any swatch cell (swatch_at returns None here, but point_in(popover_rect) is true).
        let dead_x = grid.left + 1.0;
        let dead_y = grid.top + 1.0;
        assert!(p.swatch_at(dead_x, dead_y, row).is_none(), "test setup: must actually land in the padding dead zone, not a swatch cell");

        p.on_mouse(MouseMsg::Down, dead_x, dead_y, row);
        assert!(p.is_open(), "a click in the popover's own padding dead zone must not close it");
    }

    #[test]
    fn click_genuinely_outside_popover_closes_it() {
        let mut p = RgbaPicker::new(Rgba { r: 10, g: 20, b: 30, a: 255 }, "Custom…".into());
        let row = closed_row();
        p.on_mouse(MouseMsg::Down, 5.0, 5.0, row);
        assert!(p.is_open());

        let popover = p.popover_rect(row);
        p.on_mouse(MouseMsg::Down, popover.left, popover.bottom + 50.0, row);
        assert!(!p.is_open(), "a click genuinely outside the popover must close it");
    }

    #[test]
    fn close_collapses_custom_expanded_too() {
        let mut p = RgbaPicker::new(Rgba { r: 10, g: 20, b: 30, a: 255 }, "Custom…".into());
        let row = closed_row();
        p.on_mouse(MouseMsg::Down, 5.0, 5.0, row);
        let custom = p.custom_row_rect(row);
        p.on_mouse(MouseMsg::Down, (custom.left + custom.right) / 2.0, (custom.top + custom.bottom) / 2.0, row);
        assert!(p.custom_expanded);
        p.close();
        assert!(!p.is_open());
        assert!(!p.custom_expanded);
    }

    #[test]
    fn popover_height_is_taller_when_custom_expanded() {
        let mut p = RgbaPicker::new(Rgba { r: 10, g: 20, b: 30, a: 255 }, "Custom…".into());
        let closed_h = p.popover_height();
        p.custom_expanded = true;
        let expanded_h = p.popover_height();
        assert!(expanded_h > closed_h);
        assert_eq!(expanded_h - closed_h, POPOVER_SLIDER_ROW_HEIGHT * 4.0);
    }

    // --- Dropdown ---

    fn dropdown_items(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("Item {i}")).collect()
    }

    #[test]
    fn dropdown_item_at_accounts_for_scroll_offset() {
        let dropdown = Dropdown { items: dropdown_items(10), selected: 0, open: true, scroll: 3 };
        let r = rect(0.0, 0.0, 200.0, 24.0);
        let list = dropdown.list_rect(r);

        let mid_row0_y = list.top + DROPDOWN_ROW_HEIGHT / 2.0;
        assert_eq!(dropdown.item_at(100.0, mid_row0_y, r), Some(3));

        let mid_last_row_y = list.top + DROPDOWN_ROW_HEIGHT * 5.0 + DROPDOWN_ROW_HEIGHT / 2.0;
        assert_eq!(dropdown.item_at(100.0, mid_last_row_y, r), Some(8));

        assert_eq!(dropdown.item_at(100.0, list.bottom, r), None);
        assert_eq!(dropdown.item_at(-5.0, mid_row0_y, r), None);
        assert_eq!(dropdown.item_at(500.0, mid_row0_y, r), None);
        assert_eq!(dropdown.item_at(100.0, 0.0, r), None);
    }

    #[test]
    fn dropdown_item_at_clamps_when_scroll_leaves_partial_last_page() {
        let dropdown = Dropdown { items: dropdown_items(7), selected: 0, open: true, scroll: 1 };
        assert_eq!(dropdown.max_scroll(), 1);
        let r = rect(0.0, 0.0, 200.0, 24.0);
        let list = dropdown.list_rect(r);
        let last_row_mid_y = list.top + DROPDOWN_ROW_HEIGHT * 5.0 + DROPDOWN_ROW_HEIGHT / 2.0;
        assert_eq!(dropdown.item_at(100.0, last_row_mid_y, r), Some(6));
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
        let r = rect(0.0, 0.0, 200.0, 24.0);

        let event = dropdown.on_mouse(MouseMsg::Down, 50.0, 12.0, r);
        assert!(event.is_none());
        assert!(dropdown.is_open());

        let list = dropdown.list_rect(r);
        let row2_mid_y = list.top + DROPDOWN_ROW_HEIGHT * 2.0 + DROPDOWN_ROW_HEIGHT / 2.0;
        let event = dropdown.on_mouse(MouseMsg::Down, 50.0, row2_mid_y, r);
        assert!(matches!(event, Some(ControlEvent::Changed)));
        assert_eq!(dropdown.selected, 2);
        assert!(!dropdown.is_open());
    }

    #[test]
    fn dropdown_click_outside_open_list_closes_without_changing_selection() {
        let mut dropdown = Dropdown::new(dropdown_items(10), 4);
        let r = rect(0.0, 0.0, 200.0, 24.0);
        dropdown.on_mouse(MouseMsg::Down, 50.0, 12.0, r);
        assert!(dropdown.is_open());

        let list = dropdown.list_rect(r);
        let event = dropdown.on_mouse(MouseMsg::Down, 50.0, list.bottom + 100.0, r);
        assert!(event.is_none());
        assert!(!dropdown.is_open());
        assert_eq!(dropdown.selected, 4);
    }

    #[test]
    fn dropdown_click_closed_row_while_open_collapses_without_change() {
        let mut dropdown = Dropdown::new(dropdown_items(10), 4);
        let r = rect(0.0, 0.0, 200.0, 24.0);
        dropdown.on_mouse(MouseMsg::Down, 50.0, 12.0, r);
        assert!(dropdown.is_open());

        let event = dropdown.on_mouse(MouseMsg::Down, 50.0, 12.0, r);
        assert!(event.is_none());
        assert!(!dropdown.is_open());
        assert_eq!(dropdown.selected, 4);
    }

    #[test]
    fn dropdown_ignores_clicks_when_no_items() {
        let mut dropdown = Dropdown::new(Vec::new(), 0);
        let r = rect(0.0, 0.0, 200.0, 24.0);
        let event = dropdown.on_mouse(MouseMsg::Down, 50.0, 12.0, r);
        assert!(event.is_none());
        assert!(!dropdown.is_open());
    }

    // --- Segmented ---

    fn segmented_options(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("Opt{i}")).collect()
    }

    #[test]
    fn segmented_option_at_splits_evenly_and_resolves_boundaries() {
        let seg = Segmented::new(segmented_options(3), 0);
        let r = rect(0.0, 0.0, 300.0, 24.0);
        let y = 12.0;

        assert_eq!(seg.option_at(50.0, y, r), Some(0));
        assert_eq!(seg.option_at(150.0, y, r), Some(1));
        assert_eq!(seg.option_at(250.0, y, r), Some(2));

        assert_eq!(seg.option_at(100.0, y, r), Some(1));
        assert_eq!(seg.option_at(200.0, y, r), Some(2));

        assert_eq!(seg.option_at(0.0, y, r), Some(0));
        assert_eq!(seg.option_at(299.0, y, r), Some(2));
        assert_eq!(seg.option_at(300.0, y, r), None);
        assert_eq!(seg.option_at(-1.0, y, r), None);

        assert_eq!(seg.option_at(50.0, r.top - 1.0, r), None);
        assert_eq!(seg.option_at(50.0, r.bottom, r), None);
    }

    #[test]
    fn segmented_option_at_last_segment_absorbs_uneven_remainder() {
        let seg = Segmented::new(segmented_options(3), 0);
        let r = rect(0.0, 0.0, 301.0, 24.0);
        assert_eq!(seg.option_at(300.0, 12.0, r), Some(2));
        assert_eq!(seg.option_at(199.0, 12.0, r), Some(1));
        assert_eq!(seg.option_at(200.0, 12.0, r), Some(2));
    }

    #[test]
    fn segmented_click_selects_and_emits_changed() {
        let mut seg = Segmented::new(segmented_options(3), 0);
        let r = rect(0.0, 0.0, 300.0, 24.0);
        let event = seg.on_mouse(MouseMsg::Down, 250.0, 12.0, r);
        assert!(matches!(event, Some(ControlEvent::Changed)));
        assert_eq!(seg.selected, 2);
    }

    #[test]
    fn segmented_click_on_already_selected_option_is_a_no_op() {
        let mut seg = Segmented::new(segmented_options(3), 1);
        let r = rect(0.0, 0.0, 300.0, 24.0);
        let event = seg.on_mouse(MouseMsg::Down, 150.0, 12.0, r);
        assert!(event.is_none());
        assert_eq!(seg.selected, 1);
    }

    #[test]
    fn segmented_ignores_clicks_when_no_options() {
        let mut seg = Segmented::new(Vec::new(), 0);
        let r = rect(0.0, 0.0, 300.0, 24.0);
        let event = seg.on_mouse(MouseMsg::Down, 150.0, 12.0, r);
        assert!(event.is_none());
        assert_eq!(seg.selected, 0);
    }

    #[test]
    fn segmented_single_option_always_selected_and_click_is_a_no_op() {
        let mut seg = Segmented::new(segmented_options(1), 0);
        let r = rect(0.0, 0.0, 300.0, 24.0);
        assert_eq!(seg.option_at(150.0, 12.0, r), Some(0));
        let event = seg.on_mouse(MouseMsg::Down, 150.0, 12.0, r);
        assert!(event.is_none());
        assert_eq!(seg.selected, 0);
    }

    // --- Toggle ---

    #[test]
    fn toggle_click_flips_state_and_emits_changed() {
        let mut toggle = Toggle::new(false);
        let r = rect(0.0, 0.0, 36.0, 18.0);
        let event = toggle.on_mouse(MouseMsg::Down, 10.0, 9.0, r);
        assert!(matches!(event, Some(ControlEvent::Changed)));
        assert!(toggle.on);

        let event = toggle.on_mouse(MouseMsg::Down, 10.0, 9.0, r);
        assert!(matches!(event, Some(ControlEvent::Changed)));
        assert!(!toggle.on);
    }

    #[test]
    fn toggle_ignores_clicks_outside_rect() {
        let mut toggle = Toggle::new(false);
        let r = rect(0.0, 0.0, 36.0, 18.0);
        assert!(toggle.on_mouse(MouseMsg::Down, 36.0, 9.0, r).is_none());
        assert!(toggle.on_mouse(MouseMsg::Down, -1.0, 9.0, r).is_none());
        assert!(toggle.on_mouse(MouseMsg::Down, 10.0, 18.0, r).is_none());
        assert!(!toggle.on);
    }

    #[test]
    fn toggle_ignores_clicks_past_track_edge_in_wider_rect() {
        let mut toggle = Toggle::new(false);
        let r = rect(0.0, 0.0, 200.0, 18.0);
        let track = toggle.track_rect(r);
        assert_eq!(track.right, TOGGLE_TRACK_W);

        let event = toggle.on_mouse(MouseMsg::Down, 150.0, 9.0, r);
        assert!(event.is_none());
        assert!(!toggle.on);

        let event = toggle.on_mouse(MouseMsg::Down, track.right - 1.0, 9.0, r);
        assert!(matches!(event, Some(ControlEvent::Changed)));
        assert!(toggle.on);
    }

    #[test]
    fn toggle_track_rect_never_exceeds_narrow_rects_width() {
        let toggle = Toggle::new(false);
        let r = rect(0.0, 0.0, 1.0, 100.0);
        let track = toggle.track_rect(r);
        assert!(track.width() <= r.width());
        assert_eq!(track.right, r.right);
    }

    #[test]
    fn toggle_knob_moves_from_left_to_right_edge_when_on() {
        let track = rect(0.0, 0.0, 36.0, 18.0);
        let off = Toggle::new(false);
        let on = Toggle::new(true);
        let off_knob = off.knob_rect(track);
        let on_knob = on.knob_rect(track);
        assert_eq!(off_knob.left, TOGGLE_KNOB_PAD);
        assert_eq!(on_knob.right, track.right - TOGGLE_KNOB_PAD);
        assert!(on_knob.left > off_knob.left);
    }
}
