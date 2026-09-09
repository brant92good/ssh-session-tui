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

Release gates still open: complete platform and CLI parity review, automatic
binary packaging/installation/update tests, real remote desktop qualification,
README screenshots from the native UI, and integration with Terminal Workspace's
native launch path. macOS remains beta. No existing installed Python runtime is
removed merely because this branch builds successfully.
