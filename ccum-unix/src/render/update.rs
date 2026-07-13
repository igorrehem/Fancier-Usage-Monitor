//! Update section: a poll-frequency `Segmented` (four presets + "Custom") plus a custom-minutes
//! `Slider`, both bound to `draft.poll_interval_ms`. Direct port of
//! `ccum-windows/src/config_window.rs`'s `UpdateControls`/`FREQ_PRESETS_MS`/`frequency_labels`/
//! `frequency_selection`/`update_layout`/`draw_update_controls`/`dispatch_update` -- read those
//! (and `render::appearance`'s module doc comment for the shared porting notes) before touching
//! this file.
//!
//! # Scope note (Task 11 brief)
//!
//! `ccum-windows`'s own `config_window.rs` has a "Task-17-era frequency-sync" concern where the
//! settings window's frequency controls must stay consistent with the real widget's tray-menu/
//! poll-timer state. That integration doesn't exist on `ccum-unix` yet (there is no real
//! menu/timer wiring for this popup to sync against -- `panel.rs`'s `draft` is a private working
//! copy, same as `render::appearance`'s). Per this task's brief, this section only needs to
//! read/write `draft.poll_interval_ms` correctly within the popup itself; no cross-window sync
//! is implemented (or needed) here.

use ccum_core::settings::Settings;

use super::controls::{Control, LRect, MouseMsg, Segmented, Slider};
use super::layout::{dispatch_hit, draw_field_label, RowCursor};
use super::text::TextRenderer;
use super::Canvas;

const FIELD_FREQUENCY: &str = "Frequency";
const FIELD_CUSTOM_MINUTES: &str = "Custom (min)";

/// Preset frequencies mirroring `ccum-windows/src/window.rs`'s menu options (1/5/15 minutes,
/// 1 hour), duplicated here as plain values rather than importing anything Windows-specific --
/// same duplication `ccum-windows/src/config_window.rs::FREQ_PRESETS_MS` itself already does
/// relative to `window.rs`'s private `IDM_FREQ_*` consts.
const FREQ_PRESETS_MS: [u32; 4] = [60_000, 300_000, 900_000, 3_600_000];

/// Frequency segment labels: the four presets plus "Custom". Hardcoded English, same phasing as
/// every other hardcoded label in this crate's `render` module (see `render::appearance`'s doc
/// comment) -- real i18n is a later task.
const FREQUENCY_LABELS: [&str; 5] = ["1 Minute", "5 Minutes", "15 Minutes", "1 Hour", "Custom"];

/// Direct port of `ccum-windows/src/config_window.rs::UpdateControls`.
pub struct UpdateControls {
    pub frequency: Segmented,
    /// Whole minutes, not milliseconds -- friendlier to drag than a 60,000-wide range.
    pub custom_minutes: Slider,
}

/// Maps `poll_interval_ms` to (segment index, custom-slider minutes). Falls back to the
/// "Custom" segment (index 4) with the equivalent minute count whenever the value doesn't
/// exactly match one of `FREQ_PRESETS_MS`. Direct port of
/// `ccum-windows/src/config_window.rs::frequency_selection`.
fn frequency_selection(poll_interval_ms: u32) -> (usize, f32) {
    let minutes = (poll_interval_ms as f32 / 60_000.0).max(1.0);
    match FREQ_PRESETS_MS.iter().position(|&p| p == poll_interval_ms) {
        Some(idx) => (idx, minutes),
        None => (4, minutes),
    }
}

impl UpdateControls {
    pub fn from_settings(poll_interval_ms: u32) -> Self {
        let (freq_idx, custom_minutes) = frequency_selection(poll_interval_ms);
        Self {
            frequency: Segmented::new(FREQUENCY_LABELS.iter().map(|s| s.to_string()).collect(), freq_idx),
            custom_minutes: Slider::new(custom_minutes, 1.0, 240.0),
        }
    }
}

fn update_layout(area: LRect) -> (LRect, LRect, LRect, LRect) {
    let mut cursor = RowCursor::new(area);
    let (freq_label, freq_control) = cursor.row();
    let (custom_label, custom_control) = cursor.row();
    (freq_label, freq_control, custom_label, custom_control)
}

pub fn draw_update_controls(canvas: &mut Canvas, text: &mut TextRenderer, area: LRect, dark: bool, c: &UpdateControls) {
    let (fl, fc, cl, cc) = update_layout(area);
    draw_field_label(canvas, text, fl, dark, FIELD_FREQUENCY);
    c.frequency.draw(canvas, text, fc, dark);
    draw_field_label(canvas, text, cl, dark, FIELD_CUSTOM_MINUTES);
    c.custom_minutes.draw(canvas, text, cc, dark);
}

