# Install SSH Sessions

The [README commands](../README.md#install) download a compiled Rust executable
for your OS and CPU, verify its SHA-256 checksum, and add its `bin` directory to
your user PATH. No Python, Cargo or Git is installed. Setup does not ask for a
host, open a window, start SSH, or modify SSH configuration.

Inspect [install.ps1](../install.ps1) or [install.sh](../install.sh) before running
them if you prefer. Installation needs HTTPS access to GitHub releases. The
Windows command changes execution policy only for that installer process.

## Supported binaries

| System | CPU | Notes |
| --- | --- | --- |
| Windows 10/11 | x64 | Static C runtime; no separate VC runtime installer |
| Linux | x64, ARM64 | musl static build; no recent glibc prerequisite |
| macOS | ARM64, Intel | **Beta**; see [platform evidence](verification.md) |

Windows ARM64 currently uses the x64 binary through Windows emulation. A native
Windows ARM64 build is not supplied or independently qualified.

OpenSSH (`ssh`) is needed to connect. `ssh-sessions doctor --json` checks for it.
On Windows it is the OpenSSH Client optional feature; on Linux use your
distribution's SSH client package. macOS normally includes the client.
Git is only needed for explicit catalog sync.

## Locations

| System | App files | Catalog and device state |
| --- | --- | --- |
| Windows | `%LOCALAPPDATA%\Programs\SSHSessions` | `%LOCALAPPDATA%\SSHSessions` |
| Linux | `$XDG_DATA_HOME/ssh-sessions-install`, otherwise `~/.local/share/ssh-sessions-install` | `$XDG_DATA_HOME/SSHSessions`, otherwise `~/.local/share/SSHSessions` |
| macOS | `~/.local/share/ssh-sessions-install` (honors XDG_DATA_HOME) | `~/Library/Application Support/SSHSessions` |

The executable is `bin/ssh-sessions.exe` on Windows and `bin/ssh-sessions` on Unix.
`--catalog PATH` overrides the shared file; `--state-dir PATH` overrides device
preferences, favorites and import bindings. These options work before or after
a subcommand. Keep device state outside the shared repository.

## Choose an install location

```powershell
irm https://raw.githubusercontent.com/brant92good/ssh-session-tui/v0.6.0/install.ps1 -OutFile install-ssh-sessions.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\install-ssh-sessions.ps1 -InstallDir C:\Tools\SSHSessions -NoPath
C:\Tools\SSHSessions\bin\ssh-sessions.exe --version
```

Unix equivalents are `SSH_SESSIONS_INSTALL_DIR=/absolute/path` and
`SSH_SESSIONS_NO_PATH=1` in the environment of `sh install.sh`. Those environment
variables also work with PowerShell. Unix setup adds a managed line to your shell
startup file. Bash gets it in `.bashrc` and its active login profile, so both
terminal startup modes work; Zsh uses `.zshrc`, Fish uses a `conf.d` file, and
other shells use `.profile`. Setup preserves the rest of each file.

`-Version` / `SSH_SESSIONS_VERSION` selects a release. Developer checks can use
`-Binary` / `SSH_SESSIONS_BINARY` with `-Sha256` / `SSH_SESSIONS_SHA256`. Local
test binaries require an explicit hash. Install directories must be empty or
carry this installer's ownership marker. Unrelated directories are refused.

## Update and uninstall

Rerun the current README's install command. A checksum or version mismatch leaves
the existing binary intact. Updates keep the previous executable for rollback
and do not alter your separate catalog, favorites or device preferences.

An upgrade from 0.5 replaces its command shim with the native executable. Its old
Python runtime is left in the owned install directory so existing sessions can
finish. The new command does not invoke it. Close pickers before updating on
Windows if that OS refuses to replace an executable in use.

To uninstall, close the app, remove its owned app directory, and remove that
directory's PATH entry or managed shell-startup line. Saved machine data remains
in the separate location above. Remove it only if you want to erase that data.

## Local shell

Local terminal prefers PowerShell 7 on Windows, then Windows PowerShell, then
cmd. Linux/macOS use `$SHELL`, or `/bin/sh` when unset. Your normal shell startup
files run. Type `exit` to return to the picker. An invalid shell path produces an
error; setup does not edit your shell configuration to repair it.

## Build from source

```sh
cargo build --release --locked
```

Rust 1.88 or newer is required for source builds; release CI pins its toolchain.
Python files remain temporarily as a migration reference for developer tests,
not the shipped runtime. [Checks and limitations](verification.md).
