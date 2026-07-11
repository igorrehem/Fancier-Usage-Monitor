//! Animations section: four groups (fill/shimmer/alert_glow/fade_slide), each an on/off
//! `Toggle` plus its numeric `Slider`(s), plus a standalone reduce-motion `Toggle`. All bound to
//! `draft.animation`. Direct port of `ccum-windows/src/config_window.rs`'s `AnimationControls`/
//! `AnimationRects`/`animation_layout`/`draw_animation_controls`/`dispatch_animations` -- read
//! those (and `render::appearance`'s module doc comment for the shared porting notes) before
//! touching this file.
//!
//! `FillAnim::easing` etc. are left unbound, same as the Windows original -- the brief's
//! interfaces line calls for "4 groups of Toggle+Slider(s) + reduce-motion Toggle" only.

use ccum_core::settings::{AnimationSettings, Settings};

use super::controls::{Control, LRect, MouseMsg, Slider, Toggle};
use super::layout::{dispatch_hit, draw_field_label, RowCursor};
use super::text::TextRenderer;
use super::Canvas;

const FIELD_ON: &str = "On";
const FIELD_SPEED: &str = "Speed";
const FIELD_INTENSITY: &str = "Intensity";
const FIELD_THRESHOLD: &str = "Threshold";
const FIELD_DURATION: &str = "Duration";
const FIELD_REDUCE_MOTION: &str = "Reduce Motion";
const GROUP_FILL: &str = "Fill";
const GROUP_SHIMMER: &str = "Shimmer";
const GROUP_ALERT_GLOW: &str = "Alert Glow";
const GROUP_FADE_SLIDE: &str = "Fade / Slide";

/// Direct port of `ccum-windows/src/config_window.rs::AnimationControls`.
pub struct AnimationControls {
    pub fill_on: Toggle,
    pub fill_speed: Slider,
    pub shimmer_on: Toggle,
    pub shimmer_speed: Slider,
    pub shimmer_intensity: Slider,
    pub glow_on: Toggle,
    pub glow_threshold: Slider,
    pub glow_intensity: Slider,
    pub fade_on: Toggle,
    pub fade_duration: Slider,
    pub reduce_motion: Toggle,
}

impl AnimationControls {
    pub fn from_settings(a: &AnimationSettings) -> Self {
        Self {
            fill_on: Toggle::new(a.fill.on),
            fill_speed: Slider::new(a.fill.speed, 0.1, 5.0),
            shimmer_on: Toggle::new(a.shimmer.on),
            shimmer_speed: Slider::new(a.shimmer.speed, 0.1, 3.0),
            shimmer_intensity: Slider::new(a.shimmer.intensity, 0.0, 1.0),
            glow_on: Toggle::new(a.alert_glow.on),
            glow_threshold: Slider::new(a.alert_glow.threshold, 0.0, 1.0),
            glow_intensity: Slider::new(a.alert_glow.intensity, 0.0, 1.0),
            fade_on: Toggle::new(a.fade_slide.on),
            fade_duration: Slider::new(a.fade_slide.duration_ms as f32, 50.0, 1000.0),
            reduce_motion: Toggle::new(a.reduce_motion),
        }
    }
}

struct AnimationRects {
    fill_header: LRect,
    fill_on: (LRect, LRect),
    fill_speed: (LRect, LRect),
    shimmer_header: LRect,
    shimmer_on: (LRect, LRect),
    shimmer_speed: (LRect, LRect),
    shimmer_intensity: (LRect, LRect),
    glow_header: LRect,
    glow_on: (LRect, LRect),
    glow_threshold: (LRect, LRect),
    glow_intensity: (LRect, LRect),
    fade_header: LRect,
    fade_on: (LRect, LRect),
    fade_duration: (LRect, LRect),
    reduce_motion: (LRect, LRect),
}

fn animation_layout(area: LRect) -> AnimationRects {
    let mut cursor = RowCursor::new(area);
    let fill_header = cursor.header();
    let fill_on = cursor.row();
    let fill_speed = cursor.row();
    cursor.group_gap();
    let shimmer_header = cursor.header();
    let shimmer_on = cursor.row();
    let shimmer_speed = cursor.row();
    let shimmer_intensity = cursor.row();
    cursor.group_gap();
    let glow_header = cursor.header();
    let glow_on = cursor.row();
    let glow_threshold = cursor.row();
    let glow_intensity = cursor.row();
    cursor.group_gap();
    let fade_header = cursor.header();
    let fade_on = cursor.row();
    let fade_duration = cursor.row();
    cursor.group_gap();
    let reduce_motion = cursor.row();
    AnimationRects {
        fill_header,
        fill_on,
        fill_speed,
        shimmer_header,
        shimmer_on,
        shimmer_speed,
        shimmer_intensity,
        glow_header,
        glow_on,
        glow_threshold,
        glow_intensity,
        fade_header,
        fade_on,
        fade_duration,
        reduce_motion,
    }
}

