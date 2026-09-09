"""Create a command that ignores inherited Python import/environment settings."""
import os
from pathlib import Path
import shlex
import sys


def launcher(root, python):
    root, python = Path(root).absolute(), Path(python).absolute()
    # Keep the venv path: resolving its Python symlink would launch base Python
    # without the app's installed packages on Unix.
    if not python.parent.resolve().is_relative_to(root.resolve()):
        raise ValueError('The launcher must use Python from this app installation.')
    directory = root / 'bin'
    directory.mkdir(parents=True, exist_ok=True)
    if os.name == 'nt':
        path = directory / 'ssh-sessions.cmd'
        # Relative paths keep non-ASCII user names out of cmd's file encoding.
        relative = os.path.relpath(python, directory).replace('%', '%%')
        text = '@echo off\n"%~dp0' + relative + '" -I -X utf8 -m ssh_sessions %*\n'
    else:
        path = directory / 'ssh-sessions'
        text = '#!/bin/sh\nexec ' + shlex.quote(str(python)) + ' -I -X utf8 -m ssh_sessions "$@"\n'
    path.write_text(text, encoding='utf-8', newline='\r\n' if os.name == 'nt' else '\n')
    if os.name != 'nt':
        path.chmod(0o755)
    return path


if __name__ == '__main__':
    launcher(sys.argv[1], sys.executable)
    print('Created the ssh-sessions command.')
