// ccum-unix: macOS + Linux entry point for Claude Code Usage Monitor.
//
// Task 7 scope: replace Task 6's placeholder rect+text paint with the real usage-bar widget
// (`render::bars::draw_bars`), driven by a genuine `ccum_core::animation::AnimationClock`
// ticking on a real timer -- not the fixed-frequency placeholder redraw loop Task 6 didn't
// need yet. Real poller integration (replacing the fixed demo `UsageData`) is Task 8.
//
// Animation timer mechanism (winit 0.30): winit has no built-in "repeating timer" the way
// Win32's `SetTimer`/`WM_TIMER` does (`ccum-windows/src/window.rs`'s `IDT_ANIM`). The
// documented winit-idiomatic replacement is `ControlFlow::WaitUntil` recomputed in
// `ApplicationHandler::about_to_wait` (called once per event-loop iteration, right before it
// goes to sleep): while the animation clock is still "active" after a tick, keep re-arming
// `WaitUntil(next_frame_at)` so the loop wakes itself up roughly every `ANIM_TICK` (~16ms) and
// requests a redraw only once that deadline has actually been reached (see `about_to_wait`'s
// doc comment -- calling `request_redraw()` unconditionally on every `about_to_wait` call was
// tried first and measured, via `Get-Process`'s `CPU` sampling, to busy-loop at ~140fps/~89%
// CPU instead of throttling to ~60fps, because a freshly queued redraw pre-empts the sleep
// `WaitUntil` was supposed to provide). Once the clock reports settled (`active == false`),
// set `ControlFlow::Wait` (no scheduled wake at all) so the loop goes fully idle and blocks on
// the next real OS event -- this is the exact "idle -> 0% CPU" discipline `window.rs`'s
// `KillTimer(hwnd, IDT_ANIM)` established (see `render_layered`'s doc comment there), just
// expressed through winit's control-flow API instead of a Win32 timer handle. A background
// thread posting `UserEvent`s was the other option considered; `WaitUntil` was chosen because
// it needs no extra thread, no cross-thread synchronization, and maps directly onto "the event
// loop already has an idle/active distinction built in" rather than bolting one on.
//
// winit 0.30's API: an `ApplicationHandler` impl driven by `EventLoop::run_app`, replacing
// the older `EventLoop::run(closure)` API (deprecated in 0.30, removed later). See
// https://docs.rs/winit/0.30.13/winit/application/trait.ApplicationHandler.html.
//
// softbuffer 0.4's API: a `Context<D>` (bound to a display handle) plus a `Surface<D, W>`
// (bound to a window handle) created from it; `Surface::resize` before every present (its
// size must always match the window's current size), `Surface::buffer_mut` to get a
// `&mut [u32]` to write pixels into, `Buffer::present` to hand them to the platform's
// windowing system. The window is wrapped in `Rc<Window>` because both `Context` and
// `Surface` need their own handle to it (the standard `softbuffer` + `winit` pairing
// pattern -- confirmed by reading `softbuffer-0.4.8`'s own source: `Context<D>`/`Surface<D,
// W>` are generic over anything implementing `raw_window_handle`'s
// `HasDisplayHandle`/`HasWindowHandle`, which `Rc<Window>` satisfies via `winit`'s blanket
// impls, same as a bare `Window` would, but shareable).

mod render;

use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ccum_core::animation::{AnimationClock, AnimationFrame};
use ccum_core::settings::Settings;

use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use render::bars::{self, UsageData};
use render::text::TextRenderer;

/// One animation tick, matching `ccum-windows`'s `IDT_ANIM`/`IDT_PREVIEW_ANIM` 16ms cadence
/// (`window.rs`'s `SetTimer(hwnd, IDT_ANIM, 16, None)`).
const ANIM_TICK: Duration = Duration::from_millis(16);

struct App {
    window: Option<Rc<Window>>,
    // Never read after `resumed` stores it -- kept alive only because `surface` was created
    // from it and, per softbuffer's documented contract, the `Context` a `Surface` was built
    // from must outlive that `Surface`. `#[allow(dead_code)]` because rustc's `dead_code`
    // lint only sees "written, never read" and can't see the drop-order requirement.
    #[allow(dead_code)]
    context: Option<Context<Rc<Window>>>,
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    /// Persistent across frames -- see `render::text::TextRenderer`'s doc comment for why
    /// this must NOT be rebuilt inside `Canvas`/every redraw.
    text: TextRenderer,

