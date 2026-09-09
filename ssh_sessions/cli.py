"""Launch the picker or inspect/sync its metadata without importing the UI."""
import argparse
from dataclasses import asdict
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys

from .catalog import Catalog, CatalogError
from .connection import local_command, run_session, set_title, ssh_command
from .ssh_import import connection_config


def default_directory():
    return Path(os.environ.get('LOCALAPPDATA', Path.home() / '.local/share')) / 'SSHSessions'


def picker_loop(catalog, picker_factory=None, runner=run_session):
    if picker_factory is None:
        from .ui import Picker
        picker_factory = Picker
    notice = ''
    failed = None
    while True:
        set_title('SSH Sessions')
        choice = picker_factory(catalog, notice=notice, failed_machine=failed).run()
        if choice is None:
            return 0
        failed = None
        try:
            command = local_command() if choice.kind == 'local' else ssh_command(choice.machine, choice.route,
                config=connection_config(catalog, choice.machine, choice.route))
            set_title('Local PowerShell' if choice.kind == 'local' else f'{choice.machine.name} | {choice.route.name}')
            if choice.kind == 'connect':
                print(f'Connecting to {choice.machine.name} via {choice.route.name} ({choice.route.host}:{choice.route.port})', flush=True)
            code = runner(command)
            notice = f'{"Local shell" if choice.kind == "local" else "SSH session"} ended (exit {code}). Choose a machine to connect again.'
            if choice.kind == 'connect' and code == 255:
                failed = choice.machine.id
                notice = 'SSH exited with code 255. Check your network and local SSH authentication. No alternative route was started.'
                if sys.stdin.isatty():
                    input('SSH failed. Read the message above, then press Enter to return to the picker and choose a route. ')
        except KeyboardInterrupt:
            notice = 'Session interrupted. Choose a machine to connect again.'
        except (OSError, ValueError) as error:
            notice = str(error)


