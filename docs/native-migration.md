# Native runtime migration

Version 0.6 replaces the application runtime with Rust and Ratatui. The root
one-command installers download compiled binaries; source builds are optional.
Python modules remain temporarily as the compatibility oracle for developer
tests. The binary does not import them or install a Python environment.

Catalog v1/v2, stable IDs, route preferences, favorite files, custom SSH config
bindings and the JSON commands remain compatible. Windows path identity uses
the same full Unicode 15 case-fold mappings as Python 3.12/3.13. Numeric-string
ports and BOM-prefixed JSON from existing files remain readable.

The native picker preserves number-then-Enter connections, favorite-slot
confirmation, missing-favorite clearing, grouped browsing, bulk edits, import of
the highlighted or marked aliases, and explicit fallback after SSH failure.
It releases the terminal before handing input/output to OpenSSH or the local
shell, then restores the picker when that child exits.

Windows releases statically link their C runtime; Linux binaries use musl.
macOS binaries are available for Apple Silicon and Intel with a beta label.
See [verification](verification.md) for tested paths and practical limits.

The `native-runtime` branch is the release candidate until the independent
review, published-download checks and parent integration gates pass. No working
installation is removed merely because a source build succeeds. Prerelease
assets remain immutable; stable promotion reuses their verified bytes.