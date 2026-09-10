# Working on SSH Sessions

Read README.md and docs/design.md. This public leaf owns the machine picker,
metadata schema, device-only route preferences and explicit catalog sync.
The production runtime is compiled Rust in src/. Groups/tags and atomic bulk
edits belong in src/organization.rs; browsing and keyboard behavior in src/ui/.
Keep machine IDs/routes stable across moves and renames. Group
paths and tags require catalog v2; old unorganized catalogs retain v1. Starting
a search or changing groups clears bulk selection. Favorites are global within
the catalog, even when a group filter is active.
Numbered favorites live in a separate device-local file managed by src/favorites.rs.
They reference stable machine IDs or @local; never infer identity from row order.
The optional Files handoff is in src/files.rs. X opens the compiled ssh-files
companion; F stays Favorites. Freeze machine/route/config/cwd before terminal
handoff, preserve alias plus explicit HostName, and reject stale catalog/binding
preparation. Discovery and files --json never launch a program or connect. An
invalid explicit SSH_FILES_BIN errors; Files failures never trigger SSH fallback
or change the device preference. Keep companion authentication policy in its leaf.
The Local terminal row is always available. Numbers select, Enter opens; preserve
normal numeric input and modal isolation. Public defaults contain no personal hosts.
Terminal Workspace owns Windows Terminal profiles and hotkeys. A user's private
settings repository may store the metadata file and pin the public parent.

Never put real machine details, private keys or credentials in this repository.
Examples and screenshots use demonstration addresses only. Catalog validation
allows only the documented metadata fields; device preferences live outside
Git. Never change SSH config, authorized_keys, known_hosts or key-agent state
as part of setup or sync. The existing SSH client owns authentication.
Explicit read-only SSH import is supported in src/ssh_import.rs. Never
use ssh -G for discovery: Match exec may execute commands. Preserve aliases so
local proxy/key settings apply; custom config paths belong only in local state.

A connection failure must not automatically try another route or replace the
device's preferred route. Show alternatives and wait for the user's choice.
Do not claim a public-key fingerprint proves current server authorization.
SSH-config management, agent session policy and key authorization status are
deliberately deferred in docs/backlog.md.

Run `cargo test --locked` and `cargo clippy --locked --all-targets --all-features -- -D warnings`.
Native tests include owned OS pseudo-terminals, without opening desktop windows.
Developer-only scripts/check_native_compat.py compares legacy Python files and
locks with the binary. Python code is retained as a migration reference, not
the runtime distributed by install.ps1/install.sh. The normal installer downloads
prebuilt binaries and must not install Python, Cargo or Git. Git remains optional
for catalog sync. Keep Unicode 15 casefold semantics in src/text.rs: changing a
Windows catalog's path hash loses its route/favorite/lock identity.

Generate native screenshots with `cargo run --locked --example capture --features screenshots`.
Use only isolated demonstration metadata. Platform evidence belongs in
docs/verification.md; macOS remains beta until physical desktop use is qualified.
Tag publication creates a prerelease after native CI. Run the real HTTPS
scripts/check_release_install.py before stable promotion; never replace assets
of an already tested release. Desktop checks require opt-in and
small test-owned windows; never activate unrelated existing windows. Do not
run real remote commands through tests without authorization. Git sync tests
use isolated local repositories, not an account's real remote.

In this owner's agent session all Git/gh commands, including tests that run Git,
must execute outside the sandbox. Do not use GitHub MCP. Publish the leaf first,
then Terminal Workspace's submodule pin, then the private parent's pin.
