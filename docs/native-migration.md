# Rust runtime migration — in development

The `native-runtime` branch is replacing the Python runtime with a compiled Rust
application. The currently released installer still installs version 0.5.0. Do not
describe a source build as a completed binary release.

The Rust implementation includes the catalog and device preferences, numbered
favorites, groups/tags and bulk edits, static SSH import, guarded catalog Git sync,
the JSON command interface, and a Ratatui keyboard picker. Existing catalog IDs and
device file names are preserved. The development compatibility check runs the old
file implementation against the native executable, including their shared lock.

Developers can run `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`
and `cargo build --release --locked`. Git tests use temporary local repositories.
`scripts/check_native_compat.py --binary PATH` uses Python only to compare the old
implementation; it is not part of the native app runtime. On Unix, the existing
pseudo-terminal test accepts `SSH_SESSIONS_NATIVE_BINARY=PATH` to test the compiled
app's local-shell handoff, Ctrl+C, resizing and return to the picker.

Local Windows evidence on September 9, 2026: 18 native tests passed, Clippy passed,
and the release binary passed catalog/preferences/favorites/lock/import compatibility
with Python absent from its executable search path. An isolated tool pseudo-terminal
opened local PowerShell, interrupted a running sleep with Ctrl+C, executed a second
marker, and returned to the picker after exit. No desktop window was activated.
The enclosing PowerShell tool session reported exit 1 after the interrupt; the
app's final exit status still needs an independent direct-process check.

The first native CI run passed on Windows 2025, Ubuntu 24.04 and both macOS ARM
and Intel runners, including native local-shell interrupt/resize/return on Unix:
[run 34365641785](https://github.com/brant92good/ssh-session-tui/actions/runs/34365641785).
This does not establish real remote desktop support.

Windows dependency inspection found VCRUNTIME140.dll in the default build.
Rebuilding with `-C target-feature=+crt-static` removed the external CRT imports;
`dumpbin /DEPENDENTS` then listed only Windows system DLLs. The resulting executable
passed the file/lock compatibility check again. Release builds use this setting,
and Linux release builds use musl to avoid a dependency on a recent glibc. The
installer downloads the executable and verifies its SHA-256 hash before replacing
an existing app. It never installs Python, Rust or a separate C runtime.

Release gates still open: complete platform and CLI parity review, automatic
binary packaging/installation/update tests, real remote desktop qualification,
README screenshots from the native UI, and integration with Terminal Workspace's
native launch path. macOS remains beta. No existing installed Python runtime is
removed merely because this branch builds successfully.
