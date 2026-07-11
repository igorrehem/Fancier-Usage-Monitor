// Pure-logic animation engine: easing functions and the AnimationClock/AnimationFrame state
// machine consumed by the renderer. No GDI/Win32 here -- just math and state, fully unit tested.
// The renderer integration is a later task; this module just needs to keep its public
// signatures stable.

use crate::settings::{AnimationSettings, Easing};
use std::f32::consts::PI;
use std::time::Duration;

/// Threshold below which a fill (or fade) is considered "settled" at its target.
const SETTLE_EPS: f32 = 0.001;

/// One full glow pulse cycle (dim -> bright -> dim) per second, in radians/sec.
const GLOW_PULSE_RATE: f32 = 2.0 * PI;

/// Evaluate an easing curve at `t`, after clamping `t` to `[0, 1]`.
///
/// - `Linear` is the identity.
/// - `Cubic` is a standard ease-in-out cubic.
/// - `Spring` is a damped-oscillator overshoot curve that settles at 1.0; the *result* is
///   clamped to `[0.0, 1.05]` to allow a small overshoot without runaway values.
#[allow(dead_code)] // consumed by the render-integration task
pub fn ease(kind: Easing, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match kind {
        Easing::Linear => t,
        Easing::Cubic => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
            }
        }
        Easing::Spring => {
            let v = 1.0 - (-6.0 * t).exp() * (6.0 * t).cos();
            v.clamp(0.0, 1.05)
        }
    }
}

/// A single rendered animation frame: everything the renderer needs to draw one tick.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // consumed by the render-integration task
pub struct AnimationFrame {
    pub fill_pcts: Vec<f32>,
    pub shimmer_phase: f32,
    pub glow_intensity: f32,
    pub fade_alpha: f32,
}

/// Pure state machine driving bar-fill, shimmer, alert-glow, and fade/slide animations.
///
/// Holds no timers or OS handles -- callers own the clock (they pass `dt` into `tick`) and the
/// render loop (they poll `active` to decide whether to keep ticking).
#[allow(dead_code)] // consumed by the render-integration task
pub struct AnimationClock {
    settings: AnimationSettings,

    // Per-bar fill animation state. All four vectors are always kept the same length.
    fill_pcts: Vec<f32>,
    fill_targets: Vec<f32>,
    fill_starts: Vec<f32>,
    fill_progress: Vec<f32>, // 0..1 progress through the current segment's easing curve

    shimmer_phase: f32,
    glow_pulse_phase: f32,

    fade_alpha: f32,
    fade_start: f32,
    fade_target: f32,
    fade_progress: f32, // 0..1 progress through the current fade ramp
}

impl AnimationClock {
    #[allow(dead_code)] // consumed by the render-integration task
    pub fn new(s: &AnimationSettings) -> Self {
        Self {
            settings: *s,
            fill_pcts: Vec::new(),
            fill_targets: Vec::new(),
            fill_starts: Vec::new(),
            fill_progress: Vec::new(),
            shimmer_phase: 0.0,
            glow_pulse_phase: 0.0,
            fade_alpha: 1.0,
            fade_start: 1.0,
            fade_target: 1.0,
            fade_progress: 1.0,
        }
    }

    #[allow(dead_code)] // consumed by the render-integration task
    pub fn apply_settings(&mut self, s: &AnimationSettings) {
        self.settings = *s;
    }

    #[allow(dead_code)] // consumed by the render-integration task
    pub fn set_targets(&mut self, targets: &[f32]) {
        let old_len = self.fill_pcts.len();
        let new_len = targets.len();

        if new_len > old_len {
            for &target in &targets[old_len..new_len] {
                // New bars start at their target: no animation on first data.
                self.fill_pcts.push(target);
                self.fill_targets.push(target);
                self.fill_starts.push(target);
                self.fill_progress.push(1.0);
            }
        } else if new_len < old_len {
            self.fill_pcts.truncate(new_len);
            self.fill_targets.truncate(new_len);
            self.fill_starts.truncate(new_len);
            self.fill_progress.truncate(new_len);
        }

        for i in 0..new_len {
            if self.fill_targets[i] != targets[i] {
                self.fill_starts[i] = self.fill_pcts[i];
                self.fill_targets[i] = targets[i];
                self.fill_progress[i] = 0.0;
            }
        }
    }

    #[allow(dead_code)] // consumed by the render-integration task
    pub fn trigger_fade_in(&mut self) {
        self.fade_start = self.fade_alpha;
        self.fade_target = 1.0;
        self.fade_progress = 0.0;
    }

    #[allow(dead_code)] // consumed by the render-integration task
    pub fn trigger_fade_out(&mut self) {
        self.fade_start = self.fade_alpha;
        self.fade_target = 0.0;
        self.fade_progress = 0.0;
    }

