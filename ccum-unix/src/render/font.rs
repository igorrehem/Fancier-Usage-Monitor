//! Font section: family `Dropdown` + point-size `Slider` + weight `Segmented`, all bound to
//! `draft.typography`. Direct port of `ccum-windows/src/config_window.rs`'s `FontControls`/
//! `font_layout`/`draw_font_controls`/`dispatch_font` -- read those (and this crate's
//! `render::appearance` module doc comment for the shared porting notes: no DPI scaling, a
//! `Settings`-not-just-field `dispatch_*` signature, hardcoded-English labels) before touching
//! this file.

use ccum_core::settings::{Settings, Typography, Weight};

use super::controls::{Control, Dropdown, LRect, MouseMsg, Segmented, Slider};
use super::layout::{dispatch_hit, draw_field_label, RowCursor};
use super::text::TextRenderer;
use super::Canvas;

const FIELD_FAMILY: &str = "Family";
const FIELD_SIZE: &str = "Size";
const FIELD_WEIGHT: &str = "Weight";
const WEIGHT_LABELS: [&str; 3] = ["Regular", "SemiBold", "Bold"];

/// `family`/`size`/`weight`, bound to `Typography`'s three fields. Direct port of
/// `ccum-windows/src/config_window.rs::FontControls`.
pub struct FontControls {
    pub family: Dropdown,
    pub size: Slider,
    pub weight: Segmented,
}

impl FontControls {
    /// `families` is the enumerated font-family list (`TextRenderer::font_families`), expected
    /// pre-sorted/deduplicated; if empty (enumeration found nothing -- shouldn't happen on a
    /// real machine, but keeps `Dropdown`'s never-empty-but-still-showing-a-selection invariant
    /// intact), falls back to a single-item list containing the current family, mirroring
    /// `ccum-windows/src/config_window.rs::SectionControls::from_settings`'s own
    /// `if families.is_empty()` fallback.
    pub fn from_settings(typography: &Typography, mut families: Vec<String>) -> Self {
        if families.is_empty() {
            families.push(typography.family.clone());
        }
        let family_idx = families.iter().position(|f| f == &typography.family).unwrap_or(0);
        let weight_idx = match typography.weight {
            Weight::Regular => 0,
            Weight::SemiBold => 1,
            Weight::Bold => 2,
        };
        Self {
            family: Dropdown::new(families, family_idx),
            // 6..18pt: comfortably brackets the 9.0pt default without allowing degenerate/
            // illegible sizes -- same range `ccum-windows` uses.
            size: Slider::new(typography.size_pt, 6.0, 18.0),
            weight: Segmented::new(WEIGHT_LABELS.iter().map(|s| s.to_string()).collect(), weight_idx),
        }
    }

    /// Force-closes the family dropdown's open list, if any -- called on section-switch (see
    /// `panel.rs`), mirroring `AppearanceControls::close_all_popovers`'s reasoning: navigating
    /// away while the list is open must never leave it rendering pre-expanded on return.
    pub fn close_dropdown(&mut self) {
        self.family.close();
    }
}

fn font_layout(area: LRect) -> (LRect, LRect, LRect, LRect, LRect, LRect) {
    let mut cursor = RowCursor::new(area);
    let (family_label, family_control) = cursor.row();
    let (size_label, size_control) = cursor.row();
    let (weight_label, weight_control) = cursor.row();
    (family_label, family_control, size_label, size_control, weight_label, weight_control)
}

pub fn draw_font_controls(canvas: &mut Canvas, text: &mut TextRenderer, area: LRect, dark: bool, c: &FontControls) {
    let (fl, fc, sl, sctrl, wl, wc) = font_layout(area);
    draw_field_label(canvas, text, fl, dark, FIELD_FAMILY);
    c.family.draw(canvas, text, fc, dark);
    draw_field_label(canvas, text, sl, dark, FIELD_SIZE);
    c.size.draw(canvas, text, sctrl, dark);
    draw_field_label(canvas, text, wl, dark, FIELD_WEIGHT);
    c.weight.draw(canvas, text, wc, dark);
}