    settings: Settings,
    /// Task 7 demo data (see `render::bars::demo_usage_data`'s doc comment); Task 8 replaces
    /// this with real `ccum_core::poller` output.
    usage: UsageData,
    clock: AnimationClock,
    frame: AnimationFrame,
    /// Whether the last `clock.tick()` reported still-active work (fill unsettled, shimmer/
    /// glow pulsing, or a fade in progress). Read by `about_to_wait` to decide whether to
    /// keep scheduling wakeups -- mirrors `window.rs`'s `render_layered` return value feeding
    /// its own `KillTimer`/keep-running decision.
    anim_active: bool,
    /// `None` both before the first tick and whenever the animation has settled (mirrors
    /// `window.rs`'s `LAST_ANIM_TICK`, cleared on idle so a future kick starts clean instead
    /// of computing a huge `dt` against a stale timestamp).
    last_tick: Option<Instant>,
    /// The next wall-clock instant a redraw should actually be requested, while animating.
    /// `about_to_wait` only calls `request_redraw()` once `Instant::now()` has reached this --
    /// see `about_to_wait`'s doc comment for why an unconditional `request_redraw()` there
    /// defeats `WaitUntil` throttling entirely.
    next_frame_at: Instant,
}

impl Default for App {
    fn default() -> Self {
        let settings = Settings::default();
        let usage = bars::demo_usage_data();
        let mut clock = AnimationClock::new(&settings.animation);

        // Animate the demo bars in from 0 on startup, mirroring
        // `ccum-windows/src/config_window.rs`'s live-preview "set targets to zero, then to
        // the real demo numbers" dance so the fill genuinely animates rather than snapping
        // straight to its resting value on the very first frame.
        let targets = bars::ordered_bar_targets(&usage);
        clock.set_targets(&vec![0.0; targets.len()]);
        clock.set_targets(&targets);

        Self {
            window: None,
            context: None,
            surface: None,
            text: TextRenderer::new(),
            settings,
            usage,
            clock,
            frame: AnimationFrame::default(),
            anim_active: true,
            last_tick: None,
            next_frame_at: Instant::now(),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // `resumed` can fire more than once (e.g. on some platforms after a suspend/resume
        // cycle), so only create the window/context/surface the first time.
        if self.window.is_some() {
            return;
        }

        let (w, h) = bars::natural_size(&self.settings, &self.usage);
        // `PhysicalSize`, not `LogicalSize`: `bars.rs` deliberately does not scale its layout
        // constants by the display's DPI/scale factor (see that module's doc comment), so the
        // canvas must be exactly `w`x`h` *physical* pixels, or the widget draws too small
        // inside an oversized (DPI-scaled-up) window. `LogicalSize` was tried first and
        // confirmed (via a one-off debug print, then reverted) to produce a canvas scaled up
        // by the monitor's scale factor (e.g. 217x46 requested -> 271x58 actual physical
        // canvas at 125% scaling) -- `window.inner_size()` in `redraw()` always returns
        // physical pixels regardless of which `Size` variant created the window, so asking
        // for `Logical` here just meant every later read of that size was already "wrong" by
        // the scale factor before `bars.rs` ever saw it.
        let attributes = Window::default_attributes()
            .with_title("Claude Code Usage Monitor")
            .with_inner_size(winit::dpi::PhysicalSize::new(w, h));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Rc::new(window),
            Err(err) => {
                eprintln!("ccum-unix: failed to create window: {err}");
                event_loop.exit();
                return;
            }
        };

        let context = match Context::new(window.clone()) {
            Ok(context) => context,
            Err(err) => {
                eprintln!("ccum-unix: failed to create softbuffer context: {err}");
                event_loop.exit();
                return;
            }
        };

        let surface = match Surface::new(&context, window.clone()) {
            Ok(surface) => surface,
            Err(err) => {
                eprintln!("ccum-unix: failed to create softbuffer surface: {err}");
                event_loop.exit();
                return;
            }
        };

        window.request_redraw();
        self.window = Some(window);
        self.context = Some(context);
        self.surface = Some(surface);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) => {
                // The surface itself is resized lazily in `redraw` (right before painting,
                // using the window's current size at that moment) -- just ask for a repaint.
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }

    /// Called once per event-loop iteration, after all queued events have been dispatched and
    /// right before the loop would otherwise go to sleep -- the idiomatic winit hook for
    /// deciding both "should the loop keep waking itself up" and "should another frame be
    /// drawn". See this file's top-of-module comment for why `WaitUntil` (over a background
    /// timer thread) was chosen for this.
    ///
    /// Deliberately does NOT call `request_redraw()` unconditionally here -- an earlier
    /// version did, and it was measured (via debug logging + `Get-Process`'s `CPU` sampling)
    /// to busy-loop at roughly 140 real frames/sec (~7ms between ticks) instead of the
    /// intended ~60fps/16ms `ANIM_TICK` cadence, pegging a full CPU core. Root cause: queuing
    /// a fresh redraw request *inside* `about_to_wait` gives winit a pending event to process
    /// immediately, which takes priority over actually sleeping until the `WaitUntil` deadline
    /// just set -- i.e. `WaitUntil` never got the chance to throttle anything, because there
    /// was always more "work" to do the instant it was set. The fix: only call
    /// `request_redraw()` once `Instant::now()` has genuinely reached `next_frame_at`;
    /// otherwise just re-arm `WaitUntil(next_frame_at)` and let the loop actually sleep.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.anim_active {
            if Instant::now() >= self.next_frame_at {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
                self.next_frame_at = Instant::now() + ANIM_TICK;
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_at));
        } else {
            // Nothing left to animate: no scheduled wakeup at all, so the process blocks on
            // the OS event queue and idle CPU drops back to ~0%, same end state
            // `window.rs`'s `KillTimer(hwnd, IDT_ANIM)` achieves.
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

impl App {
    /// Ticks the animation clock, builds one full frame into an off-screen `Canvas`, then
    /// presents it to the window surface in one shot. No drawing call in `render::bars`
    /// (or anything it calls) ever touches `surface`/the softbuffer-mapped pixels directly --
    /// that separation is the double-buffering fix Task 6 ported from `ccum-windows`.
    fn redraw(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
            return;
        };

        let size = window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            // Zero-sized window (e.g. minimized on some platforms) -- nothing to paint.
            return;
        };

        if let Err(err) = surface.resize(width, height) {
            eprintln!("ccum-unix: failed to resize softbuffer surface: {err}");
            return;
        }

        // --- Animation tick --- `dt` is measured against the previous tick; the first tick
        // after a timer restart (or ever) has no previous sample, so it assumes one frame's
        // worth (16ms) rather than a huge or zero delta. Direct port of `window.rs`'s
        // `render_layered` dt-measurement.
        let now = Instant::now();
        let dt = match self.last_tick {
            Some(prev) => now.duration_since(prev),
            None => ANIM_TICK,
        };
        self.last_tick = Some(now);

        let usage_max = bars::ordered_bar_targets(&self.usage)
            .iter()
            .cloned()
            .fold(0.0f32, f32::max);
        let (frame, active) = self.clock.tick(dt, usage_max);
        self.frame = frame;
        self.anim_active = active;
        if !active {
            self.last_tick = None;
        }

        let Some(mut canvas) = render::Canvas::new(width.get(), height.get()) else {
            return;
        };
        bars::draw_bars(
            &mut canvas,
            &mut self.text,
            &self.settings,
            &self.frame,
            &self.usage,
            true, // is_dark: no OS dark/light-mode detection wired up yet (see bars.rs).
        );

        let mut buffer = match surface.buffer_mut() {
            Ok(buffer) => buffer,
            Err(err) => {
                eprintln!("ccum-unix: failed to acquire softbuffer buffer: {err}");
                return;
            }
        };

        // softbuffer's buffer format is opaque `0x00RRGGBB` per pixel; tiny-skia's Pixmap is
        // premultiplied RGBA8. `render::bars::draw_bars` always starts each frame with
        // `Canvas::clear`, so every pixel ends up fully opaque (alpha == 255) by the time we
        // get here -- a premultiplied color at alpha 255 is numerically identical to its
        // straight-alpha RGB, so dropping the alpha byte here is exact for this app's
        // always-opaque window, not an approximation.
        for (dst, src) in buffer.iter_mut().zip(canvas.pixmap().pixels().iter()) {
            *dst = (u32::from(src.red()) << 16) | (u32::from(src.green()) << 8) | u32::from(src.blue());
        }

        if let Err(err) = buffer.present() {
            eprintln!("ccum-unix: failed to present softbuffer buffer: {err}");
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("ccum-unix: failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::default();
    event_loop.run_app(&mut app).expect("ccum-unix: event loop exited with an error");
}