pub fn draw_animation_controls(canvas: &mut Canvas, text: &mut TextRenderer, area: LRect, dark: bool, c: &AnimationControls) {
    let r = animation_layout(area);

    draw_field_label(canvas, text, r.fill_header, dark, GROUP_FILL);
    draw_field_label(canvas, text, r.fill_on.0, dark, FIELD_ON);
    c.fill_on.draw(canvas, text, r.fill_on.1, dark);
    draw_field_label(canvas, text, r.fill_speed.0, dark, FIELD_SPEED);
    c.fill_speed.draw(canvas, text, r.fill_speed.1, dark);

    draw_field_label(canvas, text, r.shimmer_header, dark, GROUP_SHIMMER);
    draw_field_label(canvas, text, r.shimmer_on.0, dark, FIELD_ON);
    c.shimmer_on.draw(canvas, text, r.shimmer_on.1, dark);
    draw_field_label(canvas, text, r.shimmer_speed.0, dark, FIELD_SPEED);
    c.shimmer_speed.draw(canvas, text, r.shimmer_speed.1, dark);
    draw_field_label(canvas, text, r.shimmer_intensity.0, dark, FIELD_INTENSITY);
    c.shimmer_intensity.draw(canvas, text, r.shimmer_intensity.1, dark);

    draw_field_label(canvas, text, r.glow_header, dark, GROUP_ALERT_GLOW);
    draw_field_label(canvas, text, r.glow_on.0, dark, FIELD_ON);
    c.glow_on.draw(canvas, text, r.glow_on.1, dark);
    draw_field_label(canvas, text, r.glow_threshold.0, dark, FIELD_THRESHOLD);
    c.glow_threshold.draw(canvas, text, r.glow_threshold.1, dark);
    draw_field_label(canvas, text, r.glow_intensity.0, dark, FIELD_INTENSITY);
    c.glow_intensity.draw(canvas, text, r.glow_intensity.1, dark);

    draw_field_label(canvas, text, r.fade_header, dark, GROUP_FADE_SLIDE);
    draw_field_label(canvas, text, r.fade_on.0, dark, FIELD_ON);
    c.fade_on.draw(canvas, text, r.fade_on.1, dark);
    draw_field_label(canvas, text, r.fade_duration.0, dark, FIELD_DURATION);
    c.fade_duration.draw(canvas, text, r.fade_duration.1, dark);

    draw_field_label(canvas, text, r.reduce_motion.0, dark, FIELD_REDUCE_MOTION);
    c.reduce_motion.draw(canvas, text, r.reduce_motion.1, dark);
}

