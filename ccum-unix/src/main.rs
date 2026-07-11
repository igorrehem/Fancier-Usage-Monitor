// ccum-unix: macOS + Linux entry point for Claude Code Usage Monitor.
//
// Task 7 scope: replace Task 6's placeholder rect+text paint with the real usage-bar widget
// (`render::bars::draw_bars`), driven by a genuine `ccum_core::animation::AnimationClock`
// ticking on a real timer -- not the fixed-frequency placeholder redraw loop Task 6 didn't
// need yet. Real poller integration (replacing the fixed demo `UsageData`) is Task 8.
//
// Task 8 scope: a real `tray_icon::TrayIcon` (`tray.rs`), regenerated from real (or,
// documented-fallback, synthetic-demo) `ccum_core::poller::poll()` output on a timer sourced
// from `settings.poll_interval_ms` -- the SAME field `ccum-windows`/the (future) settings UI
// read/write, not a new hardcoded interval. The demo window from Tasks 6-7 is left running
// unchanged alongside the tray icon (still `bars::demo_usage_data()`, still animating) since it
// remains useful for visual iteration on the bar-rendering code; only the TRAY icon's bitmap is
// wired to real polling in this task.
//
// Non-blocking polling mechanism (winit 0.30, `EventLoopProxy` + a custom user event): the
// context brief for this task named two reasonable options -- a background thread posting
// results through a channel `about_to_wait` polls, or `winit`'s own `EventLoopProxy` +
// `ApplicationHandler::user_event` to wake the loop the instant a poll finishes. This file uses
// BOTH together, not one instead of the other: `ccum_core::poller::poll` (confirmed, per Task
// 2's audit, to genuinely shell out to external processes / hit real HTTPS endpoints, so it can
// block for a noticeable time) always runs on a `std::thread::spawn`-ed background thread --
// never on this event-loop thread -- and that thread's ONLY job on completion is
// `proxy.send_event(AppEvent::PollComplete(result))`. `EventLoopProxy::send_event` immediately
// wakes `run_app`'s loop (regardless of what `ControlFlow` was last set to) and delivers the
// result via `user_event`, rather than requiring `about_to_wait` to wake up on its own schedule
// and then discover a channel had something waiting in it. This is also the exact mechanism
// `tray_icon`'s own crate-level doc comment recommends for forwarding ITS click events to a
// winit event loop (see `tray_icon::TrayIconEvent::set_event_handler`'s doc example) -- so one
// `AppEvent` enum/one `EventLoopProxy` clone pattern covers both "a background poll finished"
// and "the tray icon was clicked" with no second mechanism needed. Direct port, in spirit, of
// `ccum-windows/src/window.rs`'s own `std::thread::spawn` + `PostMessageW(hwnd,
// WM_APP_USAGE_UPDATED, ...)` pattern (see `window.rs:1707`'s "Initial poll" thread) -- a
// background thread computing the slow result, then posting a message that wakes the owning
// event loop, is precisely what `EventLoopProxy::send_event` does for winit instead of Win32's
// message queue.
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
// Task 9 scope: a second, independent `winit` window (`panel::Panel`) -- the popup settings
// panel's shell + section-nav sidebar, toggled open/closed by a tray-icon left-click. See
// `panel.rs`'s own module doc comment for the window-creation/positioning/decoration reasoning
// and the sidebar port; this file's job is just routing: `App::window_event` dispatches each
// incoming `WindowEvent` to whichever window's `WindowId` it belongs to (`self.panel.window_id()`
// vs the pre-existing demo window), and `App::handle_tray_event` (called from `user_event`)
// turns a toggle-worthy tray click (`tray::handle_click`'s classification) into
// `self.panel.toggle(..)`, passing along the tray icon's live on-screen rect (via
// `tray_icon::TrayIcon::rect()`) as the popup's positioning anchor.
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

mod panel;
mod render;
mod tray;

use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ccum_core::animation::{AnimationClock, AnimationFrame};
use ccum_core::models::AppUsageData;
use ccum_core::poller::{self, PollError};
use ccum_core::settings::Settings;

use softbuffer::{Context, Surface};
use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use panel::Panel;
use render::bars::{self, UsageData};
use render::text::TextRenderer;

/// One animation tick, matching `ccum-windows`'s `IDT_ANIM`/`IDT_PREVIEW_ANIM` 16ms cadence
/// (`window.rs`'s `SetTimer(hwnd, IDT_ANIM, 16, None)`).
const ANIM_TICK: Duration = Duration::from_millis(16);

