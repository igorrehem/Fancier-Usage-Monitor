// Settings schema, defaults, and migration for Claude Code Usage Monitor.
//
// This module owns the on-disk settings file (`%APPDATA%\ClaudeCodeUsageMonitor\settings.json`)
// and the full settings schema: the legacy state fields (tray offset, poll interval, visibility
// toggles, etc.) plus the newer appearance/typography/geometry/animation sections used by the
// settings window and renderer.
//
// `load()` is the migration path: any missing field (including entirely new sections) falls back
// to its serde default, so a settings.json written by an older build just gains the new sections
// with default values the first time it's loaded.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::native_interop::Color;

/// Current on-disk schema version. Bump this if a future migration needs to distinguish
/// old-shape files from new-shape files at runtime.
pub const CURRENT_VERSION: u32 = 1;

/// 15 minutes, in milliseconds. Default poll interval, ported from window.rs's POLL_15_MIN.
pub const POLL_15_MIN: u32 = 900_000;

fn default_version() -> u32 {
    CURRENT_VERSION
}

fn default_poll_interval() -> u32 {
    POLL_15_MIN
}

fn default_widget_visible() -> bool {
    true
}

fn default_show_claude_code() -> bool {
    true
}

fn default_show_codex() -> bool {
    false
}

fn default_show_antigravity() -> bool {
    false
}

/// Plain serializable RGBA color, decoupled from the native `Color` helper so the settings
/// schema doesn't depend on native_interop's Windows-specific conversions.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    #[allow(dead_code)] // used by the settings window / renderer in a later task
    pub fn to_color(&self) -> Color {
        Color {
            r: self.r,
            g: self.g,
            b: self.b,
            a: self.a,
        }
    }

    #[allow(dead_code)] // used by the settings window / renderer in a later task
    pub fn from_color(c: Color) -> Rgba {
        Rgba {
            r: c.r,
            g: c.g,
            b: c.b,
            a: c.a,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Default)]
pub enum Weight {
    #[default]
    Regular,
    SemiBold,
    Bold,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Default)]
pub enum Easing {
    Linear,
    #[default]
    Cubic,
    Spring,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum PresetId {
    Default,
    Glass,
    Neon,
    Minimal,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct PaletteStops {
    pub calm: Rgba,
    pub attention: Rgba,
    pub critical: Rgba,
}

fn default_opacity() -> f32 {
    1.0
}

// All color fields are OPTIONAL OVERRIDES. None => renderer keeps current adaptive behavior.
//
// Default is implemented by hand (not derived) because #[derive(Default)] would give `opacity`
// the primitive default (0.0), not `default_opacity()`. serde falls back to this struct-level
// Default::default() — not the field-level `#[serde(default = "...")]` functions — whenever the
// entire `appearance` object is absent from the JSON (e.g. deserializing "{}"), so the two must
// agree.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Appearance {
    #[serde(default)]
    pub palette: Option<PaletteStops>,
    #[serde(default)]
    pub background: Option<Rgba>,
    #[serde(default)]
    pub text: Option<Rgba>,
    #[serde(default)]
    pub divider: Option<Rgba>,
    #[serde(default = "default_opacity")]
    pub opacity: f32, // 0.0..=1.0
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            palette: None,
            background: None,
            text: None,
            divider: None,
            opacity: default_opacity(),
        }
    }
}

fn default_font_family() -> String {
    "Segoe UI".to_string()
}

// window.rs's CreateFontW call uses `sc(-12)` as nHeight with "Segoe UI" (see the row-label font
// setup around the SEGMENT_H rows). Windows' nHeight-to-point conversion is
// `nHeight = -(point_size * LogPixelsY / 72)`; at the 96-DPI baseline that `sc()` scales relative
// to, point_size = 12 * 72 / 96 = 9.0. So 9.0pt reproduces today's rendered text size.
fn default_font_size() -> f32 {
    9.0
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Typography {
    #[serde(default = "default_font_family")]
    pub family: String,
    #[serde(default = "default_font_size")]
    pub size_pt: f32,
    #[serde(default)]
    pub weight: Weight,
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            family: default_font_family(),
            size_pt: default_font_size(),
            weight: Weight::Regular,
        }
    }
}