/// Routes a mouse message to the Animations section's controls and, if any reports a value
/// change, syncs `draft.animation`. Returns whether anything changed. Direct port of
/// `ccum-windows/src/config_window.rs::dispatch_animations`.
pub fn dispatch_animations(c: &mut AnimationControls, draft: &mut Settings, area: LRect, msg: MouseMsg, x: f32, y: f32) -> bool {
    let r = animation_layout(area);
    let mut changed = false;

    if dispatch_hit(msg, x, y, r.fill_on.1) && c.fill_on.on_mouse(msg, x, y, r.fill_on.1).is_some() {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.fill_speed.1) && c.fill_speed.on_mouse(msg, x, y, r.fill_speed.1).is_some() {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.shimmer_on.1) && c.shimmer_on.on_mouse(msg, x, y, r.shimmer_on.1).is_some() {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.shimmer_speed.1) && c.shimmer_speed.on_mouse(msg, x, y, r.shimmer_speed.1).is_some() {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.shimmer_intensity.1)
        && c.shimmer_intensity.on_mouse(msg, x, y, r.shimmer_intensity.1).is_some()
    {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.glow_on.1) && c.glow_on.on_mouse(msg, x, y, r.glow_on.1).is_some() {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.glow_threshold.1) && c.glow_threshold.on_mouse(msg, x, y, r.glow_threshold.1).is_some() {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.glow_intensity.1) && c.glow_intensity.on_mouse(msg, x, y, r.glow_intensity.1).is_some() {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.fade_on.1) && c.fade_on.on_mouse(msg, x, y, r.fade_on.1).is_some() {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.fade_duration.1) && c.fade_duration.on_mouse(msg, x, y, r.fade_duration.1).is_some() {
        changed = true;
    }
    if dispatch_hit(msg, x, y, r.reduce_motion.1) && c.reduce_motion.on_mouse(msg, x, y, r.reduce_motion.1).is_some() {
        changed = true;
    }

    if changed {
        draft.animation.fill.on = c.fill_on.on;
        draft.animation.fill.speed = c.fill_speed.value;
        draft.animation.shimmer.on = c.shimmer_on.on;
        draft.animation.shimmer.speed = c.shimmer_speed.value;
        draft.animation.shimmer.intensity = c.shimmer_intensity.value;
        draft.animation.alert_glow.on = c.glow_on.on;
        draft.animation.alert_glow.threshold = c.glow_threshold.value;
        draft.animation.alert_glow.intensity = c.glow_intensity.value;
        draft.animation.fade_slide.on = c.fade_on.on;
        draft.animation.fade_slide.duration_ms = c.fade_duration.value.round().max(1.0) as u32;
        draft.animation.reduce_motion = c.reduce_motion.on;
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
        rect(0.0, 0.0, 400.0, 600.0)
    }

    #[test]
    fn animation_layout_stacks_groups_without_overlap() {
        let r = animation_layout(area());
        assert!(r.fill_header.bottom <= r.fill_on.1.top);
        assert!(r.fill_speed.1.bottom <= r.shimmer_header.top);
        assert!(r.shimmer_intensity.1.bottom <= r.glow_header.top);
        assert!(r.glow_intensity.1.bottom <= r.fade_header.top);
        assert!(r.fade_duration.1.bottom <= r.reduce_motion.1.top);
    }

    #[test]
    fn dispatch_animations_toggling_fill_on_writes_back_into_draft() {
        let a = AnimationSettings::default();
        assert!(a.fill.on, "test assumes the default starts on, so a click flips it off");
        let mut c = AnimationControls::from_settings(&a);
        let mut draft = Settings::default();
        let area_rect = area();
        let r = animation_layout(area_rect);

        let track_x = r.fill_on.1.left + 10.0;
        let track_y = r.fill_on.1.top + r.fill_on.1.height() / 2.0;
        let changed = dispatch_animations(&mut c, &mut draft, area_rect, MouseMsg::Down, track_x, track_y);

        assert!(changed);
        assert!(!draft.animation.fill.on, "clicking the Fill On toggle must flip it off");
    }

    #[test]
    fn dispatch_animations_dragging_shimmer_intensity_writes_back_into_draft() {
        let a = AnimationSettings::default();
        let mut c = AnimationControls::from_settings(&a);
        let mut draft = Settings::default();
        let area_rect = area();
        let r = animation_layout(area_rect);

        // `dispatch_hit`'s bounds are half-open (`x < rect.right`), so the click must land
        // strictly inside the control rect, not exactly on its right edge.
        let changed = dispatch_animations(
            &mut c,
            &mut draft,
            area_rect,
            MouseMsg::Down,
            r.shimmer_intensity.1.right - 1.0,
            r.shimmer_intensity.1.top + 1.0,
        );
        assert!(changed);
        assert!(draft.animation.shimmer.intensity > 0.99, "dragging to the far right must saturate near 1.0, got {}", draft.animation.shimmer.intensity);
    }

    #[test]
    fn dispatch_animations_toggling_reduce_motion_writes_back_into_draft() {
        let a = AnimationSettings::default();
        assert!(!a.reduce_motion, "test assumes the default starts off, so a click flips it on");
        let mut c = AnimationControls::from_settings(&a);
        let mut draft = Settings::default();
        let area_rect = area();
        let r = animation_layout(area_rect);

        let track_x = r.reduce_motion.1.left + 10.0;
        let track_y = r.reduce_motion.1.top + r.reduce_motion.1.height() / 2.0;
        let changed = dispatch_animations(&mut c, &mut draft, area_rect, MouseMsg::Down, track_x, track_y);

        assert!(changed);
        assert!(draft.animation.reduce_motion);
    }

    #[test]
    fn dispatch_animations_ignores_clicks_outside_any_row() {
        let a = AnimationSettings::default();
        let mut c = AnimationControls::from_settings(&a);
        let mut draft = Settings::default();
        let area_rect = area();
        let r = animation_layout(area_rect);

        // Click on the group header text -- not any row's control rect.
        let changed = dispatch_animations(&mut c, &mut draft, area_rect, MouseMsg::Down, r.fill_header.left + 1.0, r.fill_header.top + 1.0);
        assert!(!changed);
    }
}
