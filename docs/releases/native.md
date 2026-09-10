SSH Sessions 0.7.0 opens the optional compiled SSH Files companion with **X**.
It uses the selected machine and explicit route, including an existing SSH alias
and custom config. **F** remains Favorites. The `files --json` command returns
the selected launch arguments without executing them. Install SSH Files separately;
it is a beta and needs SSH key authentication that works without a prompt.

This release retains the 0.6.2 fix that clears display and scrollback between shell
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

macOS and the optional SSH Files companion remain **beta**. Hosted
pseudo-terminal tests are distinct from
qualification on physical desktops and every SSH/proxy setup.

See the tagged README for installation and docs/verification.md for evidence.
