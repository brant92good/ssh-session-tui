"""Exercise the advertised HTTPS one-command installer in an owned directory.

The release must already exist. This is not a local-binary override test.
No PATH/profile changes, desktop windows, SSH connections or account operations.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--version', default='0.6.1')
    parser.add_argument('--ref', help='Pinned source tag or commit, default vVERSION')
    options = parser.parse_args()
    version = options.version
    ref = options.ref or 'v' + version
    if not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+(?:[-.][A-Za-z0-9.-]+)?', version):
        parser.error('Invalid version')
    if not re.fullmatch(r'[A-Za-z0-9._/-]+', ref):
        parser.error('Invalid source ref')
    flags = getattr(subprocess, 'CREATE_NO_WINDOW', 0)
    extension = 'ps1' if os.name == 'nt' else 'sh'
    url = f'https://raw.githubusercontent.com/brant92good/ssh-session-tui/{ref}/install.{extension}'
    with tempfile.TemporaryDirectory(prefix='ssh-release-') as temporary:
        root = Path(temporary) / "space \u6e2c\u8a66 ' release"
        root.mkdir()
        installed = root / 'app'
        environment = dict(os.environ, SSH_SESSIONS_INSTALL_DIR=str(installed),
                           SSH_SESSIONS_NO_PATH='1', SSH_SESSIONS_VERSION=version,
                           PYTHONHOME=str(root/'missing-python'), PYTHONPATH=str(root/'shadow'),
                           VIRTUAL_ENV=str(root/'missing-venv'), CONDA_PREFIX=str(root/'missing-conda'))
        environment.pop('SSH_SESSIONS_BINARY', None)
        environment.pop('SSH_SESSIONS_SHA256', None)
        command = (['powershell.exe', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
                    '-Command', f'irm {url} | iex'] if os.name == 'nt' else
                   ['sh', '-c', f'curl -fsSL {url} | sh'])
        def install(success=True):
            result = subprocess.run(command, env=environment, capture_output=True, timeout=180,
                                    encoding='utf-8', errors='replace', creationflags=flags)
            assert (result.returncode == 0) == success, (result.stdout, result.stderr)
        executable = installed / ('bin/ssh-sessions.exe' if os.name == 'nt' else 'bin/ssh-sessions')
        def run(*args):
            result = subprocess.run([str(executable), *args], env=environment, cwd=root,
                                    capture_output=True, encoding='utf-8', timeout=15, creationflags=flags)
            assert result.returncode == 0, (result.stdout, result.stderr)
            return result.stdout
        install()
        assert run('--version').strip() == 'ssh-sessions ' + version
        digest = hashlib.sha256(executable.read_bytes()).hexdigest()
        args = ['--catalog', str(root/'catalog.json'), '--state-dir', str(root/'device')]
        run(*args, 'init')
        run(*args, 'favorites', 'set', '2', '--local', '--json')
        favorites = json.loads(run(*args, 'favorites', '--json'))
        install()
        assert json.loads(run(*args, 'favorites', '--json')) == favorites
        environment['SSH_SESSIONS_SHA256'] = '0' * 64
        install(success=False)
        assert hashlib.sha256(executable.read_bytes()).hexdigest() == digest
        assert json.loads(run(*args, 'favorites', '--json')) == favorites
        assert not (installed/'python').exists() and not (installed/'uv').exists()
        print(json.dumps({'ok':True, 'version':version, 'installer':url, 'sha256':digest,
                          'checks':['HTTPS install','update','checksum rejection','saved favorite','polluted environment'],
                          'platform':os.name}))


if __name__ == '__main__':
    main()
