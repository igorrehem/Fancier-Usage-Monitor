//! CPU-rasterized rendering foundation for `ccum-unix`, built on `tiny-skia`.
//!
//! This module plays the same role `ccum-windows/src/window.rs` and `controls.rs`'s GDI
//! helpers (`fill_rect`/`draw_rounded_rect`/`DrawTextW` calls) play for the Windows build:
//! a small set of drawing primitives every later widget-rendering task (usage bars, controls,
//! settings sections) builds on. The rasterization backend is completely different
//! (`tiny-skia`'s CPU `Pixmap`/`Paint`/`Path` API instead of GDI device contexts), but the
//! *shape* of the API mirrors GDI's on purpose: `fill_rect`/`fill_rounded_rect` take the same
//! rect+color(+radius) arguments GDI's helpers did, so later ports of `controls.rs`'s drawing
//! logic stay close to a mechanical translation rather than a redesign.
//!
//! Text rendering (`cosmic-text` integration) lives in the sibling `text` module, not here,
//! because it needs a persistent `FontSystem`/`SwashCache` (expensive to rebuild -- font
//! enumeration happens once in `FontSystem::new()`) that must outlive any single frame's
//! `Canvas`, whereas `Canvas` itself is cheap and rebuilt fresh every `RedrawRequested` (see
//! this module's `paint` and `ccum-unix/src/main.rs`'s double-buffering wiring).

pub mod text;

use tiny_skia::{FillRule, Paint, Path, PathBuilder, Pixmap, Transform};

pub use tiny_skia::{Color, Rect};

use text::TextRenderer;

/// The app's brand accent color (`#D97757`), matching `ccum-windows`'s `theme.rs` accent.
/// Used here only as this task's placeholder proof-of-life content -- Task 7 replaces it
/// with the real usage-bar accent wiring.
///
/// A function, not a `const`, because `tiny_skia::Color::from_rgba8` isn't a `const fn`.
pub fn brand_accent() -> Color {
    Color::from_rgba8(0xD9, 0x77, 0x57, 0xFF)
}

/// A dark background color for the placeholder frame, close to `ccum-windows`'s dark-theme
/// default background (`#1C1C1C`).
fn placeholder_bg() -> Color {
    Color::from_rgba8(0x1C, 0x1C, 0x1C, 0xFF)
}

/// An off-screen CPU pixel buffer, wrapping a `tiny_skia::Pixmap`.
///
/// One `Canvas` is built fresh (or resized) every `WindowEvent::RedrawRequested` in
/// `main.rs`, painted into completely off-screen, and only presented to the window surface
/// in a single shot once the frame is fully drawn -- mirroring the exact "build off-screen,
/// present once" pattern `ccum-windows/src/window.rs`'s flicker fix (commit `2b6d4f3` on
/// `main`) established for the GDI implementation, for the same reason: presenting a
/// partially-drawn frame is a well-known source of visible tearing/flicker with any
/// immediate-mode CPU rasterizer painting straight onto a live window surface.
pub struct Canvas {
    pixmap: Pixmap,
}

impl Canvas {
    /// Allocates a new, fully-transparent `width` x `height` canvas. Returns `None` if
    /// `width`/`height` is zero (matches `tiny_skia::Pixmap::new`'s own contract).
    pub fn new(width: u32, height: u32) -> Option<Self> {
        Pixmap::new(width, height).map(|pixmap| Self { pixmap })
    }

    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    // Not yet called by this task's own placeholder `paint()` (which only exercises
    // `fill_rounded_rect`), but a natural, minimal companion to `width()` that later
    // widget-layout code (Task 7+) will need -- kept, not speculative beyond that.
    #[allow(dead_code)]
    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }

    // Not yet called -- an escape hatch for later tasks that need direct `Pixmap` access
    // (e.g. `tray.rs`'s mini-bar bitmap export) rather than going through `Canvas`'s own
    // primitive methods.
    #[allow(dead_code)]
    pub fn pixmap_mut(&mut self) -> &mut Pixmap {
        &mut self.pixmap
    }

    /// Fills the entire canvas with a solid color. Used at the start of every frame to
    /// establish an opaque base -- without this, undrawn areas would stay transparent black,
    /// which `main.rs`'s present step (which drops the alpha channel, assuming full opacity)
    /// would then render as solid black instead of the intended background.
    pub fn clear(&mut self, color: Color) {
        self.pixmap.fill(color);
    }

    /// Fills an axis-aligned rectangle with a solid color. Mirrors the role
    /// `ccum-windows/src/controls.rs::fill_rect` (a `FillRect` GDI call) plays there. Not yet
    /// called by this task's placeholder `paint()` (which only needs the rounded variant
    /// below), but required by this task's brief as one of the two minimum `Canvas`
    /// primitives -- `#[allow(dead_code)]` for the same reason
    /// `ccum-windows/src/controls.rs::draw_rounded_rect` carries one: a primitive built
    /// ahead of the widget code (Task 7+) that will call it.
    #[allow(dead_code)]
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        self.pixmap
            .fill_rect(rect, &paint, Transform::identity(), None);
    }

    /// Fills a rectangle with rounded corners. Mirrors the role
    /// `ccum-windows/src/controls.rs::draw_rounded_rect` (a `CreateRoundRectRgn` + `FillRgn`
    /// GDI call) plays there -- used for usage-bar tracks/fills and card-style controls.
    ///
    /// `tiny-skia-path` 0.12.0 has no built-in rounded-rect path constructor (verified by
    /// reading its `path_builder.rs` source directly -- `push_rect`/`push_oval`/`push_circle`
    /// exist, no `push_round_rect`), so the rounded-rect path is built by hand below using
    /// the standard four-cubic-Bezier circular-arc approximation (Kappa constant), the same
    /// technique used inside `tiny-skia`'s own stroking code for round joins/caps.
    pub fn fill_rounded_rect(&mut self, rect: Rect, radius: f32, color: Color) {
        let Some(path) = rounded_rect_path(rect, radius) else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        self.pixmap
            .fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }

    /// Blends a single glyph-coverage pixel into the canvas at `(x, y)`, using standard
    /// "source-over" compositing in `tiny-skia`'s premultiplied-RGBA8 pixel format. Used by
    /// `text::TextRenderer::draw_text`'s per-pixel callback from `cosmic_text::Buffer::draw`;
    /// not part of this module's public primitive set (glyph compositing is an internal
    /// detail of text rendering, not a primitive later widget code should call directly).
    pub(crate) fn blend_pixel(&mut self, x: i32, y: i32, r: u8, g: u8, b: u8, a: u8) {
        if a == 0 || x < 0 || y < 0 {
            return;
        }
        let (x, y) = (x as u32, y as u32);
        let width = self.pixmap.width();
        if x >= width || y >= self.pixmap.height() {
            return;
        }
        let idx = (y * width + x) as usize;
        let pixels = self.pixmap.pixels_mut();
        let dst = pixels[idx];

        let sa = a as u32;
        let inv_sa = 255 - sa;
        // Source is composited premultiplied-by-coverage; destination is already
        // premultiplied (tiny-skia's invariant), so a standard "over" blend is a plain sum.
        let sr = (r as u32 * sa) / 255;
        let sg = (g as u32 * sa) / 255;
        let sb = (b as u32 * sa) / 255;
        let dr = (dst.red() as u32 * inv_sa) / 255;
        let dg = (dst.green() as u32 * inv_sa) / 255;
        let db = (dst.blue() as u32 * inv_sa) / 255;
        let da = (dst.alpha() as u32 * inv_sa) / 255;

        let out_a = (sa + da).min(255) as u8;
        // sr <= sa and dr <= da hold by construction above, so sr+dr <= sa+da == out_a:
        // the premultiplied invariant (component <= alpha) holds without extra clamping.
        let out_r = (sr + dr) as u8;
        let out_g = (sg + dg) as u8;
        let out_b = (sb + db) as u8;

        if let Some(blended) = tiny_skia::PremultipliedColorU8::from_rgba(out_r, out_g, out_b, out_a) {
            pixels[idx] = blended;
        }
    }
}