def main(argv=None):
    parser = argparse.ArgumentParser(description='Choose an SSH machine and a route for this device. Private keys remain with your existing SSH client.')
    parser.add_argument('--catalog', type=Path, default=default_directory() / 'catalog.json', help='Shared metadata JSON; point this at a file in your private Git repo')
    parser.add_argument('--state-dir', type=Path, default=default_directory() / 'device', help='Device-only preferences; keep outside your shared repository')
    commands = parser.add_subparsers(dest='command')
    listing = commands.add_parser('list', help='List machines and this device\'s selected routes without connecting')
    listing.add_argument('--json', action='store_true')
    commands.add_parser('init', help='Create an empty catalog if missing; preserve an existing file')
    doctor = commands.add_parser('doctor', help='Check local setup without opening SSH or changing files')
    doctor.add_argument('--json', action='store_true')
    sync = commands.add_parser('sync', help='Explicitly pull or publish the tracked catalog through its existing Git upstream')
    sync.add_argument('action', choices=('pull', 'publish'))
    sync.add_argument('--json', action='store_true')
    command = commands.add_parser('command', help='Print the selected SSH argv without starting SSH')
    command.add_argument('machine', help='Machine ID or exact name')
    command.add_argument('--route', help='Route ID or exact name; required when this device has no preference')
    command.add_argument('--json', action='store_true')
    importing = commands.add_parser('import-ssh', help='Preview local SSH hosts; import only with --apply and --host NAME or --all')
    importing.add_argument('--config', type=Path, help='Read a custom SSH config; its path stays on this device')
    importing.add_argument('--host', action='append', default=[], help='Alias to import; repeat for multiple hosts')
    importing.add_argument('--all', action='store_true', help='Select all entries without unresolved metadata')
    importing.add_argument('--apply', action='store_true', help='Save selected metadata after review; does not connect or publish')
    importing.add_argument('--json', action='store_true')
    options = parser.parse_args(argv)
    structured = getattr(options, 'json', False)
    try:
        catalog = Catalog(options.catalog, options.state_dir)
        snapshot = catalog.load()
        if options.command == 'import-ssh':
            from .ssh_import import scan_ssh, import_status, import_selected
            scan = scan_ssh(options.config)
            rows = [{**asdict(entry), 'status': import_status(snapshot.machines, entry)} for entry in scan.entries]
            result = {'ok': True, 'source': str(scan.config), 'hosts': rows, 'applied': False}
            if options.apply:
                selected = [e.alias for e in scan.entries if import_status(snapshot.machines, e) in
                    ('Ready to import', 'Already imported (select to bind this device)')] if options.all else options.host
                result.update(import_selected(catalog, scan, selected, snapshot.revision), applied=True)
            print(json.dumps(result, ensure_ascii=True) if structured else '\n'.join(
                [f"{r['alias']} | {r['user']}@{r['host']}:{r['port']} | {r['status']}" for r in rows] +
                (['Imported selected hosts. SSH config and keys were not changed.'] if options.apply else
                 ['Preview only. Use --apply with --host NAME or --all to import; nothing was saved or connected.'])))
            return 0
        if options.command == 'doctor':
            checks = {'catalog_valid': True, 'machine_count': len(snapshot.machines),
                      'ssh_available': bool(shutil.which('ssh.exe' if os.name == 'nt' else 'ssh')),
                      'git_available': bool(shutil.which('git')), 'local_pwsh_available': bool(shutil.which('pwsh'))}
            catalog.preferences()
            result = {'ok': checks['ssh_available'], 'checks': checks,
                      'next_step': 'Run without a command to choose or add a machine. Git is needed only for sync.'}
            print(json.dumps(result) if structured else '\n'.join([*(f'{k}: {v}' for k, v in checks.items()), result['next_step']]))
            return 0 if result['ok'] else 1
        if options.command == 'init':
            if not catalog.path.exists():
                catalog.save((), snapshot.revision)
            print(f'Catalog ready: {catalog.path}')
            return 0
        if options.command == 'sync':
            from .sync import GitSync
            message = GitSync(catalog).run(options.action)
            print(json.dumps({'ok': True, 'message': message}) if structured else message)
            return 0
        if options.command == 'list':
            rows = []
            preferences = catalog.preferences()
            for machine in snapshot.machines:
                route = catalog.preferred(machine, preferences)
                rows.append({**asdict(machine), 'selected_route': route.id if route else None})
            if structured:
                print(json.dumps({'ok': True, 'version': 1, 'machines': rows}, ensure_ascii=True))
            else:
                for machine in rows:
                    route = next((r for r in machine['routes'] if r['id'] == machine['selected_route']), None)
                    print(f"{machine['name']}  |  {route['name'] + ': ' + route['host'] if route else 'Choose a route'}  |  {machine['user']}")
                if not rows:
                    print('No machines yet. Run without a command and press A to add one.')
            return 0
        if options.command == 'command':
            matches = [m for m in snapshot.machines if options.machine in (m.id, m.name)]
            if len(matches) != 1:
                raise CatalogError('Select one machine by its unique ID or exact name.')
            machine = matches[0]
            if options.route:
                routes = [r for r in machine.routes if options.route in (r.id, r.name)]
                if len(routes) != 1: raise CatalogError('Select a unique route ID or name.')
                route = routes[0]
            else:
                route = catalog.preferred(machine)
            if route is None:
                raise CatalogError('Choose a route explicitly with --route or in the TUI.')
            args = ssh_command(machine, route, config=connection_config(catalog, machine, route))
            print(json.dumps({'ok': True, 'argv': args}) if structured else (subprocess.list2cmdline(args) if os.name == 'nt' else shlex.join(args)))
            return 0
        return picker_loop(catalog)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        if structured:
            print(json.dumps({'ok': False, 'error': str(error)}))
        else:
            print(f'SSH Sessions: {error}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
