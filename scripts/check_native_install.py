"""Developer check for binary installation and updates in an owned temp directory."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--with-path', action='store_true', help='Only use on a disposable CI runner')
    options = parser.parse_args()
    source = Path(__file__).resolve().parents[1]
    version = tomllib.loads((source / 'Cargo.toml').read_text(encoding='utf-8'))['package']['version']
    binary = options.binary.resolve(strict=True)
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    flags = getattr(subprocess, 'CREATE_NO_WINDOW', 0)
    with tempfile.TemporaryDirectory(prefix='native-install-') as temporary:
        root = Path(temporary) / "space 測試 and ' quote"
        root.mkdir()
        install_dir = root / 'app'
        test_home = root / 'home'
        test_home.mkdir()
        (test_home/'.bashrc').write_text('# existing interactive settings\n')
        (test_home/'.bash_login').write_text('# existing login settings\n')
        (test_home/'.profile').write_text('# existing shared settings\n')
        def install(expected=digest, destination=install_dir, success=True):
            env = dict(os.environ)
            if os.name == 'nt':
                command = ['powershell.exe', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
                           '-File', str(source / 'install.ps1'), '-InstallDir', str(destination),
                           '-Binary', str(binary), '-Sha256', expected]
                if not options.with_path:
                    command.append('-NoPath')
            else:
                command = ['sh', str(source / 'install.sh')]
                env.update(SSH_SESSIONS_INSTALL_DIR=str(destination), SSH_SESSIONS_BINARY=str(binary),
                           SSH_SESSIONS_SHA256=expected, SSH_SESSIONS_NO_PATH='0' if options.with_path else '1',
                           HOME=str(test_home), SHELL='/bin/bash')
            result = subprocess.run(command, env=env, capture_output=True, encoding='utf-8', errors='replace', timeout=45, creationflags=flags)
            assert (result.returncode == 0) == success, (result.stdout, result.stderr)
        def run(*args):
            executable = install_dir / ('bin/ssh-sessions.exe' if os.name == 'nt' else 'bin/ssh-sessions')
            environment = dict(os.environ, PYTHONHOME=str(root/'missing-python'), PYTHONPATH=str(root),
                               VIRTUAL_ENV=str(root/'missing-venv'), CONDA_PREFIX=str(root/'missing-conda'))
            result = subprocess.run([str(executable), *args], cwd=root, env=environment, capture_output=True,
                                    encoding='utf-8', timeout=10, creationflags=flags)
            assert result.returncode == 0, (result.stdout, result.stderr)
            return result.stdout
        install()
        assert run('--version').strip() == f'ssh-sessions {version}'
        assert not (install_dir/'python').exists() and not (install_dir/'uv').exists()
        catalog, state = root/'machines.json', root/'device'
        args = ['--catalog', str(catalog), '--state-dir', str(state)]
        run(*args, 'init')
        run(*args, 'favorites', 'set', '2', '--local', '--json')
        before = catalog.read_bytes()
        favorites = json.loads(run(*args, 'favorites', '--json'))
        # A 0.5 owned installation can retain its runtime for already-open tabs.
        (install_dir/'python').mkdir()
        (install_dir/'python'/'legacy-session.txt').write_text('Keep for old sessions')
        if os.name == 'nt':
            (install_dir/'bin'/'ssh-sessions.cmd').write_text('@echo off\npython -m ssh_sessions %*\n')
        install()
        assert (install_dir/'python'/'legacy-session.txt').read_text() == 'Keep for old sessions'
        if os.name == 'nt':
            assert not (install_dir/'bin'/'ssh-sessions.cmd').exists()
        assert catalog.read_bytes() == before
        assert json.loads(run(*args, 'favorites', '--json')) == favorites
        install(expected='0'*64, success=False)
        assert run('--version').strip() == f'ssh-sessions {version}'
        assert json.loads(run(*args, 'favorites', '--json')) == favorites
        collision = root/'not-owned'
        collision.mkdir()
        (collision/'keep.txt').write_text('Other project')
        install(destination=collision, success=False)
        assert (collision/'keep.txt').read_text() == 'Other project'
        if options.with_path:
            if os.name == 'nt':
                import winreg
                with winreg.OpenKey(winreg.HKEY_CURRENT_USER, 'Environment') as key:
                    user_path = winreg.QueryValueEx(key, 'Path')[0]
                assert sum(Path(p).resolve() == (install_dir/'bin').resolve() for p in user_path.split(';') if p) == 1
            else:
                fragment = install_dir/'env'
                if fragment.exists():
                    check = subprocess.run(['sh', '-c', '. "$1"; command -v ssh-sessions', 'check', str(fragment)],
                                           capture_output=True, text=True, timeout=5)
                    assert check.returncode == 0 and check.stdout.strip() == str(install_dir/'bin/ssh-sessions'), check.stderr
                expected_command = str(install_dir/'bin/ssh-sessions')
                shell_environment = dict(os.environ, HOME=str(test_home), PATH='/usr/bin:/bin')
                for mode in (['-lc'], ['--noprofile', '-ic']):
                    check = subprocess.run(['/bin/bash', *mode, 'command -v ssh-sessions'],
                                           env=shell_environment, capture_output=True, text=True, timeout=10)
                    assert check.returncode == 0 and check.stdout.strip() == expected_command, (mode, check.stdout, check.stderr)
                assert not (test_home/'.bash_profile').exists()
                assert (test_home/'.profile').read_text() == '# existing shared settings\n'
                for profile in ('.bashrc', '.bash_login'):
                    content = (test_home/profile).read_text()
                    assert content.startswith('# existing ') and content.count('# ssh-sessions') == 1, content
    print('PASS: native install/update, Unicode and quoted paths, checksum rejection, unrelated directory, data preservation, inherited dev environment.')


if __name__ == '__main__':
    main()
