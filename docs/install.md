# Install SSH Sessions

The README's one-command installers download uv 0.10.10 from Astral, then
Python 3.12 and the tagged SSH Sessions 0.5.0 package from this repository.
They do not use an existing project virtual environment or require Git.
The app command runs its installed package with isolated Python settings,
including when a project contains its own `ssh_sessions.py`.

Before running a downloaded script, you can inspect [install.ps1](../install.ps1)
or [install.sh](../install.sh). Internet access is needed during installation.
The normal install updates your user PATH through `uv tool update-shell`.
Open a new terminal if the current shell cannot find the command yet.
The Windows command sets execution policy for its installer process only;
it does not change your saved user or machine execution policy.

## Locations

| System | App files | Catalog and device preferences |
| --- | --- | --- |
| Windows | `%LOCALAPPDATA%\Programs\SSHSessions` | `%LOCALAPPDATA%\SSHSessions` |
| Linux | `$XDG_DATA_HOME/ssh-sessions-install`, or `~/.local/share/ssh-sessions-install` | `$XDG_DATA_HOME/SSHSessions`, or `~/.local/share/SSHSessions` |
| macOS | `~/.local/share/ssh-sessions-install` (honors XDG_DATA_HOME if set) | `~/Library/Application Support/SSHSessions` |

Use `--catalog PATH --state-dir PATH` before a CLI subcommand to override app
data locations. Keep device preferences outside a synced repository.
The install directory holds its own runtime and tool environment; do not move
it after installation. Reinstall into a new location instead.

## Scripted installation

Download the script to a file to pass PowerShell options:

```powershell
irm https://raw.githubusercontent.com/brant92good/ssh-session-tui/main/install.ps1 -OutFile install-ssh-sessions.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\install-ssh-sessions.ps1 -InstallDir C:\Tools\SSHSessions -NoPath
C:\Tools\SSHSessions\bin\ssh-sessions.cmd --version
```

`-NoPath` skips PATH changes. For Unix shells, the equivalents are
`SSH_SESSIONS_INSTALL_DIR=/absolute/path` and `SSH_SESSIONS_NO_PATH=1`, set in
the environment of `sh install.sh`. These installers do not open the picker,
start connections or prompt for a host.

`-Package PATH_OR_URL` / `SSH_SESSIONS_PACKAGE` overrides the release package,
for example to test a local checkout. Install directories must be empty or
marked as belonging to this installer. Other tools' directories are refused.

## Existing Python or uv

For developers who already manage Python, clone this repository and use
`python -m pip install .` in a Python 3.12+ virtual environment. The standard
package command is also `ssh-sessions`. With uv installed:

```sh
uv tool install --python 3.12 https://github.com/brant92good/ssh-session-tui/archive/refs/tags/v0.5.0.tar.gz
```

That standard uv command uses your uv tool directories; the one-command
installers above use a separate app directory and an isolated launcher.

## Missing SSH and local shells

Run `ssh-sessions doctor --json` to check setup. On Windows, install OpenSSH
Client in Settings → Optional features if `ssh` is missing. Linux needs its
distribution's OpenSSH client package; macOS includes an SSH client.

Local terminal uses PowerShell 7 on Windows when available, then Windows
PowerShell or cmd. Linux/macOS use the shell named by `SHELL`, or `/bin/sh`
when it is unset. An invalid configured shell produces an error to correct.
Your normal shell startup files still run.

## Update and uninstall

Rerun the README install command to install its current tagged release. It
recreates the tool environment while leaving the separate catalog and device
files in place. Close active SSH Sessions pickers before updating on Windows,
where an executable in use may prevent replacement. Existing SSH sessions
are not an installer test target.

To uninstall, close the app, remove the installer-owned app directory shown
above, and remove its `bin` entry from your user PATH/shell startup file.
Catalog data remains available for a later reinstall. Delete that separate
data directory only if you also want to remove your saved machines/preferences.
