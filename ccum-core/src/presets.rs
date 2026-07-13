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
//
// 20 additional named theme presets (Task 3). Each pulls its palette from a well-known,
// real-world color scheme (not byte-exact fidelity to the original product, but the same
// character) and positions its animation profile somewhere on the Default/Glass/Neon/Minimal
// spectrum established above, based on the theme's real-world personality. Per-arm doc comments
// below give the concrete chosen values and the reasoning; see `ct-task-3-report.md` for the
// full table.

use crate::settings::{
    AlertGlowAnim, Appearance, Easing, FadeSlideAnim, FillAnim, PaletteStops, PresetId, Rgba,
    Settings, ShimmerAnim,
};

fn rgba(r: u8, g: u8, b: u8) -> Rgba {
    Rgba { r, g, b, a: 255 }
}

/// All 24 `PresetId` variants (4 original + 20 added in Task 3), in `PresetId`'s own
/// declaration order. `pub` (was `pub(crate)` before the Task 1 workspace split, back when
/// `config_window.rs` shared this crate) because the Presets grid (`config_window.rs`, now in
/// the separate `ccum-windows` crate) needs this exact 24-entry list to build its card grid, and
/// the unit tests below reuse the same const rather than duplicating the 24-entry literal a
/// second time.
pub const ALL_PRESET_IDS: [PresetId; 24] = [
    PresetId::Default,
    PresetId::Glass,
    PresetId::Neon,
    PresetId::Minimal,
    PresetId::Dracula,
    PresetId::Nord,
    PresetId::SolarizedDark,
    PresetId::SolarizedLight,
    PresetId::Gruvbox,
    PresetId::Catppuccin,
    PresetId::TokyoNight,
    PresetId::OneDark,
    PresetId::Monokai,
    PresetId::Material,
    PresetId::GitHubDark,
    PresetId::Discord,
    PresetId::Spotify,
    PresetId::RosePine,
    PresetId::Everforest,
    PresetId::Kanagawa,
    PresetId::SynthwaveEighty4,
    PresetId::Ayu,
    PresetId::Palenight,
    PresetId::Cyberpunk,
];

/// Which of the three Presets-grid groupings (Task 5) a preset belongs in. English-only
/// display concerns live in this module rather than `Strings` because preset/category names
/// here are proper nouns / a fixed 3-way UI grouping, not user-facing prose that needs
/// translating (see `theme_category`'s doc comment for the exact split).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresetCategory {
    Builtin,
    Editors,
    Apps,
}

/// The literal, untranslated English display name for preset `id` (proper nouns per the
/// design spec -- unlike `preset_default`/`preset_glass`/`preset_neon`/`preset_minimal` in
/// `Strings`, which predate this task and stay localized, these 20 new names are deliberately
/// NOT part of `Strings` and render identically in every locale). Total match, no wildcard
/// arm, over all 24 `PresetId` variants -- adding a 25th variant is a compile error here until
/// this function is updated, the same safety net `apply_preset` relies on.
pub fn theme_display_name(id: PresetId) -> &'static str {
    match id {
        PresetId::Default => "Default",
        PresetId::Glass => "Glass",
        PresetId::Neon => "Neon",
        PresetId::Minimal => "Minimal",
        PresetId::Dracula => "Dracula",
        PresetId::Nord => "Nord",
        PresetId::SolarizedDark => "Solarized Dark",
        PresetId::SolarizedLight => "Solarized Light",
        PresetId::Gruvbox => "Gruvbox",
        PresetId::Catppuccin => "Catppuccin",
        PresetId::TokyoNight => "Tokyo Night",
        PresetId::OneDark => "One Dark",
        PresetId::Monokai => "Monokai",
        PresetId::Material => "Material",
        PresetId::GitHubDark => "GitHub Dark",
        PresetId::Discord => "Discord",
        PresetId::Spotify => "Spotify",
        PresetId::RosePine => "Rosé Pine",
        PresetId::Everforest => "Everforest",
        PresetId::Kanagawa => "Kanagawa",
        PresetId::SynthwaveEighty4 => "Synthwave '84",
        PresetId::Ayu => "Ayu",
        PresetId::Palenight => "Palenight",
        PresetId::Cyberpunk => "Cyberpunk",
    }
}

