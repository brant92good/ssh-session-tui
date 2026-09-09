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
The initial 0.1 release passed 31 tests locally. A fresh virtual environment in a path containing
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

## SSH import and easier new-tab keys (0.2)

The expanded suite has 44 tests. Import checks cover static Include files,
first-value and user/system precedence, wildcard exclusions, cycle detection,
refusal to execute Match commands, stale previews, existing-machine route
preservation, repeat imports and custom config paths remaining device-local.
A generated static config is also compared with the installed OpenSSH client's
output. That isolated fixture has no executable directives; production import
never invokes SSH for discovery. Keyboard tests exercise I, Esc, Space, A and
Enter through the actual import screen.

The installed Terminal shortcut passed a real Ctrl+N -> picker -> SSH -> logout
flow and Ctrl+Alt+N -> local PowerShell check in one small owned window. Existing
tab identities were preserved. The importer previewed the owner's local config;
one already-authorized alias was imported into a temporary catalog and successfully
ran a harmless SSH command. The real config hash and saved machine list stayed
unchanged. Cloudflare proxy preservation was checked in argv construction, not
with a live Cloudflare login.

The import screenshot uses `examples/ssh_config`; its selected-row checkmark,
metadata columns and keyboard instructions were rendered and inspected.

## Local terminal and numbered favorites (0.3)

The expanded suite passed 62 tests locally. New checks cover an empty catalog's
visible local row, number-then-Enter selection, preserving digits in forms/search,
modal isolation, device route choice, missing/empty favorites refusing adjacent
connections, changed catalog review, and local-shell handoff back to the picker.
Favorite storage checks cover device isolation, moving/replacing/clearing slots,
stale-edit rejection, malformed-file preservation, read-only CLI listing and
first-install defaults preserving later edits. A 70×18 layout check preserves
space for the connection list; 100×30 and 70×20 screenshots were rendered and
visually inspected with example data only.

The installed private catalog was exercised through the real Textual picker in
headless mode for slots 1, 2 and 3. The two remote choices resolved to the expected
machines and selected routes; argv construction retained the imported alias and
port. The local command ran PowerShell Core. These checks did not log into the
second server or move visible windows. They establish selection and command
construction, not that every remote server is currently reachable. The original
SSH config, existing Terminal tabs and running forward were preserved.

## Groups, tags and clearer screens (0.4)

The full suite passed 79 tests locally. Organization checks cover nested paths,
descendant counts, tag/group search, bulk moves and tag edits, parent renames,
stale-edit refusal, import into groups, catalog version compatibility and a
1,000-machine fixture. Keyboard tests use the real Textual app for G, Space,
Ctrl+A, M, T, group rename, clearing selection and favorites outside a group.
Two isolated Git clones verify that groups/tags travel while each device keeps
its own route choice and numbered favorites.

A separate headless smoke check rendered all 1,000 machines, searched to 50
matching a group/tag pair, selected those rows and moved them with the actual
M form. The saved catalog and confirmation count matched. This was a functional
check, not a cold-start or cross-platform performance benchmark.

The main, route, import and group screens were rendered at 100×30, plus a 70×20
compact main screen. The 70×18 keyboard/layout regression also passes. Wide
tables scroll horizontally in small terminals; the selected destination remains
in the details below. SVGs now embed Fira Code and its license so image rendering
does not depend on external font requests. Every screenshot uses example data.

Copy was reviewed across the picker, forms, import, route/favorite/group menus,
sync messages, CLI help and documentation. Main screens describe available
actions; data/schema and authentication behavior remain in the reference docs.
No visible desktop window was activated for this update. Linux and macOS remain
unverified; see the [platform plan](https://github.com/brant92good/terminal-workspace/blob/main/docs/platforms.md).
