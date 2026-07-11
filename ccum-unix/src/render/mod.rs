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
//! `Canvas`, whereas `Canvas` itself is cheap and rebuilt fresh every `RedrawRequested`. The
//! real usage-bar widget (`bars::draw_bars`, Task 7's frame-content entry point wired into
//! `WindowEvent::RedrawRequested` in `main.rs`) lives in the sibling `bars` module, built
//! entirely on this module's `Canvas` primitives -- see `ccum-unix/src/main.rs`'s
//! double-buffering wiring for how the two fit together each frame.

pub mod animations;
pub mod appearance;
pub mod bars;
pub mod controls;
pub mod font;
mod layout;
pub mod presets;
pub mod size;
pub mod text;
pub mod update;

use tiny_skia::{FillRule, Mask, Paint, Path, PathBuilder, Pixmap, Transform};

pub use tiny_skia::{Color, Rect};

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
    /// Active rectangular clip (Task 12: the Presets section's scrollable card grid), or `None`
    /// for no clipping. See `set_clip_rect`'s doc comment.
    clip_rect: Option<Rect>,
    /// `clip_rect` pre-rasterized into a full-canvas alpha `Mask`, consumed by every fill
    /// primitive's own `Option<&Mask>` parameter -- see `set_clip_rect`'s doc comment for why
    /// this is built once per `set_clip_rect` call rather than per draw call.
    clip_mask: Option<Mask>,
}

impl Canvas {
    /// Allocates a new, fully-transparent `width` x `height` canvas. Returns `None` if
    /// `width`/`height` is zero (matches `tiny_skia::Pixmap::new`'s own contract).
    pub fn new(width: u32, height: u32) -> Option<Self> {
        Pixmap::new(width, height).map(|pixmap| Self { pixmap, clip_rect: None, clip_mask: None })
    }

    /// Confines every subsequent `fill_rect`/`fill_rounded_rect`/`fill_circle`/glyph-blend call
    /// (`blend_pixel`) to `rect`'s bounds, replacing any previously active clip; `None` clears it
    /// (equivalent to `clear_clip`). This is `tiny-skia`'s nearest equivalent to GDI's
    /// persistent per-DC clip region (`SelectClipRgn`), which
    /// `ccum-windows/src/config_window.rs::draw_presets_controls` uses to keep the Presets
    /// section's scrolled-out cards from bleeding into the preview strip/button bar around it --
    /// see that function's doc comment. Unlike `fill_rect_clipped_to_rounded_rect` (a per-call,
    /// single-shape clip already established by Task 7 for usage-bar segments), this is a
    /// *persistent*, plain axis-aligned rect clip meant to span many heterogeneous draw calls
    /// (rect fills AND text) in one scope -- Task 12's Presets section is this crate's first
    /// caller that needs that shape, so the `Mask` is built once here (not re-rasterized on every
    /// one of the ~30 draw calls -- intro + 3 headers + 24 cards + scrollbar thumb -- a single
    /// frame of that section makes) and cached in `clip_mask` until the next `set_clip_rect`/
    /// `clear_clip` call. `blend_pixel` (glyph compositing) has no `Mask`-consuming API to hook
    /// into the way `Pixmap::fill_rect`/`fill_path` do, so it instead checks the plain `clip_rect`
    /// bounds directly -- cheaper than sampling the mask's alpha per glyph pixel, and exact since
    /// the clip is always a hard-edged rectangle (no anti-aliased mask edge text would need to
    /// blend against).
    pub fn set_clip_rect(&mut self, rect: Option<Rect>) {
        self.clip_mask = rect.and_then(|r| {
            let mut mask = Mask::new(self.pixmap.width(), self.pixmap.height())?;
            let path = PathBuilder::from_rect(r);
            mask.fill_path(&path, FillRule::Winding, true, Transform::identity());
            Some(mask)
        });
        self.clip_rect = rect;
    }

    /// Clears any active clip set by `set_clip_rect`. Mirrors GDI's
    /// `SelectClipRgn(hdc, HRGN::default())` reset call `draw_presets_controls` makes once it's
    /// done drawing the Presets section -- see that function's doc comment for why a reset
    /// (rather than a save/restore of some prior clip) is safe there, and why the same reasoning
    /// carries over here (this crate's `Canvas` is rebuilt fresh every `RedrawRequested`, so
    /// there is never a "prior clip" to restore in the first place).
    pub fn clear_clip(&mut self) {
        self.clip_mask = None;
        self.clip_rect = None;
    }

