//! Section-level assembly of the Appearance section's controls: 6 `RgbaPicker`s (calm/
//! attention/critical/background/text/divider) plus an opacity `Slider`, laid out in a 2-column
//! grid with reflow-when-open behavior. Direct port of
//! `ccum-windows/src/config_window.rs`'s `AppearanceControls`/`SectionControls::from_settings`
//! (Appearance-only slice)/`appearance_grid`/`appearance_layout`/`draw_appearance_controls`/
//! `dispatch_appearance`/`appearance_row_offset` -- read in full before writing this file.
//!
//! # Porting notes
//!
//! - **No DPI scaling**: `config_window.rs` scales every constant by the window's live DPI
//!   (`scale(px, dpi)`); `ccum-unix` doesn't plumb DPI/scale-factor handling yet, matching
//!   `render::bars`/`panel.rs`'s own documented choice to skip it for now. Every constant below
//!   is used at its raw (96-DPI-baseline) pixel value.
//! - **No preview strip / button bar**: `config_window.rs`'s `content_rect_for`/`split_content`
//!   carve the content panel into a fixed-height live-preview strip above the controls and a
//!   button bar below; Task 9's popup panel shell has neither (out of scope for both that task
//!   and this one). `panel.rs` hands this module a content `area` that's just the sidebar's
//!   right-hand remainder, inset by `CTRL_PAD` on all sides -- no preview-strip subtraction.
//! - **`Settings`, not just `Appearance`**: `dispatch_appearance` takes `&mut Settings` (not
//!   `&mut Appearance`) to stay a direct, mechanical port of the Windows original -- it writes
//!   `draft.appearance.*`, mirroring exactly how `config_window.rs::dispatch_appearance` does.
//!   This also sets up the same `draft: Settings` shape Tasks 11-13 (the remaining sections,
//!   then Save/Cancel/Reset) will need on `Panel`.

use ccum_core::settings::{Appearance, PaletteStops, Rgba, Settings};

use super::controls::{Control, LRect, MouseMsg, RgbaPicker, Slider};
use super::text::TextRenderer;
use super::{Canvas, Color};

// --- Layout constants -- exact values ported from `ccum-windows/src/config_window.rs`'s own
// constants of the same name (96-DPI-baseline pixels, unscaled -- see this module's doc
// comment). ---

const CTRL_ROW_H: f32 = 22.0;
const CTRL_LABEL_W: f32 = 96.0;
const CTRL_CONTROL_GAP: f32 = 10.0;
const CTRL_GROUP_GAP: f32 = 12.0;
const RGBA_LABEL_H: f32 = 16.0;
const RGBA_LABEL_GAP: f32 = 4.0;
const RGBA_ROW_GAP: f32 = 12.0;
const RGBA_COL_GAP: f32 = 16.0;

/// Display labels for `AppearanceControls::pickers`, in the same order as the array itself.
/// Hardcoded English -- see this module's doc comment ("No DPI scaling") sibling reasoning in
/// `controls.rs`'s `custom_label` doc comment for why (same phasing `ccum-windows` itself used).
const PICKER_LABELS: [&str; 6] = ["Calm", "Attention", "Critical", "Background", "Text", "Divider"];
const OPACITY_LABEL: &str = "Opacity";

/// Six `RgbaPicker`s bound to `Appearance`'s color fields, plus the opacity `Slider`. `pickers`
/// order: `[calm, attention, critical, background, text, divider]` -- the first three compose
/// `draft.appearance.palette` (an all-or-nothing `Option<PaletteStops>`; touching any one of
/// them sets all three), the last three are each their own independent `Option<Rgba>` override.
/// Direct port of `ccum-windows/src/config_window.rs::AppearanceControls`.
pub struct AppearanceControls {
    pub pickers: [RgbaPicker; 6],
    pub opacity: Slider,
}

