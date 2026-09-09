# Data and connection boundaries

This leaf owns a small machine catalog, its keyboard editor and an SSH handoff.
It can run independently of Terminal Workspace, Port Forward TUI or a private
setup repository. It currently uses Python/Textual: the earlier Rust experiment
measured a return launcher, not a full TUI, and did not justify a complete rewrite.

The three layers are:

```text
Your private settings repo
  connections/catalog.json          Shared machine metadata
  terminal/                         Public Terminal Workspace submodule
    apps/ssh-session-tui/            This public leaf
    apps/port-forward-tui/           Independent port-forwarding leaf
  skills/                           Optional private skills sibling
```

Only `version` and `machines` appear at the catalog root. Each machine has
`id`, `name`, `user`, and `routes`. Each route has `id`, `name`, `host`, and
`port`, and optionally an imported `ssh_alias` name. See [the example](../examples/catalog.json). IDs remain stable when a
display name changes. Unknown fields, invalid destinations, duplicate IDs and
malformed files are rejected; they are not silently repaired or overwritten.

Device preferences map a machine ID to a route ID in local app data. The file
name is derived from the local catalog path. This keeps separate catalogs from
sharing preferences accidentally; moving a checkout means selecting routes
again. With multiple routes, removing the selected route requires a fresh
choice. With one remaining route, the choice is unambiguous.

Catalog writes use a device-local file lock, a revision check and atomic file
replacement. A second tab with an older snapshot must reload instead of
overwriting a newer edit. Git synchronizes the shared file; its normal conflict
rules still apply across computers. Device preference files are never added by
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

The existing Herdr/Ports paired workspace keeps its own catalog and context
rules. Joining those catalogs and designing agent-heavy SSH behavior require
the [deferred discussion](backlog.md). This release does not claim public-key
inventory, authorization status, SFTP, terminal multiplexing or credential sync.
