// ccum-core: platform-agnostic core of Fancier Usage Monitor.
//
// Everything here is pure logic and data -- settings schema/persistence, style presets, the
// animation state machine, the usage poller, and localization data -- with zero dependency on
// the `windows` crate or any other OS-native UI toolkit. `ccum-windows` (and, in a later task,
// `ccum-unix`) depend on this crate and supply their own platform-specific rendering/windowing
// on top of it.
//
// Note (see the Task 1 report for full detail, and the Task 2 report for how this was
// addressed): `poller.rs` has no dependency on the `windows` crate itself, so it always
// satisfied this crate's "zero windows-crate dependency" build constraint. It DID, however,
// contain several Windows-only mechanisms that would fail to compile at all on macOS/Linux
// (`std::os::windows::process::CommandExt`/`creation_flags`, raw FFI to `Advapi32.dll`'s
// `CredReadW`/`CredFree`, WSL invocation via `wsl.exe`). As of Task 2, every one of those is
// now `#[cfg(target_os = "windows")]`-gated, with a `#[cfg(not(target_os = "windows"))]`
// POSIX-native counterpart alongside it (direct `claude`/`codex` invocation instead of the
// WSL bridge or `.cmd`/`.ps1` launcher resolution, and a best-effort macOS Keychain /
// Linux secret-service credential read in place of Windows Credential Manager -- see the
// doc comments on `read_windows_generic_credential`'s macOS/Linux variants for the caveats
// on that last one). This crate is not yet build-verified on macOS/Linux from this
// (Windows-only) development machine: Windows build/test are fully verified, but a full
// `cargo build`/`test` for a non-Windows target requires a cross toolchain (C compiler +
// OpenSSL dev headers for `native-tls`, which this crate depends on directly) that isn't
// available here -- see the Task 2 report for exactly how far verification could get.

pub mod animation;
pub mod diagnose;
pub mod localization;
pub mod models;
pub mod poller;
pub mod presets;
pub mod settings;