/// Fallback background/text/divider colors mirroring `render::bars::derive_colors`'s hardcoded
/// adaptive defaults -- used purely to seed the `RgbaPicker`'s starting swatch when the
/// corresponding `Appearance` field is `None`, so the picker opens showing what the widget
/// currently actually renders instead of an arbitrary color. Direct port of
/// `ccum-windows/src/config_window.rs::adaptive_default_colors`.
fn adaptive_default_colors(is_dark: bool) -> (Rgba, Rgba, Rgba) {
    let (bg, text, divider) = if is_dark {
        (hex("#1C1C1C"), hex("#888888"), hex("#444444"))
    } else {
        (hex("#F3F3F3"), hex("#404040"), hex("#AAAAAA"))
    };
    (bg, text, divider)
}

fn hex(s: &str) -> Rgba {
    let s = s.trim_start_matches('#');
    let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(0);
    Rgba { r, g, b, a: 0xff }
}

/// Seed values for `draft.appearance.palette`'s calm/attention/critical colors when it's `None`
/// -- a reasonable starting green/amber/red usage-severity gradient for the picker to open
/// with. Direct port of `ccum-windows/src/config_window.rs::default_palette_seed`.
fn default_palette_seed() -> PaletteStops {
    PaletteStops { calm: hex("#4CAF7A"), attention: hex("#E8A33D"), critical: hex("#D9534F") }
}

impl AppearanceControls {
    /// Builds the six pickers + opacity slider from `appearance`, matching
    /// `ccum-windows/src/config_window.rs::SectionControls::from_settings`'s Appearance-only
    /// slice.
    pub fn from_settings(appearance: &Appearance, is_dark: bool) -> Self {
        let (bg_default, text_default, divider_default) = adaptive_default_colors(is_dark);
        let palette = appearance.palette.unwrap_or_else(default_palette_seed);
        let background = appearance.background.unwrap_or(bg_default);
        let text = appearance.text.unwrap_or(text_default);
        let divider = appearance.divider.unwrap_or(divider_default);

        Self {
            pickers: [
                RgbaPicker::new(palette.calm, "Custom…".to_string()),
                RgbaPicker::new(palette.attention, "Custom…".to_string()),
                RgbaPicker::new(palette.critical, "Custom…".to_string()),
                RgbaPicker::new(background, "Custom…".to_string()),
                RgbaPicker::new(text, "Custom…".to_string()),
                RgbaPicker::new(divider, "Custom…".to_string()),
            ],
            opacity: Slider::new(appearance.opacity, 0.0, 1.0),
        }
    }

    /// Force-closes every picker's popover, if any is open -- called on section-switch (see
    /// `panel.rs`) so navigating away while one is open never leaves it rendering pre-expanded
    /// on return. Mirrors `config_window.rs`'s `WM_LBUTTONDOWN` section-nav handler, which does
    /// the same `for picker in &mut state.controls.appearance.pickers { picker.close(); }`
    /// loop inline.
    pub fn close_all_popovers(&mut self) {
        for picker in &mut self.pickers {
            picker.close();
        }
    }
}

/// Extra vertical offset applied to grid row `row` (0-based: rows 0..2 are the picker grid's
/// own rows, row 3 is the "virtual" row just below the grid -- the opacity row) when picker row
/// `open_row` (if any) has its popover open, `popover_h` tall. Direct port of
/// `ccum-windows/src/config_window.rs::appearance_row_offset` -- pure function, split out for
/// direct unit-test coverage independent of rect construction, exactly like the Windows
/// original.
fn appearance_row_offset(row: i32, open_row: Option<i32>, popover_h: f32) -> f32 {
    match open_row {
        Some(open_row) if row > open_row => popover_h,
        _ => 0.0,
    }
}

/// Which `AppearanceControls::pickers` index (if any) currently has its popover open, plus
/// that picker's current popover height (0 if none is open). Direct port of
/// `ccum-windows/src/config_window.rs::appearance_open_state`.
fn appearance_open_state(c: &AppearanceControls) -> (Option<usize>, f32) {
    match c.pickers.iter().position(|p| p.is_open()) {
        Some(i) => (Some(i), c.pickers[i].popover_height()),
        None => (None, 0.0),
    }
}