/// Builds a rounded-rectangle path via four corner arcs approximated with cubic Beziers
/// (the standard "Kappa" magic-constant technique: `k = 0.5522847498...` makes a
/// cubic-Bezier quarter-arc a very close approximation of a true quarter-circle). Clamps
/// `radius` to at most half of the shorter side, matching how `CreateRoundRectRgn`-style
/// rounded rects degenerate gracefully on small rects rather than self-intersecting.
fn rounded_rect_path(rect: Rect, radius: f32) -> Option<Path> {
    let radius = radius.max(0.0).min(rect.width() / 2.0).min(rect.height() / 2.0);
    if radius <= 0.0 {
        return PathBuilder::from_rect(rect).into();
    }

    const KAPPA: f32 = 0.5522847498;
    let k = radius * KAPPA;
    let (x0, y0, x1, y1) = (rect.left(), rect.top(), rect.right(), rect.bottom());

    let mut pb = PathBuilder::new();
    pb.move_to(x0 + radius, y0);
    pb.line_to(x1 - radius, y0);
    pb.cubic_to(x1 - radius + k, y0, x1, y0 + radius - k, x1, y0 + radius);
    pb.line_to(x1, y1 - radius);
    pb.cubic_to(x1, y1 - radius + k, x1 - radius + k, y1, x1 - radius, y1);
    pb.line_to(x0 + radius, y1);
    pb.cubic_to(x0 + radius - k, y1, x0, y1 - radius + k, x0, y1 - radius);
    pb.line_to(x0, y0 + radius);
    pb.cubic_to(x0, y0 + radius - k, x0 + radius - k, y0, x0 + radius, y0);
    pb.close();
    pb.finish()
}

/// The frame-content entry point wired into `WindowEvent::RedrawRequested` in `main.rs`.
///
/// Task 6 scope only: this draws simple, non-trivial placeholder content (a filled
/// brand-accent rounded rect plus a line of text) purely as end-to-end proof that
/// `tiny-skia` fills/paths, `cosmic-text` shaping/rasterization, and the double-buffered
/// present pipeline all genuinely work together -- it is NOT the real usage-bar widget
/// (that's Task 7's job, which will replace this function's body with real bar/percentage
/// rendering driven by `ccum_core`'s usage data).
pub fn paint(canvas: &mut Canvas, text: &mut TextRenderer) {
    canvas.clear(placeholder_bg());

    let width = canvas.width() as f32;

    // A filled, rounded, brand-accent rectangle roughly centered in the top half of the
    // window -- stands in for where a usage bar will eventually render (Task 7).
    if let Some(rect) = Rect::from_xywh(24.0, 24.0, (width - 48.0).max(0.0), 28.0) {
        canvas.fill_rounded_rect(rect, 8.0, brand_accent());
    }

    // A line of placeholder text below the rect, rendered via cosmic-text -> tiny-skia.
    text.draw_text(
        canvas,
        24.0,
        72.0,
        "Claude Code Usage Monitor",
        16.0,
        Color::from_rgba8(0xF0, 0xF0, 0xF0, 0xFF),
    );
}
