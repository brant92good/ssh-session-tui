The SSH machine picker now ships as a compiled Rust executable for Windows x64,
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