/// The six `RgbaPicker` cell rects (2 columns x 3 rows), each split into a label strip and a
/// picker body, plus the y just below the grid (where the opacity row starts). Direct port of
/// `ccum-windows/src/config_window.rs::appearance_grid`.
fn appearance_grid(area: LRect, open_index: Option<usize>, popover_h: f32) -> ([LRect; 6], [LRect; 6], f32) {
    let cols = 2;
    let col_gap = RGBA_COL_GAP;
    let row_gap = RGBA_ROW_GAP;
    let cell_w = (area.width() - col_gap * (cols - 1) as f32).max(cols as f32) / cols as f32;
    let label_h = RGBA_LABEL_H;
    let label_gap = RGBA_LABEL_GAP;
    let picker_h = CTRL_ROW_H;
    let cell_h = label_h + label_gap + picker_h;
    let open_row = open_index.map(|i| i as i32 / cols);

    let mut labels = [LRect::default(); 6];
    let mut bodies = [LRect::default(); 6];
    for i in 0..6i32 {
        let col = i % cols;
        let row = i / cols;
        let offset = appearance_row_offset(row, open_row, popover_h);
        let left = area.left + col as f32 * (cell_w + col_gap);
        let top = area.top + row as f32 * (cell_h + row_gap) + offset;
        labels[i as usize] = LRect { left, top, right: left + cell_w, bottom: top + label_h };
        bodies[i as usize] = LRect { left, top: top + label_h + label_gap, right: left + cell_w, bottom: top + cell_h };
    }
    let rows = 3;
    let grid_bottom = area.top + rows as f32 * cell_h + (rows - 1) as f32 * row_gap + appearance_row_offset(rows, open_row, popover_h);
    (labels, bodies, grid_bottom)
}

/// Full Appearance section layout: the six picker cells plus the opacity row below them.
/// Direct port of `ccum-windows/src/config_window.rs::appearance_layout` (minus that function's
/// `RowCursor`/DPI plumbing -- see this module's doc comment).
fn appearance_layout(area: LRect, open_index: Option<usize>, popover_h: f32) -> ([LRect; 6], [LRect; 6], LRect, LRect) {
    let (labels, bodies, grid_bottom) = appearance_grid(area, open_index, popover_h);
    let y = grid_bottom + CTRL_GROUP_GAP;
    let opacity_label = LRect { left: area.left, top: y, right: area.left + CTRL_LABEL_W, bottom: y + CTRL_ROW_H };
    let opacity_control = LRect { left: area.left + CTRL_LABEL_W + CTRL_CONTROL_GAP, top: y, right: area.right, bottom: y + CTRL_ROW_H };
    (labels, bodies, opacity_label, opacity_control)
}

/// Draws a left-aligned field label (e.g. "Opacity"), matching the muted/foreground text color
/// convention `ccum-windows/src/config_window.rs::draw_field_label` uses.
fn draw_field_label(canvas: &mut Canvas, text: &mut TextRenderer, rect: LRect, dark: bool, label: &str) {
    let color = if dark { Color::from_rgba8(0xd0, 0xd0, 0xd0, 0xff) } else { Color::from_rgba8(0x30, 0x30, 0x30, 0xff) };
    let y = rect.top + (rect.height() - 12.0) / 2.0;
    text.draw_text(canvas, rect.left, y, label, 12.0, color);
}

/// Draws the whole Appearance section's controls into `area`. Direct port of
/// `ccum-windows/src/config_window.rs::draw_appearance_controls`.
pub fn draw_appearance_controls(canvas: &mut Canvas, text: &mut TextRenderer, area: LRect, dark: bool, c: &AppearanceControls) {
    let (open_index, popover_h) = appearance_open_state(c);
    let (labels, bodies, opacity_label, opacity_control) = appearance_layout(area, open_index, popover_h);
    for i in 0..6 {
        draw_field_label(canvas, text, labels[i], dark, PICKER_LABELS[i]);
        // `bodies[i]` is only ever the picker's own closed row -- `RgbaPicker::draw` anchors
        // its popover to `bodies[i].bottom` itself (via `popover_rect`), so the space the
        // reflow above reserved for it is exactly where the popover lands; no taller rect
        // needed here.
        c.pickers[i].draw(canvas, text, bodies[i], dark);
    }
    draw_field_label(canvas, text, opacity_label, dark, OPACITY_LABEL);
    c.opacity.draw(canvas, text, opacity_control, dark);
}

