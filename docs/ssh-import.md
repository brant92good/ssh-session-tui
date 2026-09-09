# Import existing SSH machines

Press I to preview your user SSH configuration. Space selects a row, A selects
all eligible rows, Enter imports the selection, and Esc cancels. Nothing is
selected or saved automatically. With no checked rows, Enter imports only the
highlighted eligible row. Enter a different path in the path field to preview
a custom configuration.

For a script or agent, `ssh-sessions import-ssh --json` is read-only. To write, supply
`--apply --host ALIAS` (repeat `--host`) or `--apply --all`. `--config PATH` selects
a custom file. Add the normal global `--catalog PATH` before the subcommand to
choose the private catalog. Import never runs SSH or Git, so publishing remains
a separate explicit action.

In the picker, newly imported machines join the group currently open. In scripts,
`--group Work/Development` sets their group. Existing machines keep their groups
and tags when an imported alias adds or binds a route.

## What crosses devices

The catalog stores the alias's display name, address, username and port. An
imported route also stores `ssh_alias`, a name such as `workstation`, never a
command or filesystem path. Version 0.2+ is required for catalogs containing
this optional field. Old catalogs still work and unchanged manual routes keep
their original JSON shape.

The importing device stores custom config paths outside Git in its local
`.ssh-configs.json` file. On another device, the same alias must exist in the
normal user SSH config, or you can import it from that device's custom file to
bind the existing route. A missing alias stops with instructions instead of
silently trying a direct connection. Private keys, public keys, passwords,
IdentityFile paths, ProxyCommand strings and key-agent state are never exported.

At connection time, OpenSSH receives the original alias so its local key/proxy
rules still apply. Explicit HostName, User and Port arguments use the catalog's
chosen metadata. This is a snapshot, not a live mirror of later edits to SSH
config. Re-import reports changed alias details for review; it does not overwrite
them. To change the imported destination, reconcile the saved route explicitly.

An existing address/username/port gets another route in the same machine. Its
current route choice is preserved. Identical imported aliases are bound locally
without adding duplicate rows. No two machines are merged merely because their
names look similar; associating LAN and VPN addresses remains an explicit choice.

## Supported configuration

The reader handles literal Host aliases, several names per Host line, wildcard
defaults and negations, quoted values, `key=value`, and static Include paths/globs.
It follows the first-value rule for HostName/User/Port and OpenSSH's
case-sensitive alias patterns (`*` and `?`, not bracket ranges). Relative user Includes
resolve under `~/.ssh`; an Include retains its parent's Host/Match state when it
returns. For the default user file, system client defaults are read afterwards;
a custom config uses the same omission of system defaults as SSH's `-F` option.
Absent username/port values default to the current local username and port 22.
These rules follow the [OpenSSH config reference](https://man.openbsd.org/ssh_config.5)
and [Windows configuration locations](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh-server-configuration#openssh-configuration-files).

The importer does not evaluate Match exec, DNS canonicalization or dynamic
Include/address tokens. Where these could affect the address, username or port,
the preview marks the entry for manual setup. Static Match all is supported.
Wildcard patterns are not expanded into invented machines. Include cycles,
unclosed quotes, more than 128 files, excessive nesting and oversized configs
produce an actionable error. No proxy, shell command, SSH process or key helper
runs during discovery. The normal SSH client can evaluate its local rules only
after you explicitly connect.

SSH-config generation/editing, key enrollment and agent connection policy remain
in the [backlog](backlog.md). Read-only import is implemented independently of
those decisions.
