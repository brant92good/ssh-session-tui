#!/bin/sh
# Install a released SSH Sessions package without a preinstalled Python or Git.
set -eu

main() {
    package=${SSH_SESSIONS_PACKAGE:-https://github.com/brant92good/ssh-session-tui/archive/refs/tags/v0.5.0.tar.gz}
    install_root=${SSH_SESSIONS_INSTALL_DIR:-${XDG_DATA_HOME:-"$HOME/.local/share"}/ssh-sessions-install}
    case "$install_root" in /*) ;; *) printf '%s\n' 'SSH_SESSIONS_INSTALL_DIR must be an absolute path.' >&2; return 1;; esac
    if [ -d "$install_root" ] && [ ! -f "$install_root/.ssh-sessions-installer" ] && [ -n "$(ls -A "$install_root")" ]; then
        printf '%s\n' "Choose an empty install directory: $install_root already contains other files." >&2
        return 1
    fi
    mkdir -p "$install_root"
    printf '%s\n' 'ssh-session-tui' > "$install_root/.ssh-sessions-installer"
    export UV_UNMANAGED_INSTALL="$install_root/uv"
    export UV_TOOL_DIR="$install_root/tools"
    export UV_TOOL_BIN_DIR="$install_root/tool-bin"
    export UV_PYTHON_INSTALL_DIR="$install_root/python"
    export UV_PYTHON_INSTALL_BIN=0 UV_PYTHON_INSTALL_REGISTRY=0 UV_NO_CONFIG=1
    export UV_CONCURRENT_DOWNLOADS=2 UV_CONCURRENT_BUILDS=1 UV_CONCURRENT_INSTALLS=2
    uv_app="$install_root/uv/uv"
    if [ ! -x "$uv_app" ]; then
        printf '%s\n' 'Preparing the installer...'
        download=$(mktemp)
        if command -v curl >/dev/null 2>&1; then
            curl -fsSL --retry 2 https://astral.sh/uv/0.10.10/install.sh -o "$download"
        elif command -v wget >/dev/null 2>&1; then
            wget -q https://astral.sh/uv/0.10.10/install.sh -O "$download"
        else
            printf '%s\n' 'Install curl or wget, then run this installer again.' >&2
            return 1
        fi
        sh "$download"
        rm -f "$download"
    fi
    printf '%s\n' 'Installing SSH Sessions and its Python runtime...'
    "$uv_app" --no-config --quiet tool install --managed-python --python 3.12 --reinstall --force "$package"
    app_python="$UV_TOOL_DIR/ssh-session-tui/bin/python"
    "$app_python" -E -s -m ssh_sessions.install_support "$install_root"
    export UV_TOOL_BIN_DIR="$install_root/bin"
    if [ "${SSH_SESSIONS_NO_PATH:-0}" != 1 ]; then
        # CI and shells launched from PowerShell can inherit PSModulePath.
        # Configure the login shell, rather than guessing from inherited markers.
        if ! (
            unset NU_VERSION FISH_VERSION BASH_VERSION ZSH_VERSION KSH_VERSION PSModulePath
            export SHELL="${SHELL:-/bin/bash}"
            case "$SHELL" in */sh) export BASH_VERSION=installer;; esac
            "$uv_app" --no-config tool update-shell
        ); then
            printf '%s\n' 'The app is installed. Open a new terminal if PATH was already configured; otherwise add the bin directory below to your shell PATH.'
        fi
    fi
    "$app_python" -E -s -m ssh_sessions --version
    printf '\n%s\n' 'Ready. Run ssh-sessions. If it is not found, open a new terminal or run:'
    printf '  "%s/bin/ssh-sessions"\n' "$install_root"
    printf '%s\n' 'Press A to add a machine, or I to import SSH hosts.'
}

# The script also works when piped into sh; downloads complete before execution.
main "$@"
