SSH Sessions 0.6.2 clears the terminal display and scrollback between shell
sessions, so a previous PowerShell prompt or SSH logout does not reappear behind
the next connection. An SSH failure stays visible until you press Enter.
Command-history files are not changed.

The retained Python compatibility runtime also fixes a Ctrl+C race that could
return to the picker before its local shell had exited. Owned pseudo-terminal
tests cover local shell, interrupt, SSH logout, failed SSH acknowledgement and
picker return. No desktop window or remote server is used by that regression.

The SSH machine picker ships as a compiled Rust executable for Windows x64,
Linux x64/ARM64, and macOS Apple Silicon/Intel. No Python, Rust compiler or Git
is needed for normal installation. Git remains optional for catalog sync.

The migration preserves machine IDs, device route choices, numbered favorites,
groups/tags, read-only SSH import, explicit fallback and the command interface.
Unicode Windows paths and numeric-string ports retain compatibility with 0.5.

Each binary has a SHA-256 sidecar. The one-command installers check it before
replacing an existing command. The old runtime can remain for already-open tabs.

This is a **prerelease** while released-download and parent integration checks
finish. macOS remains **beta**: hosted pseudo-terminal tests are distinct from
qualification on physical desktops and every SSH/proxy setup.

See the tagged README for installation and docs/verification.md for evidence.