/// Which of the 3 Presets-grid category groups (Task 5, `strings.preset_category_*`) preset
/// `id` belongs to: `Builtin` for the original 4 (Default/Glass/Neon/Minimal), `Apps` for the
/// 2 real-app themes (Discord/Spotify), and `Editors` for the remaining 18 -- including the
/// neon-leaning Synthwave '84/Cyberpunk and the Material Design-derived Material, which the
/// design spec folds into "Code editors" rather than giving either its own single/two-item
/// category, keeping exactly 3 groups total. Total match, no wildcard arm, over all 24
/// `PresetId` variants.
pub fn theme_category(id: PresetId) -> PresetCategory {
    match id {
        PresetId::Default | PresetId::Glass | PresetId::Neon | PresetId::Minimal => {
            PresetCategory::Builtin
        }
        PresetId::Discord | PresetId::Spotify => PresetCategory::Apps,
        PresetId::Dracula
        | PresetId::Nord
        | PresetId::SolarizedDark
        | PresetId::SolarizedLight
        | PresetId::Gruvbox
        | PresetId::Catppuccin
        | PresetId::TokyoNight
        | PresetId::OneDark
        | PresetId::Monokai
        | PresetId::Material
        | PresetId::GitHubDark
        | PresetId::RosePine
        | PresetId::Everforest
        | PresetId::Kanagawa
        | PresetId::SynthwaveEighty4
        | PresetId::Ayu
        | PresetId::Palenight
        | PresetId::Cyberpunk => PresetCategory::Editors,
    }
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
        // Dracula -- background #282A36, text #F8F8F2, divider #44475A (Dracula's own
        // "current line" gray); palette calm #BD93F9 (purple), attention #FF79C6 (pink),
        // critical #FF5555 (red) -- the theme's signature purple/pink accents over a dark
        // blue-leaning background. Animation: a lively but not extreme dev-editor theme, so it
        // sits just above Default's restraint -- shimmer nudged to 0.6x/0.4 intensity and glow
        // to 0.65 intensity, everything else at Default's cadence.
        PresetId::Dracula => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0xBD, 0x93, 0xF9),
                    attention: rgba(0xFF, 0x79, 0xC6),
                    critical: rgba(0xFF, 0x55, 0x55),
                }),
                background: Some(rgba(0x28, 0x2A, 0x36)),
                text: Some(rgba(0xF8, 0xF8, 0xF2)),
                divider: Some(rgba(0x44, 0x47, 0x5A)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Cubic, speed: 1.0 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 0.6, intensity: 0.4 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.85, intensity: 0.65 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 200 };
        }
        // Nord -- background #2E3440 (Nord's "polar night" darkest), text #ECEFF4 (snow storm),
        // divider #4C566A; palette calm #88C0D0 (frost cyan), attention #EBCB8B (aurora yellow),
        // critical #BF616A (aurora red). Nord is a classic understated developer palette, so
        // animation stays close to Default's restraint -- shimmer dialed back to 0.4x/0.25
        // intensity, glow softened to 0.5 intensity.
        PresetId::Nord => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0x88, 0xC0, 0xD0),
                    attention: rgba(0xEB, 0xCB, 0x8B),
                    critical: rgba(0xBF, 0x61, 0x6A),
                }),
                background: Some(rgba(0x2E, 0x34, 0x40)),
                text: Some(rgba(0xEC, 0xEF, 0xF4)),
                divider: Some(rgba(0x4C, 0x56, 0x6A)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Cubic, speed: 1.0 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 0.4, intensity: 0.25 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.85, intensity: 0.5 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 200 };
        }
        // Solarized Dark -- background #002B36 (Solarized "base03"), text #839496 ("base0"),
        // divider #073642 ("base02"); palette calm #268BD2 (blue), attention #B58900 (yellow),
        // critical #DC322F (red) -- Solarized's own accent colors. A classic understated
        // developer palette: animation stays close to Default, shimmer softened to 0.4x/0.25.
        PresetId::SolarizedDark => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0x26, 0x8B, 0xD2),
                    attention: rgba(0xB5, 0x89, 0x00),
                    critical: rgba(0xDC, 0x32, 0x2F),
                }),
                background: Some(rgba(0x00, 0x2B, 0x36)),
                text: Some(rgba(0x83, 0x94, 0x96)),
                divider: Some(rgba(0x07, 0x36, 0x42)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Cubic, speed: 1.0 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 0.4, intensity: 0.25 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.85, intensity: 0.5 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 200 };
        }
        // Solarized Light -- the same Solarized accent colors as SolarizedDark, but on
        // Solarized's light background #FDF6E3 ("base3") with dark text #657B83 ("base00") and
        // divider #EEE8D5 ("base2"). Same restrained animation profile as SolarizedDark -- the
        // light/dark split is purely a color-scheme choice, not a motion one.
        PresetId::SolarizedLight => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0x26, 0x8B, 0xD2),
                    attention: rgba(0xB5, 0x89, 0x00),
                    critical: rgba(0xDC, 0x32, 0x2F),
                }),
                background: Some(rgba(0xFD, 0xF6, 0xE3)),
                text: Some(rgba(0x65, 0x7B, 0x83)),
                divider: Some(rgba(0xEE, 0xE8, 0xD5)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Cubic, speed: 1.0 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 0.4, intensity: 0.25 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.85, intensity: 0.5 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 200 };
        }
        // Gruvbox -- background #282828 (Gruvbox "bg0"), text #EBDBB2 ("fg1"), divider #3C3836
        // ("bg1"); palette calm #B8BB26 (green), attention #FABD2F (yellow), critical #FB4934
        // (red) -- Gruvbox's warm, retro-terminal accent colors. A classic understated developer
        // palette: animation stays close to Default, shimmer softened to 0.4x/0.25.
        PresetId::Gruvbox => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0xB8, 0xBB, 0x26),
                    attention: rgba(0xFA, 0xBD, 0x2F),
                    critical: rgba(0xFB, 0x49, 0x34),
                }),
                background: Some(rgba(0x28, 0x28, 0x28)),
                text: Some(rgba(0xEB, 0xDB, 0xB2)),
                divider: Some(rgba(0x3C, 0x38, 0x36)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Cubic, speed: 1.0 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 0.4, intensity: 0.25 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.85, intensity: 0.5 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 200 };
        }
        // Catppuccin (Mocha) -- background #1E1E2E ("base"), text #CDD6F4 ("text"), divider
        // #313244 ("surface0"); palette calm #A6E3A1 (green), attention #F9E2AF (yellow),
        // critical #F38BA8 (pastel red/pink) -- Catppuccin's soft pastel accents. Animation
        // leans slightly softer/warmer than Default (a touch more shimmer, gentler glow) to
        // match the pastel, cozy character, without going as far as Glass's translucency.
        PresetId::Catppuccin => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0xA6, 0xE3, 0xA1),
                    attention: rgba(0xF9, 0xE2, 0xAF),
                    critical: rgba(0xF3, 0x8B, 0xA8),
                }),
                background: Some(rgba(0x1E, 0x1E, 0x2E)),
                text: Some(rgba(0xCD, 0xD6, 0xF4)),
                divider: Some(rgba(0x31, 0x32, 0x44)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Cubic, speed: 1.0 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 0.55, intensity: 0.45 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.85, intensity: 0.55 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 220 };
        }
        // Tokyo Night -- background #1A1B26, text #C0CAF5, divider #292E42; palette calm #7AA2F7
        // (blue), attention #E0AF68 (orange), critical #F7768E (red) -- the neon-lit-city
        // character of the original VS Code theme. Animation moderately livelier than Default
        // (a bit more shimmer/glow) to suggest "city lights", short of Neon's full intensity.
        PresetId::TokyoNight => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0x7A, 0xA2, 0xF7),
                    attention: rgba(0xE0, 0xAF, 0x68),
                    critical: rgba(0xF7, 0x76, 0x8E),
                }),
                background: Some(rgba(0x1A, 0x1B, 0x26)),
                text: Some(rgba(0xC0, 0xCA, 0xF5)),
                divider: Some(rgba(0x29, 0x2E, 0x42)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Cubic, speed: 1.1 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 0.7, intensity: 0.45 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.8, intensity: 0.65 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 190 };
        }
        // One Dark -- background #282C34 (Atom's One Dark UI background), text #ABB2BF,
        // divider #3E4451; palette calm #98C379 (green), attention #E5C07B (yellow), critical
        // #E06C75 (red) -- the classic Atom/One Dark syntax accents. A restrained, familiar
        // editor theme: animation stays close to Default's cadence.
        PresetId::OneDark => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0x98, 0xC3, 0x79),
                    attention: rgba(0xE5, 0xC0, 0x7B),
                    critical: rgba(0xE0, 0x6C, 0x75),
                }),
                background: Some(rgba(0x28, 0x2C, 0x34)),
                text: Some(rgba(0xAB, 0xB2, 0xBF)),
                divider: Some(rgba(0x3E, 0x44, 0x51)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Cubic, speed: 1.0 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 0.45, intensity: 0.3 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.85, intensity: 0.55 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 200 };
        }
        // Monokai -- background #272822, text #F8F8F2, divider #3E3D32; palette calm #A6E22E
        // (bright green), attention #E6DB74 (yellow), critical #F92672 (hot pink/red) --
        // Monokai's famously punchy syntax-highlight accents. Animation leans livelier than
        // Default (faster shimmer, stronger glow) to match that punchiness, though not as
        // extreme as Neon.
        PresetId::Monokai => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0xA6, 0xE2, 0x2E),
                    attention: rgba(0xE6, 0xDB, 0x74),
                    critical: rgba(0xF9, 0x26, 0x72),
                }),
                background: Some(rgba(0x27, 0x28, 0x22)),
                text: Some(rgba(0xF8, 0xF8, 0xF2)),
                divider: Some(rgba(0x3E, 0x3D, 0x32)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Cubic, speed: 1.15 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 0.85, intensity: 0.5 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.75, intensity: 0.7 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 180 };
        }
        // Material -- background #263238 (Material "Blue Grey 900"/Ocean), text #EEFFFF,
        // divider #37474F; palette calm #82AAFF (blue), attention #FFCB6B (amber), critical
        // #FF5370 (red) -- Google's Material Design accent colors. Clean and modern, so
        // animation stays close to Default's restraint with just a touch more polish (Spring
        // fill) to match Material's motion-design heritage.
        PresetId::Material => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0x82, 0xAA, 0xFF),
                    attention: rgba(0xFF, 0xCB, 0x6B),
                    critical: rgba(0xFF, 0x53, 0x70),
                }),
                background: Some(rgba(0x26, 0x32, 0x38)),
                text: Some(rgba(0xEE, 0xFF, 0xFF)),
                divider: Some(rgba(0x37, 0x47, 0x4F)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Spring, speed: 1.0 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 0.45, intensity: 0.3 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.85, intensity: 0.55 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 210 };
        }
        // GitHub Dark -- background #0D1117 (GitHub's dark-mode canvas), text #C9D1D9, divider
        // #21262D; palette calm #58A6FF (blue), attention #D29922 (gold), critical #F85149
        // (red) -- GitHub's own dark-mode accent colors. A classic understated developer
        // palette: animation stays close to Default's restraint, shimmer softened to 0.4x/0.25.
        PresetId::GitHubDark => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0x58, 0xA6, 0xFF),
                    attention: rgba(0xD2, 0x99, 0x22),
                    critical: rgba(0xF8, 0x51, 0x49),
                }),
                background: Some(rgba(0x0D, 0x11, 0x17)),
                text: Some(rgba(0xC9, 0xD1, 0xD9)),
                divider: Some(rgba(0x21, 0x26, 0x2D)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Cubic, speed: 1.0 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 0.4, intensity: 0.25 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.85, intensity: 0.5 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 200 };
        }
        // Discord -- background #313338 (Discord's current dark theme surface), text #F2F3F5,
        // divider #1E1F22; palette calm #5865F2 (Discord "blurple", the brand accent), attention
        // #FEE75C (idle-status yellow), critical #ED4245 (danger/DND red) -- Discord's own brand
        // and status colors. Animation slightly livelier than Default (a snappier fade, modest
        // shimmer/glow bump) to match a chatty, social-app feel.
        PresetId::Discord => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0x58, 0x65, 0xF2),
                    attention: rgba(0xFE, 0xE7, 0x5C),
                    critical: rgba(0xED, 0x42, 0x45),
                }),
                background: Some(rgba(0x31, 0x33, 0x38)),
                text: Some(rgba(0xF2, 0xF3, 0xF5)),
                divider: Some(rgba(0x1E, 0x1F, 0x22)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Cubic, speed: 1.1 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 0.7, intensity: 0.35 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.85, intensity: 0.6 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 180 };
        }
        // Spotify -- background pure #000000 (Spotify's now-black dark theme), text #FFFFFF,
        // divider #282828; palette calm #1DB954 (Spotify green, the brand accent), attention
        // #FFA42B (amber, close to Spotify's "explicit"/warning amber), critical #E91429
        // (Spotify's red, used for live/error states). Animation evokes the pulsing "now
        // playing" progress bar: a touch faster shimmer and a livelier glow than Default.
        PresetId::Spotify => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0x1D, 0xB9, 0x54),
                    attention: rgba(0xFF, 0xA4, 0x2B),
                    critical: rgba(0xE9, 0x14, 0x29),
                }),
                background: Some(rgba(0x00, 0x00, 0x00)),
                text: Some(rgba(0xFF, 0xFF, 0xFF)),
                divider: Some(rgba(0x28, 0x28, 0x28)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Cubic, speed: 1.1 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 0.9, intensity: 0.4 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.85, intensity: 0.65 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 180 };
        }
        // Rosé Pine -- background #191724 ("base"), text #E0DEF4 ("text"), divider #26233A
        // ("surface"); palette calm #9CCFD8 ("foam"), attention #F6C177 ("gold"), critical
        // #EB6F92 ("love") -- Rosé Pine's soft, muted accents. Explicitly a calm/low-motion
        // theme per the design spec: shimmer slowed well below Default's 0.5x speed and
        // softened, glow gentle, fade relaxed.
        PresetId::RosePine => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0x9C, 0xCF, 0xD8),
                    attention: rgba(0xF6, 0xC1, 0x77),
                    critical: rgba(0xEB, 0x6F, 0x92),
                }),
                background: Some(rgba(0x19, 0x17, 0x24)),
                text: Some(rgba(0xE0, 0xDE, 0xF4)),
                divider: Some(rgba(0x26, 0x23, 0x3A)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Cubic, speed: 0.8 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 0.3, intensity: 0.2 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.9, intensity: 0.4 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 260 };
        }
        // Everforest -- background #2D353B (Everforest's "bg0" dark), text #D3C6AA (fg),
        // divider #475258 (bg2); palette calm #A7C080 (green), attention #DBBC7F (yellow),
        // critical #E67E80 (red) -- Everforest's soft, forest-inspired accents. Explicitly a
        // calm/low-motion theme per the design spec: shimmer slowed well below Default's 0.5x
        // speed, glow gentle, fade relaxed -- matching Rosé Pine's calm profile.
        PresetId::Everforest => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0xA7, 0xC0, 0x80),
                    attention: rgba(0xDB, 0xBC, 0x7F),
                    critical: rgba(0xE6, 0x7E, 0x80),
                }),
                background: Some(rgba(0x2D, 0x35, 0x3B)),
                text: Some(rgba(0xD3, 0xC6, 0xAA)),
                divider: Some(rgba(0x47, 0x52, 0x58)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Cubic, speed: 0.8 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 0.3, intensity: 0.2 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.9, intensity: 0.4 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 240 };
        }
        // Kanagawa -- background #1F1F28 ("sumiInk"), text #DCD7BA ("fujiWhite"), divider
        // #54546D ("sumiInk4"); palette calm #7E9CD8 ("crystalBlue"), attention #DCA561
        // ("roninYellow"), critical #E46876 ("waveRed") -- Kanagawa's Japanese ink-wash-inspired
        // accents. Restrained and elegant: animation stays close to Default's cadence, just
        // slightly softened.
        PresetId::Kanagawa => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0x7E, 0x9C, 0xD8),
                    attention: rgba(0xDC, 0xA5, 0x61),
                    critical: rgba(0xE4, 0x68, 0x76),
                }),
                background: Some(rgba(0x1F, 0x1F, 0x28)),
                text: Some(rgba(0xDC, 0xD7, 0xBA)),
                divider: Some(rgba(0x54, 0x54, 0x6D)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Cubic, speed: 0.9 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 0.4, intensity: 0.25 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.85, intensity: 0.5 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 220 };
        }
        // Synthwave '84 -- background #241B2F (deep purple-black), text #F8F8F2, divider
        // #2A2139; palette calm #F92AAD (hot magenta), attention #36F9F6 (neon cyan), critical
        // #FF3864 (neon red) -- the retro-futurist neon-grid aesthetic the theme is named for.
        // Per the design spec, leans toward Neon's strong-glow/fast-shimmer end: glow threshold
        // dropped and intensity raised above 0.7, shimmer sped up, Spring fill for extra punch.
        PresetId::SynthwaveEighty4 => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0xF9, 0x2A, 0xAD),
                    attention: rgba(0x36, 0xF9, 0xF6),
                    critical: rgba(0xFF, 0x38, 0x64),
                }),
                background: Some(rgba(0x24, 0x1B, 0x2F)),
                text: Some(rgba(0xF8, 0xF8, 0xF2)),
                divider: Some(rgba(0x2A, 0x21, 0x39)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Spring, speed: 1.3 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 1.5, intensity: 0.6 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.6, intensity: 0.9 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 150 };
        }
        // Ayu (Dark) -- background #0A0E14, text #B3B1AD, divider #1B222D; palette calm #95E6CB
        // (aqua/mint), attention #FFB454 (orange), critical #F07178 (red) -- Ayu's clean, airy
        // editor accents. A tidy, minimal-leaning editor theme: animation stays close to
        // Default's cadence, softened slightly.
        PresetId::Ayu => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0x95, 0xE6, 0xCB),
                    attention: rgba(0xFF, 0xB4, 0x54),
                    critical: rgba(0xF0, 0x71, 0x78),
                }),
                background: Some(rgba(0x0A, 0x0E, 0x14)),
                text: Some(rgba(0xB3, 0xB1, 0xAD)),
                divider: Some(rgba(0x1B, 0x22, 0x2D)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Cubic, speed: 1.0 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 0.45, intensity: 0.28 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.85, intensity: 0.55 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 200 };
        }
        // Palenight -- background #292D3E (Material Palenight's signature lavender-tinted
        // dark), text #A6ACCD, divider #444267; palette calm #C792EA (lavender/purple, the
        // theme's defining accent), attention #FFCB6B (amber), critical #F07178 (red).
        // Animation moderately soft (a bit more shimmer than Default, gentle glow) to echo the
        // theme's smooth, "night sky" character, short of Glass's translucency.
        PresetId::Palenight => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0xC7, 0x92, 0xEA),
                    attention: rgba(0xFF, 0xCB, 0x6B),
                    critical: rgba(0xF0, 0x71, 0x78),
                }),
                background: Some(rgba(0x29, 0x2D, 0x3E)),
                text: Some(rgba(0xA6, 0xAC, 0xCD)),
                divider: Some(rgba(0x44, 0x42, 0x67)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Cubic, speed: 1.0 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 0.5, intensity: 0.35 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.85, intensity: 0.55 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 210 };
        }
        // Cyberpunk -- background #0D0221 (near-black deep purple), text #F2F2F2, divider
        // #2A0944; palette calm #00F0FF (cyan), attention #FCEE0C (signature Cyberpunk 2077
        // yellow), critical #FF003C (hot red) -- a high-contrast neon-noir palette. Per the
        // design spec, leans toward Neon's strong-glow/fast-shimmer end: glow threshold dropped
        // and intensity raised above 0.7, shimmer sped up further than Synthwave, Spring fill.
        PresetId::Cyberpunk => {
            s.appearance = Appearance {
                palette: Some(PaletteStops {
                    calm: rgba(0x00, 0xF0, 0xFF),
                    attention: rgba(0xFC, 0xEE, 0x0C),
                    critical: rgba(0xFF, 0x00, 0x3C),
                }),
                background: Some(rgba(0x0D, 0x02, 0x21)),
                text: Some(rgba(0xF2, 0xF2, 0xF2)),
                divider: Some(rgba(0x2A, 0x09, 0x44)),
                opacity: 1.0,
            };
            s.animation.reduce_motion = false;
            s.animation.fill = FillAnim { on: true, easing: Easing::Spring, speed: 1.5 };
            s.animation.shimmer = ShimmerAnim { on: true, speed: 1.7, intensity: 0.55 };
            s.animation.alert_glow = AlertGlowAnim { on: true, threshold: 0.55, intensity: 0.9 };
            s.animation.fade_slide = FadeSlideAnim { on: true, duration_ms: 140 };
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
        for id in ALL_24_PRESETS {
            let mut s = Settings::default();
            s.geometry.height = 77;
            s.geometry.corner_radius = 9;
            let before = format!("{:?}", s.geometry);
            apply_preset(id, &mut s);
            assert_eq!(format!("{:?}", s.geometry), before, "{id:?} must not touch geometry");
        }
    }

    // `ALL_24_PRESETS` used to be duplicated here as a test-local const; it's now
    // `ALL_PRESET_IDS` at module scope (see above), reused by both these tests and
    // `config_window.rs`'s Presets grid (Task 5). Kept as a local alias so the rest of this
    // test module doesn't need a mechanical rename of every call site.
    use super::ALL_PRESET_IDS as ALL_24_PRESETS;

    // `settings::d_shimmer_speed()` (the default shimmer speed, 0.5) is a private helper on the
    // `settings` module, not reachable from here -- per the task brief, we don't add a new
    // pub(crate) helper just for this test, so the literal 0.5 below is that same value.
    const DEFAULT_SHIMMER_SPEED_FOR_TEST: f32 = 0.5;

    #[test]
    fn all_24_presets_never_touch_geometry_or_typography() {
        for id in ALL_24_PRESETS {
            let mut s = Settings::default();
            let before_geo = format!("{:?}", s.geometry);
            let before_type = s.typography.clone();
            apply_preset(id, &mut s);
            assert_eq!(format!("{:?}", s.geometry), before_geo, "{id:?} must not touch geometry");
            assert_eq!(s.typography.family, before_type.family, "{id:?} must not touch typography");
            assert_eq!(s.typography.size_pt, before_type.size_pt, "{id:?} must not touch typography");
        }
    }

    #[test]
    fn dracula_uses_purple_accent_over_dark_background() {
        let mut s = Settings::default();
        apply_preset(PresetId::Dracula, &mut s);
        let bg = s.appearance.background.expect("Dracula sets an explicit background");
        assert!(bg.b > bg.r, "Dracula's background should lean blue/purple, not warm");
    }

    #[test]
    fn spotify_uses_pure_black_background_and_vivid_green() {
        let mut s = Settings::default();
        apply_preset(PresetId::Spotify, &mut s);
        let bg = s.appearance.background.expect("Spotify sets an explicit background");
        assert_eq!((bg.r, bg.g, bg.b), (0, 0, 0), "Spotify's background is pure black");
        let palette = s.appearance.palette.expect("Spotify sets a fixed palette");
        assert_eq!(
            (palette.calm.r, palette.calm.g, palette.calm.b),
            (0x1D, 0xB9, 0x54),
            "Spotify's calm stop is Spotify green"
        );
    }

    #[test]
    fn cyberpunk_and_synthwave_enable_strong_alert_glow() {
        let mut s1 = Settings::default();
        apply_preset(PresetId::Cyberpunk, &mut s1);
        assert!(s1.animation.alert_glow.on && s1.animation.alert_glow.intensity > 0.7);

        let mut s2 = Settings::default();
        apply_preset(PresetId::SynthwaveEighty4, &mut s2);
        assert!(s2.animation.alert_glow.on && s2.animation.alert_glow.intensity > 0.7);
    }

    #[test]
    fn everforest_and_rose_pine_are_calm_low_motion() {
        let mut s1 = Settings::default();
        apply_preset(PresetId::Everforest, &mut s1);
        assert!(s1.animation.shimmer.speed < DEFAULT_SHIMMER_SPEED_FOR_TEST);

        let mut s2 = Settings::default();
        apply_preset(PresetId::RosePine, &mut s2);
        assert!(s2.animation.shimmer.speed < DEFAULT_SHIMMER_SPEED_FOR_TEST);
    }

    #[test]
    fn github_dark_nord_solarized_gruvbox_stay_close_to_default_restraint() {
        // Classic understated developer palettes: shimmer speed should stay well under Neon's
        // and Synthwave/Cyberpunk's fast-shimmer end, and glow shouldn't be cranked up either.
        for id in [
            PresetId::GitHubDark,
            PresetId::Nord,
            PresetId::SolarizedDark,
            PresetId::SolarizedLight,
            PresetId::Gruvbox,
        ] {
            let mut s = Settings::default();
            apply_preset(id, &mut s);
            assert!(s.animation.shimmer.speed <= DEFAULT_SHIMMER_SPEED_FOR_TEST, "{id:?}");
            assert!(s.animation.alert_glow.intensity <= 0.6, "{id:?}");
        }
    }

    #[test]
    fn all_24_presets_set_the_preset_field_to_themselves() {
        for id in ALL_24_PRESETS {
            let mut s = Settings::default();
            apply_preset(id, &mut s);
            assert_eq!(s.animation.preset, Some(id), "{id:?}");
        }
    }

    #[test]
    fn theme_display_name_and_category_are_total_over_all_24_presets() {
        // `theme_display_name`/`theme_category` are exhaustive `match`es with no wildcard arm,
        // so this test compiling and running at all (rather than a missing-arm compile error)
        // is itself part of the totality proof; this loop additionally checks each result is
        // non-empty / one of the 3 known categories.
        for id in ALL_24_PRESETS {
            let name = theme_display_name(id);
            assert!(!name.is_empty(), "{id:?} has an empty display name");
            match theme_category(id) {
                PresetCategory::Builtin | PresetCategory::Editors | PresetCategory::Apps => {}
            }
        }
    }

    #[test]
    fn theme_display_name_matches_the_brief_s_table_for_a_few_spot_checks() {
        assert_eq!(theme_display_name(PresetId::Default), "Default");
        assert_eq!(theme_display_name(PresetId::Dracula), "Dracula");
        assert_eq!(theme_display_name(PresetId::SolarizedDark), "Solarized Dark");
        assert_eq!(theme_display_name(PresetId::SolarizedLight), "Solarized Light");
        assert_eq!(theme_display_name(PresetId::RosePine), "Rosé Pine");
        assert_eq!(theme_display_name(PresetId::SynthwaveEighty4), "Synthwave '84");
        assert_eq!(theme_display_name(PresetId::GitHubDark), "GitHub Dark");
        assert_eq!(theme_display_name(PresetId::OneDark), "One Dark");
    }

    #[test]
    fn theme_category_groups_match_the_design_spec() {
        // Built-in: the original 4.
        for id in [PresetId::Default, PresetId::Glass, PresetId::Neon, PresetId::Minimal] {
            assert_eq!(theme_category(id), PresetCategory::Builtin, "{id:?}");
        }
        // Apps: exactly Discord and Spotify.
        for id in [PresetId::Discord, PresetId::Spotify] {
            assert_eq!(theme_category(id), PresetCategory::Apps, "{id:?}");
        }
        // Editors: everything else, including the neon-leaning and Material-derived themes the
        // design spec folds into "Code editors" rather than a 4th/5th category.
        for id in [
            PresetId::Dracula,
            PresetId::Nord,
            PresetId::SolarizedDark,
            PresetId::SolarizedLight,
            PresetId::Gruvbox,
            PresetId::Catppuccin,
            PresetId::TokyoNight,
            PresetId::OneDark,
            PresetId::Monokai,
            PresetId::Material,
            PresetId::GitHubDark,
            PresetId::RosePine,
            PresetId::Everforest,
            PresetId::Kanagawa,
            PresetId::SynthwaveEighty4,
            PresetId::Ayu,
            PresetId::Palenight,
            PresetId::Cyberpunk,
        ] {
            assert_eq!(theme_category(id), PresetCategory::Editors, "{id:?}");
        }
    }

    #[test]
    fn exactly_three_categories_and_counts_match_the_design_spec() {
        // Builtin: 4, Apps: 2, Editors: 18 -- the exact 4/2/18 split (17+Material=18) the
        // design spec calls for, keeping the grid to exactly 3 groups.
        let (mut builtin, mut editors, mut apps) = (0, 0, 0);
        for id in ALL_24_PRESETS {
            match theme_category(id) {
                PresetCategory::Builtin => builtin += 1,
                PresetCategory::Editors => editors += 1,
                PresetCategory::Apps => apps += 1,
            }
        }
        assert_eq!(builtin, 4);
        assert_eq!(apps, 2);
        assert_eq!(editors, 18);
    }
}
