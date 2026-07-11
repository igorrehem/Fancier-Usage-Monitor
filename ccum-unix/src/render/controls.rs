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
}
