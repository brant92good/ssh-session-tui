"""Network-enabled install/update check; all app data stays in a test directory.

Run outside the ordinary unit suite. No PATH/profile edits, windows or SSH logins.
Optional --package exercises a published archive instead of the local checkout.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    source = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package', default=str(source))
    parser.add_argument('--with-path', action='store_true', help='Also update PATH; for disposable CI runners only')
    options = parser.parse_args()
    # Keep the directory for diagnosis if a check fails; report the path.
    root = Path(tempfile.mkdtemp(prefix='ssh-install-')) / 'space 測試'
    root.mkdir()
    print(f'Isolated install check: {root}', flush=True)
    installed = root / 'app'
    env = dict(os.environ)
    if os.name == 'nt':
        installer = ['powershell.exe', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
                     str(source / 'install.ps1'), '-InstallDir', str(installed),
                     '-Package', options.package]
        if not options.with_path:
            installer.append('-NoPath')
        command = installed / 'bin/ssh-sessions.cmd'
    else:
        env.update(SSH_SESSIONS_INSTALL_DIR=str(installed), SSH_SESSIONS_NO_PATH='0' if options.with_path else '1',
                   SSH_SESSIONS_PACKAGE=options.package)
        installer = ['sh', str(source / 'install.sh')]
        command = installed / 'bin/ssh-sessions'
    def install():
        subprocess.run(installer, env=env, check=True, timeout=360)
    def run(*args):
        result = subprocess.run([str(command), *args], env=poisoned, cwd=root,
                                shell=os.name == 'nt',
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
        if result.returncode:
            raise AssertionError((result.stdout + result.stderr).decode('utf-8', errors='replace'))
        return result.stdout.decode('utf-8', errors='replace')
    install()
    if options.with_path:
        if os.name == 'nt':
            import winreg
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, 'Environment') as key:
                user_path = winreg.QueryValueEx(key, 'Path')[0]
            assert str(installed / 'bin').casefold() in user_path.casefold()
        else:
            found = subprocess.check_output([os.environ.get('SHELL') or '/bin/sh', '-l', '-c',
                                             'command -v ssh-sessions'], text=True).strip()
            assert found == str(command), found
    # Simulate a project that happens to contain a module with the same name,
    # plus an active Python environment. Neither should replace installed code.
    (root / 'ssh_sessions.py').write_text("raise RuntimeError('wrong import from project')\n")
    poisoned = dict(env, PYTHONHOME=str(root / 'missing-python'), PYTHONPATH=str(root))
    assert run('--version').startswith('SSH Sessions ')
    catalog, state = root / 'my machines 測試.json', root / 'device'
    arguments = ['--catalog', str(catalog), '--state-dir', str(state)]
    run(*arguments, 'init')
    run(*arguments, 'favorites', 'set', '2', '--local', '--json')
    before = catalog.read_bytes()
    favorites = json.loads(run(*arguments, 'favorites', 'list', '--json'))
    install()  # The same command upgrades/reinstalls without erasing app data.
    assert catalog.read_bytes() == before
    assert json.loads(run(*arguments, 'favorites', 'list', '--json')) == favorites
    assert json.loads(run(*arguments, 'list', '--json'))['machines'] == []
    print('PASS: install, update, Unicode paths, argument forwarding, isolated Python, data preserved.')


if __name__ == '__main__':
    main()
