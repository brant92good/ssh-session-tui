<img src="docs/brand/mark.svg" width="112" align="right" alt="SSH Sessions logo">

# SSH Sessions

Your servers, a number key away.

Save the machines you connect to, give your regulars a number, and press
**1, then Enter** to open a session. Search a longer list, browse by project,
or choose **Local terminal** when you want a shell on this computer.

[![Checks](https://github.com/brant92good/ssh-session-tui/actions/workflows/test.yml/badge.svg)](https://github.com/brant92good/ssh-session-tui/actions/workflows/test.yml)
[![MIT license](https://img.shields.io/badge/license-MIT-65d6be)](LICENSE)

[Install](#install) · [Keyboard guide](docs/usage.md) · [SSH import](#import-existing-ssh-hosts) · [Report a problem](https://github.com/brant92good/ssh-session-tui/issues)

![SSH Sessions: numbered favorites, groups and a local terminal](docs/screenshots/picker.svg)

*The actual app with example machines, numbered favorites and groups.*

## Install

**Windows — paste into PowerShell:**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://raw.githubusercontent.com/brant92good/ssh-session-tui/main/install.ps1 | iex"
```

**macOS / Linux — beta:**

```sh
curl -fsSL https://raw.githubusercontent.com/brant92good/ssh-session-tui/main/install.sh | sh
```

Then run **`ssh-sessions`**. Open a new terminal if the command isn't found yet.
The installer downloads Python and the app's dependencies. You don't need to
install Python or Git first, and setup doesn't ask for a server.
Run the same install command again to update.

Connecting uses your **OpenSSH client** (`ssh`). Git is only needed if you want
to sync a catalog. [Installer details and uninstall](docs/install.md).

macOS and Linux installation, updating and local-shell controls pass automated
tests. Real remote login in their desktop terminal apps still needs testing,
so support remains **beta**. [Platform evidence](docs/verification.md).

## Make your first connection

1. Press **I** to import existing SSH hosts, or **A** to add a machine.
2. Select it and press **Enter**. OpenSSH handles your usual login.
3. Log out to return to the list. **Local terminal** works the same way; type `exit` to return.

### Numbered favorites

Select a machine or Local terminal, press **F**, and choose a slot from **1–9**.
Press its **number, then Enter** to connect from any group. The number selects
the destination first, so you can check it before opening a session.

## Who is this for?

- You keep returning to the same few servers and want their names on screen.
- Your machine list has grown across projects, labs or locations and needs groups and search.
- You use a LAN address at home and a different route on your laptop.
- You work with a coding agent and want commands it can inspect, plus a picker you can use yourself.

## A bigger list, still easy to find

**G** browses groups such as `Work/Production`. **/** searches names, addresses,
groups and tags; try `tag:gpu`. Select several machines with **Space**, then
**M** to move them or **T** to edit tags. Favorites work across groups.

<details>
<summary>See groups and the keyboard controls</summary>

![Nested machine groups in SSH Sessions](docs/screenshots/groups.svg)

| Key | Action |
| --- | --- |
| Arrows, Enter | Choose and open a session |
| 1–9, Enter / F | Open / edit a numbered favorite |
| G / / | Browse groups / search |
| A / E / D | Add / edit / delete a machine |
| R / I | Choose a route / import SSH hosts |
| Space / Ctrl+A | Select one machine / all shown |
| M / T | Move selected machines / edit tags |
| S | Pull or publish the catalog |
| F1 / Q | Keyboard guide / close picker |

Forms use **Tab** to move, **Ctrl+S** to save and **Esc** to cancel.
[Full keyboard and organization guide](docs/usage.md).

</details>

## Import existing SSH hosts

Press **I** to preview your SSH config, mark hosts with **Space**, then press
**Enter** to import. Imported connections use their original SSH aliases, so
OpenSSH can still apply your jump-host, proxy and key settings.
You can choose another config file in the import screen.

[What gets imported, and how repeat imports work](docs/ssh-import.md).

## One machine, different routes

A machine can have routes named **LAN**, **Tailscale**, or whatever makes sense
to you. **R** chooses this computer's route. If a connection fails, you can pick
an alternative to try; the app waits for your choice.

You can [share a catalog through Git](docs/usage.md#share-a-catalog-through-git).
Machine names, addresses, usernames, groups and tags travel together.
Each computer keeps its own route choices and favorite numbers.

## Commands for scripts and agents

```sh
ssh-sessions doctor --json
ssh-sessions list --json
ssh-sessions groups list --json
ssh-sessions command MACHINE_ID --json
```

`command` prints the SSH arguments for a saved machine. Get its ID from `list`.
[More commands](docs/usage.md#commands-for-scripts-and-agents) cover import,
favorites and bulk organization. [AGENTS.md](AGENTS.md) maps the code and checks.

## Windows Terminal integration

[Terminal Workspace](https://github.com/brant92good/terminal-workspace) can make
this picker your new-tab screen and give local PowerShell its own shortcut.
It also pairs remote tabs with [Port Forward TUI](https://github.com/brant92good/port-forward-tui)
for reaching remote web apps. SSH Sessions can be installed on its own.

## Testing

CI runs on Windows, Ubuntu and macOS with Python 3.12/3.13. It checks keyboard
flows, catalog changes, installation and updates. Real SSH login, logout back
to the picker and the local PowerShell shortcut were also checked on Windows.
[Verification and reproduction](docs/verification.md) records the details.

Found a rough edge? [Open an issue](https://github.com/brant92good/ssh-session-tui/issues)
with your OS, terminal app and what you pressed. Setup reports from macOS and
Linux are especially useful while those platforms are in beta.

[Data format](docs/design.md) · [Backlog](docs/backlog.md) · [MIT license](LICENSE)