/// Routes a mouse message to the Font section's controls and, if any control reports a value
/// change, syncs the corresponding `draft.typography` field(s). Returns whether anything
/// changed. Direct port of `ccum-windows/src/config_window.rs::dispatch_font`.
pub fn dispatch_font(c: &mut FontControls, draft: &mut Settings, area: LRect, msg: MouseMsg, x: f32, y: f32) -> bool {
    let (_fl, fc, _sl, sctrl, _wl, wc) = font_layout(area);
    let mut changed = false;
    // No `dispatch_hit` gate here: `Dropdown` already fully self-validates every hit
    // (`point_in`/`item_at`), and -- unlike every other control in this section -- its valid
    // area legitimately extends *below* `fc` while open (the dropped-down item list), so gating
    // on `fc`'s own bounds would wrongly reject a click on an open list item. Exact same
    // reasoning `ccum-windows/src/config_window.rs::dispatch_font` documents for this same skip
    // (see `render::controls::Dropdown`'s doc comment, "Outside-click handling").
    if c.family.on_mouse(msg, x, y, fc).is_some() {
        changed = true;
        draft.typography.family = c.family.items[c.family.selected].clone();
    }
    if dispatch_hit(msg, x, y, sctrl) && c.size.on_mouse(msg, x, y, sctrl).is_some() {
        changed = true;
        draft.typography.size_pt = c.size.value;
    }
    if dispatch_hit(msg, x, y, wc) && c.weight.on_mouse(msg, x, y, wc).is_some() {
        changed = true;
        draft.typography.weight = match c.weight.selected {
            1 => Weight::SemiBold,
            2 => Weight::Bold,
            _ => Weight::Regular,
        };
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccum_core::settings::Settings;

    fn rect(left: f32, top: f32, right: f32, bottom: f32) -> LRect {
        LRect { left, top, right, bottom }
    }

    fn area() -> LRect {
        rect(0.0, 0.0, 400.0, 300.0)
    }

    #[test]
    fn from_settings_falls_back_to_current_family_when_enumeration_is_empty() {
        let mut typography = Typography::default();
        typography.family = "Consolas".to_string();
        let c = FontControls::from_settings(&typography, Vec::new());
        assert_eq!(c.family.items, vec!["Consolas".to_string()]);
        assert_eq!(c.family.selected, 0);
    }

    #[test]
    fn from_settings_selects_the_current_family_within_a_real_list() {
        let mut typography = Typography::default();
        typography.family = "Segoe UI".to_string();
        let families = vec!["Arial".to_string(), "Segoe UI".to_string(), "Verdana".to_string()];
        let c = FontControls::from_settings(&typography, families);
        assert_eq!(c.family.selected, 1);
    }

    #[test]
    fn dispatch_font_family_selection_writes_back_into_draft() {
        let typography = Typography::default();
        let families = vec!["Arial".to_string(), "Segoe UI".to_string(), "Verdana".to_string()];
        let mut c = FontControls::from_settings(&typography, families);
        let mut draft = Settings::default();
        let a = area();

        let (_fl, fc, _sl, _sc, _wl, _wc) = font_layout(a);
        // Open the dropdown, then pick the 3rd visible item ("Verdana"). The open list starts
        // directly below `fc` (`Dropdown::list_rect`'s own formula, mirrored here rather than
        // called since that method is private -- `on_mouse` is the only public entry point a
        // real caller has too).
        dispatch_font(&mut c, &mut draft, a, MouseMsg::Down, fc.left + 1.0, fc.top + 1.0);
        assert!(c.family.is_open());
        use super::super::controls::DROPDOWN_ROW_HEIGHT;
        let row2_mid_y = fc.bottom + DROPDOWN_ROW_HEIGHT * 2.0 + DROPDOWN_ROW_HEIGHT / 2.0;
        dispatch_font(&mut c, &mut draft, a, MouseMsg::Down, fc.left + 1.0, row2_mid_y);

        assert_eq!(draft.typography.family, "Verdana");
        assert!(!c.family.is_open());
    }

    #[test]
    fn dispatch_font_size_slider_writes_back_into_draft() {
        let typography = Typography::default();
        let mut c = FontControls::from_settings(&typography, vec!["Segoe UI".to_string()]);
        let mut draft = Settings::default();
        let a = area();
        let (_fl, _fc, _sl, sctrl, _wl, _wc) = font_layout(a);

        dispatch_font(&mut c, &mut draft, a, MouseMsg::Down, sctrl.right, sctrl.top + 1.0);
        assert_eq!(draft.typography.size_pt, c.size.value);
        assert!((6.0..=18.0).contains(&draft.typography.size_pt));
    }

    #[test]
    fn dispatch_font_weight_segment_writes_back_into_draft() {
        let typography = Typography::default();
        let mut c = FontControls::from_settings(&typography, vec!["Segoe UI".to_string()]);
        let mut draft = Settings::default();
        let a = area();
        let (_fl, _fc, _sl, _sc, _wl, wc) = font_layout(a);

        // Click the 3rd pill ("Bold").
        let x = wc.left + wc.width() * 5.0 / 6.0;
        dispatch_font(&mut c, &mut draft, a, MouseMsg::Down, x, wc.top + 1.0);
        assert_eq!(draft.typography.weight, Weight::Bold);
    }

    #[test]
    fn close_dropdown_closes_an_open_family_list() {
        let typography = Typography::default();
        let mut c = FontControls::from_settings(&typography, vec!["Segoe UI".to_string(), "Arial".to_string()]);
        let a = area();
        let (_fl, fc, ..) = font_layout(a);
        c.family.on_mouse(MouseMsg::Down, fc.left + 1.0, fc.top + 1.0, fc);
        assert!(c.family.is_open());
        c.close_dropdown();
        assert!(!c.family.is_open());
    }
}
