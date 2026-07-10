// Built-in style presets (Task 16).
//
// A preset is a one-shot mutation of `Settings.appearance` + `Settings.animation` -- geometry
// and typography are deliberately untouched (presets are about *look and motion*, not size/
// position/font, which the user tunes independently in their own sections). `apply_preset` is
// pure and synchronous: it just writes fields on the `Settings` passed in. The settings window
// (`config_window.rs`) is the only caller today -- it applies a preset to `draft` (never to the
// real widget's settings directly), then rebuilds `SectionControls` from the mutated `draft` so
// the Appearance/Animations controls' own cached values don't go stale, exactly like the
// Reset button (see `config_window::handle_button_action`'s `ButtonAction::Reset` arm).
//
// Concrete chosen values (documented here so a reviewer can check them against the qualitative
// spec in the task brief without re-deriving them from the code):
//
// - **Default** -- byte-for-byte `Appearance::default()` + `AnimationSettings::default()`: no
//   color overrides (adaptive dark/light background/text/divider, per-model accent palette),
//   opacity 1.0, all four animation families on at today's existing default speeds/intensities
//   (fill 1.0x/Cubic, shimmer 0.5x speed/0.3 intensity, glow at 0.85 threshold/0.6 intensity,
//   200ms fade). This *is* "current look" -- the preset exists so a user who's drifted away
//   from it via other controls has a one-click way back, without touching geometry/position/
//   typography the way the Reset button does.
// - **Glass** -- opacity 0.8 (the brief's literal "~0.8") for the translucent-panel look, a
//   cool blue/teal/rose palette (calm #4FB6C9 cyan-teal, attention #5B8DEF cool blue, critical
//   #C75B8A cool rose -- still visually ordered light-to-saturated so severity reads at a
//   glance despite the cool cast), a cool slate background (#26333D) with light cool text
//   (#E8F1F5) and a cool mid-tone divider (#4A6472) so the fixed override reads as "frosted
//   glass" in both light and dark OS theme (that's the point of an override -- no adaptive
//   split needed). Animation: shimmer on, slowed to 0.35x speed ("soft") but intensity raised
//   to 0.75 ("strong") per the brief's "soft shimmer strong"; fill given `Easing::Spring` for a
//   gentle bounce; fade stretched slightly to 260ms for a smoother settle; glow left on but
//   dialed back to 0.5 intensity so it doesn't compete with the shimmer.
// - **Neon** -- saturated palette (calm #39FF6A neon green, attention #FFB300 neon amber,
//   critical #FF1744 neon red-pink), a near-black background (#0A0A0F) so the saturated colors
//   pop, bright near-white text (#F5F5FF) for contrast. Animation: alert_glow on with threshold
//   dropped to 0.6 (glows sooner/more often) and intensity raised to 0.95 ("strong glow"),
//   shimmer sped up to 1.6x ("fast shimmer"), fill given `Easing::Spring` at 1.4x speed for
//   extra punch, fade tightened to 150ms for snappy transitions. Opacity stays 1.0 -- neon reads
//   as a fully-opaque, saturated look, not a translucent one.
// - **Minimal** -- palette flattened to a low-saturation blue-gray ramp (calm #9AA5B1,
//   attention #7C8793, critical #5C6670) so bars still visually order by severity without
//   shouting; background/text/divider are deliberately left `None` (adaptive) -- "minimal"
//   means the fewest overrides, not a specific fixed color scheme. Animation: everything off
//   (`shimmer.on`/`alert_glow.on`/`fade_slide.on` all `false`) except fill, which stays on but
//   slowed to 0.6x speed with `Easing::Linear` for a plain, non-bouncy "gentle fill" per the
//   brief. `reduce_motion` is left `false` (not set `true`) because `AnimationClock` treats
//   `reduce_motion` as a global kill switch that also snaps fill instantly (see
//   `animation.rs`'s `tick`) -- setting it here would contradict "except gentle fill".

