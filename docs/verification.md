# Verification

The production candidate is **SSH Sessions 0.6.0**, implemented in Rust.
macOS remains **beta**. Hosted pseudo-terminal tests do not qualify every
physical desktop, terminal emulator, SSH agent, proxy or VPN configuration.

## Native checks

On September 10, 2026, the 26 native tests passed locally on Windows. These cover
catalog validation, stale edits, route/favorite persistence, Unicode case folding,
groups, imports, explicit fallback, modal input and guarded Git sync. Clippy
passed with warnings denied, including the screenshot exporter.

The new Windows test owns a ConPTY without opening a desktop window. It launches
the actual native picker, opens Local terminal, executes nonce output, interrupts
a running PowerShell sleep with Ctrl+C, executes a second nonce, resizes, exits
the shell, returns to the picker and quits with **exit code 0**. This replaces
the earlier ambiguous observation through an outer PowerShell tool wrapper.

[Native CI](https://github.com/brant92good/ssh-session-tui/actions/workflows/native.yml)
builds Windows x64, Linux x64/ARM64 using musl, and macOS ARM64/Intel. It runs the
same tests, local-shell PTY checks and binary installer checks. Linux additionally
uses a temporary loopback OpenSSH server with test-only keys and configuration
to check remote commands, Ctrl+C, resizing, logout and return to the native picker.
A loopback server proves the SSH handoff; it does not prove an external network
or a specific third-party proxy.

## Saved-data and installer checks

`scripts/check_native_compat.py` runs the compiled binary against files created
by the legacy implementation, then reads native writes through that old reader.
It covers catalogs, route preferences, favorite filenames, shared process locks,
groups, custom SSH import bindings and argument arrays. Fixtures include sharp S,
dotted I, final sigma, a ligature and Cherokee in a Windows catalog path, numeric
string ports and UTF-8 BOMs. No real catalog or SSH host is used.

The root installers download a binary, check its SHA-256 hash and version, and
replace only the owned app command. `scripts/check_native_install.py` covers
fresh install, repeated update, invalid checksums, unrelated-directory refusal,
spaces/Unicode/quotes in paths, preserved favorites, retained legacy runtimes and
inherited Python/Conda variables. Local checks disable PATH modifications; CI
also checks PATH setup on disposable runners.

A native Windows build was inspected with `dumpbin /DEPENDENTS`: static CRT
linking removes the separate VCRUNTIME DLL import. Windows system DLLs remain
normal dependencies. Linux artifacts use musl. The app does not launch Python.

Tag publication creates a prerelease only after native checks pass. The workflow
then runs `scripts/check_release_install.py` on all five platforms against the
**public HTTPS one-command installer and those released assets**, covering fresh
installation, update, rejection of a wrong checksum and preserved favorite data.
Stable promotion requires those checks and independent review.

## Reproduce

```sh
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --release --locked
python scripts/check_native_compat.py --binary target/release/ssh-sessions
python scripts/check_native_install.py --binary target/release/ssh-sessions
```

Append `.exe` to binary paths on Windows. Python is a developer-only migration
oracle here, not an app dependency. Git tests use temporary local repositories.
In the owner's agent workspace, every Git command and tests that invoke it must
run outside the sandbox.

Generate images with:

```sh
cargo run --locked --example capture --features screenshots
```

The exporter renders the real Ratatui screen buffer using demonstration data.
The PNG inspection uses a headless browser, without activating desktop windows.
The SVG files contain no live hosts, local account paths or credentials.

## Limits and previous evidence

Native SSH handoff on a physical macOS desktop remains unqualified. So do every
Cloudflare helper, VPN state, custom shell profile and terminal emulator. The
Windows Terminal shortcut/focus behavior belongs to Terminal Workspace and has
its own qualification; it is not established by this leaf's PTY tests.

Version 0.5's Python implementation previously passed real Windows remote login
and returned to the picker in a small owned Terminal window. That observation
does not automatically qualify the rewritten native runtime. The legacy suite
remains available as a reference and is run by a separate CI workflow.
