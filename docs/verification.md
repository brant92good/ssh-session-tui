# Verification

Checked September 9, 2026 on Windows 11 Pro build 26200, Windows Terminal
1.24.11911.0, PowerShell 7.6.5 and Python 3.12.11. This is evidence from one
desktop, not a guarantee for every terminal, SSH configuration or future version.

The local suite covers metadata validation, Unicode, two independent device
preferences, stale-edit rejection, forbidden fields, keyboard route selection,
fallback confirmation, search, local-shell choice and preserving invalid form
input for correction. Connection-loop tests verify that a failed attempt does
not start another route and an explicitly chosen alternative does not change
the device preference.
All 31 tests passed locally. A fresh virtual environment in a path containing
spaces and Chinese characters also installed the built package successfully;
its console entry, UI import, read-only commands and `pip check` passed from
outside the source directory.

Git tests use temporary local repositories for two-device pull/publish. They
verify preservation of unrelated staged files, refusal to overwrite local
changes, refusal to push unrelated unpublished commits, and validation of
earlier unpublished catalog versions. A reverted unrelated file or a repaired
credential-bearing catalog still causes Publish to refuse that history.

An installed, real desktop test opened a small owned Terminal window and used
Ctrl+Shift+T to start the picker. Enter connected to its single configured
machine. A typed `printf` marker was returned by the remote shell; its expected
output was not present literally in the command text, avoiding a false pass
from terminal echo. Exiting SSH returned to the picker. Ctrl+Alt+N opened a
separate local tab, where another marker confirmed PowerShell Core.

The test closed only its own window and verified the original tab identities
remained. It checks foreground ownership before keyboard input and stops if
another application gains focus. Reproduction lives in Terminal Workspace's
[check_session_picker.py](https://github.com/brant92good/terminal-workspace/blob/main/scripts/check_session_picker.py).
It requires explicit `--yes`, an installed picker as the default profile, and
one already reachable machine. It does not install keys or change SSH config.

README images are exported by the actual Textual app with example metadata and
true-color rendering. The 100×30 main/route views and a 70×20 compact view were
rendered for inspection. Generate them with `python scripts/capture_demo.py`.
No SSH connection or real server data is used in those images.

Cloudflare configuration, a second physical laptop, key enrollment/authorization
tracking and agent-managed sessions are not tested or implemented here. Existing
SSH configuration is delegated to OpenSSH. See [the backlog](backlog.md).