    // Not called by `bars.rs` (which reads `Rect`/model geometry, not the canvas's own
    // extent, for horizontal layout -- everything is anchored from the left divider), but a
    // natural, minimal companion to `height()` (which IS called, for vertical row layout)
    // that later widget-layout code (e.g. Task 9's popup panel) will need.
    #[allow(dead_code)]
    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

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
    /// `ccum-windows/src/controls.rs::fill_rect` (a `FillRect` GDI call) plays there -- used by
    /// `bars.rs` for the widget's left divider strokes.
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        self.pixmap
            .fill_rect(rect, &paint, Transform::identity(), self.clip_mask.as_ref());
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
        self.pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            self.clip_mask.as_ref(),
        );
    }

    /// Fills `fill_rect` with `color`, but only within the rounded-rect region described by
    /// `clip_rect`/`radius` -- i.e. a clipped fill that keeps rounded corners intact even when
    /// `fill_rect` only covers part of `clip_rect`. Task 7's one new primitive, added for two
    /// call sites in `bars.rs`: a partially-filled usage-bar segment (fill width < segment
    /// width, but the segment's own corners must stay rounded) and the shimmer highlight band
    /// (swept across a bar's full rounded outline, must not poke past its corners).
    ///
    /// `ccum-windows/src/window.rs::draw_usage_bar`'s GDI equivalent built a
    /// `CreateRoundRectRgn` and called `SelectClipRgn` on the DC before `FillRect`, then
    /// restored the DC's clip afterwards -- GDI clip state lives on the device context itself.
    /// `tiny-skia` has no persistent DC-style clip; its nearest equivalent is `Mask`, a
    /// per-call alpha buffer (confirmed against the actual `tiny-skia-0.12.0` source:
    /// `Mask::new`/`Mask::fill_path` build a full-canvas 8-bit alpha mask from a path, and
    /// `Pixmap::fill_rect`'s last argument is `Option<&Mask>`). A fresh `Mask` is allocated
    /// per call here rather than cached -- this widget's canvas is small (well under
    /// 1000x100px) and at most a handful of these calls happen per frame (one partial segment
    /// plus one shimmer band per visible bar), so the extra allocation is negligible next to
    /// the rest of the frame's rasterization work.
    pub fn fill_rect_clipped_to_rounded_rect(
        &mut self,
        fill_rect: Rect,
        clip_rect: Rect,
        radius: f32,
        color: Color,
    ) {
        let Some(clip_path) = rounded_rect_path(clip_rect, radius) else {
            return;
        };
        let Some(mut mask) = Mask::new(self.pixmap.width(), self.pixmap.height()) else {
            return;
        };
        mask.fill_path(&clip_path, FillRule::Winding, true, Transform::identity());

        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        self.pixmap
            .fill_rect(fill_rect, &paint, Transform::identity(), Some(&mask));
    }

    /// Fills a circle with a solid color. Mirrors the role GDI's `Ellipse` (brush-filled, no
    /// pen) plays in `ccum-windows/src/controls.rs::Slider::draw`'s knob -- used by Task 10's
    /// `controls::Slider::draw` port for the same purpose. `tiny-skia-path` 0.12.0's
    /// `PathBuilder::push_circle` (confirmed against its own source -- see
    /// `rounded_rect_path`'s doc comment for the same kind of direct-source verification this
    /// module already does) builds the circular path directly, so no hand-rolled Bezier
    /// approximation is needed here the way `rounded_rect_path` needed one for corners.
    pub fn fill_circle(&mut self, cx: f32, cy: f32, r: f32, color: Color) {
        if r <= 0.0 {
            return;
        }
        let mut pb = PathBuilder::new();
        pb.push_circle(cx, cy, r);
        let Some(path) = pb.finish() else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        self.pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            self.clip_mask.as_ref(),
        );
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
        // `set_clip_rect`'s doc comment: glyph compositing has no `Mask`-consuming API to hook
        // into, so this checks the plain clip bounds directly instead -- exact, since the clip
        // is always a hard-edged rectangle. `clip_rect` uses the same (float, half-open-via-cast)
        // coordinate space `LRect`/`render::Rect` already use elsewhere in this crate.
        if let Some(clip) = self.clip_rect {
            if (x as f32) < clip.left() || (x as f32) >= clip.right() || (y as f32) < clip.top() || (y as f32) >= clip.bottom() {
                return;
            }
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