/// Gates a dispatch call so a control only *starts* an interaction (`MouseMsg::Down`) when the
/// click actually landed within its own `rect`. Direct port of
/// `ccum-windows/src/config_window.rs::dispatch_hit`.
fn dispatch_hit(msg: MouseMsg, x: f32, y: f32, rect: LRect) -> bool {
    if msg != MouseMsg::Down {
        return true;
    }
    x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
}

/// Routes a mouse message to the Appearance section's controls and, if any control reports a
/// value change, syncs the corresponding `draft.appearance` field(s). Returns whether anything
/// changed. Direct port of `ccum-windows/src/config_window.rs::dispatch_appearance`.
pub fn dispatch_appearance(c: &mut AppearanceControls, draft: &mut Settings, area: LRect, msg: MouseMsg, x: f32, y: f32) -> bool {
    let (open_index, popover_h) = appearance_open_state(c);
    let (_labels, bodies, _opacity_label, opacity_control) = appearance_layout(area, open_index, popover_h);
    let mut changed = false;
    for i in 0..6 {
        // No `dispatch_hit` gate here: `RgbaPicker` already fully self-validates every hit
        // (`point_in`/`swatch_at`/`popover_slider_row_at`), and its valid area legitimately
        // extends *below* `bodies[i]` while open (the quick-swatch grid / "Custom…" sliders),
        // so gating on `bodies[i]`'s own bounds would wrongly reject a click on an open
        // popover. This also gives "outside click closes the popover" for free: since grid
        // cells never overlap, clicking a *different* picker's row is always outside the
        // currently-open picker's own rect+popover, so it self-closes via its own `on_mouse`
        // right here in this same loop.
        if c.pickers[i].on_mouse(msg, x, y, bodies[i]).is_some() {
            changed = true;
        }
    }
    // Enforce at most one open popover at a time as a defensive backstop.
    if let Some(open_i) = c.pickers.iter().position(|p| p.is_open()) {
        for (j, p) in c.pickers.iter_mut().enumerate() {
            if j != open_i {
                p.close();
            }
        }
    }
    if dispatch_hit(msg, x, y, opacity_control) && c.opacity.on_mouse(msg, x, y, opacity_control).is_some() {
        changed = true;
    }
    if changed {
        draft.appearance.palette = Some(PaletteStops { calm: c.pickers[0].value, attention: c.pickers[1].value, critical: c.pickers[2].value });
        draft.appearance.background = Some(c.pickers[3].value);
        draft.appearance.text = Some(c.pickers[4].value);
        draft.appearance.divider = Some(c.pickers[5].value);
        draft.appearance.opacity = c.opacity.value.clamp(0.0, 1.0);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: f32, top: f32, right: f32, bottom: f32) -> LRect {
        LRect { left, top, right, bottom }
    }

    #[test]
    fn appearance_row_offset_pure_math() {
        assert_eq!(appearance_row_offset(0, None, 100.0), 0.0);
        assert_eq!(appearance_row_offset(0, Some(1), 100.0), 0.0, "row at or above open_row is unaffected");
        assert_eq!(appearance_row_offset(1, Some(1), 100.0), 0.0, "the open picker's own row doesn't grow");
        assert_eq!(appearance_row_offset(2, Some(1), 100.0), 100.0, "rows strictly below the open row shift down");
        assert_eq!(appearance_row_offset(3, Some(1), 100.0), 100.0, "the virtual opacity row also shifts");
    }

    #[test]
    fn appearance_grid_produces_six_non_overlapping_cells_within_area() {
        let area = rect(0.0, 0.0, 400.0, 300.0);
        let (labels, bodies, grid_bottom) = appearance_grid(area, None, 0.0);
        for i in 0..6 {
            assert!(labels[i].left >= area.left && bodies[i].right <= area.right);
            assert!(labels[i].top < bodies[i].bottom);
        }
        // Column 0/1 don't overlap horizontally.
        assert!(bodies[0].right <= bodies[1].left);
        // Row 0/1 don't overlap vertically.
        assert!(bodies[1].bottom <= bodies[2].top);
        assert!(grid_bottom > 0.0);
    }

    #[test]
    fn appearance_grid_reflows_rows_below_an_open_picker() {
        let area = rect(0.0, 0.0, 400.0, 300.0);
        let (_labels_closed, bodies_closed, bottom_closed) = appearance_grid(area, None, 0.0);
        let (_labels_open, bodies_open, bottom_open) = appearance_grid(area, Some(0), 150.0);
        // Picker 0 (row 0) itself doesn't move.
        assert_eq!(bodies_closed[0].top, bodies_open[0].top);
        // Picker 1 is in the SAME row as picker 0 (row 0, col 1) -- must also not move, or the
        // grid would go jagged.
        assert_eq!(bodies_closed[1].top, bodies_open[1].top);
        // Pickers 2..6 (rows 1-2) shift down by exactly the popover height.
        for i in 2..6 {
            assert_eq!(bodies_open[i].top, bodies_closed[i].top + 150.0);
        }
        assert_eq!(bottom_open, bottom_closed + 150.0);
    }

    #[test]
    fn dispatch_appearance_opening_a_second_picker_closes_the_first() {
        let mut c = AppearanceControls::from_settings(&Appearance::default(), true);
        let mut draft = Settings::default();
        let area = rect(0.0, 0.0, 400.0, 300.0);

        let (_l, bodies, _ol, _oc) = appearance_layout(area, None, 0.0);
        let row0 = bodies[0];
        dispatch_appearance(&mut c, &mut draft, area, MouseMsg::Down, row0.left + 1.0, row0.top + 1.0);
        assert!(c.pickers[0].is_open());

        // Recompute layout now that picker 0 is open (rows below it have reflowed) and click
        // picker 1's row -- same row as picker 0, so its position is unaffected by the reflow.
        let (open_index, popover_h) = appearance_open_state(&c);
        let (_l2, bodies2, _ol2, _oc2) = appearance_layout(area, open_index, popover_h);
        let row1 = bodies2[1];
        dispatch_appearance(&mut c, &mut draft, area, MouseMsg::Down, row1.left + 1.0, row1.top + 1.0);

        assert!(c.pickers[1].is_open());
        assert!(!c.pickers[0].is_open(), "opening a second picker must close the first");
    }

    #[test]
    fn dispatch_appearance_picking_a_swatch_writes_back_into_draft() {
        let mut c = AppearanceControls::from_settings(&Appearance::default(), true);
        let mut draft = Settings::default();
        let area = rect(0.0, 0.0, 400.0, 300.0);

        let (_l, bodies, _ol, _oc) = appearance_layout(area, None, 0.0);
        let row0 = bodies[0];
        dispatch_appearance(&mut c, &mut draft, area, MouseMsg::Down, row0.left + 1.0, row0.top + 1.0);
        assert!(c.pickers[0].is_open());

        let (open_index, popover_h) = appearance_open_state(&c);
        let (_l2, bodies2, _ol2, _oc2) = appearance_layout(area, open_index, popover_h);

        // Click the first quick-swatch cell directly via the picker's own grid math (mirrors
        // `controls.rs`'s own tests, since `swatch_cell_rect` isn't exposed outside the
        // module -- instead click a point inside the grid area known to land on cell 0: the
        // grid starts at `bodies2[0].bottom`, cell 0 is the top-left cell with `POPOVER_PAD`
        // inset).
        let grid_top = bodies2[0].bottom;
        let cx = bodies2[0].left + 4.0 + 12.0; // POPOVER_PAD + half of POPOVER_SWATCH_CELL
        let cy = grid_top + 4.0 + 12.0;
        dispatch_appearance(&mut c, &mut draft, area, MouseMsg::Down, cx, cy);

        assert!(!c.pickers[0].is_open(), "picking a swatch closes the popover");
        assert!(draft.appearance.palette.is_some(), "picking calm's swatch must write draft.appearance.palette");
        assert_eq!(draft.appearance.palette.unwrap().calm, c.pickers[0].value);
    }
}
