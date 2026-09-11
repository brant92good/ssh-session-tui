# Verification

The current native prerelease is **SSH Sessions 0.8.0**, implemented in Rust.
macOS remains **beta**. Hosted pseudo-terminal tests do not qualify every
physical desktop, terminal emulator, SSH agent, proxy or VPN configuration.

## 0.8.0 prerelease (September 11, 2026)

The immutable [0.8.0 release](https://github.com/brant92good/ssh-session-tui/releases/tag/v0.8.0)
at `9a9d3a25d9f884052f3b538af7f8d97cdfafe70d` passed
[all eleven native release jobs](https://github.com/brant92good/ssh-session-tui/actions/runs/34521685615):
five platform builds/tests, publication and five actual released-installer jobs.
Windows x64, Linux x64/ARM64 and macOS Intel/Apple Silicon are included.
macOS terminal CI remains separate from desktop qualification.

Independent Windows checks downloaded the published binary and matched both its
sidecar and GitHub asset digest. SHA-256:
`c4797d09ebb9bcd0fd239dac2e6a899c10c89b82a019c60a085990100c54d14c`.
Actual tagged HTTPS installation passed on PowerShell 5.1 and 7: fresh install,
update, checksum rejection, saved favorites and a polluted Python/Conda environment.
The compiled app also ran with Python, Cargo and Conda excluded from PATH.

Two chooser/handoff checks passed against that exact released executable in an
owned Windows pseudo-terminal. They cover selection before launch, literal saved
paths, explicit route choice, stale edits, visible failure acknowledgement and
frozen arguments. The companion was a no-network test program. The separate
chooser-to-real-SFTP check below is source evidence; it was not rerun against this
release. An independent README and rendered-visual audit passed. No physical
desktop, personal SSH server or OS file drag/drop result is implied.

## Native checks

### Files chooser and saved paths — source qualification (September 11, 2026)

The current source adds the Files start screen and a separate named-path file.
These changes first shipped in 0.8.0. The pre-release local Windows verification
passed 52 active tests; subprocess, opt-in Python compatibility and explicit
SFTP fixture cases are excluded from that active count.
All-target, all-feature Clippy and formatting checks passed. This is source
qualification, not a new release or cross-platform result.

The storage tests cover inert missing-file reads, literal Unicode paths,
catalog/sidecar revision conflicts, retained orphan records, schema and size
limits, and preservation of other machines. Ten local-Git tests cover both sync
directions, first creation and deletion, unrelated staging, merge-only changes,
invalid history later repaired, and invalid incoming JSON or Git symlinks rejected
before HEAD or working files change. A deletion regression first reproduced a
Git partial-commit pathspec error; the corrected staged and unstaged deletion
cases passed before the combined suite. The Unix filesystem symlink test was not
exercised by these Windows runs; the later native release matrix supplies the
separate Unix results.

An owned Windows ConPTY check passed with a compiled no-network companion. It
starts offline, saves a path with trailing Enter events, scrolls a 25-route list
in a 22-row terminal, and checks the exact selected route and literal path in
the companion's arguments. It also checks unchanged device preferences, readable
failure output until Enter, and a stale saved-path edit retaining its input.
Model checks cover no-launch browsing, malformed-path recovery and input limits.

A separate actual handoff test opens the chooser, selects a saved directory on
a disposable loopback OpenSSH server, finds its remote marker in the compiled
SSH Files 0.2 browser, then closes Files and returns to the same tree. It passed
in 0.91 seconds, with catalog bytes unchanged and no device preference created.
Only generated test keys/configuration were used; the fixture is shut down after
the check. This is read-only navigation, separate from Files transfer tests.

Independent source review checked the chooser handoff and storage/sync changes
separately from their authors. The later five-platform release, HTTPS installation
and independent README gates are recorded above; they do not change what these
earlier source tests observed.

### Optional Files handoff (September 10, 2026)

The Files feature at `a42f567` passed [all five native platform jobs](https://github.com/brant92good/ssh-session-tui/actions/runs/34448841729).
The 38 native tests include a real owned pseudo-terminal with a compiled,
no-network companion. It checks X versus F and modal input, explicit route
selection, literal Unicode arguments, a custom local SSH config, primary-screen
handoff, and return to the picker. A companion exit of 255 stays visible until
acknowledged and cannot overwrite a route preference changed by another view.
The independent replay passed; no desktop windows or personal servers were used.

`files --json` is checked to leave the executable unstarted and catalog/device
files unchanged. Companion discovery also never launches a program. These
checks qualify the handoff; actual SFTP behavior is qualified separately by the
[SSH Files 0.1.0 beta release](https://github.com/brant92good/ssh-files/releases/tag/v0.1.0).
The immutable [0.7.0 release](https://github.com/brant92good/ssh-session-tui/releases/tag/v0.7.0)
at `418ee7d1d9750e3774bca12776cc91291d87aadb` passed
[all eleven release jobs](https://github.com/brant92good/ssh-session-tui/actions/runs/34459548086),
including actual HTTPS installation on all five targets. A separate local Windows
run passed the tagged installer, update, rejected checksum, saved-favorite and
polluted-environment checks. The downloaded executable SHA-256 is
`742a48e6cad2060931a295c0e99f13d4afdc45c62a3af55c4e1fdbeeb37d5d17`.

The published Windows picker was also placed beside the published SSH Files
0.1.0 binary with an empty PATH. Its read-only `files --json` command found that
companion and preserved the imported alias, explicit hostname/user/port, custom
config, quoted local path and IDs. Catalog and device files remained unchanged.
This check did not start SSH or open a desktop window.

### Session screen regression (September 10, 2026)

Before the fix, both runtimes reproduced a Local shell's output and PowerShell
prompt appearing underneath the next SSH session. The picker itself was still
running: its alternate screen had merely hidden the unchanged primary buffer.
Both handoff paths now clear the primary display and request saved-line removal
before opening a shell and after it returns. Shell history files are untouched.

The owned ConPTY regression launches real PowerShell with fixture-only profile
and history isolation, executes output and Ctrl+C, opens a compiled fake SSH
client, logs out, then checks a separate exit-255 error remains until Enter.
It verifies that the picker stays alive, returns only after shell exit, and does
not expose previous shell output when it closes. The visible and alternate
buffers are checked with a VT parser. Because that parser does not implement
ED3, scrollback removal is checked as an explicit ED3 request in the real
terminal output stream; it is not a claim about every terminal emulator.

This test also reproduced an older Python Ctrl+C race: its parent returned to
the picker while PowerShell was still active. A temporary caught SIGINT handler
now keeps the Python parent waiting, then restores the previous handler when
the shell exits. Native handling was already correct. Both Windows sequences
passed locally, with 33 ordinary native tests and Clippy passing. Hosted
cross-platform qualification passed in all five native jobs at
[`7377cea`](https://github.com/brant92good/ssh-session-tui/actions/runs/34447493809),
including Windows compatibility and Linux loopback SSH checks. The initial
assetless `v0.6.1` tag is retained: it caught a stale installer-test version and
a macOS test that incorrectly rejected the shell appending its own history.
The corrections change test expectations, not session cleanup behavior.

The immutable [`v0.6.2` tag](https://github.com/brant92good/ssh-session-tui/releases/tag/v0.6.2)
passed [all eleven release jobs](https://github.com/brant92good/ssh-session-tui/actions/runs/34448027497):
five native build/test jobs, publication, and five actual HTTPS installer jobs.
The [legacy compatibility matrix](https://github.com/brant92good/ssh-session-tui/actions/runs/34448027464)
also passed. A separate Windows run used the advertised HTTPS installer in a
temporary directory and passed fresh install, update, checksum rejection,
preserved favorite and inherited-environment checks, with PATH changes disabled.
Its downloaded executable SHA-256 was
`be5cce104b4bd65906e5aa32af94752db62798aedd5b9a1c6e1d1ddac394004b`.
The runtime is commit `8a9492542e3216d3575893c512ec8007cba40d06`; these
documentation updates do not replace the qualified release assets.

On September 10, 2026, the 32 native tests passed locally on Windows. These cover
catalog validation, stale edits, route/favorite persistence, Unicode case folding,
groups, imports, explicit fallback, modal input and guarded Git sync. Clippy
passed with warnings denied, including the screenshot exporter.

Independent review added regressions for merge-only unrelated files in Git
publication, cross-view favorite changes, import group preservation and group
search. Catalog-only merges remain supported; unrelated changes in any merge
parent comparison are refused before publication.

The Git subprocess runner polls both output pipes within the same deadline as
the process, with a 16 MiB limit per stream and no detached reader threads. Tests
fill both pipes and leave a descendant holding them after its parent exits; the
timeout returns and terminates that owned descendant. One ignored test entry is
the subprocess fixture these tests launch, not a skipped qualification case.

Windows starts Git suspended, assigns a job that terminates its processes when
closed, then resumes it. Unix uses a fresh process group. A Unix program that
deliberately creates a separate session/process group is outside that group
cleanup guarantee; it cannot keep this app blocked on a pipe-reader join. Git
hooks that intentionally start persistent background processes should be run
outside the app's bounded sync operation.

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

All five native targets passed at the independently reviewed
[`270aa71`](https://github.com/brant92good/ssh-session-tui/actions/runs/34378481055),
including the loopback SSH checks on Linux x64 and ARM64. The
[legacy compatibility matrix](https://github.com/brant92good/ssh-session-tui/actions/runs/34378481045)
also passed. The same commit is tagged `v0.6.0`.

The [Windows startup comparison](benchmarks/startup.md) measured the equivalent
`list --json` command at 80.82 ms median with Python and 17.56 ms with Rust.
This is process startup through a complete JSON response, not TUI painting,
shortcut focus or network login time.

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

For [v0.6.0](https://github.com/brant92good/ssh-session-tui/releases/tag/v0.6.0),
[the complete tag workflow passed](https://github.com/brant92good/ssh-session-tui/actions/runs/34379081600):
all five build/test/package jobs and all five real HTTPS installation jobs.
An additional independent Windows run on September 10 passed the same fresh
install, update, checksum rejection, saved-favorite and inherited-environment
checks in a temporary directory with PATH changes disabled. The downloaded
Windows executable's SHA-256 was
`35c3cfdf92acc2c3b0e1371b42a6af4b9adc8b654e5a4af5bbe1936d794c4547`.
The release contains five executables, five checksum sidecars and license text.
Release assets were not replaced after qualification. This evidence does not
mark parent workspace integration or a physical macOS desktop as qualified.

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
The same command now also writes `files-picker.svg`, `files-path.svg` and
`files-route.svg` from the Files chooser's real draw function. Those images are
marked as current-source features in the documentation until a release ships them.

## Limits and previous evidence

Native SSH handoff on a physical macOS desktop remains unqualified. So do every
Cloudflare helper, VPN state, custom shell profile and terminal emulator. The
Windows Terminal shortcut/focus behavior belongs to Terminal Workspace and has
its own qualification; it is not established by this leaf's PTY tests.

Version 0.5's Python implementation previously passed real Windows remote login
and returned to the picker in a small owned Terminal window. That observation
does not automatically qualify the rewritten native runtime. The legacy suite
remains available as a reference and is run by a separate CI workflow.