    /// Advance by `dt`. `usage_max` is the max target fill across bars, used for the glow
    /// threshold check. Returns the frame to render plus whether the clock still has
    /// continuous or in-progress work (so the caller can stop its timer when idle).
    #[allow(dead_code)] // consumed by the render-integration task
    pub fn tick(&mut self, dt: Duration, usage_max: f32) -> (AnimationFrame, bool) {
        let dt_secs = dt.as_secs_f32();
        let reduce_motion = self.settings.reduce_motion;

        // --- fill ---
        if self.settings.fill.on && !reduce_motion {
            for i in 0..self.fill_pcts.len() {
                if (self.fill_pcts[i] - self.fill_targets[i]).abs() >= SETTLE_EPS {
                    self.fill_progress[i] += dt_secs * self.settings.fill.speed;
                    if self.fill_progress[i] >= 1.0 {
                        self.fill_progress[i] = 1.0;
                        self.fill_pcts[i] = self.fill_targets[i];
                    } else {
                        let te = ease(self.settings.fill.easing, self.fill_progress[i]);
                        self.fill_pcts[i] =
                            self.fill_starts[i] + (self.fill_targets[i] - self.fill_starts[i]) * te;
                    }
                }
            }
        } else {
            for i in 0..self.fill_pcts.len() {
                self.fill_pcts[i] = self.fill_targets[i];
                self.fill_progress[i] = 1.0;
            }
        }
        let fill_unsettled = self
            .fill_pcts
            .iter()
            .zip(self.fill_targets.iter())
            .any(|(c, t)| (c - t).abs() >= SETTLE_EPS);

        // --- shimmer ---
        let shimmer_running = self.settings.shimmer.on && !reduce_motion;
        if shimmer_running {
            self.shimmer_phase =
                (self.shimmer_phase + dt_secs * self.settings.shimmer.speed).rem_euclid(1.0);
        } else {
            self.shimmer_phase = 0.0;
        }

        // --- alert glow ---
        let glow_pulsing =
            self.settings.alert_glow.on && !reduce_motion && usage_max >= self.settings.alert_glow.threshold;
        let glow_intensity = if glow_pulsing {
            self.glow_pulse_phase += dt_secs * GLOW_PULSE_RATE;
            self.settings.alert_glow.intensity * (0.5 + 0.5 * self.glow_pulse_phase.sin())
        } else {
            self.glow_pulse_phase = 0.0;
            0.0
        };

        // --- fade ---
        if self.settings.fade_slide.on && !reduce_motion {
            if self.fade_progress < 1.0 {
                let duration_secs = (self.settings.fade_slide.duration_ms as f32 / 1000.0).max(1e-6);
                self.fade_progress += dt_secs / duration_secs;
                if self.fade_progress >= 1.0 {
                    self.fade_progress = 1.0;
                    self.fade_alpha = self.fade_target;
                } else {
                    self.fade_alpha =
                        self.fade_start + (self.fade_target - self.fade_start) * self.fade_progress;
                }
            }
        } else {
            self.fade_alpha = self.fade_target;
            self.fade_progress = 1.0;
        }
        let fade_in_progress = self.fade_progress < 1.0;

        let active = fill_unsettled || shimmer_running || glow_pulsing || fade_in_progress;

        let frame = AnimationFrame {
            fill_pcts: self.fill_pcts.clone(),
            shimmer_phase: self.shimmer_phase,
            glow_intensity,
            fade_alpha: self.fade_alpha,
        };
        (frame, active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{AnimationSettings, Easing};
    use std::time::Duration;

    #[test]
    fn linear_identity() {
        assert!((ease(Easing::Linear, 0.5) - 0.5).abs() < 1e-6);
    }
    #[test]
    fn cubic_endpoints() {
        assert!(ease(Easing::Cubic, 0.0).abs() < 1e-6);
        assert!((ease(Easing::Cubic, 1.0) - 1.0).abs() < 1e-6);
    }
    #[test]
    fn cubic_below_linear_early() {
        assert!(ease(Easing::Cubic, 0.25) < 0.25);
    }
    #[test]
    fn spring_settles_at_one() {
        assert!((ease(Easing::Spring, 1.0) - 1.0).abs() < 1e-2);
    }
    #[test]
    fn clamps_input() {
        assert_eq!(ease(Easing::Linear, -3.0), 0.0);
        assert_eq!(ease(Easing::Linear, 3.0), 1.0);
    }

    #[test]
    fn fill_converges_and_goes_idle() {
        let mut s = AnimationSettings::default();
        s.shimmer.on = false;
        s.alert_glow.on = false;
        let mut c = AnimationClock::new(&s);
        c.set_targets(&[0.0]);
        c.set_targets(&[0.8]);
        let (mut f, mut active) = (AnimationFrame::default(), true);
        for _ in 0..600 {
            let r = c.tick(Duration::from_millis(16), 0.8);
            f = r.0;
            active = r.1;
            if !active {
                break;
            }
        }
        assert!((f.fill_pcts[0] - 0.8).abs() < 0.01);
        assert!(!active);
    }

    #[test]
    fn reduce_motion_snaps() {
        let mut s = AnimationSettings::default();
        s.reduce_motion = true;
        let mut c = AnimationClock::new(&s);
        c.set_targets(&[0.0]);
        c.set_targets(&[0.9]);
        let (f, active) = c.tick(Duration::from_millis(16), 0.9);
        assert!((f.fill_pcts[0] - 0.9).abs() < 1e-6);
        assert!(!active);
    }

    #[test]
    fn glow_zero_below_threshold() {
        let mut s = AnimationSettings::default();
        s.alert_glow.on = true;
        s.alert_glow.threshold = 0.9;
        let mut c = AnimationClock::new(&s);
        let (f, _) = c.tick(Duration::from_millis(16), 0.5);
        assert_eq!(f.glow_intensity, 0.0);
    }

    #[test]
    fn glow_active_above_threshold() {
        let mut s = AnimationSettings::default();
        s.alert_glow.on = true;
        s.alert_glow.threshold = 0.5;
        s.shimmer.on = false;
        s.fill.on = false;
        let mut c = AnimationClock::new(&s);
        c.set_targets(&[0.9]);
        let (_, active) = c.tick(Duration::from_millis(16), 0.9);
        assert!(active);
    }

    #[test]
    fn shimmer_phase_in_range() {
        let mut s = AnimationSettings::default();
        s.shimmer.on = true;
        let mut c = AnimationClock::new(&s);
        let (f, active) = c.tick(Duration::from_millis(16), 0.5);
        assert!(f.shimmer_phase >= 0.0 && f.shimmer_phase < 1.0);
        assert!(active);
    }
}