/// Routes a mouse message to the Update section's controls and, if any reports a value change,
/// syncs `draft.poll_interval_ms`. Returns whether anything changed. Direct port of
/// `ccum-windows/src/config_window.rs::dispatch_update`.
pub fn dispatch_update(c: &mut UpdateControls, draft: &mut Settings, area: LRect, msg: MouseMsg, x: f32, y: f32) -> bool {
    let (_fl, fc, _cl, cc) = update_layout(area);
    let mut changed = false;
    if dispatch_hit(msg, x, y, fc) && c.frequency.on_mouse(msg, x, y, fc).is_some() {
        changed = true;
        if let Some(&ms) = FREQ_PRESETS_MS.get(c.frequency.selected) {
            draft.poll_interval_ms = ms;
            c.custom_minutes.value = (ms as f32 / 60_000.0).max(1.0);
        }
        // Selecting "Custom" (index 4, past FREQ_PRESETS_MS's end) leaves poll_interval_ms
        // untouched until the custom slider itself is dragged.
    }
    if dispatch_hit(msg, x, y, cc) && c.custom_minutes.on_mouse(msg, x, y, cc).is_some() {
        changed = true;
        let minutes = c.custom_minutes.value.round().max(1.0);
        draft.poll_interval_ms = (minutes as u32).saturating_mul(60_000);
        c.frequency.selected = frequency_selection(draft.poll_interval_ms).0;
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
    fn frequency_selection_matches_a_known_preset() {
        assert_eq!(frequency_selection(300_000), (1, 5.0));
    }

    #[test]
    fn frequency_selection_falls_back_to_custom_for_an_unmatched_value() {
        let (idx, minutes) = frequency_selection(120_000);
        assert_eq!(idx, 4);
        assert_eq!(minutes, 2.0);
    }

    #[test]
    fn from_settings_seeds_the_matching_preset_segment() {
        let c = UpdateControls::from_settings(900_000);
        assert_eq!(c.frequency.selected, 2);
        assert_eq!(c.custom_minutes.value, 15.0);
    }

    #[test]
    fn dispatch_update_selecting_a_preset_segment_writes_back_into_draft() {
        let mut c = UpdateControls::from_settings(60_000);
        let mut draft = Settings::default();
        let a = area();
        let (_fl, fc, ..) = update_layout(a);

        // Click the 3rd pill ("15 Minutes").
        let x = fc.left + fc.width() * 5.0 / 10.0;
        let changed = dispatch_update(&mut c, &mut draft, a, MouseMsg::Down, x, fc.top + 1.0);

        assert!(changed);
        assert_eq!(draft.poll_interval_ms, 900_000);
        assert_eq!(c.custom_minutes.value, 15.0);
    }

    #[test]
    fn dispatch_update_dragging_custom_minutes_writes_back_and_resyncs_segment() {
        let mut c = UpdateControls::from_settings(60_000);
        let mut draft = Settings::default();
        let a = area();
        let (_fl, _fc, _cl, cc) = update_layout(a);

        // Drag near the far right (`dispatch_hit`'s bounds are half-open, so the click must
        // land strictly inside `cc`, not exactly on its right edge) -- custom_minutes' range is
        // 1..240, so this lands close to, but not necessarily exactly at, 240 minutes.
        let click_x = cc.right - 1.0;
        let changed = dispatch_update(&mut c, &mut draft, a, MouseMsg::Down, click_x, cc.top + 1.0);
        assert!(changed);

        let expected_minutes = c.custom_minutes.pos_to_value(click_x, cc).round().max(1.0) as u32;
        assert_eq!(draft.poll_interval_ms, expected_minutes.saturating_mul(60_000));
        assert!(expected_minutes >= 239, "expected the drag to land very close to the 240-minute max, got {expected_minutes}");
        // That value matches no preset, so the segment must re-sync to "Custom" (index 4).
        assert_eq!(c.frequency.selected, 4);
    }

    #[test]
    fn dispatch_update_dragging_custom_minutes_to_a_preset_value_resyncs_to_that_preset() {
        let mut c = UpdateControls::from_settings(60_000);
        let mut draft = Settings::default();
        let a = area();
        let (_fl, _fc, _cl, cc) = update_layout(a);

        // custom_minutes ranges 1..240; landing exactly on 1 minute matches FREQ_PRESETS_MS[0].
        dispatch_update(&mut c, &mut draft, a, MouseMsg::Down, cc.left, cc.top + 1.0);

        assert_eq!(draft.poll_interval_ms, 60_000);
        assert_eq!(c.frequency.selected, 0);
    }
}
