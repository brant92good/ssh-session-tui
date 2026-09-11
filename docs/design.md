# Data and connection boundaries

This leaf owns a small machine catalog, its keyboard editor and session handoffs.
It can run independently of Terminal Workspace, Port Forward TUI or a private
setup repository. The production app uses Rust and Ratatui. Installation downloads
a compiled binary; Python files remain only as a developer migration reference.

The three layers are:

```text
Your private settings repo
  connections/catalog.json          Shared machine metadata
  connections/catalog.json.files.json  Shared saved remote paths (0.8+)
  terminal/                         Public Terminal Workspace submodule
    apps/ssh-session-tui/            This public leaf
    apps/port-forward-tui/           Independent port-forwarding leaf
    apps/ssh-files/                  Optional SFTP companion
  skills/                           Optional private skills sibling
```

Only `version` and `machines` appear at the catalog root. Each machine has
`id`, `name`, `user`, and `routes`, plus optional `group` and `tags`. Each route has `id`, `name`, `host`, and
`port`, and optionally an imported `ssh_alias` name. See [the example](../examples/catalog.json). IDs remain stable when a
display name changes. Unknown fields, invalid destinations, duplicate IDs and
malformed files are rejected; they are not silently repaired or overwritten.

Groups use slash-separated paths such as `Work/Production`. A machine has one
group and any number of tags up to 30. Group paths allow up to eight levels and
160 characters; individual tags allow 40 characters. Group membership is
case-insensitive. Parent groups and counts are derived from their descendants;
there is no separate empty-group record. A group disappears when its last
machine moves away. Renaming a parent changes all descendant paths atomically.
Group membership affects browsing only; routes and SSH options remain per machine.
Nested groups follow a familiar workflow in [Termius](https://termius.com/api-docs/),
whose groups also support inherited connection settings; this app does not add
that inheritance.

Catalog version 2 adds groups and tags. Version 1 remains readable, and catalogs
without organization metadata keep the version 1 shape. Saving a group or tag
requires SSH Sessions 0.4+ on every device reading that catalog. Older clients
reject version 2 rather than overwrite fields they do not understand.

Bulk move/tag commands share the same revision check and atomic save as single
machine edits. They preserve machine IDs, routes and device favorites. Space
selects individual rows; Ctrl+A selects the shown machines. Starting a search
or changing the group clears selection so an edit does not affect hidden rows.
Favorites remain global to the catalog and can select a machine outside the
current group. Search combines words with AND; `tag:gpu` matches an exact tag,
and `group:Work` includes Work and its descendants.

Device preferences map a machine ID to a route ID in local app data. The file
name is derived from the local catalog path. This keeps separate catalogs from
sharing preferences accidentally; moving a checkout means selecting routes
again. With multiple routes, removing the selected route requires a fresh
choice. With one remaining route, the choice is unambiguous.

Windows catalog identity uses full Unicode 15 case folding, matching the previous
Python 3.12/3.13 runtime. The mapping is embedded and frozen: a routine dependency
upgrade must not silently change device state filenames. Numeric-string ports
and UTF-8 BOMs accepted by the old reader remain readable by the native app.

Numbered favorites are a separate `.favorites.json` file next to those device
preferences. Version 1 stores `slots`, a map from strings `1`–`9` to stable
machine IDs or the reserved `@local` target. It contains no commands, credentials
or copied route addresses and is not included in catalog sync. Writes use the
catalog's device lock and an optional revision check, so two open slot menus
cannot silently overwrite each other. Deleted targets remain visibly missing
until cleared/replaced; numbers never shift to adjacent machines. `Favorites.seed`
installs validated initial values only if the file does not yet exist.

The Local terminal row exists independently of the machine catalog. Number keys
select favorites on the main list and Enter opens the selected item. Digits in
inputs remain text and favorites do not escape a modal dialog. Machines with
ambiguous routes still require an explicit route choice. `favorites list/set/remove`
are CLI operations for local metadata and never start a session.

Catalog writes use a device-local file lock, a revision check and atomic file
replacement. A second tab with an older snapshot must reload instead of
overwriting a newer edit. The 0.7.0 sync command handles the catalog;
0.8 also handles the saved-path file described below. Git's normal
conflict rules still apply across computers. Device preference files are never added by
the sync command. Explicit SSH import reads config metadata, never private/public
key files. Custom config paths remain device-local. See [import boundaries](ssh-import.md).

The picker exits its alternate screen before starting the installed SSH client
with an argument list, not a shell command string. It supplies the chosen host,
username, port and bounded initial connection attempts. The client retains its
normal host-key checks, local config, key-agent behavior and authentication
prompts. The child owns terminal input and output. After SSH exits, the picker
opens again. Exit 255 is treated as a possible connection/authentication failure;
the app does not diagnose it as a specific network fault.

An alternative route requires a new user selection. Trying one after failure
does not update the device preference. There is no route probing or automatic
fallback in the background. A hostname/alias route does not provision a VPN,
Cloudflare helper, proxy or key.

The optional Files handoff freezes the selected machine, route, custom config
path and working directory before releasing the terminal. It resolves a compiled
companion through an explicit absolute `SSH_FILES_BIN`, the executable's own
directory, or PATH, without executing candidates during discovery. The same
stable machine/route IDs and imported alias/HostName combination reach the
companion through an argument array. Catalog revision and local binding changes
during preparation are rejected. This does not change the catalog schema or
authentication policy. A Files error returns to the picker without automatic fallback or a
preferred-route change. See [the user path](usage.md#open-the-file-browser).

## Saved remote paths and the Files chooser

Added in 0.8. `ssh-sessions files`
without a machine opens a separate Group → Server → Path chooser. It reads the
same catalog as the SSH picker. A server opens the companion's default remote
directory (`.`); a path row adds one literal `--remote=PATH` argument. An explicit
machine with `--json` stays read-only and never launches the chooser or companion.

The saved-path filename appends `.files.json` to the **whole** catalog filename.
For example, `catalog.json` uses `catalog.json.files.json`; `work.json` uses
`work.json.files.json`. Version 1 contains only names and remote paths:

```json
{
  "version": 1,
  "machines": {
    "development": [
      { "id": "api", "name": "API source", "path": "/srv/api" }
    ]
  }
}
```

Machine and preset IDs use the catalog's identifier rules. Preset IDs and names
are unique within a machine; names compare using trimmed Unicode 15 case folding.
Names allow 1–80 characters, and paths allow 1–4096 UTF-8 bytes. Control characters
are rejected. The file is limited to 128 KiB, 64 paths per machine and 2048 paths
in total. Remote path bytes are otherwise preserved, including spaces, `~`,
`$HOME` and `..`; the picker does not expand or normalize them locally.

Missing data means no saved paths and does not create a file. Malformed data
shows an error and disables path operations while leaving server-home selection
available. Unknown fields, duplicate records, symlinks and non-files are rejected.
Records for a machine removed from the catalog remain in the saved-path file;
editing another machine does not discard them.

Edits hold the existing catalog lock and require both the catalog and saved-path
digests to match. A stale edit retains its typed input and reports the conflict.
Saving a path does not connect, change a route, or modify anything on the server.
The saved confirmation stays open until Esc, so trailing pasted newlines cannot
turn a successful save into a connection.

Before handoff, the chooser freezes the selected machine/preset IDs, literal path
and revisions. It checks saved paths before and after the existing catalog/config
preparation. A changed or deleted selection requires review. R chooses a route
once; it does not write the device preference. The normal SSH picker still owns
default-route changes. Companion failures remain readable until acknowledged,
then return to the chooser without trying another route.

## Explicit metadata sync

App-created publication allows exactly the catalog and its own `.files.json`
sibling. It does not scan for other matching filenames or add device state. Publish validates each
unpublished revision, changes against every merge parent, and HEAD after commit
hooks. An invalid earlier revision is refused even if a later commit repairs it.
Unrelated staged files remain staged and are not included in the app's commit.

The optional saved-path file can be absent, created for the first time or
explicitly deleted. Deleting it through Git removes the saved shortcuts, not
remote files. Publish handles both staged and unstaged deletion. Pull requires
a clean checkout, fetches, and validates both incoming metadata files before
fast-forwarding the entire repository. That pull can update other tracked files
in the checkout; the two-file publication limit does not restrict a Git pull.
Invalid incoming metadata leaves HEAD and the working
files unchanged. Resolve Git conflicts outside the app; sync never silently
merges conflicting path edits or runs automatically during startup.

The existing Herdr/Ports paired workspace keeps its own catalog and context
rules. Joining those catalogs and designing agent-heavy SSH behavior require
the [deferred discussion](backlog.md). This release does not claim public-key
inventory, authorization status, terminal multiplexing or credential sync.
