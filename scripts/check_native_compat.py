"""Developer-only migration check: old files/locks against the compiled Rust CLI.

All data lives in a temporary directory. Does not open a window or contact a host.
Python is used only as the old implementation under comparison, not by the binary.
"""
import argparse
from dataclasses import asdict
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from ssh_sessions.catalog import Catalog, Machine, Route, locked
from ssh_sessions.favorites import Favorites, LOCAL
from ssh_sessions.ssh_import import scan_ssh


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    options = parser.parse_args()
    binary = options.binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix='native-ssh-compat-') as temporary:
        root = Path(temporary)
        catalog = Catalog(root / 'shared folder/catalog.json', root / 'device')
        machine = Machine('demo', 'Development 開發', 'dev', (
            Route('lan', 'LAN', '192.0.2.10'), Route('vpn', 'VPN', 'demo.example.test', 2222)),
            group='Work/Lab', tags=('gpu', 'windows'))
        catalog.save((machine,), catalog.load().revision)
        catalog.choose(machine, 'vpn')
        Favorites(catalog).assign(1, 'demo')
        Favorites(catalog).assign(2, LOCAL)
        def run(*args, success=True):
            result = subprocess.run([str(binary), '--catalog', str(catalog.path), '--state-dir',
                str(catalog.state_dir), *args, '--json'], capture_output=True, encoding='utf-8',
                timeout=10, creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
            data = json.loads(result.stdout)
            assert (result.returncode == 0) == success, (args, data, result.stderr)
            return data

        rows = run('list')['machines']
        expected = json.loads(json.dumps(asdict(machine)))
        expected['selected_route'] = 'vpn'
        assert rows == [expected], (rows, expected)
        assert [r['target'] for r in run('favorites')['favorites']] == ['demo', LOCAL]
        run('favorites', 'set', '3', '--local')
        assert Favorites(catalog).load().slots == {'1': 'demo', '3': LOCAL}
        run('organize', '--machine', 'demo', '--group', 'Projects/GPU', '--add-tag', 'training')
        changed = catalog.load().machines[0]
        assert changed.id == machine.id and changed.routes == machine.routes
        assert changed.group == 'Projects/GPU' and changed.tags == ('gpu', 'windows', 'training')
        assert catalog.preferred(changed).id == 'vpn'
        before = Favorites(catalog).path.read_bytes()
        with locked(catalog.lock_path):
            error = run('favorites', 'set', '4', '--local', success=False)
            assert 'Another tab' in error['error'], error
        assert Favorites(catalog).path.read_bytes() == before

        config = root / 'SSH config'
        include = root / 'included'
        include.write_text('Host alpha beta\n HostName 192.0.2.44\n User dev\n Port 2222\n', encoding='utf-8')
        config.write_text(f'Include "{include.as_posix()}"\nHost * !beta\n User fallback\n', encoding='utf-8')
        old = [asdict(entry) for entry in scan_ssh(config).entries]
        new = run('import-ssh', '--config', str(config))['hosts']
        assert [{k: v for k, v in row.items() if k != 'status'} for row in new] == old
        run('import-ssh', '--config', str(config), '--apply', '--host', 'alpha')
        imported = next(m for m in catalog.load().machines if m.id != 'demo')
        assert imported.routes[0].ssh_alias == 'alpha'
        assert run('command', imported.id)['argv'][-1] == 'alpha'
        assert not (root / 'shared folder' / 'device').exists()
        # A compiled executable still works with only the system SSH directory in PATH.
        environment = dict(os.environ, PATH=str(Path(os.environ.get('SystemRoot', '/usr')) / 'System32/OpenSSH') if os.name == 'nt' else '/usr/bin:/bin')
        result = subprocess.run([str(binary), '--catalog', str(catalog.path), '--state-dir', str(catalog.state_dir),
            'list', '--json'], capture_output=True, encoding='utf-8', env=environment, timeout=10,
            creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0), cwd=root)
        assert result.returncode == 0 and json.loads(result.stdout)['ok'], result.stderr
    print('Native compatibility passed: catalog, device routes, favorite path, shared lock, groups, import, argv, no Python PATH.')


if __name__ == '__main__':
    main()