use crate::settings::{
    AlertGlowAnim, Appearance, Easing, FadeSlideAnim, FillAnim, PaletteStops, PresetId, Rgba,
    Settings, ShimmerAnim,
};

fn rgba(r: u8, g: u8, b: u8) -> Rgba {
    Rgba { r, g, b, a: 255 }
}

/// Mutates `s.appearance` and `s.animation` in place to match built-in preset `id`.
/// `s.geometry` and `s.typography` are never touched.
pub fn apply_preset(id: PresetId, s: &mut Settings) {
    match id {
        PresetId::Default => {
            s.appearance = Appearance {
                palette: None,
                background: None,
                text: None,
                divider: None,
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim {
                on: true,
                easing: Easing::Cubic,
                speed: 1.0,
            };
            s.animation.shimmer = ShimmerAnim {
                on: true,
                speed: 0.5,
                intensity: 0.3,
            };
            s.animation.alert_glow = AlertGlowAnim {
                on: true,
                threshold: 0.85,
                intensity: 0.6,
            };
            s.animation.fade_slide = FadeSlideAnim {
                on: true,
                duration_ms: 200,
            };
        }
        PresetId::Glass => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0x4F, 0xB6, 0xC9),
                    attention: rgba(0x5B, 0x8D, 0xEF),
                    critical: rgba(0xC7, 0x5B, 0x8A),
                }),
                background: Some(rgba(0x26, 0x33, 0x3D)),
                text: Some(rgba(0xE8, 0xF1, 0xF5)),
                divider: Some(rgba(0x4A, 0x64, 0x72)),
                opacity: 0.8,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim {
                on: true,
                easing: Easing::Spring,
                speed: 1.0,
            };
            s.animation.shimmer = ShimmerAnim {
                on: true,
                speed: 0.35,
                intensity: 0.75,
            };
            s.animation.alert_glow = AlertGlowAnim {
                on: true,
                threshold: 0.85,
                intensity: 0.5,
            };
            s.animation.fade_slide = FadeSlideAnim {
                on: true,
                duration_ms: 260,
            };
        }
        PresetId::Neon => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0x39, 0xFF, 0x6A),
                    attention: rgba(0xFF, 0xB3, 0x00),
                    critical: rgba(0xFF, 0x17, 0x44),
                }),
                background: Some(rgba(0x0A, 0x0A, 0x0F)),
                text: Some(rgba(0xF5, 0xF5, 0xFF)),
                divider: Some(rgba(0x2A, 0x2A, 0x35)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim {
                on: true,
                easing: Easing::Spring,
                speed: 1.4,
            };
            s.animation.shimmer = ShimmerAnim {
                on: true,
                speed: 1.6,
                intensity: 0.5,
            };
            s.animation.alert_glow = AlertGlowAnim {
                on: true,
                threshold: 0.6,
                intensity: 0.95,
            };
            s.animation.fade_slide = FadeSlideAnim {
                on: true,
                duration_ms: 150,
            };
        }
        PresetId::Minimal => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0x9A, 0xA5, 0xB1),
                    attention: rgba(0x7C, 0x87, 0x93),
                    critical: rgba(0x5C, 0x66, 0x70),
                }),
                background: None,
                text: None,
                divider: None,
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim {
                on: true,
                easing: Easing::Linear,
                speed: 0.6,
            };
            s.animation.shimmer = ShimmerAnim {
                on: false,
                ..s.animation.shimmer
            };
            s.animation.alert_glow = AlertGlowAnim {
                on: false,
                ..s.animation.alert_glow
            };
            s.animation.fade_slide = FadeSlideAnim {
                on: false,
                ..s.animation.fade_slide
            };
        }
    }
    s.animation.preset = Some(id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn minimal_turns_off_shimmer_and_leaves_geometry_untouched() {
        let mut s = Settings::default();
        // `Geometry` doesn't derive `PartialEq` (out of scope for this task to add), so compare
        // via its `Debug` output instead of `assert_eq!` on the struct directly.
        let geometry_before = format!("{:?}", s.geometry);
        apply_preset(PresetId::Minimal, &mut s);
        assert_eq!(s.animation.shimmer.on, false);
        assert_eq!(format!("{:?}", s.geometry), geometry_before);
    }

    #[test]
    fn minimal_turns_off_glow_and_fade_but_keeps_gentle_fill() {
        let mut s = Settings::default();
        apply_preset(PresetId::Minimal, &mut s);
        assert_eq!(s.animation.alert_glow.on, false);
        assert_eq!(s.animation.fade_slide.on, false);
        assert_eq!(s.animation.fill.on, true);
        assert_eq!(s.animation.reduce_motion, false);
        assert_eq!(s.animation.preset, Some(PresetId::Minimal));
    }

    #[test]
    fn minimal_leaves_typography_untouched() {
        let mut s = Settings::default();
        s.typography.family = "Comic Sans MS".to_string();
        s.typography.size_pt = 24.0;
        let typography_before = s.typography.clone();
        apply_preset(PresetId::Minimal, &mut s);
        assert_eq!(s.typography.family, typography_before.family);
        assert_eq!(s.typography.size_pt, typography_before.size_pt);
    }

    #[test]
    fn default_leaves_all_four_animation_families_on() {
        let mut s = Settings::default();
        // Start from a state where everything is off/overridden, to prove Default actually
        // resets rather than being a no-op that happens to match Settings::default().
        s.animation.fill.on = false;
        s.animation.shimmer.on = false;
        s.animation.alert_glow.on = false;
        s.animation.fade_slide.on = false;
        s.appearance.opacity = 0.3;
        apply_preset(PresetId::Default, &mut s);
        assert_eq!(s.animation.fill.on, true);
        assert_eq!(s.animation.shimmer.on, true);
        assert_eq!(s.animation.alert_glow.on, true);
        assert_eq!(s.animation.fade_slide.on, true);
        assert_eq!(s.appearance.opacity, 1.0);
        assert_eq!(s.appearance.palette, None);
        assert_eq!(s.animation.preset, Some(PresetId::Default));
    }

    #[test]
    fn glass_sets_translucent_opacity_and_cool_palette() {
        let mut s = Settings::default();
        apply_preset(PresetId::Glass, &mut s);
        assert!(s.appearance.opacity < 1.0);
        assert!((s.appearance.opacity - 0.8).abs() < f32::EPSILON);
        assert!(s.appearance.palette.is_some());
        assert!(s.appearance.background.is_some());
        assert_eq!(s.animation.shimmer.on, true);
        assert!(s.animation.shimmer.intensity >= 0.7);
        assert_eq!(s.animation.preset, Some(PresetId::Glass));
    }

    #[test]
    fn neon_sets_strong_glow_and_fast_shimmer_on_dark_background() {
        let mut s = Settings::default();
        apply_preset(PresetId::Neon, &mut s);
        assert_eq!(s.animation.alert_glow.on, true);
        assert!(s.animation.alert_glow.intensity >= 0.9);
        assert!(s.animation.shimmer.speed >= 1.5);
        assert_eq!(s.appearance.opacity, 1.0);
        let bg = s.appearance.background.expect("neon sets a fixed dark background");
        // "Dark background": all three channels close to black.
        assert!(bg.r < 40 && bg.g < 40 && bg.b < 40);
        assert_eq!(s.animation.preset, Some(PresetId::Neon));
    }

    #[test]
    fn apply_preset_never_touches_geometry() {
        for id in [PresetId::Default, PresetId::Glass, PresetId::Neon, PresetId::Minimal] {
            let mut s = Settings::default();
            s.geometry.height = 77;
            s.geometry.corner_radius = 9;
            let before = format!("{:?}", s.geometry);
            apply_preset(id, &mut s);
            assert_eq!(format!("{:?}", s.geometry), before, "{id:?} must not touch geometry");
        }
    }
}