/// Custom user-event type for this app's `EventLoop<AppEvent>` -- see this file's module doc
/// comment ("Non-blocking polling mechanism") for why both variants share one `EventLoopProxy`.
enum AppEvent {
    /// A background poll thread (`App::trigger_poll`) finished; carries the same
    /// `Result<AppUsageData, PollError>` `ccum_core::poller::poll` itself returns.
    PollComplete(Result<AppUsageData, PollError>),
    /// Forwarded from `tray_icon::TrayIconEvent::set_event_handler` -- see `App::resumed`.
    Tray(TrayIconEvent),
}

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
    /// Task 7 demo data (see `render::bars::demo_usage_data`'s doc comment) -- still used ONLY
    /// by the Tasks 6-7 demo window. Task 8 wires real polling into `tray_usage` (below), the
    /// tray icon's own render input, not this field: the demo window stays exactly as it was so
    /// it remains useful for visual iteration on the bar-rendering code (see this file's
    /// top-of-module comment).
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

    // --- Task 8: tray icon + real polling state ---
    /// `None` until `resumed` successfully builds the tray icon; stays `None` forever if tray
    /// creation itself fails (e.g. no system tray/menu-bar host available). The demo window
    /// keeps running either way -- the tray icon is additive, never load-bearing for this
    /// process to stay alive.
    tray_icon: Option<TrayIcon>,
    /// Cloned into both the background poll thread and the `TrayIconEvent` click-forwarding
    /// closure so either can wake this event loop from off-thread -- see this file's top-of-
    /// module comment ("Non-blocking polling mechanism").
    proxy: EventLoopProxy<AppEvent>,
    /// The tray icon's own render input -- see `usage`'s doc comment for why this is a
    /// SEPARATE field from the demo window's data. Seeded with `tray::fallback_usage_data(0)`
    /// so the very first icon (drawn in `resumed`, before any poll has completed) still shows
    /// something plausible rather than an all-zero/empty bar.
    tray_usage: UsageData,
    /// Guards against overlapping poll attempts: `ccum_core::poller::poll` can genuinely block
    /// for seconds (it shells out to external `claude`/`codex` CLI tools or hits real HTTPS
    /// endpoints -- see Task 2's audit), so if one poll is still running when the next
    /// `poll_interval_ms` deadline arrives, `trigger_poll` skips starting a second one instead
    /// of piling up background threads. `Arc<AtomicBool>` (not a plain `bool`) because the
    /// background thread itself clears it on completion, from off-thread.
    poll_in_flight: Arc<AtomicBool>,
    /// Next wall-clock instant a poll attempt should be started. Seeded to `Instant::now()` so
    /// the very first `resumed` call fires an immediate startup poll (mirrors `window.rs`'s
    /// explicit "Initial poll" thread spawn -- `window.rs:1707` -- kept separate from its
    /// recurring `SetTimer(..., poll_interval_ms)` cadence).
    next_poll_at: Instant,
    /// Read once from `settings.poll_interval_ms` at startup -- the SAME field
    /// `ccum-windows`/the future settings UI read/write, not a new hardcoded interval (see this
    /// file's top-of-module comment).
    poll_interval: Duration,
    /// Whether ANY real poll has ever succeeded. Once `true`, a later transient failure keeps
    /// showing the last real numbers instead of snapping back to fallback demo data (matches
    /// `window.rs`'s own "keep the last good numbers on a transient error" behavior -- see
    /// `window.rs:2418`'s `s.session_text = "...".to_string()` branch, which likewise does NOT
    /// discard `s.session_pct`). Staying `false` for an entire run is the expected, disclosed
    /// outcome on a dev machine with no usable Claude Code/Codex/Antigravity credentials -- see
    /// the Task 8 report for what was actually observed on this machine.
    real_poll_ever_succeeded: bool,
    /// Poll-ATTEMPT counter (not wall-clock time), incremented once per `handle_poll_result`
    /// call, feeding `tray::fallback_usage_data`'s synthetic oscillation -- see that function's
    /// doc comment.
    poll_tick: u64,

    // --- Task 9: popup settings panel ---
    /// The popup panel's own window/surface/section-nav state -- see `panel.rs`'s module doc
    /// comment. Lives for the whole process; its window is created lazily on first open.
    panel: Panel,
}

impl App {
    fn new(proxy: EventLoopProxy<AppEvent>) -> Self {
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

        let poll_interval = Duration::from_millis(u64::from(settings.poll_interval_ms));

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
            tray_icon: None,
            proxy,
            tray_usage: tray::fallback_usage_data(0),
            poll_in_flight: Arc::new(AtomicBool::new(false)),
            next_poll_at: Instant::now(),
            poll_interval,
            real_poll_ever_succeeded: false,
            poll_tick: 0,
            panel: Panel::new(),
        }
    }
}

impl ApplicationHandler<AppEvent> for App {
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

