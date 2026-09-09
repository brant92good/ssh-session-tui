# SSH Sessions

Choose a server with your keyboard. Use its LAN address on one computer and
its VPN address on another, while keeping private keys with each computer's
existing SSH client.

![SSH machine picker](docs/screenshots/picker.svg)

*The actual picker with example metadata. No connections were opened for this image.*

## Try it

You need Python 3.12+ and an installed OpenSSH client. Windows is the tested
target; Terminal integration is optional.

```powershell
git clone https://github.com/brant92good/ssh-session-tui.git
cd ssh-session-tui
py -3 -m venv .venv
.\.venv\Scripts\python.exe -m pip install -e .
.\.venv\Scripts\python.exe app.py
```

Press **A** to add a machine: give it a name, remote username and address.
Then press **Enter** to connect. SSH uses your existing local keys, key agent
and configuration. The picker does not install keys or change SSH config.
After the session ends, it returns to the list.

No host is required during installation. No account or Git repository is
needed to use the picker locally. Run `app.py doctor` for local setup checks.

## Import the machines you already use

Press **I** to preview your local `~/.ssh/config` (on Windows,
`%USERPROFILE%\.ssh\config`). Use **Space** to select hosts, **A** for all/none,
then **Enter** to import. Enter with no selection imports the highlighted host.
Esc cancels. Tab/Shift+Tab lets you enter a different config path; Enter reloads it.

![Review local SSH hosts before importing](docs/screenshots/import.svg)

*Actual import screen using the included example SSH config. No connections are opened.*

Import copies names, addresses, usernames, ports and the SSH alias name. It keeps
key choices, proxy commands and custom config paths on this device. Connecting
through an imported route still uses its local alias, so existing Cloudflare,
jump-host and key settings remain available to OpenSSH.

Existing machines keep their names and routes. An alias for an already-saved
address/user/port adds a route while preserving the previous device choice.
Re-importing the same alias does not duplicate it. Changed imported values are
flagged for review instead of overwriting your catalog. Import does not connect,
publish to Git, or edit SSH settings.

Named `Host` entries, wildcard defaults, negations and static `Include` files
are supported. System defaults are considered for the normal user config.
Conditional/dynamic address rules that cannot be resolved without executing
commands are shown as needing manual setup. The importer never runs `ssh -G`,
`Match exec`, proxies or key helpers. See [import details](docs/ssh-import.md).

## One server, different routes

A **machine** is the server you want. A **route** is an address you can reach
it through. For example, the same workstation can have a Home LAN route and
a Private network route. Press **R** to add routes and choose one for this
device. Another device's choice stays unchanged.

![Choosing a route for this device](docs/screenshots/routes.svg)

*Actual route chooser with example addresses. Enter saves this device's choice.*

If there is only one route, Enter uses it. With several routes and no saved
choice, the app asks first. After an SSH connection failure, it preserves the
client's error on screen, then offers the route list. Choosing an alternative
tries it **once**; it does not replace the preferred route or try other routes
automatically. Esc leaves the connection stopped.

A route can be an IP, hostname or an existing local SSH alias. A Cloudflare
hostname alone does not set up Cloudflare Access: any required local helper
and SSH configuration must already work. Authoring that configuration is
reserved for a [later design discussion](docs/backlog.md).

| Key | Action |
| --- | --- |
| Up / Down, Enter | Choose a machine and connect |
| /, then Enter | Search, then return to the machine list |
| A / E / D | Add, edit or delete a machine |
| R | Manage routes and select this device's route |
| I | Preview and import local SSH settings |
| S | Open explicit Pull / Publish options |
| F5 | Reload changes from another tab or editor |
| F1 | Show every shortcut, including those hidden in a narrow footer |
| Ctrl+L | Use local PowerShell, then return to the picker |
| Q | Close the picker |

Forms use Tab to move, Ctrl+S to save and Esc to cancel. Invalid fields keep
their entered values so you can correct them.

## Sync addresses through your own private Git repo

The shared JSON contains only machine names, usernames and named address/port
routes, plus optional SSH alias names for imported routes. Private keys, passwords and device route preferences are not catalog
fields. The app does not synchronize credentials or claim to verify key
authorization. Read the [data boundaries](docs/design.md) before setting up sync.

Choose a catalog file inside your existing private repository. Start with an
empty file using `init`, then commit it with your normal Git workflow:

```powershell
.\.venv\Scripts\python.exe app.py --catalog C:\MyPrivateSetup\connections\catalog.json init
# In that private repository, add/commit this file and configure its Git upstream.
.\.venv\Scripts\python.exe app.py --catalog C:\MyPrivateSetup\connections\catalog.json
```

Press **S**, then **P** to Pull or **U** to Publish. Sync is explicit:

- Pull uses a fast-forward Git pull of the containing repo. It refuses local
  changes and does not install other settings or update submodule working trees.
- Publish commits only the tracked catalog file. It preserves unrelated staged
  work and refuses unpublished commits that touch other files or contain invalid
  catalog snapshots, including changes later reverted.
- If both devices changed the catalog, resolve the Git conflict explicitly.
  The app does not discard edits, force-push or silently pick a winner.

Git sign-in is handled outside the picker. The app requires an already tracked
catalog on a branch with an upstream; it does not create GitHub repositories.
Use a **private** repository for addresses you do not want to publish. Point each
device at its own checkout. Device choices default to local app data and stay
outside that checkout.

Without `--catalog`, Windows data lives under `%LOCALAPPDATA%\SSHSessions`.
An empty first-run catalog is ready for A; it contains no example servers.

## Make it the Windows Terminal new-tab screen

[Terminal Workspace](https://github.com/brant92good/terminal-workspace) includes
this app as a separate pinned submodule. In that checkout:

```powershell
.\install.ps1 -IntegrationOnly -SessionPicker
# Optional private catalog:
.\install.ps1 -IntegrationOnly -SessionPicker -SessionCatalog C:\MyPrivateSetup\connections\catalog.json
```

The explicit `-SessionPicker` option makes new tabs open this picker and adds
**Ctrl+Alt+N** for a normal local PowerShell tab. Existing R/P/L Herdr and Ports
shortcuts keep their behavior. The paired workspace button remains separate.
For an easier new-tab key, add `-NewTabShortcut ctrl+n`. This is optional because
Terminal intercepts that key before shells and editors can use it; `none` removes
the extra binding. Ctrl+Shift+T remains available.

## Inspect or test

```powershell
.\.venv\Scripts\python.exe app.py list --json
.\.venv\Scripts\python.exe app.py doctor --json
.\.venv\Scripts\python.exe app.py command MACHINE_ID --route ROUTE_ID --json
.\.venv\Scripts\python.exe app.py import-ssh --json
# After reviewing the preview:
.\.venv\Scripts\python.exe app.py import-ssh --apply --host YOUR_SSH_ALIAS --json
.\.venv\Scripts\python.exe -m unittest discover -s tests -v
```

`list`, `doctor` and `command` do not start SSH. `command` prints argument data
for inspection; it is not an agent authorization policy. See [AGENTS.md](AGENTS.md)
for repository work, [verification](docs/verification.md) for checked behavior,
and [the backlog](docs/backlog.md) for SSH-config management, key status and
agent-heavy session design. This first release is a picker, not a complete
Termius replacement.
