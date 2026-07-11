// ccum-core: platform-agnostic core of the Claude Code Usage Monitor.
//
// Everything here is pure logic and data -- settings schema/persistence, style presets, the
// animation state machine, the usage poller, and localization data -- with zero dependency on
// the `windows` crate or any other OS-native UI toolkit. `ccum-windows` (and, in a later task,
// `ccum-unix`) depend on this crate and supply their own platform-specific rendering/windowing
// on top of it.
//
// Note (see the Task 1 report for full detail): `poller.rs` currently still contains
// Windows-specific implementation details of its own (`std::os::windows::process::CommandExt`,
// raw FFI to `Advapi32.dll`'s `CredReadW`/`CredFree`, WSL invocation via `wsl.exe`). It has no
// dependency on the `windows` crate itself, so it satisfies this crate's "zero windows-crate
// dependency" build constraint today, but it is not yet actually portable to macOS/Linux --
// that OS-path audit and split is a later task.

pub mod animation;
pub mod diagnose;
pub mod localization;
pub mod models;
pub mod poller;
pub mod presets;
pub mod settings;