        // --- Task 8: tray icon setup ---
        // `tray_icon`'s own crate-level doc comment: on macOS, an icon must not be created
        // before the event loop is actually running -- "the earliest you can create icons is on
        // `StartCause::Init`" -- and on Windows/Linux, tray creation must happen on the same
        // thread the (win32/gtk) event loop runs on. `resumed` (called once the loop is live,
        // guarded above so this whole block only runs once) satisfies both constraints; doing
        // this in `App::new`/`main` before `run_app` starts would not.
        let proxy_for_clicks = self.proxy.clone();
        TrayIconEvent::set_event_handler(Some(move |event| {
            let _ = proxy_for_clicks.send_event(AppEvent::Tray(event));
        }));

        match tray::render_icon(&self.settings, &self.tray_usage, true) {
            Ok(icon) => {
                let build_result = TrayIconBuilder::new()
                    .with_icon(icon)
                    .with_tooltip(tray::tooltip_text(&self.tray_usage))
                    .build();
                match build_result {
                    Ok(tray) => self.tray_icon = Some(tray),
                    Err(err) => {
                        eprintln!("ccum-unix: failed to create tray icon: {err}");
                    }
                }
            }
            Err(err) => {
                eprintln!("ccum-unix: failed to render initial tray icon bitmap: {err}");
            }
        }

