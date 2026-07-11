//! Size section: one `Slider` per `Geometry` field, bound to `draft.geometry`. Direct port of
//! `ccum-windows/src/config_window.rs`'s `SizeControls`/`size_layout`/`draw_size_controls`/
//! `dispatch_size` -- read those (and `render::appearance`'s module doc comment for the shared
//! porting notes) before touching this file.
//!
//! `Geometry` (`ccum-core::settings`) has six tunable fields and no field literally named
//! "width" (the widget's total width is *derived* from these, not stored directly) -- this
//! binds all six real fields, exactly mirroring `ccum-windows/src/config_window.rs::SizeControls`'s
//! own doc comment on the same point.

use ccum_core::settings::{Geometry, Settings};

use super::controls::{Control, LRect, MouseMsg, Slider};
use super::layout::{dispatch_hit, draw_field_label, RowCursor};
use super::text::TextRenderer;
use super::Canvas;

const LABELS: [&str; 6] = ["Height", "Corner Radius", "Bar Thickness", "Label Width", "Text Width", "Spacing"];

/// One `Slider` per `Geometry` field, in field order: height, corner_radius, bar_thickness,
/// label_width, text_width, spacing. Direct port of
/// `ccum-windows/src/config_window.rs::SizeControls`.
pub struct SizeControls {
    pub height: Slider,
    pub corner_radius: Slider,
    pub bar_thickness: Slider,
    pub label_width: Slider,
    pub text_width: Slider,
    pub spacing: Slider,
}

impl SizeControls {
    pub fn from_settings(g: &Geometry) -> Self {
        Self {
            height: Slider::new(g.height as f32, 30.0, 80.0),
            corner_radius: Slider::new(g.corner_radius as f32, 0.0, 12.0),
            bar_thickness: Slider::new(g.bar_thickness as f32, 6.0, 24.0),
            label_width: Slider::new(g.label_width as f32, 8.0, 40.0),
            text_width: Slider::new(g.text_width as f32, 30.0, 100.0),
            spacing: Slider::new(g.spacing as f32, 0.0, 6.0),
        }
    }
}

/// Row rects for the six `SizeControls` sliders, in the same field order as `SizeControls`/
/// `LABELS` themselves.
fn size_layout(area: LRect) -> [(LRect, LRect); 6] {
    let mut cursor = RowCursor::new(area);
    [cursor.row(), cursor.row(), cursor.row(), cursor.row(), cursor.row(), cursor.row()]
}

pub fn draw_size_controls(canvas: &mut Canvas, text: &mut TextRenderer, area: LRect, dark: bool, c: &SizeControls) {
    let rows = size_layout(area);
    let sliders = [&c.height, &c.corner_radius, &c.bar_thickness, &c.label_width, &c.text_width, &c.spacing];
    for i in 0..6 {
        draw_field_label(canvas, text, rows[i].0, dark, LABELS[i]);
        sliders[i].draw(canvas, text, rows[i].1, dark);
    }
}

/// Routes a mouse message to the Size section's sliders and, if any reports a value change,
/// syncs `draft.geometry`. Returns whether anything changed. Direct port of
/// `ccum-windows/src/config_window.rs::dispatch_size`.
pub fn dispatch_size(c: &mut SizeControls, draft: &mut Settings, area: LRect, msg: MouseMsg, x: f32, y: f32) -> bool {
    let rows = size_layout(area);
    let sliders = [&mut c.height, &mut c.corner_radius, &mut c.bar_thickness, &mut c.label_width, &mut c.text_width, &mut c.spacing];
    let mut changed = false;
    for i in 0..6 {
        if dispatch_hit(msg, x, y, rows[i].1) && sliders[i].on_mouse(msg, x, y, rows[i].1).is_some() {
            changed = true;
        }
    }
    if changed {
        // Rounded to i32 and floored at a strictly-positive minimum (0 for corner_radius/
        // spacing, 1 elsewhere) -- `Geometry`'s fields drive widget layout math (segment
        // widths, divider positions) that assumes strictly positive sizes for most of them.
        // Exact same clamps `ccum-windows/src/config_window.rs::dispatch_size` applies.
        draft.geometry.height = (c.height.value.round() as i32).max(1);
        draft.geometry.corner_radius = (c.corner_radius.value.round() as i32).max(0);
        draft.geometry.bar_thickness = (c.bar_thickness.value.round() as i32).max(1);
        draft.geometry.label_width = (c.label_width.value.round() as i32).max(1);
        draft.geometry.text_width = (c.text_width.value.round() as i32).max(1);
        draft.geometry.spacing = (c.spacing.value.round() as i32).max(0);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: f32, top: f32, right: f32, bottom: f32) -> LRect {
        LRect { left, top, right, bottom }
    }

    fn area() -> LRect {
        rect(0.0, 0.0, 400.0, 300.0)
    }

    #[test]
    fn size_layout_produces_six_non_overlapping_rows() {
        let rows = size_layout(area());
        for i in 0..5 {
            assert!(rows[i].1.bottom <= rows[i + 1].1.top, "row {i} must not overlap row {}", i + 1);
        }
    }

    #[test]
    fn dispatch_size_dragging_each_slider_writes_back_into_draft() {
        let g = Geometry::default();
        let mut c = SizeControls::from_settings(&g);
        let mut draft = Settings::default();
        let a = area();
        let rows = size_layout(a);

        // Drag every slider to (just within) its rect's right edge -- `dispatch_hit`'s bounds
        // are half-open (`x < rect.right`), so the click must land strictly inside, not exactly
        // on the boundary; a single pixel out of a control column several hundred px wide is
        // negligible next to each slider's own value range, so this still saturates to (and
        // rounds to) each slider's max.
        dispatch_size(&mut c, &mut draft, a, MouseMsg::Down, rows[0].1.right - 1.0, rows[0].1.top + 1.0);
        dispatch_size(&mut c, &mut draft, a, MouseMsg::Down, rows[1].1.right - 1.0, rows[1].1.top + 1.0);
        dispatch_size(&mut c, &mut draft, a, MouseMsg::Down, rows[2].1.right - 1.0, rows[2].1.top + 1.0);
        dispatch_size(&mut c, &mut draft, a, MouseMsg::Down, rows[3].1.right - 1.0, rows[3].1.top + 1.0);
        dispatch_size(&mut c, &mut draft, a, MouseMsg::Down, rows[4].1.right - 1.0, rows[4].1.top + 1.0);
        let changed = dispatch_size(&mut c, &mut draft, a, MouseMsg::Down, rows[5].1.right - 1.0, rows[5].1.top + 1.0);

        assert!(changed);
        assert_eq!(draft.geometry.height, 80);
        assert_eq!(draft.geometry.corner_radius, 12);
        assert_eq!(draft.geometry.bar_thickness, 24);
        assert_eq!(draft.geometry.label_width, 40);
        assert_eq!(draft.geometry.text_width, 100);
        assert_eq!(draft.geometry.spacing, 6);
    }

    #[test]
    fn dispatch_size_ignores_clicks_outside_any_row() {
        let g = Geometry::default();
        let mut c = SizeControls::from_settings(&g);
        let mut draft = Settings::default();
        let a = area();
        let before = draft.geometry;

        // Click in the label column of the first row -- outside every slider's control rect.
        let changed = dispatch_size(&mut c, &mut draft, a, MouseMsg::Down, a.left + 1.0, a.top + 1.0);
        assert!(!changed);
        assert_eq!(draft.geometry, before);
    }
}