// Geometry defaults are ported verbatim from window.rs's layout constants.
fn d_height() -> i32 {
    46 // WIDGET_HEIGHT
}
fn d_corner() -> i32 {
    2 // CORNER_RADIUS
}
fn d_bar_thickness() -> i32 {
    13 // SEGMENT_H
}
fn d_label_width() -> i32 {
    18 // LABEL_WIDTH
}
fn d_text_width() -> i32 {
    62 // TEXT_WIDTH
}
fn d_spacing() -> i32 {
    1 // SEGMENT_GAP
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Geometry {
    #[serde(default = "d_height")]
    pub height: i32, // WIDGET_HEIGHT = 46
    #[serde(default = "d_corner")]
    pub corner_radius: i32, // CORNER_RADIUS = 2
    #[serde(default = "d_bar_thickness")]
    pub bar_thickness: i32, // SEGMENT_H = 13
    #[serde(default = "d_label_width")]
    pub label_width: i32, // LABEL_WIDTH = 18
    #[serde(default = "d_text_width")]
    pub text_width: i32, // TEXT_WIDTH = 62
    #[serde(default = "d_spacing")]
    pub spacing: i32, // SEGMENT_GAP = 1
}

impl Default for Geometry {
    fn default() -> Self {
        Self {
            height: d_height(),
            corner_radius: d_corner(),
            bar_thickness: d_bar_thickness(),
            label_width: d_label_width(),
            text_width: d_text_width(),
            spacing: d_spacing(),
        }
    }
}

fn t() -> bool {
    true
}
fn d_fill_speed() -> f32 {
    1.0
}
fn d_shimmer_speed() -> f32 {
    0.5
}
fn d_shimmer_intensity() -> f32 {
    0.3
}
fn d_glow_threshold() -> f32 {
    0.85
}
fn d_glow_intensity() -> f32 {
    0.6
}
fn d_fade_ms() -> u32 {
    200
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct FillAnim {
    #[serde(default = "t")]
    pub on: bool,
    #[serde(default)]
    pub easing: Easing,
    #[serde(default = "d_fill_speed")]
    pub speed: f32,
}

impl Default for FillAnim {
    fn default() -> Self {
        Self {
            on: t(),
            easing: Easing::default(),
            speed: d_fill_speed(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ShimmerAnim {
    #[serde(default = "t")]
    pub on: bool,
    #[serde(default = "d_shimmer_speed")]
    pub speed: f32,
    #[serde(default = "d_shimmer_intensity")]
    pub intensity: f32,
}

impl Default for ShimmerAnim {
    fn default() -> Self {
        Self {
            on: t(),
            speed: d_shimmer_speed(),
            intensity: d_shimmer_intensity(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct AlertGlowAnim {
    #[serde(default = "t")]
    pub on: bool,
    #[serde(default = "d_glow_threshold")]
    pub threshold: f32,
    #[serde(default = "d_glow_intensity")]
    pub intensity: f32,
}

impl Default for AlertGlowAnim {
    fn default() -> Self {
        Self {
            on: t(),
            threshold: d_glow_threshold(),
            intensity: d_glow_intensity(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct FadeSlideAnim {
    #[serde(default = "t")]
    pub on: bool,
    #[serde(default = "d_fade_ms")]
    pub duration_ms: u32,
}

impl Default for FadeSlideAnim {
    fn default() -> Self {
        Self {
            on: t(),
            duration_ms: d_fade_ms(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default)]
pub struct AnimationSettings {
    #[serde(default)]
    pub reduce_motion: bool,
    #[serde(default)]
    pub fill: FillAnim,
    #[serde(default)]
    pub shimmer: ShimmerAnim,
    #[serde(default)]
    pub alert_glow: AlertGlowAnim,
    #[serde(default)]
    pub fade_slide: FadeSlideAnim,
    #[serde(default)]
    pub preset: Option<PresetId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_version")]
    pub version: u32,
    // ---- existing fields (ported verbatim from window.rs's SettingsFile) ----
    #[serde(default)]
    pub tray_offset: i32,
    #[serde(default)]
    pub taskbar_index: usize,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_update_check_unix: Option<u64>,
    #[serde(default = "default_widget_visible")]
    pub widget_visible: bool,
    #[serde(default = "default_show_claude_code")]
    pub show_claude_code: bool,
    #[serde(default = "default_show_codex")]
    pub show_codex: bool,
    #[serde(default = "default_show_antigravity")]
    pub show_antigravity: bool,
    // ---- new sections ----
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(default)]
    pub typography: Typography,
    #[serde(default)]
    pub geometry: Geometry,
    #[serde(default)]
    pub animation: AnimationSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            tray_offset: 0,
            taskbar_index: 0,
            poll_interval_ms: default_poll_interval(),
            language: None,
            last_update_check_unix: None,
            widget_visible: true,
            show_claude_code: true,
            show_codex: false,
            show_antigravity: false,
            appearance: Appearance::default(),
            typography: Typography::default(),
            geometry: Geometry::default(),
            animation: AnimationSettings::default(),
        }
    }
}

impl Settings {
    /// A fresh `Settings::default()` with the state-derived fields copied over from `current`
    /// instead of reset: `tray_offset`, `taskbar_index`, `poll_interval_ms`, `language`,
    /// `last_update_check_unix`, `widget_visible`, `show_claude_code`, `show_codex`,
    /// `show_antigravity`. Everything else (appearance/typography/geometry/animation) comes
    /// back as `Settings::default()`'s values.
    ///
    /// This is the settings window's Reset button: the user wants the widget's *look* put back
    /// to factory defaults without also relocating it, changing its poll cadence, losing which
    /// models it's tracking, forgetting a language override, or re-triggering an update check
    /// that already happened. The field list mirrors exactly what `window::save_state_settings`
    /// treats as "state-derived, must survive" a settings write -- these two lists must be kept
    /// in sync if either grows a new state-derived field.
    pub fn default_preserving_position(current: Settings) -> Settings {
        Settings {
            tray_offset: current.tray_offset,
            taskbar_index: current.taskbar_index,
            poll_interval_ms: current.poll_interval_ms,
            language: current.language,
            last_update_check_unix: current.last_update_check_unix,
            widget_visible: current.widget_visible,
            show_claude_code: current.show_claude_code,
            show_codex: current.show_codex,
            show_antigravity: current.show_antigravity,
            ..Settings::default()
        }
    }
}

pub fn settings_path() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(appdata)
        .join("ClaudeCodeUsageMonitor")
        .join("settings.json")
}

/// Load settings from disk, applying defaults for anything missing or unparsable.
///
/// This is the migration path: a legacy file (or one from an older build missing the newer
/// appearance/typography/geometry/animation sections) simply gets serde defaults for whatever
/// isn't present. Any I/O or parse error yields a fresh `Settings::default()`.
pub fn load() -> Settings {
    let content = match std::fs::read_to_string(settings_path()) {
        Ok(c) => c,
        Err(_) => return Settings::default(),
    };
    let mut settings: Settings = serde_json::from_str(&content).unwrap_or_default();
    if !settings.show_claude_code && !settings.show_codex && !settings.show_antigravity {
        settings.show_claude_code = true;
    }
    settings.version = CURRENT_VERSION;
    settings
}

pub fn save(s: &Settings) {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_json_gets_defaults() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.version, CURRENT_VERSION);
        assert_eq!(s.animation.fill.on, true);
        assert_eq!(s.appearance.opacity, 1.0);
        assert!(s.geometry.height > 0);
        assert!(s.appearance.palette.is_none());
    }

    #[test]
    fn legacy_file_preserves_known_fields() {
        let legacy = r#"{"poll_interval_ms":900000,"show_claude_code":true,"widget_visible":false}"#;
        let s: Settings = serde_json::from_str(legacy).unwrap();
        assert_eq!(s.poll_interval_ms, 900000);
        assert_eq!(s.widget_visible, false);
        assert_eq!(s.show_claude_code, true);
        assert_eq!(s.appearance.opacity, 1.0);
    }

    #[test]
    fn default_preserving_position_resets_appearance_but_keeps_state_fields() {
        let mut current = Settings::default();
        // Non-default appearance/typography/geometry/animation, so we can confirm it's reset.
        current.appearance.opacity = 0.4;
        current.appearance.background = Some(Rgba { r: 9, g: 9, b: 9, a: 255 });
        current.typography.family = "Comic Sans MS".to_string();
        current.typography.size_pt = 24.0;
        current.geometry.height = 99;
        current.animation.fill.on = false;
        current.animation.reduce_motion = true;
        // State-derived fields the Reset button must preserve.
        current.tray_offset = 123;
        current.taskbar_index = 2;
        current.poll_interval_ms = 60_000;
        current.language = Some("pt-BR".to_string());
        current.last_update_check_unix = Some(1_700_000_000);
        current.widget_visible = false;
        current.show_claude_code = false;
        current.show_codex = true;
        current.show_antigravity = true;

        let reset = Settings::default_preserving_position(current);

        // Appearance/typography/geometry/animation reset to Settings::default()'s values.
        let defaults = Settings::default();
        assert_eq!(reset.appearance.opacity, defaults.appearance.opacity);
        assert_eq!(reset.appearance.background, None);
        assert_eq!(reset.typography.family, defaults.typography.family);
        assert_eq!(reset.typography.size_pt, defaults.typography.size_pt);
        assert_eq!(reset.geometry.height, defaults.geometry.height);
        assert_eq!(reset.animation.fill.on, true);
        assert_eq!(reset.animation.reduce_motion, false);

        // State-derived fields preserved from `current`.
        assert_eq!(reset.tray_offset, 123);
        assert_eq!(reset.taskbar_index, 2);
        assert_eq!(reset.poll_interval_ms, 60_000);
        assert_eq!(reset.language.as_deref(), Some("pt-BR"));
        assert_eq!(reset.last_update_check_unix, Some(1_700_000_000));
        assert_eq!(reset.widget_visible, false);
        assert_eq!(reset.show_claude_code, false);
        assert_eq!(reset.show_codex, true);
        assert_eq!(reset.show_antigravity, true);
    }

    #[test]
    fn roundtrip_preserves_new_sections() {
        let mut s = Settings::default();
        s.geometry.height = 40;
        s.appearance.background = Some(Rgba { r: 1, g: 2, b: 3, a: 4 });
        let js = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&js).unwrap();
        assert_eq!(back.geometry.height, 40);
        assert_eq!(back.appearance.background, Some(Rgba { r: 1, g: 2, b: 3, a: 4 }));
    }
}