        // Kick off the first poll immediately rather than waiting a full `poll_interval_ms` for
        // real (or fallback) numbers to appear -- mirrors `window.rs`'s separate "Initial poll"
        // thread spawn (`window.rs:1707`), distinct from its recurring timer cadence.
        self.trigger_poll();
    }

    /// Handles both `AppEvent` variants this app's `EventLoopProxy` can wake the loop with --
    /// see this file's top-of-module comment ("Non-blocking polling mechanism").
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::PollComplete(result) => self.handle_poll_result(result),
            AppEvent::Tray(event) => self.handle_tray_event(event_loop, &event),
        }
    }

    /// Routes every `WindowEvent` to whichever window it actually belongs to -- the Tasks 6-7
    /// demo window (this function's own `match`, unchanged from before Task 9) or the Task 9
    /// popup panel (`self.panel.handle_window_event`, checked first via `window_id`). Both
    /// windows are live members of the SAME event loop (`ActiveEventLoop::create_window` was
    /// called twice -- once in `resumed` for the demo window, once lazily in
    /// `panel::Panel::show` for the popup), so this dispatch-by-`WindowId` is the only thing
    /// keeping their `RedrawRequested`/`CloseRequested`/mouse-input handling from interfering
    /// with each other.
    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        if self.panel.window_id() == Some(window_id) {
            self.panel.handle_window_event(&event, &mut self.text);
            return;
        }

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
    ///
    /// Task 8 addition: also folds in the tray icon's poll-timer deadline (`next_poll_at`) --
    /// when a tray icon exists, the loop must keep waking itself up on `poll_interval_ms`
    /// cadence even while the demo window's animation is fully idle (`anim_active == false`),
    /// or the tray icon would never poll again after the startup animation settles. The actual
    /// poll COMPLETING doesn't need a scheduled wakeup of its own -- `EventLoopProxy::send_event`
    /// (see `trigger_poll`) wakes the loop immediately whenever that happens, regardless of
    /// `ControlFlow` -- only STARTING the next poll attempt needs a deadline here.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let mut wake_at: Option<Instant> = None;

        if self.anim_active {
            if Instant::now() >= self.next_frame_at {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
                self.next_frame_at = Instant::now() + ANIM_TICK;
            }
            wake_at = Some(self.next_frame_at);
        }

        if self.tray_icon.is_some() {
            if Instant::now() >= self.next_poll_at {
                self.trigger_poll();
            }
            wake_at = Some(match wake_at {
                Some(t) => t.min(self.next_poll_at),
                None => self.next_poll_at,
            });
        }

        match wake_at {
            Some(t) => event_loop.set_control_flow(ControlFlow::WaitUntil(t)),
            // Nothing left to animate and no tray icon to poll for: no scheduled wakeup at
            // all, so the process blocks on the OS event queue and idle CPU drops back to
            // ~0%, same end state `window.rs`'s `KillTimer(hwnd, IDT_ANIM)` achieves.
            None => event_loop.set_control_flow(ControlFlow::Wait),
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

    /// Starts a background poll if one isn't already running, and reschedules `next_poll_at`
    /// `poll_interval` out regardless (so a skipped-because-already-in-flight attempt doesn't
    /// spin-retry on every subsequent `about_to_wait` call). See this file's top-of-module
    /// comment for why `ccum_core::poller::poll` -- confirmed able to block for a noticeable
    /// time -- must never run directly on this event-loop thread, and why a background thread +
    /// `EventLoopProxy::send_event` (rather than a channel `about_to_wait` polls) was chosen.
    fn trigger_poll(&mut self) {
        self.next_poll_at = Instant::now() + self.poll_interval;

        // `swap(true, ...)` both reads the previous value and sets it atomically -- if it was
        // already `true`, a poll is genuinely still in flight, so bail without spawning a
        // second one (see `poll_in_flight`'s doc comment on `App`).
        if self.poll_in_flight.swap(true, Ordering::SeqCst) {
            return;
        }

        let show_claude_code = self.settings.show_claude_code;
        let show_codex = self.settings.show_codex;
        let show_antigravity = self.settings.show_antigravity;
        let proxy = self.proxy.clone();
        let in_flight = Arc::clone(&self.poll_in_flight);

        std::thread::spawn(move || {
            // The actual (potentially slow) blocking call -- see module doc comment.
            let result = poller::poll(show_claude_code, show_codex, show_antigravity);
            in_flight.store(false, Ordering::SeqCst);
            // `send_event` returning `Err` only happens if the event loop has already shut
            // down (the `EventLoopClosed` case) -- nothing useful to do about that from a
            // background thread that's about to exit anyway.
            let _ = proxy.send_event(AppEvent::PollComplete(result));
        });
    }

    /// Handles a completed poll (`AppEvent::PollComplete`, delivered via `user_event`):
    /// converts a real success into the tray's `UsageData`, falls back to CLEARLY-SYNTHETIC demo
    /// data on the first-ever failure (so the icon still visibly updates rather than silently
    /// doing nothing -- see `tray::fallback_usage_data`'s doc comment), and otherwise keeps the
    /// last known-good numbers on a later transient failure (mirrors `window.rs`'s own behavior
    /// -- see `real_poll_ever_succeeded`'s doc comment on `App`).
    fn handle_poll_result(&mut self, result: Result<AppUsageData, PollError>) {
        self.poll_tick = self.poll_tick.saturating_add(1);

        let should_repaint_icon = match result {
            Ok(data) => {
                self.real_poll_ever_succeeded = true;
                self.tray_usage = tray::build_usage_data(&data, &self.settings);
                true
            }
            Err(err) if !self.real_poll_ever_succeeded => {
                eprintln!(
                    "ccum-unix: real poll failed ({err:?}); showing synthetic FALLBACK data on \
                     the tray icon (see tray::fallback_usage_data's doc comment) -- this is \
                     expected on a dev machine with no usable Claude Code/Codex/Antigravity \
                     credentials for ccum_core::poller::poll to find"
                );
                self.tray_usage = tray::fallback_usage_data(self.poll_tick);
                true
            }
            Err(err) => {
                eprintln!(
                    "ccum-unix: poll failed ({err:?}) after at least one prior success; keeping \
                     the last known-good tray icon data instead of the failed attempt"
                );
                false
            }
        };

        if !should_repaint_icon {
            return;
        }

        let Some(tray) = &self.tray_icon else {
            return;
        };
        match tray::render_icon(&self.settings, &self.tray_usage, true) {
            Ok(icon) => {
                if let Err(err) = tray.set_icon(Some(icon)) {
                    eprintln!("ccum-unix: failed to update tray icon bitmap: {err}");
                }
                let _ = tray.set_tooltip(Some(tray::tooltip_text(&self.tray_usage)));
            }
            Err(err) => eprintln!("ccum-unix: failed to render tray icon bitmap: {err}"),
        }
    }

    /// Task 9: reacts to a raw `TrayIconEvent` (forwarded via `AppEvent::Tray`, see this file's
    /// top-of-module comment). `tray::handle_click` does the actual event-shape classification
    /// (which button, up vs down, single vs double click) and just returns whether this event
    /// should toggle the popup panel; if so, this method looks up the tray icon's current
    /// on-screen rect (`TrayIcon::rect()` -- `None` on Linux, where that method is documented
    /// unsupported, or on a transient query failure) and hands it to `panel::Panel::toggle` as
    /// the popup's positioning anchor (see `panel.rs`'s doc comment for how that rect is used).
    fn handle_tray_event(&mut self, event_loop: &ActiveEventLoop, event: &TrayIconEvent) {
        if !tray::handle_click(event) {
            return;
        }
        let anchor = self.tray_icon.as_ref().and_then(TrayIcon::rect).map(|r| panel::TrayAnchor {
            x: r.position.x,
            y: r.position.y,
            width: f64::from(r.size.width),
            height: f64::from(r.size.height),
        });
        self.panel.toggle(event_loop, anchor);
    }
}

fn main() {
    // `with_user_event` (rather than plain `EventLoop::new`, which is `EventLoop<()>`) is what
    // makes `EventLoopProxy<AppEvent>`/`ApplicationHandler<AppEvent>::user_event` available at
    // all -- see this file's top-of-module comment for why this app needs a custom user event.
    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .expect("ccum-unix: failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);

    let proxy = event_loop.create_proxy();
    let mut app = App::new(proxy);
    event_loop.run_app(&mut app).expect("ccum-unix: event loop exited with an error");
}
